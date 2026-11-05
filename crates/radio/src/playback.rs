//! Plays a recording back through the radio's audio output, and lists what `record` has saved.
//!
//! A `Player` reads the Ogg Opus file the recorder writes, decodes with libopus (dlopened like the
//! encoder) and hands the engine `FRAME`-sized packets of 48 kHz audio on demand. Seeking skips
//! packets without decoding them, then decodes `PREROLL` and drops them so the decoder has settled.

use crate::demod::{Mode, AUDIO_RATE};
use anyhow::{anyhow, Result};
use ogg::reading::PacketReader;
use serde::Serialize;
use std::ffi::c_int;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Samples in one packet the recorder writes (20 ms).
const FRAME: usize = 960;
/// Packets decoded and dropped before a seek target, so the decoder has settled.
const PREROLL: usize = 4;
/// The longest Opus packet, in samples (120 ms).
const MAX_FRAME: usize = 5760;
/// Bytes read from the end of a file to find its last page.
const TAIL: usize = 64 * 1024;
const OPUS_RESET_STATE: c_int = 4028;

/// One saved recording, as the list shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recording {
    pub file: String,
    /// When it started, seconds since the Unix epoch (from the file name, UTC).
    pub at: i64,
    pub hz: f64,
    pub mode: Mode,
    pub secs: f64,
    pub bytes: u64,
}

/// Where playback stands, in the status.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Playing {
    pub file: String,
    pub secs: f64,
    pub len_secs: f64,
    pub paused: bool,
    /// The end was reached; it stays here until stopped or seeked.
    pub done: bool,
}

/// Every recording in `dir`, newest first. Files the recorder did not name are left out.
pub fn list(dir: &Path) -> Vec<Recording> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Recording> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            let (at, hz, mode) = parse_name(name)?;
            let bytes = e.metadata().ok()?.len();
            let secs = length_secs(&path).unwrap_or(0.0);
            Some(Recording { file: path.to_string_lossy().into_owned(), at, hz, mode, secs, bytes })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at).then(b.file.cmp(&a.file)));
    out
}

/// `2026-11-01T153012Z 101.100MHz wfm.opus` → (Unix seconds, Hz, mode).
pub fn parse_name(name: &str) -> Option<(i64, f64, Mode)> {
    let stem = name.strip_suffix(".opus")?;
    let mut parts = stem.split(' ');
    let (time, mhz, mode) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || time.len() != 18 || !time.ends_with('Z') || time.as_bytes()[10] != b'T' {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let (y, mo, d) = (num(&time[0..4])?, num(&time[5..7])?, num(&time[8..10])?);
    let (h, mi, s) = (num(&time[11..13])?, num(&time[13..15])?, num(&time[15..17])?);
    let at = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s;
    let hz = mhz.strip_suffix("MHz")?.parse::<f64>().ok()? * 1e6;
    let mode = [Mode::Wfm, Mode::Nfm, Mode::Am, Mode::Usb, Mode::Lsb, Mode::None, Mode::P25].into_iter().find(|m| format!("{m:?}").to_lowercase() == mode)?;
    Some((at, hz, mode))
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The recording's length from its last page's granule, less the pre-skip in its header.
pub fn length_secs(path: &Path) -> Option<f64> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut head = vec![0u8; 512.min(size as usize)];
    f.read_exact(&mut head).ok()?;
    let at = head.windows(8).position(|w| w == b"OpusHead")?;
    let pre = u16::from_le_bytes([*head.get(at + 10)?, *head.get(at + 11)?]) as u64;
    let take = TAIL.min(size as usize);
    f.seek(SeekFrom::Start(size - take as u64)).ok()?;
    let mut tail = vec![0u8; take];
    f.read_exact(&mut tail).ok()?;
    let page = tail.windows(4).rposition(|w| w == b"OggS")?;
    let g = tail.get(page + 6..page + 14)?;
    let granule = u64::from_le_bytes(g.try_into().ok()?);
    Some(granule.saturating_sub(pre) as f64 / AUDIO_RATE as f64)
}

/// Removes a recording; only files inside `dir` with the recorder's name shape.
pub fn delete(dir: &Path, file: &str) -> Result<()> {
    let path = PathBuf::from(file);
    let inside = path.parent() == Some(dir) && path.file_name().and_then(|n| n.to_str()).and_then(parse_name).is_some();
    if !inside {
        return Err(anyhow!("not a recording: {file}"));
    }
    std::fs::remove_file(&path)?;
    Ok(())
}

struct OpusDec {
    _lib: libloading::Library,
    dec: *mut u8,
    decode: unsafe extern "C" fn(*mut u8, *const u8, c_int, *mut f32, c_int, c_int) -> c_int,
    ctl: unsafe extern "C" fn(*mut u8, c_int, ...) -> c_int,
    destroy: unsafe extern "C" fn(*mut u8),
}

// Only the radio thread touches the decoder.
unsafe impl Send for OpusDec {}

impl OpusDec {
    fn open() -> Result<Self> {
        unsafe {
            let lib = ["libopus.so.0", "libopus.so"]
                .iter()
                .find_map(|n| libloading::Library::new(n).ok())
                .ok_or_else(|| anyhow!("libopus not found (pacman -S opus)"))?;
            let create: unsafe extern "C" fn(c_int, c_int, *mut c_int) -> *mut u8 = *lib.get(b"opus_decoder_create\0")?;
            let decode = *lib.get(b"opus_decode_float\0")?;
            let ctl = *lib.get(b"opus_decoder_ctl\0")?;
            let destroy = *lib.get(b"opus_decoder_destroy\0")?;
            let mut err = 0;
            let dec = create(AUDIO_RATE as c_int, 1, &mut err);
            if dec.is_null() || err != 0 {
                return Err(anyhow!("opus decoder: error {err}"));
            }
            Ok(OpusDec { _lib: lib, dec, decode, ctl, destroy })
        }
    }

    fn decode(&mut self, packet: &[u8], out: &mut [f32; MAX_FRAME]) -> Result<usize> {
        let n = unsafe { (self.decode)(self.dec, packet.as_ptr(), packet.len() as c_int, out.as_mut_ptr(), MAX_FRAME as c_int, 0) };
        if n < 0 {
            return Err(anyhow!("opus decode: error {n}"));
        }
        Ok(n as usize)
    }

    fn reset(&mut self) {
        unsafe { (self.ctl)(self.dec, OPUS_RESET_STATE) };
    }
}

impl Drop for OpusDec {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.dec) }
    }
}

