//! HD Radio (NRSC-5): digital audio, station data and images from the sidebands of an FM station.
//! The tuned station is cut from the capture, resampled for libnrsc5 and decoded on its own thread.

pub mod ffi;

use crate::demod::AUDIO_RATE;
use crate::detect::Detected;
use crate::dsp::{lowpass, Decimator, Mixer, C32};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

/// Digital audio replaces the analog once this much is queued, so bursts from the decoder play smoothly.
const PRIME_SECS: f32 = 2.0;
/// Queued digital audio beyond this is dropped, so it can't lag further and further behind.
const MAX_QUEUE_SECS: f32 = 6.0;
/// Capture blocks waiting for the decoder thread; more are dropped rather than slowing the receiver.
const BACKLOG: usize = 8;
/// Images kept per station, newest first.
const MAX_LOTS: usize = 64;
/// The digital blocks sit 129–198 kHz either side of the carrier.
const SIDEBAND_HZ: (f64, f64) = (129_000.0, 198_000.0);
/// Each block must average this far above the floor (dB). Measured 2026-10-31 at eagle:
/// HD stations 5.7–8.9 dB, analog-only at most 2.9 dB.
const SIDEBAND_DB: f32 = 4.0;
/// How far from an FM channel a signal may sit and still be a station. A sideband centre sits 36.5 kHz off.
const GRID_SLACK_HZ: f64 = 20_000.0;
/// A picture that has had no fragment for this long has stopped arriving.
const PENDING_SECS: f32 = 30.0;

/// What the tuned station's HD signal has given so far.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct HdInfo {
    /// Set when decoding can't run at all, e.g. libnrsc5 is missing.
    pub error: Option<String>,
    /// 0 = HD1, 1 = HD2, …
    pub program: u32,
    pub programs: Vec<u32>,
    pub synced: bool,
    /// True while the speaker plays the digital audio rather than the analog.
    pub digital: bool,
    pub name: Option<String>,
    pub slogan: Option<String>,
    pub message: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Modulation error ratio, dB, the weaker sideband.
    pub mer_db: Option<f32>,
    pub ber: Option<f32>,
    /// Bumped whenever the image set may have changed.
    pub images_rev: u32,
    /// How far along the furthest logo or album art still arriving is, 0–1.
    pub incoming: Option<f32>,
}

/// A picture for the card. `kind`: logo, art, traffic (a tile of a 3×3 map) or weather.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HdImage {
    pub kind: &'static str,
    pub name: String,
    /// Traffic tiles only, from 1.
    pub row: u32,
    pub col: u32,
    pub data_url: String,
}

struct Lot {
    lot: u32,
    name: String,
    logo: bool,
    data: Vec<u8>,
}

#[derive(Default)]
struct Shared {
    info: HdInfo,
    audio: VecDeque<f32>,
    lots: VecDeque<Lot>,
    /// Album art LOT per program, from ID3.
    art: BTreeMap<u32, u32>,
    here: BTreeMap<(bool, u32), Lot>,
    /// Whether digital audio replaces analog; off while decoding in the background.
    play: bool,
    /// Pictures still arriving, by LOT: (size, bytes so far, last fragment).
    pending: BTreeMap<u32, (u32, u32, Instant)>,
}

