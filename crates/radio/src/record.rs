//! Records the audio that is heard to an Ogg Opus file.
//!
//! The radio thread only sends 48 kHz chunks down a channel; a writer thread encodes them with libopus
//! (dlopened, like nrsc5 and mbelib: `pacman -S opus`) and muxes them with the `ogg` crate.
//! Files go to `<music dir>/squelch/`, named by UTC time, frequency and mode.

use crate::demod::{Mode, AUDIO_RATE};
use anyhow::{anyhow, Context, Result};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use serde::Serialize;
use std::ffi::c_int;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime};

/// 20 ms of 48 kHz audio per Opus packet.
const FRAME: usize = 960;
/// Longest packet libopus is asked to produce.
const MAX_PACKET: usize = 1500;
/// Packets per Ogg page: a page is one second of audio.
const PAGE_PACKETS: u32 = 50;
const OPUS_APPLICATION_AUDIO: c_int = 2049;
const OPUS_GET_LOOKAHEAD: c_int = 4027;
const OPUS_SET_BITRATE: c_int = 4002;
/// Mono speech and radio audio: plenty at this rate.
const BITRATE: c_int = 32_000;

/// What the UI shows: the file being written, or the last one finished.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Info {
    pub recording: bool,
    pub file: Option<String>,
    /// Seconds recorded so far, or in the finished file.
    pub secs: f64,
    pub error: Option<String>,
}

struct Shared {
    file: Option<String>,
    started: Option<Instant>,
    secs: f64,
    error: Option<String>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { file: None, started: None, secs: 0.0, error: None });

/// The recording state, for the shell to hand to the page.
pub fn info() -> Info {
    let s = SHARED.lock().unwrap();
    Info {
        recording: s.started.is_some(),
        file: s.file.clone(),
        secs: s.started.map_or(s.secs, |t| t.elapsed().as_secs_f64()),
        error: s.error.clone(),
    }
}

/// Note a failure to start, so the page can say why nothing is recording.
pub fn fail(e: &anyhow::Error) {
    let mut s = SHARED.lock().unwrap();
    s.error = Some(format!("{e:#}"));
    s.started = None;
}

/// Where recordings go: the XDG music directory, else `~/Music`, then `squelch/`.
pub fn dir() -> PathBuf {
    dirs::audio_dir().or_else(|| dirs::home_dir().map(|h| h.join("Music"))).unwrap_or_else(|| PathBuf::from("Music")).join("squelch")
}

struct Opus {
    _lib: libloading::Library,
    encoder: *mut u8,
    encode: unsafe extern "C" fn(*mut u8, *const f32, c_int, *mut u8, c_int) -> c_int,
    destroy: unsafe extern "C" fn(*mut u8),
    /// Samples the decoder drops at the start.
    lookahead: u16,
}

// The encoder is only touched by the writer thread.
unsafe impl Send for Opus {}

impl Opus {
    fn open() -> Result<Self> {
        unsafe {
            let lib = ["libopus.so.0", "libopus.so"]
                .iter()
                .find_map(|n| libloading::Library::new(n).ok())
                .ok_or_else(|| anyhow!("libopus not found (pacman -S opus)"))?;
            let create: unsafe extern "C" fn(c_int, c_int, c_int, *mut c_int) -> *mut u8 = *lib.get(b"opus_encoder_create\0")?;
            let ctl: unsafe extern "C" fn(*mut u8, c_int, ...) -> c_int = *lib.get(b"opus_encoder_ctl\0")?;
            let encode = *lib.get(b"opus_encode_float\0")?;
            let destroy = *lib.get(b"opus_encoder_destroy\0")?;
            let mut err = 0;
            let encoder = create(AUDIO_RATE as c_int, 1, OPUS_APPLICATION_AUDIO, &mut err);
            if encoder.is_null() || err != 0 {
                return Err(anyhow!("opus encoder: error {err}"));
            }
            let mut look: c_int = 312;
            ctl(encoder, OPUS_GET_LOOKAHEAD, &mut look as *mut c_int);
            ctl(encoder, OPUS_SET_BITRATE, BITRATE);
            Ok(Opus { _lib: lib, encoder, encode, destroy, lookahead: look.clamp(0, 65535) as u16 })
        }
    }