/// A recording being played.
pub struct Player {
    path: PathBuf,
    opus: OpusDec,
    reader: PacketReader<BufReader<File>>,
    pre_skip: u64,
    len_samples: u64,
    /// Decoded samples handed out so far, after the pre-skip.
    pos: u64,
    /// Samples still to drop at the start of the stream.
    to_skip: u64,
    /// Decoded audio not yet handed out.
    buf: Vec<f32>,
    buf_at: usize,
    pub paused: bool,
    pub done: bool,
    scratch: Box<[f32; MAX_FRAME]>,
}

impl Player {
    pub fn open(path: &Path) -> Result<Self> {
        let opus = OpusDec::open()?;
        let len = length_secs(path).ok_or_else(|| anyhow!("not an Ogg Opus file: {}", path.display()))?;
        let mut p = Player {
            path: path.to_path_buf(),
            opus,
            reader: PacketReader::new(BufReader::new(File::open(path)?)),
            pre_skip: 0,
            len_samples: (len * AUDIO_RATE as f64).round() as u64,
            pos: 0,
            to_skip: 0,
            buf: Vec::new(),
            buf_at: 0,
            paused: false,
            done: false,
            scratch: Box::new([0.0; MAX_FRAME]),
        };
        p.rewind()?;
        Ok(p)
    }

