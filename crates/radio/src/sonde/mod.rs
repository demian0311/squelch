//! Radiosonde (weather balloon) decoders.
//!
//! The scanner gives each narrow signal in the capture its own channel and runs every
//! decoder on it; whichever finds frames names the sonde.

pub mod channel;
pub mod dfm;
pub mod fsk;
pub mod geo;
pub mod rs;
pub mod rs41;

#[cfg(test)]
pub mod testutil {
    /// Frequency samples for `bits`, with a carrier offset, a slightly wrong baud and noise.
    pub fn fsk_signal(bits: &[bool], rate: u32, baud: f32, dev: f32, offset: f32, noise: f32) -> Vec<f32> {
        let sps = rate as f32 / baud;
        let mut seed = 12345u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        let total = (bits.len() as f32 * sps) as usize;
        (0..total).map(|i| offset + if bits[(i as f32 / sps) as usize] { dev } else { -dev } + noise * rnd()).collect()
    }

}

use crate::detect::Detected;
use crate::dsp::C32;
use channel::Channel;
use fsk::Fsk;
use serde::Serialize;
use std::time::{Duration, Instant};

/// Every decoder this build knows, fresh.
pub fn all_decoders() -> Vec<Box<dyn FrameDecoder>> {
    vec![Box::new(dfm::Dfm::new()), Box::new(rs41::Rs41::new())]
}

/// Channels decoded at once; each costs a mixer and filter at the full capture rate.
pub const MAX_CHANNELS: usize = 4;
/// Sondes are narrow; wider signals are something else.
const MAX_WIDTH_HZ: f64 = 40_000.0;
const CHANNEL_BW_HZ: f32 = 20_000.0;
/// A detection this close to an existing channel is the same transmitter.
const SAME_HZ: f64 = 10_000.0;
/// Channels with no detection for this long are dropped.
const CHANNEL_IDLE: Duration = Duration::from_secs(15);
/// A channel that decodes nothing in this long is not a sonde (or too weak); try others.
const TRY_FOR: Duration = Duration::from_secs(20);
/// ...and leave that frequency alone for this long.
const RETRY_AFTER: Duration = Duration::from_secs(120);
/// Sondes stay listed this long after their last frame.
const SONDE_KEEP: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub speed_ms: f32,
    pub heading_deg: f32,
    pub climb_ms: f32,
}

/// What one decoded frame says. Fields a frame doesn't carry are `None`.
#[derive(Clone, Debug, Default)]
pub struct Update {
    pub serial: Option<String>,
    /// Exact model, when the frames say more than the decoder's family name.
    pub model: Option<String>,
    pub frame: Option<u32>,
    /// UTC time of day from GPS, "hh:mm:ss".
    pub time: Option<String>,
    pub fix: Option<Fix>,
}

pub trait FrameDecoder: Send {
    fn kind(&self) -> &'static str;
    fn baud(&self) -> f32;
    /// Feed soft symbols (positive = higher tone); decoded frames are appended to `out`.
    fn push(&mut self, soft: &[f32], out: &mut Vec<Update>);
}

#[derive(Clone, Debug, Serialize)]
pub struct Sonde {
    pub kind: String,
    /// Empty until a frame carrying the serial number arrives.
    pub serial: String,
    pub hz: f64,
    pub frame: Option<u32>,
    pub time: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_m: Option<f64>,
    pub speed_ms: Option<f32>,
    pub heading_deg: Option<f32>,
    pub climb_ms: Option<f32>,
    pub frames: u32,
    /// Seconds since the last decoded frame.
    pub age_s: f32,
    #[serde(skip)]
    last: Instant,
}

struct Slot {
    hz: f64,
    chan: Channel,
    decoders: Vec<(Fsk, Box<dyn FrameDecoder>)>,
    seen: Instant,
    opened: Instant,
    sonde: Option<usize>,
}

pub struct Scanner {
    rate: u32,
    center: f64,
    make: fn() -> Vec<Box<dyn FrameDecoder>>,
    slots: Vec<Slot>,
    /// Frequencies that decoded nothing, and when they were given up on.
    tried: Vec<(f64, Instant)>,
    sondes: Vec<Sonde>,
    freq: Vec<f32>,
    soft: Vec<f32>,
    updates: Vec<Update>,
}

impl Scanner {
    pub fn new(rate: u32, make: fn() -> Vec<Box<dyn FrameDecoder>>) -> Self {
        Scanner { rate, center: 0.0, make, slots: Vec::new(), tried: Vec::new(), sondes: Vec::new(), freq: Vec::new(), soft: Vec::new(), updates: Vec::new() }
    }

