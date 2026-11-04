//! What a public database knows about one aircraft, by its address: type, owner, a photo. Asked only when a plane is
//! opened, with curl, and kept in `~/.cache/squelch/aircraft/`. Offline means nothing to show, and nothing is kept.
use crate::terrain::{prune, write_atomic};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const URL: &str = "https://api.adsbdb.com/v0/aircraft";
const USER_AGENT: &str = "squelch (https://github.com/demian0311/squelch)";
const MAX_PLANES: usize = 2000;
const MAX_AGE: Duration = Duration::from_secs(90 * 24 * 3600);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaneInfo {
    /// False when the database has never heard of this address.
    pub known: bool,
    pub maker: String,
    pub model: String,
    pub owner: String,
    pub registration: String,
    /// A small picture as a data URL; the page may only show images that come with it.
    pub photo: Option<String>,
}

fn valid(icao: &str) -> bool {
    icao.len() == 6 && icao.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse(body: &str) -> Option<(PlaneInfo, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let r = v.get("response")?;
    if r.as_str().is_some() {
        return Some((PlaneInfo::default(), None));
    }
    let a = r.get("aircraft")?;
    let text = |k: &str| a.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let info = PlaneInfo { known: true, maker: text("manufacturer"), model: text("type"), owner: text("registered_owner"), registration: text("registration"), photo: None };
    Some((info, a.get("url_photo_thumbnail").and_then(|x| x.as_str()).map(str::to_string)))
}

fn curl(url: &str) -> Option<Vec<u8>> {
    let out = Command::new("curl").args(["-sL", "--max-time", "15", "-A", USER_AGENT, url]).output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn cache_dir() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("squelch").join("aircraft"))
}

/// None when neither the cache nor the network has an answer.
pub fn lookup(icao: &str) -> Option<PlaneInfo> {
    if !valid(icao) {
        return None;
    }
    let path = cache_dir()?.join(format!("{}.json", icao.to_ascii_uppercase()));
    if let Some(info) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        return Some(info);
    }
    let body = curl(&format!("{URL}/{icao}"))?;
    let (mut info, thumb) = parse(std::str::from_utf8(&body).ok()?)?;
    info.photo = thumb.and_then(|u| curl(&u)).filter(|b| !b.is_empty()).and_then(|b| crate::hd::data_url(&b));
    if let Ok(json) = serde_json::to_vec(&info) {
        let _ = write_atomic(&path, &json);
        if let Some(dir) = path.parent() {
            prune(dir, MAX_PLANES, MAX_AGE);
        }
    }
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_database_answer() {
        // api.adsbdb.com/v0/aircraft/A9C508, 2026-11-03
        let body = r#"{"response":{"aircraft":{"type":"777 323ER","icao_type":"B77W","manufacturer":"Boeing","mode_s":"A9C508","registration":"N729AN","registered_owner":"American Airlines","url_photo_thumbnail":"https://x/y.jpg"}}}"#;
        let (i, thumb) = parse(body).unwrap();
        assert!(i.known);
        assert_eq!((i.maker.as_str(), i.model.as_str(), i.owner.as_str(), i.registration.as_str()), ("Boeing", "777 323ER", "American Airlines", "N729AN"));
        assert_eq!(thumb.as_deref(), Some("https://x/y.jpg"));
    }

    #[test]
    fn an_unknown_plane_is_an_answer_and_garbage_is_not() {
        assert!(!parse(r#"{"response":"unknown aircraft"}"#).unwrap().0.known);
        assert!(parse("<html>busy</html>").is_none());
    }

    #[test]
    fn only_addresses_reach_the_network() {
        assert!(valid("A9C508"));
        assert!(!valid("../etc") && !valid("A9C50") && !valid("A9C50G"));
    }
}
