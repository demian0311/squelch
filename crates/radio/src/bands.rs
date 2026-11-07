//! The band list, read from ~/.config/squelch/bands.toml.

use crate::demod::Mode;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use toml_edit::{value, Array, DocumentMut, InlineTable, Item, Value};

pub const DEFAULT_BANDS: &str = include_str!("default_bands.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
}

/// What a frequency is for, known to the app; nobody has to save it. Written `names` on a band.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Name {
    pub mhz: f64,
    pub name: String,
    #[serde(default)]
    pub mode: Option<Mode>,
}

/// A frequency the user chose to save.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub mhz: f64,
    pub name: String,
    #[serde(default)]
    pub mode: Option<Mode>,
    /// Free text about the frequency ("county ambulance dispatch").
    #[serde(default)]
    pub notes: Option<String>,
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
    pub names: Vec<Name>,
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
        let mut text = std::fs::read_to_string(&path)?;
        // once only: afterwards a bookmark on a shipped frequency is one the user saved
        if let Some(done) = Self::split_path().filter(|p| !p.exists()) {
            if let Some(split) = split_names(&text, DEFAULT_BANDS)? {
                std::fs::write(&path, &split)?;
                text = split;
            }
            if let Some(dir) = done.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(done, "")?;
        }
        let seen_path = Self::seen_path();
        let seen = seen_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.lines().map(str::to_string).collect::<HashSet<_>>());
        if let Some(merged) = add_new_defaults(&text, DEFAULT_BANDS, seen.as_ref()) {
            std::fs::write(&path, &merged)?;
            text = merged;
        }
        if let Some(renamed) = rename_groups(&text, DEFAULT_BANDS) {
            std::fs::write(&path, &renamed)?;
            text = renamed;
        }
        let ids = default_ids(DEFAULT_BANDS);
        if let (Some(p), true) = (&seen_path, seen.as_ref() != Some(&ids.iter().cloned().collect())) {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(p, ids.join("\n"))?;
        }
        let mut plan = Self::parse(&text).with_context(|| format!("in {}", path.display()))?;
        plan.add_default_names(&Self::parse(DEFAULT_BANDS)?);
        Ok((plan, Some(path)))
    }

    /// A file older than `names` has none: its bands take them from the defaults, by id.
    fn add_default_names(&mut self, defaults: &BandPlan) {
        for band in self.bands.iter_mut().filter(|b| b.names.is_empty()) {
            if let Some(d) = defaults.bands.iter().find(|d| d.id == band.id) {
                band.names = d.names.clone();
            }
        }
    }

    /// Exists once `split_names` has run on this install's bands file.
    fn split_path() -> Option<PathBuf> {
        dirs::state_dir().or_else(dirs::cache_dir).map(|d| d.join("squelch").join("names_split"))
    }

    /// Default groups and bands this install has already offered, so one the user deleted stays deleted.
    fn seen_path() -> Option<PathBuf> {
        dirs::state_dir().or_else(dirs::cache_dir).map(|d| d.join("squelch").join("default_bands_seen"))
    }

    /// The band containing `mhz`, preferring the narrowest when bands overlap.
    pub fn band_at(&self, mhz: f64) -> Option<&Band> {
        self.bands
            .iter()
            .filter(|b| mhz >= b.low_mhz && mhz <= b.high_mhz)
            .min_by(|a, b| (a.high_mhz - a.low_mhz).partial_cmp(&(b.high_mhz - b.low_mhz)).unwrap())
    }
}

/// Two bookmark frequencies are the same entry when they agree to 1 Hz.
fn same_mhz(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn mhz_of(v: &Value) -> Option<f64> {
    let m = v.as_inline_table()?.get("mhz")?;
    m.as_float().or_else(|| m.as_integer().map(|i| i as f64))
}

/// The `bookmarks` array of band `band_id`, created when the band has none.
fn bookmarks_of<'a>(doc: &'a mut DocumentMut, band_id: &str) -> Result<&'a mut Array> {
    let bands = doc.get_mut("band").and_then(Item::as_array_of_tables_mut).context("no [[band]] tables")?;
    let band = bands.iter_mut().find(|b| b.get("id").and_then(Item::as_str) == Some(band_id)).with_context(|| format!("no band {band_id}"))?;
    if !band.contains_key("bookmarks") {
        let mut a = Array::new();
        a.set_trailing("\n");
        a.set_trailing_comma(true);
        band.insert("bookmarks", value(a));
    }
    band.get_mut("bookmarks").and_then(Item::as_array_mut).context("bookmarks is not an inline list")
}

