//! ADS-B: aircraft broadcasting identity, position and velocity as Mode S extended squitters on 1090 MHz.
//!
//! `phy` finds replies in the whole capture's magnitude, `msg` reads them, `cpr` turns position
//! reports into latitude and longitude, and `Adsb` keeps a table of aircraft by 24-bit address.
//! Time is the sample clock, so a recording decodes the same as live.

pub mod cpr;
pub mod msg;
pub mod phy;

use crate::dsp::C32;
use msg::Msg;
use phy::{Demod, Frame};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

/// Aircraft stay listed this long after their last message, seconds.
const KEEP_SECS: f64 = 60.0;
/// An even and an odd position report this close together pair into a position.
const PAIR_SECS: f64 = 10.0;
/// A position younger than this is the reference for decoding a single report.
const LOCAL_SECS: f64 = 60.0;
/// A new position further from the last than this speed allows (plus `JUMP_KM`) is a bad decode.
const MAX_KM_PER_SEC: f64 = 0.4;
const JUMP_KM: f64 = 5.0;
/// Frames per second are counted over this long.
const RATE_SECS: f64 = 5.0;

/// One aircraft, as sent to the UI.
#[derive(Clone, Debug, Serialize)]
pub struct Aircraft {
    /// 24-bit address, six hex digits.
    pub icao: String,
    pub callsign: Option<String>,
    /// Pressure altitude.
    pub alt_ft: Option<i32>,
    pub speed_kt: Option<f32>,
    /// Track over the ground, or heading when `airspeed`.
    pub heading_deg: Option<f32>,
    pub airspeed: bool,
    pub vrate_fpm: Option<i32>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Seconds since the last message, and since the last position.
    pub age_s: f32,
    pub pos_age_s: Option<f32>,
    pub messages: u32,
    /// Strength of the last message: preamble pulses over the gaps between them, dB.
    pub snr_db: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub aircraft: Vec<Aircraft>,
    /// Good frames per second, over the last `RATE_SECS`.
    pub frames_per_sec: f32,
}

#[derive(Default)]
struct Plane {
    callsign: Option<String>,
    alt_ft: Option<i32>,
    speed_kt: Option<f32>,
    heading_deg: Option<f32>,
    airspeed: bool,
    vrate_fpm: Option<i32>,
    /// Latitude, longitude and when.
    pos: Option<(f64, f64, f64)>,
    /// Last even and odd report: CPR latitude, longitude and when.
    even: Option<(u32, u32, f64)>,
    odd: Option<(u32, u32, f64)>,
    seen: f64,
    messages: u32,
    snr_db: f32,
}

pub struct Adsb {
    rate: f64,
    demod: Demod,
    frames: Vec<Frame>,
    planes: HashMap<u32, Plane>,
    /// Samples fed so far.
    clock: u64,
    recent: VecDeque<f64>,
}

impl Adsb {
    pub fn new(rate: u32) -> Self {
        Adsb { rate: rate as f64, demod: Demod::new(rate), frames: Vec::new(), planes: HashMap::new(), clock: 0, recent: VecDeque::new() }
    }

    /// Search a block of the capture (centred near 1090 MHz) for replies.
    pub fn process(&mut self, iq: &[C32]) {
        self.clock += iq.len() as u64;
        self.frames.clear();
        self.demod.process(iq, &mut self.frames);
        for f in std::mem::take(&mut self.frames) {
            self.take(&f);
        }
        let now = self.now();
        self.planes.retain(|_, p| now - p.seen < KEEP_SECS);
        while self.recent.front().is_some_and(|&t| now - t > RATE_SECS) {
            self.recent.pop_front();
        }
    }

    fn now(&self) -> f64 {
        self.clock as f64 / self.rate
    }

    fn take(&mut self, f: &Frame) {
        let Some((icao, m)) = msg::decode(&f.bytes) else { return };
        let t = f.at as f64 / self.rate;
        // a corrected frame is a guess; trust it only for an aircraft already heard cleanly
        if f.corrected && !self.planes.contains_key(&icao) {
            return;
        }
        self.recent.push_back(t);
        let p = self.planes.entry(icao).or_default();
        p.seen = t;
        p.messages += 1;
        p.snr_db = f.snr_db;
        match m {
            Msg::Ident { callsign } => p.callsign = (!callsign.is_empty()).then_some(callsign),
            Msg::Velocity { speed_kt, heading_deg, vrate_fpm, airspeed } => {
                p.speed_kt = speed_kt;
                p.heading_deg = heading_deg;
                p.vrate_fpm = vrate_fpm;
                p.airspeed = airspeed;
            }
            Msg::Position { alt_ft, odd, lat, lon } => {
                if alt_ft.is_some() {
                    p.alt_ft = alt_ft;
                }
                if odd {
                    p.odd = Some((lat, lon, t));
                } else {
                    p.even = Some((lat, lon, t));
                }
                let pair = match (p.even, p.odd) {
                    (Some(e), Some(o)) if (e.2 - o.2).abs() <= PAIR_SECS => cpr::global((e.0, e.1), (o.0, o.1), odd),
                    _ => None,
                };
                let near = p.pos.filter(|q| t - q.2 <= LOCAL_SECS);
                let fix = pair.or_else(|| near.map(|q| cpr::local((lat, lon), odd, (q.0, q.1))));
                if let Some((la, lo)) = fix {
                    let plausible = near.is_none_or(|q| km(q.0, q.1, la, lo) <= MAX_KM_PER_SEC * (t - q.2) + JUMP_KM);
                    if plausible {
                        p.pos = Some((la, lo, t));
                    } else if pair.is_some() {
                        // a fresh pair disagrees with the track: trust neither until the next pair
                        p.pos = None;
                    }
                }
            }
        }
    }

