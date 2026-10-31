//! Elevation tiles for the local map: AWS Terrain Tiles (Mapzen "terrarium" PNGs, no key), cached forever.
//! Each pixel is metres = R * 256 + G + B / 256 - 32768; US data is USGS 3DEP. Best effort: no network, no tile.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

const URL: &str = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";
const USER_AGENT: &str = "squelch (https://github.com/demian0311/squelch)";
/// Deepest zoom we ask for; the local map needs no more than about 500 m per pixel.
pub const MAX_ZOOM: u8 = 10;

/// `~/.cache/squelch/terrain/<z>/<x>/<y>.png`
pub fn cache_path(z: u8, x: u32, y: u32) -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("squelch").join("terrain").join(z.to_string()).join(x.to_string()).join(format!("{y}.png")))
}

/// The tile's PNG bytes, from the cache or else downloaded once with curl.
pub fn tile(z: u8, x: u32, y: u32) -> Result<Vec<u8>> {
    let n = 1u32 << z;
    if z > MAX_ZOOM || x >= n || y >= n {
        bail!("no tile {z}/{x}/{y}");
    }
    let path = cache_path(z, x, y).context("no cache directory")?;
    if let Ok(data) = std::fs::read(&path) {
        return Ok(data);
    }
    let url = format!("{URL}/{z}/{x}/{y}.png");
    let out = Command::new("curl").args(["-sfL", "--max-time", "20", "-A", USER_AGENT, &url]).output().context("running curl")?;
    if !out.status.success() || !out.stdout.starts_with(b"\x89PNG") {
        bail!("could not download {url}");
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &out.stdout)?;
    std::fs::rename(&tmp, &path)?;
    Ok(out.stdout)
}

/// The tile as a `data:image/png;base64,…` URL for the UI, or None when it cannot be had.
pub fn tile_url(z: u8, x: u32, y: u32) -> Option<String> {
    crate::hd::data_url(&tile(z, x, y).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_tiles_outside_the_world() {
        assert!(tile(2, 4, 0).is_err());
        assert!(tile(MAX_ZOOM + 1, 0, 0).is_err());
    }
}
