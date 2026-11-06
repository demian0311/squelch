//! What the user keeps as files: the schedule and settings as TOML in the config directory (theirs to
//! edit, version and share), and what the app remembers for this machine as JSON in the state directory.
//! Saves edit the TOML in place, so comments and layout stay, and a file that does not parse is never overwritten.
use crate::demod::Mode;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

const SCHEDULE_HEADER: &str = "# Times to tune in. Squelch rereads this file when it changes.
# time is local; times = [\"09:00\", \"21:00\"] for several a day. days = [\"mon\", \"wed\"] repeats weekly; without days,
# date = \"2026-11-08\" is once. week = 1 to 4, or 5 for the last, keeps only that week of the month.
# minutes is how long it runs (60). action is remind, tune or record; go_back = true returns to the earlier frequency.
# pass marks a slot booked from a space station pass; the app moves it when the orbit data changes.
#
# [[slot]]
# name = \"Tuesday net\"
# mhz = 146.52
# mode = \"nfm\"
# time = \"19:30\"
# days = [\"tue\"]
#
# [[slot]]
# name = \"3-3-3\"
# mhz = 462.6125
# times = [\"00:00\", \"03:00\", \"06:00\", \"09:00\", \"12:00\", \"15:00\", \"18:00\", \"21:00\"]
# days = [\"sun\", \"mon\", \"tue\", \"wed\", \"thu\", \"fri\", \"sat\"]
# minutes = 3
# action = \"tune\"

";

const SETTINGS_HEADER: &str = "# Squelch settings. Squelch rereads this file when it changes; waterfall applies at the next start.
# location = \"80203\"     # ZIP code, state, or \"lat, lon\"
# squelch = 12           # dB above the noise floor, 0 = off
# crystal_ppm = 57       # set by hand; without it the dongle's error is measured
# show_dead_air = false  # list bands that rarely have signals
# waterfall = \"cpu\"      # force the 2D renderer

";

/// What happens when a time to tune in starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[default]
    Remind,
    Tune,
    Record,
}

const DEFAULT_MINUTES: u32 = 60;

fn default_minutes() -> u32 {
    DEFAULT_MINUTES
}

/// A time to tune in. `at` is its position in the file when read, none for a new one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    #[serde(default)]
    pub at: Option<usize>,
    pub name: String,
    pub mhz: f64,
    pub mode: Mode,
    /// Local "HH:MM", at least one.
    pub times: Vec<String>,
    /// 0 = Sunday. Empty = once, on `date`.
    pub days: Vec<u8>,
    /// Local "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Only this week of the month, 1 to 4, or 5 for the last. None = every week.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week: Option<u8>,
    #[serde(default = "default_minutes")]
    pub minutes: u32,
    #[serde(default)]
    pub action: Action,
    /// When it ends, back to the frequency it took over from.
    #[serde(default)]
    pub go_back: bool,
    /// Booked from a space station pass: the pass's predicted rise (Unix seconds) when it was booked,
    /// so the page can move the slot when fresh orbit data moves the pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSlot {
    #[serde(default)]
    name: String,
    mhz: f64,
    #[serde(default = "default_mode")]
    mode: Mode,
    time: Option<String>,
    times: Option<Vec<String>>,
    #[serde(default)]
    days: Vec<String>,
    date: Option<String>,
    week: Option<u8>,
    #[serde(default = "default_minutes")]
    minutes: u32,
    #[serde(default)]
    action: Action,
    #[serde(default)]
    go_back: bool,
    pass: Option<i64>,
}

fn default_mode() -> Mode {
    Mode::Nfm
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleFile {
    #[serde(default)]
    slot: Vec<FileSlot>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub location: Option<String>,
    pub squelch: Option<f64>,
    /// Set by hand; without it the engine's measurement applies.
    pub crystal_ppm: Option<i64>,
    pub show_dead_air: Option<bool>,
    pub waterfall: Option<String>,
}

fn config_file(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("squelch").join(name))
}

