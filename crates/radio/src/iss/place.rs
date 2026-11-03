//! US ZIP codes and states → approximate location, offline.

use super::Site;

const ZIPS: &str = include_str!("zips.txt");
const STATES: &str = include_str!("states.txt");

pub fn zip(zip: &str) -> Option<Site> {
    if zip.len() != 5 || !zip.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let line = ZIPS.lines().find(|l| l.starts_with(zip) && l.as_bytes().get(5) == Some(&b' '))?;
    let mut f = line[6..].split(' ').map(|v| v.parse::<f64>());
    Some(Site { lat: f.next()?.ok()?, lon: f.next()?.ok()? })
}

/// A state's two-letter code or full name, any case. Its centre can be a few hundred km from the user.
pub fn state(name: &str) -> Option<Site> {
    let line = state_line(name)?;
    let mut f = line.split(' ').skip(1).map(|v| v.parse::<f64>());
    Some(Site { lat: f.next()?.ok()?, lon: f.next()?.ok()? })
}

/// A state's FIPS code, which RepeaterBook uses as its state_id.
pub fn state_id(name: &str) -> Option<&'static str> {
    state_line(name)?.split(' ').nth(3)
}

fn state_line(name: &str) -> Option<&'static str> {
    let name = name.trim();
    STATES.lines().filter(|l| !l.starts_with('#')).find(|l| {
        let mut f = l.splitn(5, ' ');
        let code = f.next().unwrap_or("");
        let full = f.nth(3).unwrap_or("");
        code.eq_ignore_ascii_case(name) || full.eq_ignore_ascii_case(name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_boulder() {
        let s = zip("80302").unwrap();
        assert!((s.lat - 40.0).abs() < 0.2 && (s.lon + 105.4).abs() < 0.3, "{s:?}");
    }

    #[test]
    fn finds_states_by_code_or_name() {
        let s = state("co").unwrap();
        assert!((s.lat - 39.0).abs() < 0.3 && (s.lon + 105.5).abs() < 0.3, "{s:?}");
        assert_eq!(state(" New Mexico ").unwrap().lat, state("NM").unwrap().lat);
        assert!(state("District of Columbia").is_some());
        assert!(state("Atlantis").is_none());
        assert!(state("New").is_none());
        assert_eq!(state_id("fl"), Some("12"));
        assert_eq!(state_id("New Mexico"), Some("35"));
        assert_eq!(state_id("33131"), None);
    }

    #[test]
    fn rejects_unknown_and_malformed() {
        assert!(zip("00000").is_none());
        assert!(zip("8030").is_none());
        assert!(zip("# US Z").is_none());
    }
}
