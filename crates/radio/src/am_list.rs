//! US AM stations near a place, offline, from the FCC's public list (`am_list.txt`, written by `ui/scripts/amlist.py`).

use crate::repeaters::km_between;
use serde::Serialize;

const LIST: &str = include_str!("am_list.txt");
/// Further than this, a station is not sent, unless it is a clear-channel station within `NIGHT_KM`.
pub const LOOK_KM: f64 = 300.0;
/// A clear-channel (class A) station is heard this far after dark, off the sky.
pub const NIGHT_KM: f64 = 1500.0;
/// By day, a 1 kW station's ground wave carries about this far, km; further scales with the square root of power.
const KM_AT_ONE_KW: f64 = 30.0;

#[derive(Debug, Clone, Serialize)]
pub struct AmStation {
    pub hz: f64,
    pub call: &'static str,
    pub city: &'static str,
    pub state: &'static str,
    pub km: f64,
    /// By day, a guess from its daytime power. A station beyond it still lists once it has been heard.
    pub in_reach: bool,
    /// A clear-channel station within `NIGHT_KM`: heard after dark from far away.
    pub at_night: bool,
}

/// Stations within `LOOK_KM` of the point, and clear-channel ones within `NIGHT_KM`, nearest first. With a `state`
/// (its two-letter code, the point being only its centre), that state's stations count as in reach wherever they are.
pub fn near(lat: f64, lon: f64, state: Option<&str>) -> Vec<AmStation> {
    let mut out: Vec<AmStation> = LIST.lines().filter_map(|l| parse(l, (lat, lon), state)).collect();
    out.sort_by(|a, b| a.km.total_cmp(&b.km));
    out
}

fn parse(line: &'static str, at: (f64, f64), home: Option<&str>) -> Option<AmStation> {
    let mut f = line.splitn(9, ' ');
    let call = f.next()?;
    let mut num = || f.next()?.parse::<f64>().ok();
    let (khz, lat, lon, day_kw, _night_kw) = (num()?, num()?, num()?, num()?, num()?);
    let (class, state, city) = (f.next()?, f.next()?, f.next()?);
    let km = km_between(at, (lat, lon));
    let own = home.is_some_and(|h| h.eq_ignore_ascii_case(state));
    let at_night = class == "A" && km <= NIGHT_KM;
    if km > LOOK_KM && !own && !at_night {
        return None;
    }
    let in_reach = own || km <= KM_AT_ONE_KW * day_kw.sqrt();
    Some(AmStation { hz: khz * 1_000.0, call, city, state, km, in_reach, at_night })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DENVER: (f64, f64) = (39.74, -104.99);

    #[test]
    fn every_line_parses_onto_the_10_khz_grid() {
        let all: Vec<_> = LIST.lines().collect();
        assert!(all.len() > 3000);
        for l in all {
            let s = parse(l, (0.0, 0.0), Some(l.split(' ').nth(7).unwrap())).unwrap_or_else(|| panic!("{l}"));
            assert!((530_000.0..=1_710_000.0).contains(&s.hz), "{l}");
        }
    }

    #[test]
    fn koa_is_in_reach_in_denver_and_wbap_only_at_night() {
        let list = near(DENVER.0, DENVER.1, None);
        let koa = list.iter().find(|s| s.call == "KOA").unwrap();
        assert_eq!(koa.hz, 850_000.0);
        assert!(koa.in_reach, "{koa:?}");
        let wbap = list.iter().find(|s| s.call == "WBAP").unwrap();
        assert!(!wbap.in_reach && wbap.at_night, "{wbap:?}");
        assert!(!list.iter().any(|s| s.call == "WWL"), "New Orleans is past NIGHT_KM");
        assert!(!list.iter().any(|s| s.call == "WINZ"), "a Miami regional station is not sent to Denver");
    }
}