    pub fn info(&self) -> Info {
        let now = self.now();
        let mut aircraft: Vec<Aircraft> = self
            .planes
            .iter()
            .map(|(&icao, p)| Aircraft {
                icao: format!("{icao:06X}"),
                callsign: p.callsign.clone(),
                alt_ft: p.alt_ft,
                speed_kt: p.speed_kt,
                heading_deg: p.heading_deg,
                airspeed: p.airspeed,
                vrate_fpm: p.vrate_fpm,
                lat: p.pos.map(|q| q.0),
                lon: p.pos.map(|q| q.1),
                age_s: (now - p.seen).max(0.0) as f32,
                pos_age_s: p.pos.map(|q| (now - q.2).max(0.0) as f32),
                messages: p.messages,
                snr_db: p.snr_db,
            })
            .collect();
        aircraft.sort_by(|a, b| a.icao.cmp(&b.icao));
        Info { aircraft, frames_per_sec: (self.recent.len() as f64 / RATE_SECS.min(now.max(1e-3))) as f32 }
    }
}

/// Great-circle distance, km.
fn km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let a = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * ((lon2 - lon1).to_radians() / 2.0).sin().powi(2);
    2.0 * 6371.0 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::phy::testutil::{hex, synth};
    use super::*;

    #[test]
    fn table_from_iq() {
        let frames: Vec<_> = ["8D40621D58C386435CC412692AD6", "8D4840D6202CC371C32CE0576098", "8D40621D58C382D690C8AC2863A7", "8D485020994409940838175B284F"].iter().map(|s| hex(s)).collect();
        let iq = synth(&frames, 2_400_000, 1_000.0, 0.3, 0.03, -200_000.0);
        let mut a = Adsb::new(2_400_000);
        for piece in iq.chunks(65_536) {
            a.process(piece);
        }
        let info = a.info();
        let by = |id: &str| info.aircraft.iter().find(|x| x.icao == id).unwrap();
        assert_eq!(info.aircraft.len(), 3);
        assert_eq!(by("4840D6").callsign.as_deref(), Some("KLM1023"));
        let p = by("40621D");
        assert_eq!(p.alt_ft, Some(38_000));
        assert!((p.lat.unwrap() - 52.25720).abs() < 1e-5 && (p.lon.unwrap() - 3.91937).abs() < 1e-5, "{:?} {:?}", p.lat, p.lon);
        assert_eq!(p.messages, 2);
        let v = by("485020");
        assert!((v.speed_kt.unwrap() - 159.2).abs() < 0.1 && v.vrate_fpm == Some(-832));
    }

    #[test]
    fn local_decode_then_age_out() {
        let mut a = Adsb::new(2_400_000);
        let mut f = |s: &str, t: f64| a.take(&Frame { bytes: hex(s), at: (t * 2.4e6) as u64, snr_db: 20.0, corrected: false });
        f("8D40621D58C386435CC412692AD6", 0.0);
        f("8D40621D58C382D690C8AC2863A7", 2.0);
        // a lone even report 30 s later decodes against the last position
        f("8D40621D58C382D690C8AC2863A7", 32.0);
        let p = &a.planes[&0x40621D];
        let (lat, lon, t) = p.pos.unwrap();
        assert!((lat - 52.25720).abs() < 1e-5 && (lon - 3.91937).abs() < 1e-5 && t == 32.0);
        a.clock = (100.0 * 2.4e6) as u64;
        a.process(&[]);
        assert!(a.info().aircraft.is_empty());
    }

    #[test]
    fn corrected_frame_needs_a_known_aircraft() {
        let mut a = Adsb::new(2_400_000);
        a.take(&Frame { bytes: hex("8D4840D6202CC371C32CE0576098"), at: 0, snr_db: 10.0, corrected: true });
        assert!(a.planes.is_empty());
    }
}