/// Add or update a bookmark in the text of a bands file, keeping everything else (comments,
/// order, layout) as written. The entry is found by its frequency within `band_id`.
pub fn upsert_bookmark(text: &str, band_id: &str, b: &Bookmark) -> Result<String> {
    let mut doc: DocumentMut = text.parse()?;
    let list = bookmarks_of(&mut doc, band_id)?;
    let at = list.iter().position(|v| mhz_of(v).is_some_and(|m| same_mhz(m, b.mhz)));
    let multiline = list.is_empty() || list.iter().next().is_some_and(|v| v.decor().prefix().and_then(|p| p.as_str()).is_some_and(|s| s.contains('\n')));
    let mut fresh = InlineTable::new();
    let t = match at {
        Some(i) => list.get_mut(i).and_then(Value::as_inline_table_mut).context("bookmark is not a table")?,
        None => {
            fresh.insert("mhz", Value::from((b.mhz * 1e6).round() / 1e6));
            &mut fresh
        }
    };
    t.insert("name", Value::from(b.name.as_str()));
    match b.notes.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => t.insert("notes", Value::from(n)),
        None => t.remove("notes"),
    };
    match &b.mode {
        Some(m) => {
            let name = toml::Value::try_from(m)?;
            t.insert("mode", Value::from(name.as_str().unwrap_or_default()))
        }
        None => t.remove("mode"),
    };
    if at.is_none() {
        if multiline {
            fresh.decor_mut().set_prefix("\n  ");
            list.set_trailing("\n");
            list.set_trailing_comma(true);
        }
        list.push(fresh);
    }
    Ok(doc.to_string())
}

/// Remove the bookmark at `mhz` from band `band_id`; the text is unchanged when it is not there.
pub fn remove_bookmark(text: &str, band_id: &str, mhz: f64) -> Result<String> {
    let mut doc: DocumentMut = text.parse()?;
    let list = bookmarks_of(&mut doc, band_id)?;
    let at = list.iter().position(|v| mhz_of(v).is_some_and(|m| same_mhz(m, mhz)));
    if let Some(i) = at {
        list.remove(i);
    }
    Ok(doc.to_string())
}

/// Files from before `names` listed every shipped frequency as a bookmark, and a star (`fav`) or a
/// note was the only sign the user cared about one. Drops the bookmarks identical to a default name
/// and the `fav` and `pin` keys, leaving what the user starred, noted or wrote. `None` when nothing changes.
pub fn split_names(user: &str, defaults: &str) -> Result<Option<String>> {
    let defs = BandPlan::parse(defaults)?;
    let mut doc: DocumentMut = user.parse()?;
    let Some(bands) = doc.get_mut("band").and_then(Item::as_array_of_tables_mut) else {
        return Ok(None);
    };
    let mut changed = false;
    for band in bands.iter_mut() {
        let names = band.get("id").and_then(Item::as_str).and_then(|id| defs.bands.iter().find(|d| d.id == id)).map_or(&[][..], |d| &d.names);
        let Some(list) = band.get_mut("bookmarks").and_then(Item::as_array_mut) else {
            continue;
        };
        let shipped = |v: &Value| {
            let Some(t) = v.as_inline_table() else {
                return false;
            };
            !t.get("fav").and_then(Value::as_bool).unwrap_or(false)
                && !t.contains_key("notes")
                && names.iter().any(|n| mhz_of(v).is_some_and(|m| same_mhz(m, n.mhz)) && t.get("name").and_then(Value::as_str) == Some(n.name.as_str()))
        };
        for i in (0..list.len()).rev() {
            if list.get(i).is_some_and(shipped) {
                list.remove(i);
                changed = true;
            } else if let Some(t) = list.get_mut(i).and_then(Value::as_inline_table_mut) {
                changed |= t.remove("fav").is_some();
                changed |= t.remove("pin").is_some();
            }
        }
        if list.is_empty() {
            band.remove("bookmarks");
        }
    }
    Ok(changed.then(|| doc.to_string()))
}

/// Apply `edit` to the bands file and write it back through a temp file, so a crash never leaves half a file.
pub fn edit_file(edit: impl FnOnce(&str) -> Result<String>) -> Result<()> {
    let path = BandPlan::path().context("no config directory")?;
    let new = edit(&std::fs::read_to_string(&path)?)?;
    BandPlan::parse(&new).context("the edit would break the bands file")?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, new)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// `group:<id>` and `band:<id>` for every entry of a bands file, in file order.
fn default_ids(text: &str) -> Vec<String> {
    let Ok(plan) = BandPlan::parse(text) else { return Vec::new() };
    plan.groups.iter().map(|g| format!("group:{}", g.id)).chain(plan.bands.iter().map(|b| format!("band:{}", b.id))).collect()
}