impl Shared {
    fn images(&self) -> Vec<HdImage> {
        let mut out = Vec::new();
        let mut push = |kind, row, col, l: &Lot| {
            if let Some(url) = data_url(&l.data) {
                out.push(HdImage { kind, name: l.name.clone(), row, col, data_url: url });
            }
        };
        if let Some(l) = self.lots.iter().find(|l| l.logo) {
            push("logo", 0, 0, l);
        }
        if let Some(l) = self.art.get(&self.info.program).and_then(|id| self.lots.iter().find(|l| l.lot == *id)) {
            push("art", 0, 0, l);
        }
        // traffic tiles (NavTeq over LOT): TMT_<area>_<row>_<col>_<date>_<time>…; keep the newest of each
        let mut tiles: BTreeMap<(u32, u32), &Lot> = BTreeMap::new();
        for l in &self.lots {
            if let Some(rc) = traffic_tile(&l.name) {
                tiles.entry(rc).or_insert(l);
            }
        }
        for ((r, c), l) in tiles {
            push("traffic", r, c, l);
        }
        // weather radar overlay: DWRO_…
        if let Some(l) = self.lots.iter().find(|l| l.name.starts_with("DWRO")) {
            push("weather", 0, 0, l);
        }
        for ((traffic, part), l) in &self.here {
            if *traffic {
                let p = part.max(&1) - 1;
                push("traffic", p / 3 + 1, p % 3 + 1, l);
            } else {
                push("weather", 0, 0, l);
            }
        }
        out
    }
}

fn traffic_tile(name: &str) -> Option<(u32, u32)> {
    let p: Vec<&str> = name.split('_').collect();
    if p.len() < 4 || p[0] != "TMT" {
        return None;
    }
    Some((p[2].parse().ok()?, p[3].parse().ok()?))
}

pub(crate) fn data_url(data: &[u8]) -> Option<String> {
    let mime = if data.starts_with(b"\x89PNG") {
        "image/png"
    } else if data.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{}", base64(data)))
}

pub(crate) fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// Drops the digital sidebands of HD stations from `signals`, so a station reads as one signal.
/// A sideband sits `SIDEBAND_HZ` off a stronger signal and, unlike a neighbouring station, off
/// the US FM channel grid (odd tenths of a MHz).
pub fn drop_sidebands(signals: &mut Vec<Detected>) {
    let off_grid = |hz: f64| ((hz - 100_000.0).rem_euclid(200_000.0) - 100_000.0).abs() < 100_000.0 - GRID_SLACK_HZ;
    let all = signals.clone();
    signals.retain(|s| {
        !(off_grid(s.hz)
            && all.iter().any(|t| {
                let d = (t.hz - s.hz).abs();
                t.snr_db > s.snr_db && d >= SIDEBAND_HZ.0 - GRID_SLACK_HZ && d <= SIDEBAND_HZ.1 + GRID_SLACK_HZ
            }))
    });
}

/// Whether the averaged spectrum `row` shows digital sidebands around `station_hz`.
pub fn sidebands(row: &[f32], floor: f32, center: f64, station_hz: f64, rate: f64) -> bool {
    sidebands_seen(row, floor, center, station_hz, rate) == Some(true)
}

/// Marks each signal in `signals` with whether it shows sidebands, on the FM channel it sits on.
pub fn mark_sidebands(signals: &mut [Detected], row: &[f32], floor: f32, center: f64, rate: f64) {
    for s in signals {
        s.hd = sidebands_seen(row, floor, center, (s.hz / 100_000.0).round() * 100_000.0, rate);
    }
}

/// Like `sidebands`, but `None` when one side falls outside the capture.
fn sidebands_seen(row: &[f32], floor: f32, center: f64, station_hz: f64, rate: f64) -> Option<bool> {
    let n = row.len();
    let bin_hz = rate / n as f64;
    let bin = |f: f64| ((f - center) / bin_hz + n as f64 / 2.0).round();
    let side = |sign: f64| {
        let (a, b) = (bin(station_hz + sign * SIDEBAND_HZ.0), bin(station_hz + sign * SIDEBAND_HZ.1));
        let (lo, hi) = (a.min(b), a.max(b));
        if lo < 0.0 || hi >= n as f64 {
            return None;
        }
        let p: f32 = row[lo as usize..=hi as usize].iter().map(|db| 10f32.powf(db / 10.0)).sum::<f32>() / (hi - lo + 1.0) as f32;
        Some(10.0 * p.log10() - floor >= SIDEBAND_DB)
    };
    // both, so a quiet side doesn't answer for one off the capture
    let (low, high) = (side(-1.0)?, side(1.0)?);
    Some(low && high)
}

