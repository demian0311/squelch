//! ADS-B: aircraft broadcasting identity, position and velocity as Mode S extended squitters on 1090 MHz.
//!
//! `phy` finds replies in the whole capture's magnitude, `msg` reads them, `cpr` turns position
//! reports into latitude and longitude, and `Adsb` keeps a table of aircraft by 24-bit address.
//! Time is the sample clock, so a recording decodes the same as live.

pub mod cpr;
pub mod ident;
pub mod msg;
pub mod phy;

use crate::airlog::{Attrs, Line, Point};
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
/// A position goes in the log at most this often, seconds.
const LOG_EVERY: f64 = 15.0;
/// A collision advisory shows for this long after the last message about it, seconds.
const ADVISORY_SECS: f64 = 30.0;
/// Without a broadcast (only DF11 replies so far) an address needs this many to be believed.
const MIN_REPLIES: u32 = 2;
/// Surface positions need a reference this close, km (45 NM).
const SURFACE_REF_KM: f64 = 83.0;
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
    /// From the address alone: null where no rule is known.
    pub registration: Option<String>,
    pub country: Option<&'static str>,
    /// Wake or type category it declares.
    pub category: Option<&'static str>,
    pub squawk: Option<String>,
    pub emergency: Option<&'static str>,
    /// A collision-avoidance advisory is active.
    pub advisory: bool,
    /// Transponder alert or identification pulse, from radar replies.
    pub alert: bool,
    pub spi: bool,
    pub on_ground: Option<bool>,
    /// Satellite height: reported, or pressure altitude plus the reported difference.
    pub gnss_alt_ft: Option<i32>,
    pub selected_alt_ft: Option<i32>,
    pub selected_alt_fms: bool,
    pub selected_heading_deg: Option<f32>,
    /// Altimeter setting, hPa.
    pub baro_hpa: Option<f32>,
    /// Automation modes on, once it reports them.
    pub modes: Vec<&'static str>,
    pub adsb_version: Option<u8>,
    /// Has broadcast a squitter; otherwise it is known only from radar replies.
    pub broadcasting: bool,
    /// Seconds since first heard this run.
    pub seen_s: f32,
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
    first: f64,
    messages: u32,
    snr_db: f32,
    category: Option<&'static str>,
    squawk: Option<String>,
    emergency: Option<&'static str>,
    advisory_at: Option<f64>,
    alert: bool,
    spi: bool,
    on_ground: Option<bool>,
    gnss_ft: Option<i32>,
    gnss_minus_baro_ft: Option<i32>,
    target: msg::Target,
    adsb_version: Option<u8>,
    broadcasting: bool,
    /// Changed since the log last looked.
    dirty: bool,
    logged: Option<Attrs>,
    logged_pos_at: Option<f64>,
}

impl Plane {
    fn believed(&self) -> bool {
        self.broadcasting || self.messages >= MIN_REPLIES
    }

    fn attrs(&self) -> Attrs {
        Attrs {
            callsign: self.callsign.clone(),
            category: self.category.map(str::to_string),
            squawk: self.squawk.clone(),
            emergency: self.emergency.map(str::to_string),
        }
    }
}

pub struct Adsb {
    rate: f64,
    demod: Demod,
    frames: Vec<Frame>,
    planes: HashMap<u32, Plane>,
    /// Samples fed so far.
    clock: u64,
    recent: VecDeque<f64>,
    /// The listener, as a reference for surface positions.
    site: Option<(f64, f64)>,
}

impl Adsb {
    pub fn new(rate: u32) -> Self {
        Adsb { rate: rate as f64, demod: Demod::new(rate), frames: Vec::new(), planes: HashMap::new(), clock: 0, recent: VecDeque::new(), site: None }
    }

    pub fn set_site(&mut self, site: Option<(f64, f64)>) {
        self.site = site;
    }

    /// Lines for the aircraft log since the last call: changed identities, and positions every `LOG_EVERY`.
    pub fn drain_log(&mut self, now_ms: u64, out: &mut Vec<Line>) {
        for (&icao, p) in &mut self.planes {
            if !p.dirty || !p.believed() {
                continue;
            }
            p.dirty = false;
            let id = format!("{icao:06X}");
            let attrs = p.attrs();
            if p.logged.as_ref() != Some(&attrs) {
                out.push(Line::Attrs { at_ms: now_ms, icao: id.clone(), attrs: attrs.clone() });
                p.logged = Some(attrs);
            }
            if let Some((lat, lon, at)) = p.pos {
                if p.logged_pos_at.is_none_or(|t| at - t >= LOG_EVERY) {
                    p.logged_pos_at = Some(at);
                    out.push(Line::Point { icao: id, point: Point { at_ms: now_ms, lat, lon, alt_ft: p.alt_ft } });
                }
            }
        }
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
        let known = &mut self.demod.known;
        self.planes.retain(|icao, p| {
            let keep = now - p.seen < KEEP_SECS;
            if !keep {
                known.remove(icao);
            }
            keep
        });
        while self.recent.front().is_some_and(|&t| now - t > RATE_SECS) {
            self.recent.pop_front();
        }
    }