/// The text of each `[[group]]` / `[[band]]` table in a file, keyed by kind and id.
fn blocks(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in text.lines() {
        let t = line.trim();
        let starts = t == "[[group]]" || t == "[[band]]";
        if starts || t.starts_with("# ──") {
            out.extend(cur.take());
        }
        if starts {
            cur = Some((t.trim_matches(['[', ']']).to_string(), String::new()));
        }
        if let Some((kind, body)) = &mut cur {
            body.push_str(line);
            body.push('\n');
            if let Some(id) = t.strip_prefix("id = ").filter(|_| !kind.contains(':')) {
                *kind = format!("{kind}:{}", id.trim_matches('"'));
            }
        }
    }
    out.extend(cur);
    out.into_iter().map(|(k, b)| (k, b.trim_end().to_string() + "\n")).collect()
}

/// Group names the defaults once shipped, by id. A user's group still wearing one takes the name the
/// defaults carry now (2026-11-05: "Listen" and "Ham" meant little to someone new to radio); a group
/// the user renamed is left alone.
const OLD_GROUP_NAMES: &[(&str, &str)] = &[("listen", "Listen"), ("ham", "Ham")];

/// Rename groups still carrying a name from `OLD_GROUP_NAMES` to the defaults' current name.
/// Returns the new text, or `None` when nothing changes.
pub fn rename_groups(user: &str, defaults: &str) -> Option<String> {
    let defs = BandPlan::parse(defaults).ok()?;
    let mut out = user.to_string();
    for (id, old) in OLD_GROUP_NAMES {
        let Some(now) = defs.groups.iter().find(|g| g.id == *id).map(|g| g.name.as_str()) else { continue };
        let was = format!("[[group]]\nid = \"{id}\"\nname = \"{old}\"\n");
        if now != *old && out.contains(&was) {
            out = out.replace(&was, &format!("[[group]]\nid = \"{id}\"\nname = \"{now}\"\n"));
        }
    }
    (out != user).then_some(out)
}