/// Fractional resampler, 4-point cubic (Catmull-Rom). Good enough when the signal fills at most half the band.
struct Resampler {
    step: f64,
    pos: f64,
    hist: [C32; 4],
}

impl Resampler {
    fn new(in_rate: f64, out_rate: f64) -> Self {
        Resampler { step: in_rate / out_rate, pos: 0.0, hist: [C32::default(); 4] }
    }

    fn process(&mut self, input: &[C32], out: &mut Vec<C32>) {
        for &x in input {
            self.hist = [self.hist[1], self.hist[2], self.hist[3], x];
            // output points fall between hist[1] and hist[2]
            while self.pos < 1.0 {
                let t = self.pos as f32;
                let [p0, p1, p2, p3] = self.hist;
                let a = p1 * 1.5 - p2 * 1.5 + (p3 - p0) * 0.5;
                let b = p0 - p1 * 2.5 + p2 * 2.0 - p3 * 0.5;
                let c = (p2 - p0) * 0.5;
                out.push(((a * t + b) * t + c) * t + p1);
                self.pos += self.step;
            }
            self.pos -= 1.0;
        }
    }
}

/// Capture → the decoder's input: the station mixed to 0 Hz, cut to ±200 kHz and brought to `ffi::SAMPLE_RATE`.
pub struct FrontEnd {
    mixer: Mixer,
    dec: Decimator<C32>,
    rs: Resampler,
    a: Vec<C32>,
    b: Vec<C32>,
}

impl FrontEnd {
    /// `offset_hz` is the station relative to the capture centre.
    pub fn new(offset_hz: f64, in_rate: u32) -> Self {
        let fs = in_rate as f32;
        let d = (in_rate / 800_000).max(1) as usize;
        let mid = in_rate as f64 / d as f64;
        // pass ±200 kHz, stop by mid/2 so the resampler sees a clean band
        let edge = mid as f32 / 2.0;
        FrontEnd {
            mixer: Mixer::new(offset_hz, in_rate as f64),
            dec: Decimator::new(lowpass((200_000.0 + edge) / 2.0 / fs, (edge - 200_000.0) / fs), d),
            rs: Resampler::new(mid, ffi::SAMPLE_RATE),
            a: Vec::new(),
            b: Vec::new(),
        }
    }

    /// The capture centre moved; the station stays the same, so what is decoded does not restart.
    pub fn set_offset(&mut self, offset_hz: f64, in_rate: u32) {
        self.mixer.set(offset_hz, in_rate as f64);
    }

    /// `flat` receives interleaved I/Q at `ffi::SAMPLE_RATE`; `block` is mixed in place.
    pub fn process(&mut self, block: &mut [C32], flat: &mut Vec<f32>) {
        self.mixer.process(block);
        self.a.clear();
        self.dec.process(block, &mut self.a);
        self.b.clear();
        self.rs.process(&self.a, &mut self.b);
        flat.clear();
        flat.extend(self.b.iter().flat_map(|c| [c.re, c.im]));
    }
}

/// 44.1 kHz interleaved stereo i16 → 48 kHz mono f32, by linear interpolation.
struct AudioRate {
    pos: f64,
    prev: f32,
}

impl AudioRate {
    fn process(&mut self, pcm: &[i16], out: &mut VecDeque<f32>) {
        let step = ffi::AUDIO_RATE as f64 / AUDIO_RATE as f64;
        for lr in pcm.chunks_exact(2) {
            let x = (lr[0] as f32 + lr[1] as f32) / 65_536.0;
            while self.pos < 1.0 {
                out.push_back(self.prev + (x - self.prev) * self.pos as f32);
                self.pos += step;
            }
            self.pos -= 1.0;
            self.prev = x;
        }
    }
}

