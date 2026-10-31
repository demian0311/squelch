//! Ham repeaters from RepeaterBook's Google Earth (KML) exports, kept in a user file.
//! The `repeaters` example converts the KML; the UI names repeater outputs from the file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Repeater {
    pub call: String,
    pub city: String,
    /// Output frequency: what a receiver hears.
    pub mhz: f64,
    /// Input offset: "+", "-", "s" for a non-standard split, or "" for none listed.
    pub shift: String,
    /// CTCSS tone in Hz ("123.0"), DCS code ("D023") or "CSQ"; empty when none listed.
    #[serde(default)]
    pub tone: String,
    pub lat: f64,
    pub lon: f64,
    /// Digital modes and links, from the per-mode files it also appears in.
    #[serde(default)]
    pub modes: Vec<String>,
    /// RepeaterBook's "state_id-ID", unique across files.
    pub id: String,
    #[serde(skip)]
    pub on_air: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Repeaters {
    #[serde(default, rename = "repeater")]
    pub list: Vec<Repeater>,
}

/// Per-mode export files, by a word in the file name.
const MODE_FILES: [(&str, &str); 11] = [
    ("DMR", "DMR"), ("P-25", "P25"), ("NXDN", "NXDN"), ("D-Star", "D-STAR"), ("Fusion", "Fusion"), ("M17", "M17"),
    ("ATV", "ATV"), ("AllStar", "AllStar"), ("Echolink", "EchoLink"), ("IRLP", "IRLP"), ("WIRES", "WIRES-X"),
];

/// The mode a RepeaterBook file lists, from its name ("Colorado DMR.kml" → DMR); None for "All" and band files.
pub fn file_mode(name: &str) -> Option<&'static str> {
    MODE_FILES.iter().find(|(word, _)| name.contains(word)).map(|&(_, mode)| mode)
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    let end = s[start..].find(close)?;
    Some(&s[start..start + end])
}

/// Every placemark in a RepeaterBook KML export. Entries that don't parse are skipped.
pub fn parse_kml(text: &str) -> Vec<Repeater> {
    text.split("<Placemark>").skip(1).filter_map(parse_placemark).collect()
}

// <description>City <![CDATA[ <br> ]]> 147.04500+ 123.0 <![CDATA[ <br> ]]> On-air: Yes ... ID=185' ...
fn parse_placemark(p: &str) -> Option<Repeater> {
    let call = between(p, "<name>", "</name>")?.trim().to_string();
    let desc = between(p, "<description>", "</description>")?;
    let mut parts = desc.split("]]>");
    let city = parts.next()?.split("<![CDATA[").next()?.trim().to_string();
    let mut freq = parts.next()?.split("<![CDATA[").next()?.split_whitespace();
    let f = freq.next()?;
    let (num, shift) = match f.strip_suffix(['+', '-', 's']) {
        Some(n) => (n, &f[n.len()..]),
        None => (f, ""),
    };
    let mhz: f64 = num.parse().ok()?;
    let tone = freq.next().unwrap_or("").to_string();
    let on_air = between(desc, "On-air:", "<").map_or(true, |s| s.trim() != "No");
    let state = between(desc, "state_id=", "&")?;
    let id = between(desc, "ID=", "'")?;
    let mut coords = between(p, "<coordinates>", "</coordinates>")?.split(',');
    let lon: f64 = coords.next()?.trim().parse().ok()?;
    let lat: f64 = coords.next()?.trim().parse().ok()?;
    Some(Repeater { call, city, mhz, shift: shift.into(), tone, lat, lon, modes: vec![], id: format!("{state}-{id}"), on_air })
}

