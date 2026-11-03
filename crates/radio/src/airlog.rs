//! What aircraft were heard, kept across runs: append-only JSON lines, one file per UTC day, in
//! `~/.local/state/squelch/aircraft/`. The radio thread only sends records down a channel; an `airlog`
//! thread writes them, flushed per batch, so a crash loses at most the last batch. Whole days are the
//! unit of pruning: past `KEEP_DAYS`, or past `MAX_BYTES` oldest first.
//!
//! Lines are small on purpose (a busy day is many thousands): `k` says which kind, and readers fold
//! `a` and `p` lines into flights.

use crate::acars::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_DAYS: u64 = 60;
const MAX_BYTES: u64 = 500_000_000;
/// The same address heard again after this long is another flight.
const FLIGHT_GAP_MS: u64 = 30 * 60 * 1000;
const DAY_MS: u64 = 86_400_000;

/// What an aircraft says about itself, logged when it changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Attrs {
    #[serde(rename = "c", default, skip_serializing_if = "Option::is_none")]
    pub callsign: Option<String>,
    #[serde(rename = "g", default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(rename = "q", default, skip_serializing_if = "Option::is_none")]
    pub squawk: Option<String>,
    #[serde(rename = "e", default, skip_serializing_if = "Option::is_none")]
    pub emergency: Option<String>,
}

/// A point along a track.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    #[serde(rename = "t")]
    pub at_ms: u64,
    #[serde(rename = "la")]
    pub lat: f64,
    #[serde(rename = "lo")]
    pub lon: f64,
    #[serde(rename = "a", default, skip_serializing_if = "Option::is_none")]
    pub alt_ft: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k")]
pub enum Line {
    /// First heard, or what it says about itself changed.
    #[serde(rename = "a")]
    Attrs {
        #[serde(rename = "t")]
        at_ms: u64,
        #[serde(rename = "i")]
        icao: String,
        #[serde(flatten)]
        attrs: Attrs,
    },
    #[serde(rename = "p")]
    Point {
        #[serde(rename = "i")]
        icao: String,
        #[serde(flatten)]
        point: Point,
    },
    #[serde(rename = "m")]
    Message(Message),
}

impl Line {
    fn at_ms(&self) -> u64 {
        match self {
            Line::Attrs { at_ms, .. } => *at_ms,
            Line::Point { point, .. } => point.at_ms,
            Line::Message(m) => m.at_ms,
        }
    }
}

/// Everything known about one aircraft on one flight (a run of lines with no `FLIGHT_GAP_MS` gap).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Flight {
    pub icao: String,
    pub callsign: Option<String>,
    pub registration: Option<String>,
    pub country: Option<&'static str>,
    pub category: Option<String>,
    pub squawk: Option<String>,
    pub emergency: Option<String>,
    pub first_ms: u64,
    pub last_ms: u64,
    pub points: u32,
    pub max_alt_ft: Option<i32>,
    /// ACARS blocks from the same registration during the flight.
    pub messages: u32,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn dir() -> Option<PathBuf> {
    dirs::state_dir().or_else(dirs::cache_dir).map(|d| d.join("squelch").join("aircraft"))
}