pub fn schedule_path() -> Option<PathBuf> {
    config_file("schedule.toml")
}

pub fn settings_path() -> Option<PathBuf> {
    config_file("settings.toml")
}

pub fn state_path() -> Option<PathBuf> {
    dirs::state_dir().or_else(dirs::cache_dir).map(|d| d.join("squelch").join("state.json"))
}

fn is_time(t: &str) -> bool {
    matches!(t.split_once(':'), Some((h, m)) if (1..=2).contains(&h.len()) && m.len() == 2 && h.parse::<u8>().is_ok_and(|h| h < 24) && m.parse::<u8>().is_ok_and(|m| m < 60))
}

fn is_date(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

fn day_number(name: &str) -> Option<u8> {
    DAYS.iter().position(|n| name.eq_ignore_ascii_case(n)).map(|p| p as u8)
}

pub fn parse_schedule(text: &str) -> Result<Vec<Slot>> {
    let file: ScheduleFile = toml::from_str(text)?;
    file.slot
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let at = i + 1;
            let times: Vec<String> = s.time.into_iter().chain(s.times.into_iter().flatten()).collect();
            if times.is_empty() {
                bail!("slot {at}: needs a time");
            }
            if let Some(t) = times.iter().find(|t| !is_time(t)) {
                bail!("slot {at}: time {t:?} is not HH:MM");
            }
            if s.week.is_some_and(|w| !(1..=5).contains(&w)) {
                bail!("slot {at}: week is 1 to 4, or 5 for the last");
            }
            if s.minutes == 0 {
                bail!("slot {at}: minutes must be above 0");
            }
            if let Some(d) = s.date.as_deref().filter(|d| !is_date(d)) {
                bail!("slot {at}: date {d:?} is not YYYY-MM-DD");
            }
            let days = s
                .days
                .iter()
                .map(|d| day_number(d).with_context(|| format!("slot {at}: day {d:?} is not one of {DAYS:?}")))
                .collect::<Result<_>>()?;
            Ok(Slot { at: Some(i), name: s.name, mhz: s.mhz, mode: s.mode, times, days, date: s.date, week: s.week, minutes: s.minutes, action: s.action, go_back: s.go_back, pass: s.pass })
        })
        .collect()
}

pub fn parse_settings(text: &str) -> Result<Settings> {
    Ok(toml::from_str(text)?)
}

fn read_or(path: Option<PathBuf>, default: &str) -> Result<(PathBuf, String)> {
    let path = path.context("no config directory")?;
    match std::fs::read_to_string(&path) {
        Ok(t) => Ok((path, t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((path, default.to_string())),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}

pub fn load_schedule() -> Result<Vec<Slot>> {
    parse_schedule(&read_or(schedule_path(), "")?.1)
}

pub fn load_settings() -> Result<Settings> {
    parse_settings(&read_or(settings_path(), "")?.1)
}

/// Write through a temp file, so a crash never leaves half a file.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn save_schedule(slots: &[Slot]) -> Result<Vec<Slot>> {
    let (path, text) = read_or(schedule_path(), SCHEDULE_HEADER)?;
    let new = edit_schedule(&text, slots)?;
    write_file(&path, &new)?;
    parse_schedule(&new)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let (path, text) = read_or(settings_path(), SETTINGS_HEADER)?;
    let new = edit_settings(&text, settings)?;
    if new != text {
        write_file(&path, &new)?;
    }
    Ok(())
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(a), Value::String(b)) => a.value() == b.value(),
        (Value::Float(a), Value::Float(b)) => a.value() == b.value(),
        (Value::Integer(a), Value::Integer(b)) => a.value() == b.value(),
        (Value::Integer(a), Value::Float(b)) | (Value::Float(b), Value::Integer(a)) => *a.value() as f64 == *b.value(),
        (Value::Boolean(a), Value::Boolean(b)) => a.value() == b.value(),
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| same(a, b)),
        _ => false,
    }
}