/// Great-circle distance in km.
pub fn km_between((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let a = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * ((lon2 - lon1).to_radians() / 2.0).sin().powi(2);
    2.0 * 6371.0 * a.sqrt().asin()
}

/// Repeaters kept around a location given as a ZIP code or "lat, lon"; a state keeps the whole file.
pub const NEARBY_KM: f64 = 100.0;

/// Merge parsed files (each with the mode its file name gives) and drop off-air entries.
pub fn merge(files: Vec<(Option<&str>, Vec<Repeater>)>) -> Vec<Repeater> {
    let mut by_id: BTreeMap<String, Repeater> = BTreeMap::new();
    for (mode, list) in files {
        for r in list {
            let e = by_id.entry(r.id.clone()).or_insert(r);
            if let Some(m) = mode {
                if !e.modes.iter().any(|x| x == m) {
                    e.modes.push(m.into());
                }
            }
        }
    }
    let mut list: Vec<_> = by_id.into_values().filter(|r| r.on_air).collect();
    list.sort_by(|a, b| a.mhz.total_cmp(&b.mhz));
    list
}

/// Those within `km` of `near`, one per output frequency: the nearest, or the first without a location.
pub fn nearby(list: Vec<Repeater>, near: Option<(f64, f64)>, km: f64) -> Vec<Repeater> {
    let dist = |r: &Repeater| near.map_or(0.0, |n| km_between(n, (r.lat, r.lon)));
    let mut by_hz: BTreeMap<u64, Repeater> = BTreeMap::new();
    for r in list.into_iter().filter(|r| dist(r) <= km) {
        let hz = (r.mhz * 1e6).round() as u64;
        match by_hz.get(&hz) {
            Some(old) if dist(old) <= dist(&r) => {}
            _ => {
                by_hz.insert(hz, r);
            }
        }
    }
    by_hz.into_values().collect()
}

impl Repeaters {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("squelch").join("repeaters.toml"))
    }

    /// The user's repeaters file; empty when there is none.
    pub fn load() -> Result<Vec<Repeater>> {
        let Some(path) = Self::path().filter(|p| p.exists()) else { return Ok(vec![]) };
        let text = std::fs::read_to_string(&path)?;
        let r: Repeaters = toml::from_str(&text).with_context(|| format!("in {}", path.display()))?;
        Ok(r.list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placemark(call: &str, city: &str, freq: &str, air: &str, id: u32, lon: f64, lat: f64) -> String {
        format!(
            "  <Placemark>\r\n    <name>{call}</name>\r\n    \t<styleUrl>IDLE</styleUrl>\r\n    <description>{city}\r\n \
             <![CDATA[ <br>  ]]> {freq}\r\n <![CDATA[ <br>  ]]> On-air: {air}                    <![CDATA[<br> \
             <a href='https://www.repeaterbook.com/repeaters/details.php?state_id=08&ID={id}'>Click for Details</a>    ]]>\r\n \
             </description>\r\n    <Point>\r\n      <coordinates>{lon}, {lat},0</coordinates>\r\n    </Point>  \r\n  </Placemark>\r\n"
        )
    }

    #[test]
    fn parses_tones_shifts_and_air() {
        let kml = [
            placemark("K0ML", "Northglenn", "147.04500+ 123.0", "Yes", 185, -104.987, 39.8856),
            placemark("W0X", "Pueblo", "449.30000- D023", "No", 7, -104.7, 38.2),
            placemark("N0T", "Idaho Springs", "146.94000-", "Unknown", 81, -105.49, 39.68),
            placemark("W0SKY", "Golden", "927.97500s 67.0", "Yes", 18156, -105.237, 39.733),
        ]
        .concat();
        let r = parse_kml(&format!("<Document>\r\n{kml}</Document>"));
        assert_eq!(r.len(), 4);
        assert_eq!((r[0].call.as_str(), r[0].city.as_str(), r[0].mhz, r[0].shift.as_str()), ("K0ML", "Northglenn", 147.045, "+"));
        assert_eq!((r[0].tone.as_str(), r[0].id.as_str(), r[0].lat, r[0].lon), ("123.0", "08-185", 39.8856, -104.987));
        assert_eq!((r[1].tone.as_str(), r[1].on_air), ("D023", false));
        assert_eq!((r[2].tone.as_str(), r[2].shift.as_str(), r[2].on_air), ("", "-", true));
        assert_eq!((r[3].mhz, r[3].shift.as_str()), (927.975, "s"));
    }

    #[test]
    fn modes_from_file_names() {
        assert_eq!(file_mode("Colorado DMR.kml"), Some("DMR"));
        assert_eq!(file_mode("New Mexico System Fusion (YSF).kml"), Some("Fusion"));
        assert_eq!(file_mode("Colorado All.kml"), None);
        assert_eq!(file_mode("Colorado 70 Centimeters.kml"), None);
    }

    #[test]
    fn merge_tags_modes_and_drops_off_air() {
        let all = parse_kml(
            &[
                placemark("NEAR", "Denver", "447.00000- 100.0", "Yes", 1, -104.99, 39.74),
                placemark("FAR", "Grand Junction", "447.00000- 100.0", "Yes", 2, -108.55, 39.06),
                placemark("GONE", "Denver", "146.88000- 88.5", "No", 3, -104.99, 39.74),
                placemark("DIGI", "Boulder", "145.25000- 100.0", "Yes", 4, -105.27, 40.01),
                placemark("SKI", "Steamboat", "146.70000-", "Yes", 5, -106.83, 40.48),
            ]
            .concat(),
        );
        let dmr = all.iter().filter(|r| r.call == "DIGI").cloned().collect();
        let r = merge(vec![(None, all), (Some("DMR"), dmr)]);
        let calls: Vec<_> = r.iter().map(|r| r.call.as_str()).collect();
        assert_eq!(calls, ["DIGI", "SKI", "NEAR", "FAR"]);
        assert_eq!(r[0].modes, ["DMR"]);
        let denver = (39.74, -104.99);
        let calls = |l: Vec<Repeater>| l.iter().map(|r| r.call.clone()).collect::<Vec<_>>();
        assert_eq!(calls(nearby(r.clone(), Some(denver), 400.0)), ["DIGI", "SKI", "NEAR"]);
        assert_eq!(calls(nearby(r.clone(), Some(denver), 50.0)), ["DIGI", "NEAR"]);
        assert_eq!(calls(nearby(r, None, f64::INFINITY)), ["DIGI", "SKI", "NEAR"]);
    }

    #[test]
    fn file_round_trips() {
        let list = parse_kml(&placemark("K0ML", "Northglenn", "147.04500+ 123.0", "Yes", 185, -104.987, 39.8856));
        let text = toml::to_string(&Repeaters { list: list.clone() }).unwrap();
        let back: Repeaters = toml::from_str(&text).unwrap();
        assert_eq!(back.list[0].call, "K0ML");
        assert_eq!(back.list[0].mhz, 147.045);
        assert!(!back.list[0].on_air, "on_air is only for the converter");
    }
}