/// "2026-11-03" for a time, UTC.
fn day_name(at_ms: u64) -> String {
    // civil-from-days, Howard Hinnant's algorithm
    let z = (at_ms / DAY_MS) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Sends records to the writing thread; dropping it ends and joins the thread.
pub struct Writer {
    tx: Option<Sender<Line>>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn start(dir: PathBuf) -> Writer {
        let (tx, rx) = channel::<Line>();
        let thread = std::thread::Builder::new()
            .name("airlog".into())
            .spawn(move || {
                let mut day = String::new();
                let mut out: Option<BufWriter<File>> = None;
                while let Ok(first) = rx.recv() {
                    let mut next = Some(first);
                    while let Some(line) = next.take() {
                        let name = day_name(line.at_ms());
                        if name != day || out.is_none() {
                            prune(&dir, KEEP_DAYS, MAX_BYTES, line.at_ms());
                            out = open_day(&dir, &name);
                            day = name;
                        }
                        if let (Some(o), Ok(s)) = (&mut out, serde_json::to_string(&line)) {
                            let _ = writeln!(o, "{s}");
                        }
                        next = rx.try_recv().ok();
                    }
                    if let Some(o) = &mut out {
                        let _ = o.flush();
                    }
                }
            })
            .ok();
        Writer { tx: Some(tx), thread }
    }

    pub fn send(&self, line: Line) {
        if let Some(t) = &self.tx {
            let _ = t.send(line);
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn open_day(dir: &Path, name: &str) -> Option<BufWriter<File>> {
    fs::create_dir_all(dir).ok()?;
    OpenOptions::new().create(true).append(true).open(dir.join(format!("{name}.jsonl"))).ok().map(BufWriter::new)
}

/// Day files, oldest first.
fn days(dir: &Path) -> Vec<(String, PathBuf, u64)> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let day = path.file_name()?.to_str()?.strip_suffix(".jsonl")?.to_string();
            Some((day, path, e.metadata().ok()?.len()))
        })
        .collect();
    out.sort();
    out
}

fn prune(dir: &Path, keep_days: u64, max_bytes: u64, now: u64) {
    let oldest = day_name(now.saturating_sub(keep_days * DAY_MS));
    let mut files = days(dir);
    let mut total: u64 = files.iter().map(|f| f.2).sum();
    // the newest file stays whatever its size: it is today's
    let last = files.pop();
    for (day, path, len) in files {
        if day < oldest || total > max_bytes {
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
    drop(last);
}

/// Each line since `since_ms`, oldest first; lines that do not parse (a torn last write) are skipped.
fn read_lines(dir: &Path, since_ms: u64, mut each: impl FnMut(Line)) {
    let first = day_name(since_ms);
    for (day, path, _) in days(dir) {
        if day < first {
            continue;
        }
        let Ok(f) = File::open(&path) else { continue };
        for l in BufReader::new(f).lines().map_while(Result::ok) {
            if let Ok(line) = serde_json::from_str::<Line>(&l) {
                if line.at_ms() >= since_ms {
                    each(line);
                }
            }
        }
    }
}

/// Flights with any line since `since_ms`, newest first.
pub fn flights(dir: &Path, since_ms: u64) -> Vec<Flight> {
    let mut done: Vec<Flight> = Vec::new();
    let mut open: HashMap<String, Flight> = HashMap::new();
    let mut messages: Vec<(u64, String)> = Vec::new();
    read_lines(dir, since_ms, |line| {
        let (icao, at) = match &line {
            Line::Attrs { icao, at_ms, .. } => (icao.clone(), *at_ms),
            Line::Point { icao, point } => (icao.clone(), point.at_ms),
            Line::Message(m) => {
                if !m.registration.is_empty() {
                    messages.push((m.at_ms, m.registration.clone()));
                }
                return;
            }
        };
        let f = open.entry(icao.clone()).or_insert_with(|| new_flight(&icao, at));
        if at.saturating_sub(f.last_ms) > FLIGHT_GAP_MS {
            let old = std::mem::replace(f, new_flight(&icao, at));
            done.push(old);
        }
        f.last_ms = f.last_ms.max(at);
        match line {
            Line::Attrs { attrs, .. } => {
                f.callsign = attrs.callsign.or(f.callsign.take());
                f.category = attrs.category.or(f.category.take());
                f.squawk = attrs.squawk.or(f.squawk.take());
                f.emergency = attrs.emergency.or(f.emergency.take());
            }
            Line::Point { point, .. } => {
                f.points += 1;
                if point.alt_ft > f.max_alt_ft {
                    f.max_alt_ft = point.alt_ft;
                }
            }
            Line::Message(_) => {}
        }
    });
    done.extend(open.into_values());
    for (at, reg) in messages {
        if let Some(f) = done.iter_mut().find(|f| f.registration.as_deref() == Some(reg.as_str()) && at + FLIGHT_GAP_MS >= f.first_ms && at <= f.last_ms + FLIGHT_GAP_MS) {
            f.messages += 1;
        }
    }
    done.sort_by_key(|f| std::cmp::Reverse(f.last_ms));
    done
}

fn new_flight(icao: &str, at: u64) -> Flight {
    let address = u32::from_str_radix(icao, 16).unwrap_or(0);
    Flight {
        icao: icao.to_string(),
        registration: crate::adsb::ident::registration(address),
        country: crate::adsb::ident::country(address),
        first_ms: at,
        last_ms: at,
        ..Default::default()
    }
}

/// The track of one address between two times, oldest first.
pub fn track(dir: &Path, icao: &str, from_ms: u64, to_ms: u64) -> Vec<Point> {
    let mut out = Vec::new();
    read_lines(dir, from_ms, |line| {
        if let Line::Point { icao: i, point } = line {
            if i == icao && point.at_ms <= to_ms {
                out.push(point);
            }
        }
    });
    out
}

/// Tracks of every address since `since_ms`, for the map's trails.
pub fn tracks(dir: &Path, since_ms: u64) -> HashMap<String, Vec<Point>> {
    let mut out: HashMap<String, Vec<Point>> = HashMap::new();
    read_lines(dir, since_ms, |line| {
        if let Line::Point { icao, point } = line {
            out.entry(icao).or_default().push(point);
        }
    });
    out
}

/// The last `max` ACARS blocks since `since_ms`, oldest first.
pub fn messages(dir: &Path, since_ms: u64, max: usize) -> Vec<Message> {
    let mut out = std::collections::VecDeque::with_capacity(max + 1);
    read_lines(dir, since_ms, |line| {
        if let Line::Message(m) = line {
            out.push_back(m);
            if out.len() > max {
                out.pop_front();
            }
        }
    });
    out.into()
}

/// Bytes kept and days covered, for the panel's foot.
pub fn size(dir: &Path) -> (u64, usize) {
    let f = days(dir);
    (f.iter().map(|d| d.2).sum(), f.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("squelch-airlog-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn pt(i: &str, at_ms: u64) -> Line {
        Line::Point { icao: i.into(), point: Point { at_ms, lat: 25.7, lon: -80.2, alt_ft: Some((at_ms / 1000) as i32) } }
    }

    fn attrs(i: &str, at_ms: u64, c: &str) -> Line {
        Line::Attrs { at_ms, icao: i.into(), attrs: Attrs { callsign: Some(c.into()), ..Default::default() } }
    }

    const T0: u64 = 1_793_600_000_000;

    #[test]
    fn day_names() {
        assert_eq!(day_name(0), "1970-01-01");
        assert_eq!(day_name(951_782_400_000), "2000-02-29");
        assert_eq!(day_name(1_793_664_000_000), "2026-11-03");
    }

    #[test]
    fn writes_survive_and_fold_into_flights() {
        let d = tmp("fold");
        {
            let w = Writer::start(d.clone());
            w.send(attrs("A835AF", T0, "N628TS"));
            w.send(pt("A835AF", T0 + 1000));
            w.send(pt("A835AF", T0 + 2000));
            // same address after the gap: another flight, and it has lost the callsign
            w.send(pt("A835AF", T0 + 2000 + FLIGHT_GAP_MS + 1));
            w.send(Line::Message(Message { at_ms: T0 + 1500, registration: "N628TS".into(), text: "hi".into(), ..Default::default() }));
        }
        let f = flights(&d, T0 - 1);
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(f[0].callsign, None);
        assert_eq!(f[1].callsign.as_deref(), Some("N628TS"));
        assert_eq!(f[1].registration.as_deref(), Some("N628TS"));
        assert_eq!(f[1].country, Some("United States"));
        assert_eq!(f[1].points, 2);
        assert_eq!(f[1].messages, 1);
        assert_eq!(track(&d, "A835AF", T0, T0 + 5000).len(), 2);
        assert_eq!(messages(&d, T0, 10)[0].text, "hi");
        assert_eq!(tracks(&d, T0)["A835AF"].len(), 3);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_torn_line_is_skipped_and_writing_continues() {
        let d = tmp("torn");
        {
            let w = Writer::start(d.clone());
            w.send(pt("ABC123", T0));
        }
        let file = d.join(format!("{}.jsonl", day_name(T0)));
        let mut f = OpenOptions::new().append(true).open(&file).unwrap();
        write!(f, "{{\"k\":\"p\",\"i\":\"ABC").unwrap();
        writeln!(f).unwrap();
        drop(f);
        {
            let w = Writer::start(d.clone());
            w.send(pt("ABC123", T0 + 1000));
        }
        assert_eq!(track(&d, "ABC123", T0, T0 + 5000).len(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn prunes_old_days_and_over_size_but_never_today() {
        let d = tmp("prune");
        fs::create_dir_all(&d).unwrap();
        for (name, size) in [("2020-01-01", 10), ("2026-11-01", 100), ("2026-11-02", 100), ("2026-11-03", 500)] {
            fs::write(d.join(format!("{name}.jsonl")), vec![b'x'; size]).unwrap();
        }
        prune(&d, 60, 650, 1_793_664_000_000);
        let left: Vec<String> = days(&d).into_iter().map(|f| f.0).collect();
        assert_eq!(left, ["2026-11-02", "2026-11-03"]);
        prune(&d, 60, 10, 1_793_664_000_000);
        assert_eq!(days(&d).len(), 1);
        fs::remove_dir_all(&d).unwrap();
    }
}
