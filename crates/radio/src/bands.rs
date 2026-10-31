//! The band list, read from ~/.config/squelch/bands.toml.

use crate::demod::Mode;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_BANDS: &str = include_str!("default_bands.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub mhz: f64,
    pub name: String,
    #[serde(default)]
    pub mode: Option<Mode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Band {
    pub id: String,
    pub group: String,
    pub name: String,
    pub icon: String,
    pub mode: Mode,
    pub low_mhz: f64,
    pub high_mhz: f64,
    pub start_mhz: f64,
    #[serde(default)]
    pub decoder: Option<String>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BandPlan {
    #[serde(rename(deserialize = "group"))]
    pub groups: Vec<Group>,
    #[serde(rename(deserialize = "band"))]
    pub bands: Vec<Band>,
}

impl BandPlan {
    pub fn parse(text: &str) -> Result<Self> {
        let plan: BandPlan = toml::from_str(text)?;
        Ok(plan)
    }

    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("squelch").join("bands.toml"))
    }

    /// Read the user's file, creating it from the defaults the first time.
    pub fn load() -> Result<(Self, Option<PathBuf>)> {
        let Some(path) = Self::path() else {
            return Ok((Self::parse(DEFAULT_BANDS)?, None));
        };
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, DEFAULT_BANDS)?;
        }
        let text = std::fs::read_to_string(&path)?;
        let plan = Self::parse(&text).with_context(|| format!("in {}", path.display()))?;
        Ok((plan, Some(path)))
    }

    /// The band containing `mhz`, preferring the narrowest when bands overlap.
    pub fn band_at(&self, mhz: f64) -> Option<&Band> {
        self.bands
            .iter()
            .filter(|b| mhz >= b.low_mhz && mhz <= b.high_mhz)
            .min_by(|a, b| (a.high_mhz - a.low_mhz).partial_cmp(&(b.high_mhz - b.low_mhz)).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_are_consistent() {
        let plan = BandPlan::parse(DEFAULT_BANDS).unwrap();
        assert!(plan.bands.len() >= 20);
        for b in &plan.bands {
            assert!(plan.groups.iter().any(|g| g.id == b.group), "{} has unknown group", b.id);
            assert!(b.low_mhz < b.high_mhz, "{}", b.id);
            assert!(b.start_mhz >= b.low_mhz && b.start_mhz <= b.high_mhz, "{} start outside band", b.id);
            // the R820T tuner covers roughly 24–1766 MHz
            assert!(b.low_mhz >= 24.0 && b.high_mhz <= 1766.0, "{} outside tuner range", b.id);
            for m in &b.bookmarks {
                assert!(m.mhz >= b.low_mhz && m.mhz <= b.high_mhz, "{} bookmark {} outside band", b.id, m.name);
            }
        }
        let mut ids: Vec<_> = plan.bands.iter().map(|b| &b.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), plan.bands.len(), "duplicate band id");
    }

    #[test]
    fn narrowest_band_wins_on_overlap() {
        let plan = BandPlan::parse(DEFAULT_BANDS).unwrap();
        assert_eq!(plan.band_at(145.8).unwrap().id, "iss");
        assert_eq!(plan.band_at(146.52).unwrap().id, "2m");
    }
}