/// Decodes one station. Dropping it stops the decoder thread.
pub struct Hd {
    /// Each block travels with the station's offset when it was taken, since the queue can outlast a retune.
    tx: Option<SyncSender<(f64, Vec<C32>)>>,
    offset_hz: f64,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
    /// Revision of the images last handed out.
    images_rev: Option<u32>,
}

impl Hd {
    /// `offset_hz` is the station relative to the capture centre. `play` = false decodes
    /// for the panel only and leaves the analog audio playing.
    pub fn new(offset_hz: f64, in_rate: u32, program: u32, play: bool) -> Self {
        let shared = Arc::new(Mutex::new(Shared { play, ..Shared::default() }));
        shared.lock().unwrap().info.program = program;
        let s = shared.clone();
        let (tx, rx) = sync_channel::<(f64, Vec<C32>)>(BACKLOG);
        let thread = std::thread::Builder::new()
            .name("hd".into())
            .spawn(move || {
                let mut audio = AudioRate { pos: 0.0, prev: 0.0 };
                let sink = s.clone();
                let mut session = match ffi::Session::open(move |e| handle(&sink, &mut audio, e)) {
                    Ok(x) => x,
                    Err(e) => {
                        s.lock().unwrap().info.error = Some(e);
                        return;
                    }
                };
                let mut front = FrontEnd::new(offset_hz, in_rate);
                let mut flat = Vec::new();
                let mut at = offset_hz;
                for (offset, mut block) in rx {
                    if offset != at {
                        at = offset;
                        front.set_offset(at, in_rate);
                    }
                    front.process(&mut block, &mut flat);
                    session.push(&flat);
                }
            })
            .expect("spawn hd thread");
        Hd { tx: Some(tx), offset_hz, shared, thread: Some(thread), images_rev: None }
    }

    /// The capture centre moved while the station stayed tuned.
    pub fn set_offset(&mut self, offset_hz: f64) {
        self.offset_hz = offset_hz;
    }

    /// Hand a block of the capture to the decoder.
    pub fn feed(&self, iq: &[C32]) {
        if let Some(tx) = &self.tx {
            match tx.try_send((self.offset_hz, iq.to_vec())) {
                Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
            }
        }
    }

    pub fn set_program(&self, program: u32) {
        let mut s = self.shared.lock().unwrap();
        if s.info.program != program {
            s.info.program = program;
            s.info.title = None;
            s.info.artist = None;
            s.info.album = None;
            s.info.digital = false;
            s.info.images_rev += 1;
            s.audio.clear();
        }
    }

    pub fn set_play(&self, play: bool) {
        self.shared.lock().unwrap().play = play;
    }

    /// Replace `pcm` with digital audio when enough is queued and playing. Returns whether it did.
    pub fn fill(&self, pcm: &mut [f32]) -> bool {
        let mut s = self.shared.lock().unwrap();
        let queued = s.audio.len();
        let max = (MAX_QUEUE_SECS * AUDIO_RATE as f32) as usize;
        if queued > max {
            s.audio.drain(..queued - max);
        }
        if !s.info.digital && s.audio.len() as f32 >= PRIME_SECS * AUDIO_RATE as f32 {
            s.info.digital = true;
        }
        if s.info.digital && s.audio.len() < pcm.len() {
            s.info.digital = false;
        }
        if !s.info.digital {
            return false;
        }
        let n = pcm.len();
        if !s.play {
            // keep the queue moving so `digital` still says whether audio would be there
            s.audio.drain(..n);
            return false;
        }
        for (o, a) in pcm.iter_mut().zip(s.audio.drain(..n)) {
            *o = a;
        }
        true
    }

