//! US ZIP code → approximate location, offline.

use super::Site;

const ZIPS: &str = include_str!("zips.txt");

pub fn lookup(zip: &str) -> Option<Site> {
    if zip.len() != 5 || !zip.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let line = ZIPS.lines().find(|l| l.starts_with(zip) && l.as_bytes().get(5) == Some(&b' '))?;
    let mut f = line[6..].split(' ').map(|v| v.parse::<f64>());
    Some(Site { lat: f.next()?.ok()?, lon: f.next()?.ok()? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_boulder() {
        let s = lookup("80302").unwrap();
        assert!((s.lat - 40.0).abs() < 0.2 && (s.lon + 105.4).abs() < 0.3, "{s:?}");
    }

    #[test]
    fn rejects_unknown_and_malformed() {
        assert!(lookup("00000").is_none());
        assert!(lookup("8030").is_none());
        assert!(lookup("# US Z").is_none());
    }
}