/// Set a key, leaving it as written (number format, trailing comment) when the value is the same; `None` removes it.
fn put(t: &mut Table, key: &str, v: Option<Value>) {
    let Some(mut v) = v else {
        t.remove(key);
        return;
    };
    match t.get_mut(key) {
        Some(item) => {
            if let Some(old) = item.as_value() {
                if same(old, &v) {
                    return;
                }
                *v.decor_mut() = old.decor().clone();
            }
            *item = Item::Value(v);
        }
        None => {
            t.insert(key, Item::Value(v));
        }
    }
}

fn set_slot(t: &mut Table, s: &Slot) -> Result<()> {
    // a key left out by hand stays out while it still reads the same
    let mode = (t.contains_key("mode") || s.mode != default_mode()).then(|| toml::Value::try_from(s.mode)).transpose()?;
    put(t, "name", (!s.name.is_empty()).then(|| s.name.as_str().into()));
    put(t, "mhz", Some(s.mhz.into()));
    put(t, "mode", mode.map(|m| m.as_str().unwrap_or_default().into()));
    // one time reads as `time`, several as `times`
    match s.times.as_slice() {
        [one] => {
            put(t, "time", Some(one.as_str().into()));
            t.remove("times");
        }
        many => {
            let times: Array = many.iter().map(String::as_str).collect();
            put(t, "times", Some(times.into()));
            t.remove("time");
        }
    }
    let written = t.get("days").and_then(Item::as_array).map(|a| a.iter().filter_map(|d| day_number(d.as_str()?)).collect::<Vec<u8>>());
    if written.as_ref() != Some(&s.days) {
        let days: Array = s.days.iter().filter_map(|d| DAYS.get(*d as usize).copied()).collect();
        put(t, "days", (!days.is_empty()).then(|| days.into()));
    }
    put(t, "date", s.date.as_deref().filter(|d| !d.is_empty()).map(Value::from));
    put(t, "week", s.week.map(|w| i64::from(w).into()));
    put(t, "minutes", (t.contains_key("minutes") || s.minutes != DEFAULT_MINUTES).then(|| i64::from(s.minutes).into()));
    let action = (t.contains_key("action") || s.action != Action::default()).then(|| toml::Value::try_from(s.action)).transpose()?;
    put(t, "action", action.map(|a| a.as_str().unwrap_or_default().into()));
    put(t, "go_back", s.go_back.then(|| true.into()));
    put(t, "pass", s.pass.map(Value::from));
    Ok(())
}

/// The schedule file with its slots made `slots`. A slot with a position in the file keeps that
/// table and its comments. The comment heading the file stays at the head whichever slot is first.
pub fn edit_schedule(text: &str, slots: &[Slot]) -> Result<String> {
    parse_schedule(text).context("the schedule file does not parse, so it was left alone")?;
    let mut doc: DocumentMut = text.parse()?;
    let old: Vec<Table> = doc.get("slot").and_then(Item::as_array_of_tables).map(|a| a.iter().cloned().collect()).unwrap_or_default();
    let head = match old.first() {
        Some(t) => t.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string(),
        None => doc.trailing().as_str().unwrap_or("").to_string(),
    };
    let mut out = ArrayOfTables::new();
    for s in slots {
        let mut t = s.at.and_then(|i| old.get(i).cloned()).unwrap_or_else(|| {
            let mut t = Table::new();
            t.decor_mut().set_prefix("\n");
            t
        });
        set_slot(&mut t, s)?;
        out.push(t);
    }
    if let Some(first) = out.get_mut(0) {
        first.decor_mut().set_prefix(head);
        if old.is_empty() {
            doc.set_trailing("");
        }
        doc.insert("slot", Item::ArrayOfTables(out));
    } else {
        doc.remove("slot");
        if !old.is_empty() {
            doc.set_trailing(head);
        }
    }
    let new = doc.to_string();
    parse_schedule(&new).context("the edit would break the schedule file")?;
    Ok(new)
}

