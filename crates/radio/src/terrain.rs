//! Elevation tiles for the local map: AWS Terrain Tiles (Mapzen "terrarium" PNGs, no key), cached on disk.
//! Each pixel is metres = R * 256 + G + B / 256 - 32768; US data is USGS 3DEP. Best effort: no network, no tile.
//! The shaded picture the UI makes from them is cached too, so a known place draws at once.
//! Both caches keep what was used most recently: `MAX_GROUNDS` pictures, `MAX_TILES` tiles, nothing unused for `MAX_AGE`.

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

const URL: &str = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";
const USER_AGENT: &str = "squelch (https://github.com/demian0311/squelch)";
/// Deepest zoom we ask for; the local map needs no more than about 500 m per pixel.
pub const MAX_ZOOM: u8 = 10;
/// Shaded maps kept: one per place and theme.
pub const MAX_GROUNDS: usize = 100;
/// Tiles kept: a map takes 10–15, so this is about `MAX_GROUNDS` maps' worth (~100 MB).
pub const MAX_TILES: usize = 1500;
/// Anything not used for a year goes.
pub const MAX_AGE: Duration = Duration::from_secs(365 * 24 * 3600);

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("squelch"))
}

/// `~/.cache/squelch/terrain/<z>/<x>/<y>.png`
pub fn cache_path(z: u8, x: u32, y: u32) -> Option<PathBuf> {
    cache_dir().map(|d| d.join("terrain").join(z.to_string()).join(x.to_string()).join(format!("{y}.png")))
}

/// Reads a cached file and marks it used now, so it is the last to be pruned.
fn read_used(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    if let Ok(f) = File::options().write(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
    Some(data)
}

pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Deletes the `.png` files under `dir` beyond the `keep` most recently used, and any unused for `max_age`.
pub fn prune(dir: &Path, keep: usize, max_age: Duration) {
    fn walk(dir: &Path, out: &mut Vec<(SystemTime, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "png") {
                out.push((e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH), p));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let cutoff = SystemTime::now().checked_sub(max_age).unwrap_or(SystemTime::UNIX_EPOCH);
    for (i, (when, path)) in files.iter().enumerate() {
        if i >= keep || *when < cutoff {
            let _ = fs::remove_file(path);
        }
    }
}

/// The tile's PNG bytes, from the cache or else downloaded once with curl.
pub fn tile(z: u8, x: u32, y: u32) -> Result<Vec<u8>> {
    let n = 1u32 << z;
    if z > MAX_ZOOM || x >= n || y >= n {
        bail!("no tile {z}/{x}/{y}");
    }
    let path = cache_path(z, x, y).context("no cache directory")?;
    if let Some(data) = read_used(&path) {
        return Ok(data);
    }
    let url = format!("{URL}/{z}/{x}/{y}.png");
    let out = Command::new("curl").args(["-sfL", "--max-time", "20", "-A", USER_AGENT, &url]).output().context("running curl")?;
    if !out.status.success() || !out.stdout.starts_with(b"\x89PNG") {
        bail!("could not download {url}");
    }
    write_atomic(&path, &out.stdout)?;
    if let Some(d) = cache_dir() {
        prune(&d.join("terrain"), MAX_TILES, MAX_AGE);
    }
    Ok(out.stdout)
}

fn ground_path(key: &str) -> Option<PathBuf> {
    // the UI's key is a hash; anything else is refused rather than used as a path
    if key.is_empty() || key.len() > 64 || !key.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    cache_dir().map(|d| d.join("ground").join(format!("{key}.png")))
}

/// A shaded map saved earlier under `key`, as a data URL.
pub fn ground(key: &str) -> Option<String> {
    crate::hd::data_url(&read_used(&ground_path(key)?)?)
}

/// Saves a shaded map (a PNG data URL) under `key` and prunes the oldest beyond `MAX_GROUNDS`.
pub fn save_ground(key: &str, data_url: &str) -> Result<()> {
    let path = ground_path(key).context("bad map key")?;
    let png = data_url.strip_prefix("data:image/png;base64,").and_then(unbase64).context("not a PNG data URL")?;
    if !png.starts_with(b"\x89PNG") {
        bail!("not a PNG");
    }
    write_atomic(&path, &png)?;
    prune(path.parent().unwrap(), MAX_GROUNDS, MAX_AGE);
    Ok(())
}

fn unbase64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let s = s.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for c in s.chunks(4) {
        let mut n = 0u32;
        for (i, &b) in c.iter().enumerate() {
            n |= (val(b)? as u32) << (18 - 6 * i);
        }
        for i in 0..c.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

/// The tile as a `data:image/png;base64,…` URL for the UI, or None when it cannot be had.
pub fn tile_url(z: u8, x: u32, y: u32) -> Option<String> {
    crate::hd::data_url(&tile(z, x, y).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbase64_undoes_base64() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foob", b"\x89PNG\r\n"] {
            assert_eq!(unbase64(&crate::hd::base64(s)).unwrap(), s);
        }
        assert!(unbase64("ab$d").is_none());
    }

    #[test]
    fn refuses_map_keys_that_are_not_plain() {
        assert!(ground_path("../x").is_none());
        assert!(ground_path("").is_none());
        assert!(ground_path("a1b2").is_some());
    }

    #[test]
    fn prune_keeps_the_most_recently_used() {
        let dir = std::env::temp_dir().join(format!("squelch-prune-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        let now = SystemTime::now();
        for (i, name) in ["a.png", "sub/b.png", "c.png", "d.png"].iter().enumerate() {
            let p = dir.join(name);
            fs::write(&p, b"x").unwrap();
            File::options().write(true).open(&p).unwrap().set_modified(now - Duration::from_secs(60 * i as u64)).unwrap();
        }
        // d is old enough to go by age; c is beyond the two newest
        prune(&dir, 2, Duration::from_secs(150));
        let left: Vec<bool> = ["a.png", "sub/b.png", "c.png", "d.png"].iter().map(|n| dir.join(n).exists()).collect();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left, [true, true, false, false]);
    }

    #[test]
    fn refuses_tiles_outside_the_world() {
        assert!(tile(2, 4, 0).is_err());
        assert!(tile(MAX_ZOOM + 1, 0, 0).is_err());
    }
}