    fn encode(&mut self, frame: &[f32; FRAME], out: &mut [u8; MAX_PACKET]) -> Result<Vec<u8>> {
        let n = unsafe { (self.encode)(self.encoder, frame.as_ptr(), FRAME as c_int, out.as_mut_ptr(), MAX_PACKET as c_int) };
        if n < 0 {
            return Err(anyhow!("opus encode: error {n}"));
        }
        Ok(out[..n as usize].to_vec())
    }
}

impl Drop for Opus {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.encoder) }
    }
}

/// A recording in progress. Dropping it finishes the file.
pub struct Recorder {
    tx: Option<Sender<Vec<f32>>>,
    handle: Option<JoinHandle<()>>,
}

impl Recorder {
    /// Start a file for `tuned_hz` in `mode` under `dir`.
    pub fn start(dir: PathBuf, tuned_hz: f64, mode: Mode) -> Result<Self> {
        std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
        let path = dir.join(file_name(SystemTime::now(), tuned_hz, mode));
        let opus = Opus::open()?;
        let file = BufWriter::new(File::create(&path).with_context(|| format!("cannot create {}", path.display()))?);
        let (tx, rx) = channel::<Vec<f32>>();
        *SHARED.lock().unwrap() = Shared { file: Some(path.display().to_string()), started: Some(Instant::now()), secs: 0.0, error: None };
        let handle = std::thread::Builder::new().name("record".into()).spawn(move || {
            let result = write(file, opus, rx);
            let mut s = SHARED.lock().unwrap();
            s.started = None;
            match result {
                Ok(samples) => s.secs = samples as f64 / AUDIO_RATE as f64,
                Err(e) => s.error = Some(format!("{e:#}")),
            }
        })?;
        Ok(Recorder { tx: Some(tx), handle: Some(handle) })
    }

    /// Hand over the audio just played; never blocks.
    pub fn push(&self, pcm: &[f32]) {
        if let (Some(tx), false) = (&self.tx, pcm.is_empty()) {
            let _ = tx.send(pcm.to_vec());
        }
    }
}

impl Drop for Recorder {
    /// Closing the channel makes the writer encode what is left and end the file; the wait is a few milliseconds.
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Encode chunks as they arrive until the channel closes. Returns the samples written.
fn write(file: BufWriter<File>, mut opus: Opus, rx: Receiver<Vec<f32>>) -> Result<u64> {
    let serial = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() ^ d.as_secs() as u32);
    let mut ogg = PacketWriter::new(file);
    let mut head = Vec::from(*b"OpusHead");
    head.extend([1, 1]);
    head.extend(opus.lookahead.to_le_bytes());
    head.extend(AUDIO_RATE.to_le_bytes());
    head.extend([0, 0, 0]);
    ogg.write_packet(head, serial, PacketWriteEndInfo::EndPage, 0)?;
    let mut tags = Vec::from(*b"OpusTags");
    let vendor = concat!("squelch ", env!("CARGO_PKG_VERSION"));
    tags.extend((vendor.len() as u32).to_le_bytes());
    tags.extend(vendor.as_bytes());
    tags.extend(0u32.to_le_bytes());
    ogg.write_packet(tags, serial, PacketWriteEndInfo::EndPage, 0)?;

