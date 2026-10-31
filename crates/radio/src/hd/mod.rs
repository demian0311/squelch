//! HD Radio (NRSC-5): digital audio, station data and images from the sidebands of an FM station.
//! The tuned station is cut from the capture, resampled for libnrsc5 and decoded on its own thread.

mod ffi;

use crate::demod::AUDIO_RATE;
use crate::dsp::{lowpass, Decimator, Mixer, C32};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

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

fn base64(data: &[u8]) -> String {
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

/// Whether the averaged spectrum `row` shows digital sidebands around `station_hz`.
pub fn sidebands(row: &[f32], floor: f32, center: f64, station_hz: f64, rate: f64) -> bool {
    let n = row.len();
    let bin_hz = rate / n as f64;
    let bin = |f: f64| ((f - center) / bin_hz + n as f64 / 2.0).round();
    let side = |sign: f64| {
        let (a, b) = (bin(station_hz + sign * SIDEBAND_HZ.0), bin(station_hz + sign * SIDEBAND_HZ.1));
        let (lo, hi) = (a.min(b), a.max(b));
        if lo < 0.0 || hi >= n as f64 {
            return false;
        }
        let p: f32 = row[lo as usize..=hi as usize].iter().map(|db| 10f32.powf(db / 10.0)).sum::<f32>() / (hi - lo + 1.0) as f32;
        10.0 * p.log10() - floor >= SIDEBAND_DB
    };
    side(-1.0) && side(1.0)
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
    tx: Option<SyncSender<Vec<C32>>>,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
    /// Revision of the images last handed out.
    images_rev: Option<u32>,
}

impl Hd {
    /// `offset_hz` is the station relative to the capture centre.
    pub fn new(offset_hz: f64, in_rate: u32, program: u32) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        shared.lock().unwrap().info.program = program;
        let s = shared.clone();
        let (tx, rx) = sync_channel::<Vec<C32>>(BACKLOG);
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
                let fs = in_rate as f32;
                let d = (in_rate / 800_000).max(1) as usize;
                let mid = in_rate as f64 / d as f64;
                let mut mixer = Mixer::new(offset_hz, in_rate as f64);
                // pass ±200 kHz, stop by mid/2 so the resampler sees a clean band
                let edge = mid as f32 / 2.0;
                let mut dec = Decimator::new(lowpass((200_000.0 + edge) / 2.0 / fs, (edge - 200_000.0) / fs), d);
                let mut rs = Resampler::new(mid, ffi::SAMPLE_RATE);
                let (mut a, mut b, mut flat) = (Vec::new(), Vec::new(), Vec::new());
                for mut block in rx {
                    mixer.process(&mut block);
                    a.clear();
                    dec.process(&block, &mut a);
                    b.clear();
                    rs.process(&a, &mut b);
                    flat.clear();
                    flat.extend(b.iter().flat_map(|c| [c.re, c.im]));
                    session.push(&flat);
                }
            })
            .expect("spawn hd thread");
        Hd { tx: Some(tx), shared, thread: Some(thread), images_rev: None }
    }

    /// Hand a block of the capture to the decoder.
    pub fn feed(&self, iq: &[C32]) {
        if let Some(tx) = &self.tx {
            match tx.try_send(iq.to_vec()) {
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

    /// Replace `pcm` with digital audio when enough is queued. Returns whether it did.
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
        for (o, a) in pcm.iter_mut().zip(s.audio.drain(..n)) {
            *o = a;
        }
        true
    }

    pub fn info(&self) -> HdInfo {
        self.shared.lock().unwrap().info.clone()
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
        LostSync => {
            i.synced = false;
            i.mer_db = None;
        }
        Mer(lo, hi) => i.mer_db = Some(lo.min(hi)),
        Ber(b) => i.ber = Some(b),
        Audio { program: p, pcm } if p == program => {
            if let Some(pcm) = pcm {
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
        Lot { lot, name, logo, data } => {
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
        assert!(!sidebands(&row, -60.0, 0.0, -station, rate));
    }
}
