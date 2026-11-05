//! Airports and the channels pilots talk on, offline, from OurAirports (`airports.txt`, written by `ui/scripts/airports.py`),
//! and the en-route centres' radio sites from the FAA's NASR (`centers.txt`, written by `ui/scripts/centers.py`): above
//! 18,000 ft a plane talks to a centre through the site nearest its sector, so a site is served as an airport of size `C`.

use crate::repeaters::km_between;
use serde::Serialize;

const LIST: &str = include_str!("airports.txt");
const CENTERS: &str = include_str!("centers.txt");
/// Further than this, an airport is off any map the receiver can fill.
pub const LOOK_KM: f64 = 500.0;
/// A centre's remote site reaches planes at height about this far; past it another site is nearer anyway.
pub const CENTER_KM: f64 = 400.0;

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    /// TWR, GND, APP, DEP, A/D, CTAF, UNIC or CNTR; a centre site's LOW, HIGH, LOW/HIGH or ULTRA-HIGH.
    pub kind: &'static str,
    pub mhz: f64,
    pub name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Airport {
    pub ident: &'static str,
    pub iata: &'static str,
    /// L, M or S; C is a centre's radio site.
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

/// A centre site line: centre id, centre name, site, lat, lon, use:MHz;...
fn parse_center(line: &'static str) -> Option<Airport> {
    let mut f = line.split('\t');
    let (id, centre, site) = (f.next()?, f.next()?, f.next()?);
    let (lat, lon) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
    let channels = f
        .next()?
        .split(';')
        .filter_map(|c| {
            let (kind, mhz) = c.split_once(':')?;
            Some(Channel { kind, mhz: mhz.parse().ok()?, name: centre })
        })
        .collect();
    // the names are leaked once per call; the list is small and asked for rarely
    let ident: &'static str = Box::leak(format!("{id}/{site}").into_boxed_str());
    let name: &'static str = Box::leak(format!("{centre} Center, {site} site").into_boxed_str());
    Some(Airport { ident, iata: "", size: "C", lat, lon, name, city: site, channels })
}

/// Airports within `LOOK_KM` of the point and centre sites within `CENTER_KM`, nearest first.
pub fn near(lat: f64, lon: f64) -> Vec<Airport> {
    let mut out: Vec<(f64, Airport)> = LIST
        .lines()
        .filter_map(parse)
        .filter_map(|a| {
            let km = km_between((lat, lon), (a.lat, a.lon));
            (km <= LOOK_KM).then_some((km, a))
        })
        .chain(CENTERS.lines().filter_map(parse_center).filter_map(|a| {
            let km = km_between((lat, lon), (a.lat, a.lon));
            (km <= CENTER_KM).then_some((km, a))
        }))
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
        // the centre's nearest site, with its high and low frequencies; the FAA's list as of the 2026-10-29 cycle
        let site = list.iter().find(|a| a.size == "C").unwrap();
        assert_eq!(site.ident, "ZMA/Miami");
        assert_eq!(site.name, "Miami Center, Miami site");
        assert!(site.channels.iter().all(|c| c.name == "Miami" && (118.0..137.0).contains(&c.mhz) && c.mhz != 121.5), "{:?}", site.channels);
        assert!(site.channels.iter().any(|c| c.kind == "HIGH" || c.kind == "LOW"), "{:?}", site.channels);
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