    let mut pending: Vec<f32> = Vec::new();
    let mut total = 0u64;
    let mut out = [0u8; MAX_PACKET];
    let mut frame = [0f32; FRAME];
    let mut packets = 0u32;
    // Each packet is written one step late, so the last one can carry the end-of-stream flag.
    // `held` is a packet and the granule position (audio through its end, plus the pre-skip).
    let mut held: Option<(Vec<u8>, u64)> = None;
    let mut granule = opus.lookahead as u64;
    let mut put = |ogg: &mut PacketWriter<BufWriter<File>>, held: &mut Option<(Vec<u8>, u64)>, next: (Vec<u8>, u64)| -> Result<()> {
        if let Some((data, at)) = held.replace(next) {
            packets += 1;
            let end = if packets % PAGE_PACKETS == 0 { PacketWriteEndInfo::EndPage } else { PacketWriteEndInfo::NormalPacket };
            ogg.write_packet(data, serial, end, at)?;
        }
        Ok(())
    };
    while let Ok(chunk) = rx.recv() {
        pending.extend(chunk);
        let mut used = 0;
        while pending.len() - used >= FRAME {
            frame.copy_from_slice(&pending[used..used + FRAME]);
            used += FRAME;
            granule += FRAME as u64;
            total += FRAME as u64;
            put(&mut ogg, &mut held, (opus.encode(&frame, &mut out)?, granule))?;
        }
        pending.drain(..used);
    }
    // the tail is padded with silence; its granule trims the padding off again
    if !pending.is_empty() {
        frame.fill(0.0);
        frame[..pending.len()].copy_from_slice(&pending);
        granule += pending.len() as u64;
        total += pending.len() as u64;
        put(&mut ogg, &mut held, (opus.encode(&frame, &mut out)?, granule))?;
    }
    // one silent frame flushes the encoder's lookahead; the granule stays, so a player trims it off again
    frame.fill(0.0);
    put(&mut ogg, &mut held, (opus.encode(&frame, &mut out)?, granule))?;
    // nothing heard: one silent packet, so the stream still has an end
    let (data, at) = match held.take() {
        Some(last) => last,
        None => {
            frame.fill(0.0);
            (opus.encode(&frame, &mut out)?, granule)
        }
    };
    ogg.write_packet(data, serial, PacketWriteEndInfo::EndStream, at)?;
    Ok(total)
}

/// `2026-11-01T153012Z 101.100MHz wfm.opus`
pub fn file_name(at: SystemTime, hz: f64, mode: Mode) -> String {
    let secs = at.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    let mode = format!("{mode:?}").to_lowercase();
    format!("{y:04}-{m:02}-{d:02}T{:02}{:02}{:02}Z {:.3}MHz {mode}.opus", t / 3600, t / 60 % 60, t % 60, hz / 1e6)
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn names_by_utc_time() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_547_012);
        assert_eq!(file_name(at, 101.1e6, Mode::Wfm), "2026-11-01T153012Z 101.100MHz wfm.opus");
    }

    /// A 440 Hz tone recorded in odd-sized chunks comes back as Ogg Opus of the right length.
    #[test]
    fn records_a_tone() {
        let dir = std::env::temp_dir().join(format!("squelch-rec-{}", std::process::id()));
        let rec = Recorder::start(dir.clone(), 101.1e6, Mode::Wfm).expect("start");
        assert!(info().recording);
        let secs = 3.37;
        let n = (secs * AUDIO_RATE as f64) as usize;
        let tone: Vec<f32> = (0..n).map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / AUDIO_RATE as f32).sin()).collect();
        for c in tone.chunks(1000) {
            rec.push(c);
        }
        drop(rec);
        let i = info();
        assert!(!i.recording && i.error.is_none(), "{i:?}");
        assert!((i.secs - secs).abs() < 0.001, "{}", i.secs);
        let path = i.file.unwrap();
        assert!(path.ends_with("wfm.opus"));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"OggS");
        let at = bytes.windows(8).position(|w| w == b"OpusHead").unwrap();
        let pre = u16::from_le_bytes([bytes[at + 10], bytes[at + 11]]) as u64;

        // read the packets back: the last granule less the pre-skip is the length
        let mut r = ogg::reading::PacketReader::new(std::io::Cursor::new(&bytes));
        let (mut last, mut ended, mut packets) = (0, false, 0);
        while let Some(p) = r.read_packet().unwrap() {
            last = p.absgp_page();
            ended = p.last_in_stream();
            packets += 1;
        }
        assert!(ended && packets > 100, "{packets}");
        assert!(((last - pre) as f64 / AUDIO_RATE as f64 - secs).abs() < 0.001);

        // and an independent decoder agrees, when there is one
        if let Ok(o) = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name,channels,sample_rate:format=duration", "-of", "default=nw=1"]).arg(&path).output() {
            let s = String::from_utf8_lossy(&o.stdout);
            eprintln!("{s}");
            assert!(s.contains("codec_name=opus") && s.contains("sample_rate=48000"), "{s}");
            let d: f64 = s.lines().find_map(|l| l.strip_prefix("duration=")).unwrap().parse().unwrap();
            assert!((d - secs).abs() < 0.03, "{d}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