    /// Back to the first audio packet; reads the header for the pre-skip.
    fn rewind(&mut self) -> Result<()> {
        self.reader = PacketReader::new(BufReader::new(File::open(&self.path)?));
        let head = self.reader.read_packet()?.ok_or_else(|| anyhow!("empty file"))?;
        if head.data.len() < 12 || &head.data[..8] != b"OpusHead" {
            return Err(anyhow!("not an Opus stream"));
        }
        self.pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]) as u64;
        self.reader.read_packet()?; // OpusTags
        self.to_skip = self.pre_skip;
        self.pos = 0;
        self.buf.clear();
        self.buf_at = 0;
        self.done = false;
        self.opus.reset();
        Ok(())
    }

    /// Appends up to `n` samples; fewer at the end, where `done` is set.
    pub fn fill(&mut self, out: &mut Vec<f32>, n: usize) {
        let want = out.len() + n;
        while out.len() < want && !self.done {
            if self.buf_at >= self.buf.len() {
                self.next_packet();
                continue;
            }
            // the last packet runs past the granule: what it holds beyond the length is padding
            let left = self.len_samples.saturating_sub(self.pos) as usize;
            if left == 0 {
                self.done = true;
                break;
            }
            let take = (want - out.len()).min(self.buf.len() - self.buf_at).min(left);
            out.extend_from_slice(&self.buf[self.buf_at..self.buf_at + take]);
            self.buf_at += take;
            self.pos += take as u64;
        }
    }

    /// Decodes the next packet into `buf`, dropping the pre-skip at the start.
    fn next_packet(&mut self) {
        self.buf.clear();
        self.buf_at = 0;
        let Ok(Some(p)) = self.reader.read_packet() else {
            self.done = true;
            return;
        };
        let Ok(n) = self.opus.decode(&p.data, &mut self.scratch) else {
            self.done = true;
            return;
        };
        let skip = (self.to_skip as usize).min(n);
        self.to_skip -= skip as u64;
        self.buf.extend_from_slice(&self.scratch[skip..n]);
    }

    /// Jumps to `secs` from the start: packets before it are skipped unread, `PREROLL` decoded and dropped.
    pub fn seek(&mut self, secs: f64) -> Result<()> {
        let target = ((secs.max(0.0) * AUDIO_RATE as f64) as u64 + self.pre_skip) / FRAME as u64;
        self.rewind()?;
        let first = target.saturating_sub(PREROLL as u64);
        for _ in 0..first {
            if self.reader.read_packet()?.is_none() {
                self.done = true;
                return Ok(());
            }
        }
        for _ in first..target {
            match self.reader.read_packet()? {
                Some(p) => {
                    let _ = self.opus.decode(&p.data, &mut self.scratch);
                }
                None => {
                    self.done = true;
                    return Ok(());
                }
            }
        }
        let skipped = target * FRAME as u64;
        self.to_skip = self.pre_skip.saturating_sub(skipped);
        self.pos = skipped.saturating_sub(self.pre_skip);
        Ok(())
    }

    pub fn info(&self) -> Playing {
        Playing {
            file: self.path.to_string_lossy().into_owned(),
            secs: self.pos as f64 / AUDIO_RATE as f64,
            len_secs: self.len_samples as f64 / AUDIO_RATE as f64,
            paused: self.paused,
            done: self.done,
        }
    }
}

/// Seconds since the Unix epoch now; the list's `at` is on this clock.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Recorder;

    #[test]
    fn reads_the_name_back() {
        let (at, hz, mode) = parse_name("2026-11-01T153012Z 101.100MHz wfm.opus").unwrap();
        assert_eq!((at, hz, mode), (1_793_547_012, 101.1e6, Mode::Wfm));
        assert!(parse_name("notes.txt").is_none());
        assert!(parse_name("2026-11-01T153012Z 101.100MHz.opus").is_none());
    }

    /// A recorded tone lists with its length, plays back as the same tone, and seeks to the right place.
    #[test]
    fn plays_a_tone_back() {
        let dir = std::env::temp_dir().join(format!("squelch-play-{}", std::process::id()));
        let rec = Recorder::start(dir.clone(), 146.7e6, Mode::Nfm).expect("start");
        let secs = 2.5;
        let n = (secs * AUDIO_RATE as f64) as usize;
        let tone: Vec<f32> = (0..n).map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / AUDIO_RATE as f32).sin()).collect();
        rec.push(&tone);
        drop(rec);

        let listed = list(&dir);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert!((listed[0].secs - secs).abs() < 0.001, "{}", listed[0].secs);
        assert_eq!((listed[0].hz, listed[0].mode), (146.7e6, Mode::Nfm));

        let path = PathBuf::from(&listed[0].file);
        let mut p = Player::open(&path).expect("open");
        let mut out = Vec::new();
        while !p.done {
            p.fill(&mut out, 1000);
        }
        assert!((out.len() as f64 / AUDIO_RATE as f64 - secs).abs() < 0.001, "{}", out.len());
        // past the codec's settling, the tone is there at full level
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        assert!((rms(&out[48_000..96_000]) - 0.3536).abs() < 0.02, "{}", rms(&out[48_000..96_000]));

        p.seek(1.0).expect("seek");
        assert!((p.info().secs - 1.0).abs() < 0.021, "{}", p.info().secs);
        let mut rest = Vec::new();
        while !p.done {
            p.fill(&mut rest, 1000);
        }
        assert!((rest.len() as f64 / AUDIO_RATE as f64 - 1.5).abs() < 0.021, "{}", rest.len());
        assert!((rms(&rest[4800..52_800]) - 0.3536).abs() < 0.02);

        delete(&dir, &listed[0].file).expect("delete");
        assert!(list(&dir).is_empty());
        assert!(delete(&dir, "/etc/passwd").is_err());
    }
}
