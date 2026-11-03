//! US FM stations near a place, offline, from the FCC's public list (`fm_list.txt`, written by `ui/scripts/fmlist.py`).

use crate::repeaters::km_between;
use serde::Serialize;

const LIST: &str = include_str!("fm_list.txt");
/// Further than this, a station is not worth sending even as a maybe.
pub const LOOK_KM: f64 = 300.0;
/// Where a car or a dongle's antenna sits above the ground, for the radio horizon, m.
const RECEIVER_M: f64 = 10.0;
/// A 1 kW station carries about this far, km; further scales with the square root of power.
const KM_AT_ONE_KW: f64 = 30.0;

#[derive(Debug, Clone, Serialize)]
pub struct FmStation {
    pub hz: f64,
    pub call: &'static str,
    pub city: &'static str,
    pub state: &'static str,
    pub km: f64,
    /// A guess from its power and antenna height. A station beyond it still lists once it has been heard.
    pub in_reach: bool,
}

/// Radio horizon from the antenna height, or less for a weak station.
fn reach_km(kw: f64, haat_m: f64) -> f64 {
    let horizon = 4.12 * (haat_m.max(30.0).sqrt() + RECEIVER_M.sqrt());
    horizon.min(KM_AT_ONE_KW * kw.sqrt())
}

/// Stations within `LOOK_KM` of the point, nearest first. With a `state` (its two-letter code, the point being only
/// its centre), that state's stations count as in reach wherever they are.
pub fn near(lat: f64, lon: f64, state: Option<&str>) -> Vec<FmStation> {
    let mut out: Vec<FmStation> = LIST.lines().filter_map(|l| parse(l, (lat, lon), state)).collect();
    out.sort_by(|a, b| a.km.total_cmp(&b.km));
    out
}

fn parse(line: &'static str, at: (f64, f64), home: Option<&str>) -> Option<FmStation> {
    let mut f = line.splitn(8, ' ');
    let call = f.next()?;
    let mut num = || f.next()?.parse::<f64>().ok();
    let (mhz, lat, lon, kw, haat) = (num()?, num()?, num()?, num()?, num()?);
    let (state, city) = (f.next()?, f.next()?);
    let km = km_between(at, (lat, lon));
    let own = home.is_some_and(|h| h.eq_ignore_ascii_case(state));
    if km > LOOK_KM && !own {
        return None;
    }
    Some(FmStation { hz: (mhz * 10.0).round() * 100_000.0, call, city, state, km, in_reach: own || km <= reach_km(kw, haat) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIAMI: (f64, f64) = (25.77, -80.19);

    #[test]
    fn every_line_parses() {
        let all: Vec<_> = LIST.lines().collect();
        assert!(all.len() > 5000);
        for l in all {
            let s = parse(l, (0.0, 0.0), Some(l.split(' ').nth(6).unwrap())).unwrap_or_else(|| panic!("{l}"));
            assert_eq!(s.hz as u64 % 100_000, 0, "{l}");
        }
    }

    #[test]
    fn a_strong_station_nearby_is_in_reach_and_a_far_one_is_not_listed() {
        let list = near(MIAMI.0, MIAMI.1, None);
        let wkis = list.iter().find(|s| s.call == "WKIS").unwrap();
        assert_eq!(wkis.hz, 99_900_000.0);
        assert!(wkis.in_reach, "{wkis:?}");
        assert!(!list.iter().any(|s| s.call == "KOSI"));
    }

    #[test]
    fn a_zip_code_finds_its_stations() {
        let at = crate::iss::parse_site("33131").unwrap();
        assert!(near(at.lat, at.lon, None).iter().any(|s| s.call == "WKIS" && s.in_reach));
    }

    #[test]
    fn a_weak_station_reaches_less_than_its_horizon() {
        assert!(reach_km(0.05, 300.0) < 10.0);
        assert!(reach_km(100.0, 300.0) > 70.0);
    }

    #[test]
    fn a_state_keeps_all_its_own() {
        let list = near(39.0, -105.5, Some("co"));
        assert!(list.iter().filter(|s| s.state == "CO").all(|s| s.in_reach));
        assert!(list.iter().any(|s| s.call == "KOSI"));
    }
}