/// The settings file with its keys made `s`: `None` removes a key, an empty location too.
pub fn edit_settings(text: &str, s: &Settings) -> Result<String> {
    parse_settings(text).context("the settings file does not parse, so it was left alone")?;
    let mut doc: DocumentMut = text.parse()?;
    let t = doc.as_table_mut();
    let first = t.iter().next().map(|(k, _)| k.to_string());
    let head = match &first {
        Some(k) => t.key(k).and_then(|k| k.leaf_decor().prefix()).and_then(|p| p.as_str()).unwrap_or("").to_string(),
        None => String::new(),
    };
    put(t, "location", s.location.as_deref().filter(|l| !l.is_empty()).map(Value::from));
    put(t, "squelch", s.squelch.map(|v| if v.fract() == 0.0 { Value::from(v as i64) } else { Value::from(v) }));
    put(t, "crystal_ppm", s.crystal_ppm.map(Value::from));
    put(t, "show_dead_air", s.show_dead_air.map(Value::from));
    put(t, "waterfall", s.waterfall.as_deref().map(Value::from));
    // the comment heading the file stays at the head whichever key is first
    let now_first = t.iter().next().map(|(k, _)| k.to_string());
    let head = if first.is_some() { head } else { doc.trailing().as_str().unwrap_or("").to_string() };
    let t = doc.as_table_mut();
    match now_first {
        Some(k) => {
            if let Some(mut k) = t.key_mut(&k) {
                k.leaf_decor_mut().set_prefix(head);
            }
            if first.is_none() {
                doc.set_trailing("");
            }
        }
        None if first.is_some() => doc.set_trailing(head),
        None => {}
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(name: &str) -> Slot {
        Slot { at: None, name: name.into(), mhz: 146.52, mode: Mode::Nfm, times: vec!["19:30".into()], days: vec![2, 4], date: None, week: None, minutes: 60, action: Action::Remind, go_back: false, pass: None }
    }

    const FILE: &str = "# my nets

# the good one
[[slot]]
name = \"Tuesday\" # with Bob
mhz = 146.520
time = \"19:30\"
days = [\"tue\", \"thu\"]

# the other one
[[slot]]
name = \"Sunday\"
mhz = 147.0
mode = \"nfm\"
time = \"9:00\"
days = [\"SUN\"]
";

    #[test]
    fn reads_days_by_name_and_numbers_slots_by_position() {
        let s = parse_schedule(FILE).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].at, s[0].days.as_slice(), s[0].mode), (Some(0), &[2u8, 4][..], Mode::Nfm));
        assert_eq!((s[1].at, s[1].days.as_slice()), (Some(1), &[0u8][..]));
    }

    #[test]
    fn a_plan_with_several_times_round_trips() {
        let plan = Slot { times: vec!["00:00".into(), "03:00".into()], days: vec![0, 1, 2, 3, 4, 5, 6], minutes: 3, action: Action::Tune, go_back: true, week: Some(1), ..slot("3-3-3") };
        let text = edit_schedule("", &[plan.clone()]).unwrap();
        assert!(text.contains("times = [\"00:00\", \"03:00\"]") && !text.contains("time ="), "{text}");
        assert!(text.contains("action = \"tune\"") && text.contains("go_back = true") && text.contains("minutes = 3") && text.contains("week = 1"), "{text}");
        let read = parse_schedule(&text).unwrap();
        assert_eq!(read[0], Slot { at: Some(0), ..plan });
        // back to one time and the defaults: only `time` is left
        let plain = edit_schedule(&text, &[Slot { at: Some(0), ..slot("3-3-3") }]).unwrap();
        assert!(plain.contains("time = \"19:30\"") && !plain.contains("times") && !plain.contains("go_back") && !plain.contains("week"), "{plain}");
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for bad in ["[[slot]]\nmhz = 1.0", "[[slot]]\nmhz = 1.0\ntime = \"9:00\"\nweek = 6", "[[slot]]\nmhz = 1.0\ntime = \"9:00\"\nminutes = 0", "[[slot]]\nmhz = 1.0\ntime = \"9:00\"\naction = \"dance\"", "[[slot]]\nmhz = 1.0\ntime = \"25:00\"", "[[slot]]\nmhz = 1.0\ntime = \"9:00\"\ndays = [\"funday\"]", "[[slot]]\nmhz = 1.0\ntime = \"9:00\"\nwhen = 1", "nope"] {
            assert!(parse_schedule(bad).is_err(), "{bad}");
        }
        assert!(edit_schedule("nope", &[]).is_err());
        assert!(edit_settings("sqelch = 1", &Settings::default()).is_err());
    }

    #[test]
    fn saving_what_was_read_changes_nothing() {
        assert_eq!(edit_schedule(FILE, &parse_schedule(FILE).unwrap()).unwrap(), FILE);
    }

    #[test]
    fn an_edit_keeps_comments_and_touches_only_that_key() {
        let mut s = parse_schedule(FILE).unwrap();
        s[1].times = vec!["10:15".into()];
        let new = edit_schedule(FILE, &s).unwrap();
        assert_eq!(new, FILE.replace("\"9:00\"", "\"10:15\""));
    }

    #[test]
    fn a_new_slot_goes_last_and_a_deleted_one_takes_its_comment() {
        let mut s = parse_schedule(FILE).unwrap();
        s.push(slot("New"));
        let added = edit_schedule(FILE, &s).unwrap();
        assert!(added.starts_with(FILE));
        assert_eq!(parse_schedule(&added).unwrap()[2].name, "New");

        let s = parse_schedule(FILE).unwrap();
        let one = edit_schedule(FILE, &s[1..]).unwrap();
        assert!(!one.contains("Tuesday"));
        assert!(one.starts_with("# my nets"));
        assert_eq!(parse_schedule(&one).unwrap()[0].name, "Sunday");
    }

    #[test]
    fn the_header_survives_an_empty_list() {
        let first = edit_schedule(SCHEDULE_HEADER, &[slot("Net")]).unwrap();
        assert!(first.starts_with(SCHEDULE_HEADER));
        assert_eq!(parse_schedule(&first).unwrap()[0].name, "Net");
        let empty = edit_schedule(&first, &[]).unwrap();
        assert_eq!(empty.trim(), SCHEDULE_HEADER.trim());
        assert!(parse_schedule(&empty).unwrap().is_empty());
    }

    #[test]
    fn settings_keep_comments_and_drop_unset_keys() {
        let text = "# mine\nlocation = \"80203\" # home\nsquelch = 12\n";
        let same = Settings { location: Some("80203".into()), squelch: Some(12.0), ..Default::default() };
        assert_eq!(parse_settings(text).unwrap(), same);
        assert_eq!(edit_settings(text, &same).unwrap(), text);
        let new = edit_settings(text, &Settings { location: Some("80023".into()), crystal_ppm: Some(57), ..Default::default() }).unwrap();
        assert_eq!(new, "# mine\nlocation = \"80023\" # home\ncrystal_ppm = 57\n");
    }

    #[test]
    fn the_settings_header_stays_at_the_head() {
        let one = edit_settings(SETTINGS_HEADER, &Settings { squelch: Some(20.0), ..Default::default() }).unwrap();
        assert!(one.starts_with(SETTINGS_HEADER));
        assert!(one.ends_with("squelch = 20\n"));
        let two = edit_settings(&one, &Settings { location: Some("80203".into()), ..Default::default() }).unwrap();
        assert!(two.starts_with(SETTINGS_HEADER));
        assert_eq!(parse_settings(&two).unwrap().location.as_deref(), Some("80203"));
        let none = edit_settings(&two, &Settings::default()).unwrap();
        assert_eq!(none.trim(), SETTINGS_HEADER.trim());
    }
}