/// Add groups and bands that newer defaults have and the user's file lacks. A missing entry is
/// added when `seen` (what earlier runs offered) does not list it; before any record exists,
/// only whole groups the file lacks are added, since a lone missing band was likely deleted.
/// Returns the new text, or `None` when nothing changes or the result would not parse.
pub fn add_new_defaults(user: &str, defaults: &str, seen: Option<&HashSet<String>>) -> Option<String> {
    let mine: HashSet<String> = default_ids(user).into_iter().collect();
    if mine.is_empty() {
        return None;
    }
    let defs = BandPlan::parse(defaults).ok()?;
    let blocks: Vec<(String, String)> = blocks(defaults);
    let text_of = |key: &str| blocks.iter().find(|(k, _)| k == key).map(|(_, b)| b.clone());
    let new_group = |id: &str| {
        let key = format!("group:{id}");
        !mine.contains(&key) && seen.is_none_or(|s| !s.contains(&key))
    };
    let mut out = user.to_string();
    let mut added = false;
    for (i, g) in defs.groups.iter().enumerate() {
        if !new_group(&g.id) {
            continue;
        }
        let block = text_of(&format!("group:{}", g.id))? + "\n";
        // before the next default group the user has, so the rail keeps the defaults' order
        let at = defs.groups[i + 1..]
            .iter()
            .filter_map(|n| out.find(&format!("[[group]]\nid = \"{}\"", n.id)))
            .next()
            .or_else(|| out.find("[[band]]"));
        match at {
            Some(at) => out.insert_str(at, &block),
            None => out.push_str(&block),
        }
        added = true;
    }
    let mut tail = String::new();
    for b in &defs.bands {
        let key = format!("band:{}", b.id);
        let wanted = match seen {
            Some(s) => !s.contains(&key),
            None => new_group(&b.group),
        };
        if wanted && !mine.contains(&key) {
            tail.push('\n');
            tail.push_str(&text_of(&key)?);
        }
    }
    if !tail.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\n# ── Added from newer defaults ──────────────────────────\n");
        out.push_str(&tail);
        added = true;
    }
    (added && BandPlan::parse(&out).is_ok()).then_some(out)
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
            // the R820T tuner covers roughly 24–1766 MHz; below it direct sampling reads true to 14.4 MHz
            let direct = b.high_mhz * 1e6 <= crate::device::DIRECT_TOP_HZ as f64;
            assert!(direct || (b.low_mhz >= 24.0 && b.high_mhz <= 1766.0), "{} outside tuner range", b.id);
            for m in &b.names {
                assert!(m.mhz >= b.low_mhz && m.mhz <= b.high_mhz, "{} bookmark {} outside band", b.id, m.name);
            }
        }
        let mut ids: Vec<_> = plan.bands.iter().map(|b| &b.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), plan.bands.len(), "duplicate band id");
    }

    #[test]
    fn docs_example_parses() {
        let doc = include_str!("../../../docs/bands.md");
        let toml = doc.split("```toml\n").nth(1).and_then(|s| s.split("```").next()).unwrap();
        let plan = BandPlan::parse(toml).unwrap();
        assert_eq!(plan.bands[0].group, plan.groups[0].id);
        assert_eq!(plan.bands[0].names.len(), 2);
    }

    /// The defaults as an older install wrote them: no Public safety group.
    fn without_safety() -> String {
        let mut s = DEFAULT_BANDS.to_string();
        let g = s.find("[[group]]\nid = \"safety\"").unwrap();
        let h = s.find("[[group]]\nid = \"ham\"").unwrap();
        s.replace_range(g..h, "");
        let a = s.find("# ── Public safety").unwrap();
        let b = s.find("# ── Ham").unwrap();
        s.replace_range(a..b, "");
        s
    }

    #[test]
    fn adds_a_new_group_to_an_old_file() {
        let old = without_safety().replace("name = \"Railroad\"", "name = \"Trains\"");
        let new = add_new_defaults(&old, DEFAULT_BANDS, None).unwrap();
        let plan = BandPlan::parse(&new).unwrap();
        let groups: Vec<_> = plan.groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(groups[..3], ["listen", "safety", "ham"]);
        assert!(plan.bands.iter().any(|b| b.id == "ps700" && b.group == "safety"));
        assert!(plan.bands.iter().any(|b| b.name == "Trains"), "user edits kept");
        assert_eq!(add_new_defaults(&new, DEFAULT_BANDS, None), None, "second run changes nothing");
    }

    #[test]
    fn old_group_names_take_the_new_ones_once() {
        let old = DEFAULT_BANDS.replace("name = \"Everyday\"", "name = \"Listen\"").replace("name = \"Ham radio\"", "name = \"Ham\"");
        let new = rename_groups(&old, DEFAULT_BANDS).unwrap();
        let plan = BandPlan::parse(&new).unwrap();
        assert_eq!(plan.groups.iter().find(|g| g.id == "listen").unwrap().name, "Everyday");
        assert_eq!(plan.groups.iter().find(|g| g.id == "ham").unwrap().name, "Ham radio");
        assert_eq!(rename_groups(&new, DEFAULT_BANDS), None, "second run changes nothing");
        let mine = old.replace("name = \"Ham\"", "name = \"My repeaters\"");
        assert!(rename_groups(&mine, DEFAULT_BANDS).unwrap().contains("My repeaters"), "a renamed group stays");
    }

    #[test]
    fn deleted_entries_stay_deleted() {
        let all: HashSet<String> = default_ids(DEFAULT_BANDS).into_iter().collect();
        assert_eq!(add_new_defaults(&without_safety(), DEFAULT_BANDS, Some(&all)), None);
        // with no record yet, a lone missing band is taken as deleted
        let no_cb = DEFAULT_BANDS.replace("id = \"cb\"", "id = \"cb-mine\"");
        assert_eq!(add_new_defaults(&no_cb, DEFAULT_BANDS, None), None);
    }

    #[test]
    fn adds_a_band_new_since_the_last_run() {
        let mut seen: HashSet<String> = default_ids(DEFAULT_BANDS).into_iter().collect();
        seen.remove("band:ps800");
        let user = DEFAULT_BANDS.replace("id = \"ps800\"", "id = \"elsewhere\"");
        let new = add_new_defaults(&user, DEFAULT_BANDS, Some(&seen)).unwrap();
        assert!(BandPlan::parse(&new).unwrap().bands.iter().any(|b| b.id == "ps800"));
    }

    fn mark(mhz: f64, name: &str) -> Bookmark {
        Bookmark { mhz, name: name.into(), mode: None, notes: None }
    }

    fn bookmarks(text: &str, band: &str) -> Vec<Bookmark> {
        BandPlan::parse(text).unwrap().bands.into_iter().find(|b| b.id == band).unwrap().bookmarks
    }

    #[test]
    fn splitting_names_keeps_only_what_the_user_chose() {
        let defaults = "[[group]]\nid = \"g\"\nname = \"G\"\n\n[[band]]\nid = \"wx\"\ngroup = \"g\"\nname = \"W\"\nicon = \"cloud\"\nmode = \"nfm\"\nlow_mhz = 162.0\nhigh_mhz = 163.0\nstart_mhz = 162.4\nnames = [\n  { mhz = 162.4, name = \"WX1\" },\n  { mhz = 162.425, name = \"WX2\" },\n  { mhz = 162.45, name = \"WX3\" },\n  { mhz = 162.475, name = \"WX4\" },\n]\n";
        let user = defaults.replace("names = [", "# keep me\nbookmarks = [").replace(
            "{ mhz = 162.425, name = \"WX2\" }",
            "{ mhz = 162.425, name = \"WX2\", fav = true }",
        ).replace(
            "{ mhz = 162.45, name = \"WX3\" }",
            "{ mhz = 162.45, name = \"WX3\", notes = \"mine\", pin = false }",
        ).replace("{ mhz = 162.475, name = \"WX4\" }", "{ mhz = 162.475, name = \"Renamed\" }");
        let new = split_names(&user, defaults).unwrap().unwrap();
        assert!(new.contains("# keep me") && !new.contains("fav") && !new.contains("pin"));
        let got: Vec<String> = bookmarks(&new, "wx").into_iter().map(|b| b.name).collect();
        assert_eq!(got, ["WX2", "WX3", "Renamed"]);
        // a band left with no bookmarks loses the key
        assert!(!split_names(defaults.replace("names = [", "bookmarks = [").as_str(), defaults).unwrap().unwrap().contains("bookmarks"));
    }

    #[test]
    fn old_files_take_names_from_the_defaults() {
        let defaults = BandPlan::parse(DEFAULT_BANDS).unwrap();
        let mut plan = BandPlan::parse(&DEFAULT_BANDS.replace("names = [", "bookmarks = [")).unwrap();
        assert!(plan.bands.iter().all(|b| b.names.is_empty()));
        plan.add_default_names(&defaults);
        let marine = plan.bands.iter().find(|b| b.id == "marine").unwrap();
        assert!(marine.names.iter().any(|n| n.name == "Ch 16 distress"));
    }

    #[test]
    fn bookmark_edits_keep_comments_and_layout() {
        let text = DEFAULT_BANDS.replacen("[[band]]", "# my own comment\n[[band]]", 1);
        let text = upsert_bookmark(&text, "wx", &mark(162.4, "First")).unwrap();
        let before = bookmarks(&text, "wx").len();
        let mut b = mark(162.6, "Spotter net");
        b.notes = Some("storm \"spotter\" net".into());
        b.mode = Some(Mode::Nfm);
        let new = upsert_bookmark(&text, "wx", &b).unwrap();
        assert!(new.contains("# my own comment"));
        let got = bookmarks(&new, "wx");
        assert_eq!(got.len(), before + 1);
        let added = got.iter().find(|m| m.name == "Spotter net").unwrap();
        assert!(added.mode == Some(Mode::Nfm) && added.notes.as_deref() == Some("storm \"spotter\" net"));
        // the only new line is the bookmark
        assert_eq!(new.lines().count(), text.lines().count() + 1, "{new}");
        // editing in place keeps the count, and clearing fields drops their keys
        let again = upsert_bookmark(&new, "wx", &mark(162.6, "Spotter net 2")).unwrap();
        let got = bookmarks(&again, "wx");
        assert_eq!(got.len(), before + 1);
        let e = got.iter().find(|m| m.name == "Spotter net 2").unwrap();
        assert!(e.mode.is_none() && e.notes.is_none());
        // delete puts the file back
        assert_eq!(remove_bookmark(&again, "wx", 162.6).unwrap().lines().count(), text.lines().count());
        assert_eq!(bookmarks(&remove_bookmark(&again, "wx", 162.6).unwrap(), "wx").len(), before);
    }

    #[test]
    fn bookmark_goes_into_a_band_that_has_none() {
        let plan = BandPlan::parse(DEFAULT_BANDS).unwrap();
        let bare = plan.bands.iter().find(|b| b.bookmarks.is_empty()).unwrap();
        let new = upsert_bookmark(DEFAULT_BANDS, &bare.id, &mark(bare.start_mhz, "Mine")).unwrap();
        assert_eq!(bookmarks(&new, &bare.id)[0].name, "Mine");
        assert!(upsert_bookmark(DEFAULT_BANDS, "nope", &mark(bare.start_mhz, "x")).is_err());
    }

    #[test]
    fn narrowest_band_wins_on_overlap() {
        let plan = BandPlan::parse(DEFAULT_BANDS).unwrap();
        assert_eq!(plan.band_at(145.8).unwrap().id, "iss");
        assert_eq!(plan.band_at(146.52).unwrap().id, "2m");
    }
}
