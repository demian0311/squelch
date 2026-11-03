//! Airports and the channels pilots talk on, offline, from OurAirports (`airports.txt`, written by `ui/scripts/airports.py`).

use crate::repeaters::km_between;
use serde::Serialize;

const LIST: &str = include_str!("airports.txt");
/// Further than this, an airport is off any map the receiver can fill.
pub const LOOK_KM: f64 = 500.0;

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    /// TWR, GND, APP, DEP, A/D, CTAF, UNIC or CNTR.
    pub kind: &'static str,
    pub mhz: f64,
    pub name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Airport {
    pub ident: &'static str,
    pub iata: &'static str,
    /// L, M or S.
    pub size: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub name: &'static str,
    pub city: &'static str,
    pub channels: Vec<Channel>,
}

fn parse(line: &'static str) -> Option<Airport> {
    let mut f = line.split('\t');
    let (ident, iata, size) = (f.next()?, f.next()?, f.next()?);
    let (lat, lon) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
    let (name, city, chans) = (f.next()?, f.next()?, f.next()?);
    let channels = chans
        .split(';')
        .filter_map(|c| {
            let mut p = c.splitn(3, ':');
            Some(Channel { kind: p.next()?, mhz: p.next()?.parse().ok()?, name: p.next()? })
        })
        .collect();
    Some(Airport { ident, iata, size, lat, lon, name, city, channels })
}

/// Airports within `LOOK_KM` of the point, nearest first.
pub fn near(lat: f64, lon: f64) -> Vec<Airport> {
    let mut out: Vec<(f64, Airport)> = LIST
        .lines()
        .filter_map(parse)
        .filter_map(|a| {
            let km = km_between((lat, lon), (a.lat, a.lon));
            (km <= LOOK_KM).then_some((km, a))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, a)| a).collect()
}

/// The airports with these ICAO idents or IATA codes, wherever they are.
pub fn by_code(codes: &[String]) -> Vec<Airport> {
    LIST.lines().filter_map(parse).filter(|a| codes.iter().any(|c| c == a.ident || c == a.iata)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_airports_near_a_place_nearest_first() {
        let list = near(25.7617, -80.1918);
        assert_eq!(list[0].ident, "KMIA");
        let fll = list.iter().find(|a| a.ident == "KFLL").unwrap();
        assert_eq!(fll.iata, "FLL");
        assert!(fll.channels.iter().any(|c| c.kind == "TWR" && (c.mhz - 119.3).abs() < 1e-6), "{:?}", fll.channels);
    }

    #[test]
    fn looks_up_by_either_code() {
        let list = by_code(&["STL".into(), "KFLL".into()]);
        let names: Vec<_> = list.iter().map(|a| a.ident).collect();
        assert!(names.contains(&"KSTL") && names.contains(&"KFLL"), "{names:?}");
    }

    #[test]
    fn every_line_parses() {
        assert_eq!(LIST.lines().filter(|l| parse(l).is_none()).count(), 0);
    }
}