    fn now(&self) -> f64 {
        self.clock as f64 / self.rate
    }

    fn take(&mut self, f: &Frame) {
        let df = f.bytes[0] >> 3;
        let decoded = if matches!(df, 17 | 18) { msg::decode(&f.bytes) } else { msg::decode_reply(&f.bytes, f.overlaid.unwrap_or(0)) };
        let Some((icao, m)) = decoded else { return };
        let t = f.at as f64 / self.rate;
        // a corrected frame is a guess; trust it only for an aircraft already heard cleanly
        if f.corrected && !self.planes.contains_key(&icao) {
            return;
        }
        // addresses that come with their own check are what replies overlaid on parity are matched against
        if f.overlaid.is_none() {
            self.demod.known.insert(icao);
        }
        self.recent.push_back(t);
        let site = self.site;
        let p = self.planes.entry(icao).or_insert_with(|| Plane { first: t, ..Default::default() });
        p.seen = t;
        p.messages += 1;
        p.snr_db = f.snr_db;
        p.dirty = true;
        if matches!(df, 17 | 18) {
            p.broadcasting = true;
        }
        match m {
            Msg::Ident { callsign, category } => {
                p.callsign = (!callsign.is_empty()).then_some(callsign);
                if category.is_some() {
                    p.category = category;
                }
            }
            Msg::Velocity { speed_kt, heading_deg, vrate_fpm, airspeed, gnss_minus_baro_ft } => {
                p.speed_kt = speed_kt;
                p.heading_deg = heading_deg;
                p.vrate_fpm = vrate_fpm;
                p.airspeed = airspeed;
                p.on_ground = Some(false);
                if gnss_minus_baro_ft.is_some() {
                    p.gnss_minus_baro_ft = gnss_minus_baro_ft;
                }
            }
            Msg::Status { emergency, squawk } => {
                p.emergency = emergency;
                p.squawk = Some(squawk);
            }
            Msg::Advisory => p.advisory_at = Some(t),
            Msg::Target(target) => {
                // a report without valid mode bits leaves the last ones standing
                let modes = target.modes.clone().or(p.target.modes.take());
                p.target = msg::Target { modes, ..target };
            }
            Msg::Operational { version, on_ground } => {
                p.adsb_version = Some(version);
                p.on_ground = Some(on_ground);
            }
            Msg::Reply { alt_ft, squawk, on_ground, alert, spi } => {
                if alt_ft.is_some() {
                    p.alt_ft = alt_ft;
                }
                if squawk.is_some() {
                    p.squawk = squawk;
                }
                if on_ground.is_some() {
                    p.on_ground = on_ground;
                }
                p.alert = alert;
                p.spi = spi;
            }
            Msg::Surface { speed_kt, track_deg, odd, lat, lon } => {
                p.on_ground = Some(true);
                p.alt_ft = None;
                p.vrate_fpm = None;
                p.airspeed = false;
                if speed_kt.is_some() {
                    p.speed_kt = speed_kt;
                }
                if track_deg.is_some() {
                    p.heading_deg = track_deg;
                }
                // ambiguous without something within 45 NM: its own last fix, else the listener
                let near = p.pos.filter(|q| t - q.2 <= LOCAL_SECS).map(|q| (q.0, q.1)).or(site);
                if let Some(r) = near {
                    let (la, lo) = cpr::surface_local((lat, lon), odd, r);
                    if km(r.0, r.1, la, lo) <= SURFACE_REF_KM {
                        p.pos = Some((la, lo, t));
                    }
                }
            }
            Msg::Position { alt_ft, gnss_ft, odd, lat, lon } => {
                p.on_ground = Some(false);
                if alt_ft.is_some() {
                    p.alt_ft = alt_ft;
                }
                if gnss_ft.is_some() {
                    p.gnss_ft = gnss_ft;
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
            .filter(|(_, p)| p.believed())
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
                registration: ident::registration(icao),
                country: ident::country(icao),
                category: p.category,
                squawk: p.squawk.clone(),
                emergency: p.emergency,
                advisory: p.advisory_at.is_some_and(|t| now - t < ADVISORY_SECS),
                alert: p.alert,
                spi: p.spi,
                on_ground: p.on_ground,
                gnss_alt_ft: p.gnss_ft.or_else(|| Some(p.alt_ft? + p.gnss_minus_baro_ft?)),
                selected_alt_ft: p.target.selected_alt_ft,
                selected_alt_fms: p.target.selected_alt_fms,
                selected_heading_deg: p.target.selected_heading_deg,
                baro_hpa: p.target.baro_hpa,
                modes: p.target.modes.clone().unwrap_or_default(),
                adsb_version: p.adsb_version,
                broadcasting: p.broadcasting,
                seen_s: (now - p.first).max(0.0) as f32,
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
        let mut f = |s: &str, t: f64| a.take(&Frame { bytes: hex(s), at: (t * 2.4e6) as u64, snr_db: 20.0, corrected: false, overlaid: None });
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
        a.take(&Frame { bytes: hex("8D4840D6202CC371C32CE0576098"), at: 0, snr_db: 10.0, corrected: true, overlaid: None });
        assert!(a.planes.is_empty());
    }

    fn frame(s: &str, t: f64, overlaid: Option<u32>) -> Frame {
        Frame { bytes: hex(s), at: (t * 2.4e6) as u64, snr_db: 20.0, corrected: false, overlaid }
    }

    #[test]
    fn surface_position_near_the_listener() {
        // The 1090 MHz Riddle's surface example decoded locally: 52.320561, 4.735735 at 17 kt; receiver 51.990, 4.375
        let mut a = Adsb::new(2_400_000);
        a.set_site(Some((51.990, 4.375)));
        a.take(&frame("8C4841753A9A153237AEF0F275BE", 0.0, None));
        let p = &a.planes[&0x484175];
        let (la, lo, _) = p.pos.unwrap();
        assert!((la - 52.320561).abs() < 1e-5 && (lo - 4.735735).abs() < 1e-5, "{la} {lo}");
        assert_eq!(p.on_ground, Some(true));
        assert_eq!(a.info().aircraft[0].speed_kt, Some(17.0));
    }

    #[test]
    fn no_reference_no_surface_position() {
        let mut a = Adsb::new(2_400_000);
        a.take(&frame("8C4841753AAB238733C8CD4020B1", 0.0, None));
        assert!(a.planes[&0x484175].pos.is_none());
    }

    #[test]
    fn replies_fill_in_a_known_aircraft() {
        let mut a = Adsb::new(2_400_000);
        a.take(&frame("8D4840D6202CC371C32CE0576098", 0.0, None));
        a.take(&frame("2A00516D492B80", 1.0, Some(0x4840D6)));
        a.take(&frame("2000171806A983", 2.0, Some(0x4840D6)));
        let i = a.info();
        let p = &i.aircraft[0];
        assert_eq!(p.squawk.as_deref(), Some("0356"));
        assert_eq!(p.alt_ft, Some(36_000));
        assert_eq!(p.category, None);
        assert_eq!(p.country, Some("Netherlands"));
    }

    #[test]
    fn a_lone_all_call_is_not_believed() {
        let mut a = Adsb::new(2_400_000);
        a.take(&frame("5D484FDEA248F5", 0.0, None));
        assert!(a.info().aircraft.is_empty());
        a.take(&frame("5D484FDEA248F5", 1.0, None));
        let i = a.info();
        assert_eq!(i.aircraft.len(), 1);
        assert!(!i.aircraft[0].broadcasting);
    }

    #[test]
    fn log_gets_changes_and_spaced_positions() {
        let mut a = Adsb::new(2_400_000);
        let mut out = Vec::new();
        a.take(&frame("8D40621D58C386435CC412692AD6", 0.0, None));
        a.take(&frame("8D40621D58C382D690C8AC2863A7", 1.0, None));
        a.drain_log(1000, &mut out);
        assert!(matches!(&out[0], Line::Attrs { icao, .. } if icao == "40621D"));
        assert!(matches!(out[1], Line::Point { .. }));
        out.clear();
        a.take(&frame("8D40621D58C386435CC412692AD6", 2.0, None));
        a.drain_log(2000, &mut out);
        assert!(out.is_empty(), "{out:?}");
        a.take(&frame("8D40621D58C386435CC412692AD6", 20.0, None));
        a.drain_log(3000, &mut out);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], Line::Point { .. }));
    }
}