    pub fn info(&self) -> HdInfo {
        let mut s = self.shared.lock().unwrap();
        s.pending.retain(|_, p| p.2.elapsed().as_secs_f32() < PENDING_SECS);
        let incoming = s.pending.values().filter(|p| p.0 > 0 && p.1 > 0).map(|p| (p.1 as f32 / p.0 as f32).min(1.0)).fold(None, |m: Option<f32>, f| Some(m.map_or(f, |m| m.max(f))));
        HdInfo { incoming, ..s.info.clone() }
    }

    /// The images, when they changed since the last call.
    pub fn new_images(&mut self) -> Option<Vec<HdImage>> {
        let s = self.shared.lock().unwrap();
        if self.images_rev == Some(s.info.images_rev) {
            return None;
        }
        self.images_rev = Some(s.info.images_rev);
        Some(s.images())
    }
}

impl Drop for Hd {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn handle(shared: &Mutex<Shared>, audio: &mut AudioRate, e: ffi::Event) {
    use ffi::Event::*;
    let mut guard = shared.lock().unwrap();
    let s = &mut *guard;
    let program = s.info.program;
    let i = &mut s.info;
    match e {
        Sync => i.synced = true,
        // only a locked decoder measures or plays, so these also mean locked
        Mer(lo, hi) => {
            i.synced = true;
            i.mer_db = Some(lo.min(hi));
        }
        LostSync => {
            i.synced = false;
            i.mer_db = None;
        }
        Ber(b) => i.ber = Some(b),
        Audio { program: p, pcm } if p == program => {
            if let Some(pcm) = pcm {
                s.info.synced = true;
                audio.process(pcm, &mut s.audio);
            }
        }
        Audio { .. } => {}
        Id3 { program: p, title, artist, album, art_lot } => {
            if let Some(lot) = art_lot {
                if s.art.insert(p, lot) != Some(lot) {
                    s.info.images_rev += 1;
                }
            }
            if p == program {
                let i = &mut s.info;
                i.title = title.or(i.title.take());
                i.artist = artist.or(i.artist.take());
                i.album = album.or(i.album.take());
            }
        }
        LotHeader { lot, size } => {
            s.pending.entry(lot).or_insert((0, 0, Instant::now())).0 = size;
        }
        LotFragment { lot, got } => {
            let p = s.pending.entry(lot).or_insert((0, 0, Instant::now()));
            p.1 = got;
            p.2 = Instant::now();
        }
        Lot { lot, name, logo, data } => {
            s.pending.remove(&lot);
            if data_url(data).is_some() {
                s.lots.retain(|l| l.lot != lot);
                s.lots.push_front(self::Lot { lot, name, logo, data: data.to_vec() });
                s.lots.truncate(MAX_LOTS);
                s.info.images_rev += 1;
            }
        }
        Program(p) => {
            if !i.programs.contains(&p) {
                i.programs.push(p);
                i.programs.sort();
            }
        }
        Name(n) => i.name = Some(n),
        Slogan(n) => i.slogan = Some(n),
        Message(n) => i.message = Some(n),
        HereImage { traffic, part, name, data } => {
            s.here.insert((traffic, part), self::Lot { lot: 0, name, logo: false, data: data.to_vec() });
            s.info.images_rev += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebands_fold_into_their_station() {
        let sig = |mhz: f64, snr_db: f32| Detected { hz: mhz * 1e6, width_hz: 70_000.0, snr_db, hd: None };
        // 106.7 with sidebands, a real neighbour at 106.9, and a lone weak signal off-grid
        let mut v = vec![sig(106.537, 12.0), sig(106.7, 30.0), sig(106.863, 12.0), sig(106.9, 10.0), sig(105.45, 8.0)];
        drop_sidebands(&mut v);
        let left: Vec<f64> = v.iter().map(|s| (s.hz / 1e3).round() / 1e3).collect();
        assert_eq!(left, vec![106.7, 106.9, 105.45]);
    }

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn traffic_tile_names() {
        assert_eq!(traffic_tile("TMT_02q4cp_3_2_20261031_1304_010d.png"), Some((3, 2)));
        assert_eq!(traffic_tile("DWRO_02q4cp_20261031.png"), None);
    }

    /// Mean frequency of interleaved I/Q, skipping the filter's settling at the start.
    fn mean_hz(flat: &[f32]) -> f64 {
        let iq: Vec<C32> = flat.chunks_exact(2).map(|c| C32::new(c[0], c[1])).collect();
        let late = &iq[iq.len() / 2..];
        let turn: C32 = late.windows(2).map(|w| w[1] * w[0].conj()).sum();
        (turn.im as f64).atan2(turn.re as f64) * ffi::SAMPLE_RATE / std::f64::consts::TAU
    }

    #[test]
    fn a_moved_capture_keeps_the_station_where_it_was() {
        let rate = 2_400_000u32;
        let tone = |hz: f64, start: usize, n: usize| -> Vec<C32> {
            (start..start + n).map(|i| C32::from_polar(1.0, (std::f64::consts::TAU * hz * i as f64 / rate as f64) as f32)).collect()
        };
        // a tone 50 kHz above the station, with the station 300 kHz off the centre, then 100 kHz off it
        let mut front = FrontEnd::new(300_000.0, rate);
        let mut flat = Vec::new();
        front.process(&mut tone(350_000.0, 0, 240_000), &mut flat);
        let before = mean_hz(&flat);
        front.set_offset(100_000.0, rate);
        front.process(&mut tone(150_000.0, 240_000, 240_000), &mut flat);
        let after = mean_hz(&flat);
        assert!((before - 50_000.0).abs() < 500.0, "{before} Hz before the move");
        assert!((after - 50_000.0).abs() < 500.0, "{after} Hz after the move");
    }

    #[test]
    fn resampler_keeps_a_tone() {
        let (fin, fout, f) = (800_000.0, ffi::SAMPLE_RATE, 150_000.0);
        let input: Vec<C32> = (0..80_000).map(|n| C32::from_polar(1.0, (std::f64::consts::TAU * f * n as f64 / fin) as f32)).collect();
        let mut out = Vec::new();
        Resampler::new(fin, fout).process(&input, &mut out);
        assert!((out.len() as f64 - 80_000.0 * fout / fin).abs() < 3.0);
        // compare with the ideal tone at the output rate, skipping the start-up samples
        let lag = 2.0 / fin; // the interpolator runs two input samples behind
        let worst = out[10..]
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let t = (k + 10) as f64 / fout - lag;
                (s - C32::from_polar(1.0, (std::f64::consts::TAU * f * t) as f32)).norm()
            })
            .fold(0f32, f32::max);
        assert!(worst < 0.05, "worst error {worst}");
    }

    #[test]
    fn audio_rate_44k1_to_48k() {
        let mut out = VecDeque::new();
        AudioRate { pos: 0.0, prev: 0.0 }.process(&vec![16_384i16; 44_100 * 2], &mut out);
        assert!((out.len() as i64 - 48_000).abs() <= 1);
        assert!((out[100] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn finds_sidebands() {
        let n = 8192;
        let rate = 2_400_000.0;
        let bin_hz = rate / n as f64;
        let mut row = vec![-60.0f32; n];
        let station = 300_000.0;
        for (i, v) in row.iter_mut().enumerate() {
            let off = ((i as f64 - n as f64 / 2.0) * bin_hz - station).abs();
            if off < 100_000.0 {
                *v = -35.0;
            } else if (130_000.0..197_000.0).contains(&off) {
                *v = -52.0;
            }
        }
        assert!(sidebands(&row, -60.0, 0.0, station, rate));
        // one side off the capture's edge: unknown, not absent
        assert_eq!(sidebands_seen(&row, -60.0, 0.0, rate / 2.0 - 100_000.0, rate), None);
        assert!(!sidebands(&row, -60.0, 0.0, -station, rate));
    }
}
