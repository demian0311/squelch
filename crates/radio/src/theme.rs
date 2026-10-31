//! Colours from the current Omarchy theme (~/.local/state/omarchy/current/theme/colors.toml).

use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".local/state/omarchy/current/theme/colors.toml"))
}

pub fn theme_name() -> Option<String> {
    let p = dirs::home_dir()?.join(".local/state/omarchy/current/theme.name");
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

/// Every `key = "#rrggbb"` (and `mode`) entry of the colours file.
pub fn parse(text: &str) -> BTreeMap<String, String> {
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(_) => return BTreeMap::new(),
    };
    table
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect()
}

pub fn load() -> BTreeMap<String, String> {
    let mut colors = path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse(&t)).unwrap_or_default();
    if let Some(name) = theme_name() {
        colors.insert("name".into(), name);
    }
    colors
}

/// Modification time of the colours file, to notice theme switches.
pub fn stamp() -> Option<std::time::SystemTime> {
    let p = path()?;
    // the theme directory is replaced on a switch, so check the resolved file
    std::fs::metadata(std::fs::canonicalize(p).ok()?).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_omarchy_colors() {
        let c = super::parse("# Slate\nmode = \"dark\"\naccent = \"#5b9bd5\"\nbackground = \"#161b22\"\n");
        assert_eq!(c["accent"], "#5b9bd5");
        assert_eq!(c["mode"], "dark");
        assert_eq!(c.len(), 3);
    }
}