    /// Follow the latest detections: open channels on new narrow signals, close idle ones.
    pub fn retarget(&mut self, center: f64, signals: &[Detected]) {
        if (center - self.center).abs() > 1.0 {
            self.center = center;
            self.slots.clear();
            self.tried.clear();
        }
        let now = Instant::now();
        let half = self.rate as f64 * 0.45;
        self.tried.retain(|(_, t)| now - *t < RETRY_AFTER);
        for t in self.slots.iter().filter(|t| t.sonde.is_none() && now - t.opened >= TRY_FOR) {
            self.tried.push((t.hz, now));
        }
        self.slots.retain(|t| t.sonde.is_some() || now - t.opened < TRY_FOR);
        let mut candidates: Vec<&Detected> = signals
            .iter()
            .filter(|s| s.width_hz <= MAX_WIDTH_HZ && (s.hz - center).abs() < half)
            .filter(|s| !self.tried.iter().any(|(hz, _)| (hz - s.hz).abs() < SAME_HZ))
            .collect();
        // strongest first, so a faint spur doesn't take a channel from a real sonde
        candidates.sort_by(|a, b| b.snr_db.total_cmp(&a.snr_db));
        for s in candidates {
            if let Some(slot) = self.slots.iter_mut().find(|t| (t.hz - s.hz).abs() < SAME_HZ) {
                slot.seen = now;
            } else if self.slots.len() < MAX_CHANNELS {
                let decoders = (self.make)().into_iter().map(|d| (Fsk::new(channel::RATE, d.baud()), d)).collect();
                self.slots.push(Slot { hz: s.hz, chan: Channel::new(s.hz - center, self.rate, CHANNEL_BW_HZ), decoders, seen: now, opened: now, sonde: None });
            }
        }
        // a channel that is decoding stays open through a fade
        self.slots.retain(|t| now - t.seen < CHANNEL_IDLE || t.sonde.is_some_and(|i| now - self.sondes[i].last < CHANNEL_IDLE));
        self.sondes.retain(|s| now - s.last < SONDE_KEEP);
        // indexes into `sondes` may have shifted; re-find them by frequency
        for t in &mut self.slots {
            t.sonde = self.sondes.iter().position(|s| (s.hz - t.hz).abs() < SAME_HZ);
        }
    }

    pub fn process(&mut self, iq: &[C32]) {
        for t in &mut self.slots {
            self.freq.clear();
            t.chan.process(iq, &mut self.freq);
            for (fsk, dec) in &mut t.decoders {
                self.soft.clear();
                fsk.process(&self.freq, &mut self.soft);
                self.updates.clear();
                dec.push(&self.soft, &mut self.updates);
                for u in self.updates.drain(..) {
                    let i = match t.sonde {
                        Some(i) => i,
                        None => {
                            self.sondes.push(Sonde::new(dec.kind(), t.hz));
                            t.sonde = Some(self.sondes.len() - 1);
                            self.sondes.len() - 1
                        }
                    };
                    self.sondes[i].apply(dec.kind(), u);
                }
            }
        }
    }

    pub fn sondes(&self) -> Vec<Sonde> {
        let now = Instant::now();
        self.sondes.iter().map(|s| Sonde { age_s: (now - s.last).as_secs_f32(), ..s.clone() }).collect()
    }

    /// Frequencies being decoded right now.
    pub fn channels(&self) -> Vec<f64> {
        self.slots.iter().map(|t| t.hz).collect()
    }
}

impl Sonde {
    fn new(kind: &str, hz: f64) -> Self {
        Sonde {
            kind: kind.to_string(),
            serial: String::new(),
            hz,
            frame: None,
            time: None,
            lat: None,
            lon: None,
            alt_m: None,
            speed_ms: None,
            heading_deg: None,
            climb_ms: None,
            frames: 0,
            age_s: 0.0,
            last: Instant::now(),
        }
    }

    fn apply(&mut self, kind: &str, u: Update) {
        if !self.kind.starts_with(kind) {
            self.kind = kind.to_string();
        }
        self.frames += 1;
        self.last = Instant::now();
        if let Some(m) = u.model {
            self.kind = m;
        }
        if let Some(s) = u.serial {
            self.serial = s;
        }
        if u.frame.is_some() {
            self.frame = u.frame;
        }
        if u.time.is_some() {
            self.time = u.time;
        }
        if let Some(f) = u.fix {
            self.lat = Some(f.lat);
            self.lon = Some(f.lon);
            self.alt_m = Some(f.alt_m);
            self.speed_ms = Some(f.speed_ms);
            self.heading_deg = Some(f.heading_deg);
            self.climb_ms = Some(f.climb_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_finds_and_decodes_a_sonde_in_the_capture() {
        let rate = 2_400_000u32;
        let sym = dfm::tests::air();
        let sps = rate as f64 / dfm::BAUD as f64;
        let mut phase = 0.0f64;
        let iq: Vec<C32> = (0..(sym.len() as f64 * sps) as usize)
            .map(|i| {
                let dev = if sym[(i as f64 / sps) as usize] { 3_000.0 } else { -3_000.0 };
                phase += std::f64::consts::TAU * (300_000.0 + dev) / rate as f64;
                C32::new(phase.cos() as f32, phase.sin() as f32) * 0.3
            })
            .collect();
        let mut sc = Scanner::new(rate, all_decoders);
        sc.retarget(403_000_000.0, &[Detected { hz: 403_300_000.0, width_hz: 12_000.0, snr_db: 25.0 }]);
        for chunk in iq.chunks(65_536) {
            sc.process(chunk);
        }
        let s = sc.sondes();
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!((s[0].kind.as_str(), s[0].serial.as_str()), ("DFM-17", "23038743"));
        assert!((s[0].lat.unwrap() - 39.7392).abs() < 1e-6);
    }
}
