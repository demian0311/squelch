// The window shell: wires the radio engine to the web UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod feed;

use feed::Feed;
use radio::bands::{BandPlan, Bookmark};
use radio::demod::Mode;
use radio::device::Gain;
use radio::engine::{Command, Engine, Status, BELOW_FLOOR_DB, FFT_SIZE, RANGE_DB, SAMPLE_RATE};
use radio::prefs::{self, Slot};
use radio::repeaters::{in_state, nearby, within, Repeater, Repeaters, MAP_KM, NEARBY_KM};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use base64::Engine as _;
use tauri::{Emitter, State};

#[derive(Default)]
struct App {
    engine: Mutex<Option<Engine>>,
    feed: Arc<Feed>,
    /// Last gain, volume and squelch asked for, replayed on each start: the UI sends them before the engine exists.
    settings: Mutex<Settings>,
}

#[derive(Default)]
struct Settings {
    gain: Option<Command>,
    volume: Option<Command>,
    squelch: Option<Command>,
    ppm: Option<Command>,
}

impl Settings {
    fn commands(&self) -> impl Iterator<Item = Command> + '_ {
        [&self.gain, &self.volume, &self.squelch, &self.ppm].into_iter().flatten().cloned()
    }
}

#[derive(Serialize)]
struct Setup {
    sample_rate: u32,
    fft_size: usize,
    below_floor_db: f32,
    range_db: f32,
}

#[derive(Serialize, Clone)]
struct Bands {
    plan: BandPlan,
    path: Option<String>,
    error: Option<String>,
}

fn load_bands() -> Bands {
    match BandPlan::load() {
        Ok((plan, path)) => Bands { plan, path: path.map(|p| p.display().to_string()), error: None },
        Err(e) => Bands {
            plan: BandPlan::parse(radio::bands::DEFAULT_BANDS).expect("default bands parse"),
            path: BandPlan::path().map(|p| p.display().to_string()),
            error: Some(format!("{e:#}")),
        },
    }
}

#[tauri::command]
fn setup() -> Setup {
    Setup { sample_rate: SAMPLE_RATE, fft_size: FFT_SIZE, below_floor_db: BELOW_FLOOR_DB, range_db: RANGE_DB }
}

#[tauri::command]
fn bands() -> Bands {
    load_bands()
}

/// Add or change a bookmark in the bands file (found by its frequency in band `band`), keeping the
/// rest of the file as the user wrote it. Returns the list as now read, so the page need not wait
/// for the file poll.
#[tauri::command]
fn save_bookmark(band: String, bookmark: Bookmark) -> Result<Bands, String> {
    radio::bands::edit_file(|t| radio::bands::upsert_bookmark(t, &band, &bookmark)).map_err(|e| format!("{e:#}"))?;
    Ok(load_bands())
}

#[tauri::command]
fn delete_bookmark(band: String, mhz: f64) -> Result<Bands, String> {
    radio::bands::edit_file(|t| radio::bands::remove_bookmark(t, &band, mhz)).map_err(|e| format!("{e:#}"))?;
    Ok(load_bands())
}

#[derive(Serialize, Clone)]
struct ScheduleFile {
    slots: Vec<Slot>,
    path: Option<String>,
    error: Option<String>,
}

fn load_schedule() -> ScheduleFile {
    let path = prefs::schedule_path().map(|p| p.display().to_string());
    match prefs::load_schedule() {
        Ok(slots) => ScheduleFile { slots, path, error: None },
        Err(e) => ScheduleFile { slots: vec![], path, error: Some(format!("schedule file: {e:#}")) },
    }
}

#[derive(Serialize, Clone)]
struct SettingsFile {
    settings: prefs::Settings,
    path: Option<String>,
    error: Option<String>,
}

fn load_settings() -> SettingsFile {
    let path = prefs::settings_path().map(|p| p.display().to_string());
    match prefs::load_settings() {
        Ok(settings) => SettingsFile { settings, path, error: None },
        Err(e) => SettingsFile { settings: Default::default(), path, error: Some(format!("settings file: {e:#}")) },
    }
}

#[tauri::command]
fn schedule() -> ScheduleFile {
    load_schedule()
}

/// Returns the list as now read, with new slots' places in the file.
#[tauri::command]
fn save_schedule(slots: Vec<Slot>) -> Result<ScheduleFile, String> {
    prefs::save_schedule(&slots).map_err(|e| format!("{e:#}"))?;
    Ok(load_schedule())
}

/// Wake the computer at this time (seconds since the epoch), or at no time. The error says why it can't.
#[tauri::command]
fn set_wake(at: Option<i64>) -> Result<(), String> {
    radio::wake::set_alarm(radio::wake::ALARM, at).map_err(|e| format!("{e:#}"))
}

/// The command that lets Squelch set the wake alarm, ready to paste in a terminal.
#[tauri::command]
fn wake_setup_command() -> String {
    radio::wake::setup_command()
}

/// Keep the computer awake for this many seconds from now; 0 lets it go.
#[tauri::command]
fn hold_awake(secs: u32) {
    if let Err(e) = radio::wake::hold(secs) {
        eprintln!("hold awake: {e:#}");
    }
}

#[tauri::command]
fn settings() -> SettingsFile {
    load_settings()
}

#[tauri::command]
fn save_settings(settings: prefs::Settings) -> Result<(), String> {
    prefs::save_settings(&settings).map_err(|e| format!("{e:#}"))
}

/// What the app remembers for this machine, as the page wrote it; empty before the first save.
#[tauri::command]
fn read_state() -> String {
    prefs::state_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()
}

#[tauri::command]
fn save_state(json: String) -> Result<(), String> {
    let path = prefs::state_path().ok_or("no state directory")?;
    prefs::write_file(&path, &json).map_err(|e| format!("{e:#}"))
}

/// A desktop notification, best effort: no `notify-send`, no notice.
#[tauri::command]
fn notify(title: String, body: String) {
    let _ = std::process::Command::new("notify-send").args(["--app-name=Squelch", "--urgency=normal", &title, &body]).spawn().map(|mut c| std::thread::spawn(move || c.wait()));
}

/// Repeaters near `location` (ZIP code, state or "lat, lon"), none without one; a state keeps its own.
/// A broken repeaters file just means no repeater names, not a broken receiver.
#[tauri::command]
async fn repeaters(location: String) -> Vec<Repeater> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(s) = radio::iss::parse_site(&location) else { return vec![] };
        let list = Repeaters::shared().unwrap_or_else(|e| {
            eprintln!("repeaters: {e:#}");
            Default::default()
        });
        match radio::iss::place::state_id(&location) {
            Some(id) => nearby(in_state(&list, id), Some((s.lat, s.lon)), f64::INFINITY),
            None => nearby(list.iter(), Some((s.lat, s.lon)), NEARBY_KM),
        }
    })
    .await
    .unwrap_or_default()
}

/// What the repeater map draws: further out than the channel plan, and every repeater on a shared frequency.
#[tauri::command]
async fn repeaters_map(location: String) -> Vec<Repeater> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(s) = radio::iss::parse_site(&location) else { return vec![] };
        let list = Repeaters::shared().unwrap_or_default();
        match radio::iss::place::state_id(&location) {
            Some(id) => within(in_state(&list, id), (s.lat, s.lon), f64::INFINITY),
            None => within(list.iter(), (s.lat, s.lon), MAP_KM),
        }
    })
    .await
    .unwrap_or_default()
}

/// FM stations near `location`, from the FCC's list bundled in the app; none without a location.
#[tauri::command]
fn airports_near(lat: f64, lon: f64) -> Vec<radio::airports::Airport> {
    radio::airports::near(lat, lon)
}

#[tauri::command]
fn airports_by_code(codes: Vec<String>) -> Vec<radio::airports::Airport> {
    radio::airports::by_code(&codes)
}

#[tauri::command]
fn fm_stations(location: String) -> Vec<radio::fm_list::FmStation> {
    let Some(s) = radio::iss::parse_site(&location) else { return vec![] };
    radio::fm_list::near(s.lat, s.lon, radio::iss::place::state_code(&location))
}

#[tauri::command]
fn theme() -> BTreeMap<String, String> {
    radio::theme::load()
}

/// Start the engine. The page pulls its spectrum rows and status with `feed`.
#[tauri::command]
fn start(app: State<App>) {
    let mut slot = app.engine.lock().unwrap();
    *slot = None; // stop any earlier engine first so the dongle is free
    let (src, why) = Engine::open_source();
    let (rows, status) = (app.feed.clone(), app.feed.clone());
    let engine = Engine::start(
        src,
        why,
        move |row| {
            let mut bytes = Vec::with_capacity(8 + row.bins.len());
            bytes.extend_from_slice(&row.center_hz.to_le_bytes());
            bytes.extend_from_slice(&row.bins);
            rows.push_row(bytes);
        },
        move |s: Status| {
            if let Ok(json) = serde_json::to_vec(&s) {
                status.set_status(json);
            }
        },
    );
    for cmd in app.settings.lock().unwrap().commands() {
        engine.send(cmd);
    }
    *slot = Some(engine);
}

/// How long `feed` waits for something new before answering empty.
const FEED_WAIT: Duration = Duration::from_millis(500);

/// What the engine produced since the last call, in the layout `Feed::take` describes; each row is an f64 centre (little endian) then one byte per bin.
/// It waits on a blocking thread, so a slow page simply calls less often. Base64, because Tauri's Linux IPC turns a raw
/// body into a JSON array of numbers, several times the size and slow to parse.
#[tauri::command]
async fn feed(app: State<'_, App>) -> Result<String, String> {
    let feed = app.feed.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || feed.take(FEED_WAIT)).await.map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn send(app: &State<App>, cmd: Command) {
    if let Some(e) = app.engine.lock().unwrap().as_ref() {
        e.send(cmd);
    }
}

#[tauri::command]
fn tune(app: State<App>, center_hz: f64, tuned_hz: f64) {
    send(&app, Command::Tune { center_hz, tuned_hz });
}

#[tauri::command]
fn set_mode(app: State<App>, mode: Mode) {
    send(&app, Command::SetMode(mode));
}

/// `tenths` of a dB, or null for automatic gain.
#[tauri::command]
fn set_gain(app: State<App>, tenths: Option<i32>) {
    let cmd = Command::SetGain(tenths.map(Gain::Tenths).unwrap_or(Gain::Auto));
    app.settings.lock().unwrap().gain = Some(cmd.clone());
    send(&app, cmd);
}

#[tauri::command]
fn set_volume(app: State<App>, volume: f32) {
    app.settings.lock().unwrap().volume = Some(Command::SetVolume(volume));
    send(&app, Command::SetVolume(volume));
}

#[tauri::command]
fn set_squelch(app: State<App>, db: Option<f32>) {
    app.settings.lock().unwrap().squelch = Some(Command::SetSquelch(db));
    send(&app, Command::SetSquelch(db));
}

/// Crystal correction, positive when the crystal runs fast.
#[tauri::command]
fn set_ppm(app: State<App>, ppm: i32) {
    app.settings.lock().unwrap().ppm = Some(Command::SetPpm(ppm));
    send(&app, Command::SetPpm(ppm));
}

#[tauri::command]
fn set_sondes(app: State<App>, on: bool) {
    send(&app, Command::SetSondes(on));
}

#[tauri::command]
fn set_acars(app: State<App>, on: bool) {
    send(&app, Command::SetAcars(on));
}

#[tauri::command]
fn set_adsb(app: State<App>, on: bool) {
    send(&app, Command::SetAdsb(on));
}

#[tauri::command]
fn set_air_turns(app: State<App>, planes_s: Option<f64>, messages_s: Option<f64>) {
    send(&app, Command::AirTurns(planes_s.zip(messages_s)));
}

/// Ranges of the band to check now and then, `[low Hz, high Hz]` each, or null to stop.
#[tauri::command]
fn set_band_look(app: State<App>, ranges: Option<Vec<(f64, f64)>>) {
    send(&app, Command::BandLook(ranges));
}

/// The listener's position, for decoding aircraft on the ground.
#[tauri::command]
fn set_site(app: State<App>, lat: Option<f64>, lon: Option<f64>) {
    send(&app, Command::SetSite(lat.zip(lon)));
}

#[derive(Serialize)]
struct Iss {
    /// The location text did not parse as a ZIP code, state or "lat, lon".
    bad_location: bool,
    /// The location is a whole state, so times are rougher.
    approximate: bool,
    site: Option<radio::iss::Site>,
    /// Unix seconds of the TLE epoch; null with no TLE yet.
    tle_epoch: Option<f64>,
    passes: Vec<radio::iss::Pass>,
}

/// Station passes over `location` (ZIP code, state or "lat, lon") for the next two days, from the cached TLE.
#[tauri::command]
fn iss(location: String) -> Iss {
    let site = radio::iss::parse_site(&location);
    let orbit = radio::iss::load_cached();
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    Iss {
        bad_location: site.is_none() && !location.trim().is_empty(),
        approximate: radio::iss::place::state(&location).is_some(),
        site,
        tle_epoch: orbit.as_ref().map(|o| o.epoch),
        passes: match (site, &orbit) {
            (Some(s), Some(o)) => o.passes(s, now, 48.0),
            _ => Vec::new(),
        },
    }
}

/// The station's ground track from 45 min ago to 100 min ahead, every 30 s; empty with no TLE.
#[tauri::command]
fn iss_track() -> Vec<radio::iss::GroundPoint> {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    radio::iss::load_cached().map(|o| o.track(now - 2700.0, now + 6000.0, 30.0)).unwrap_or_default()
}

/// Flights in the aircraft log from the last `days` days, newest first.
#[tauri::command]
async fn air_flights(days: u64) -> Vec<radio::airlog::Flight> {
    let since = radio::airlog::now_ms().saturating_sub(days * 86_400_000);
    tauri::async_runtime::spawn_blocking(move || radio::airlog::dir().map(|d| radio::airlog::flights(&d, since)).unwrap_or_default()).await.unwrap_or_default()
}

/// One flight's track from the aircraft log.
#[tauri::command]
async fn air_track(icao: String, from_ms: u64, to_ms: u64) -> Vec<radio::airlog::Point> {
    tauri::async_runtime::spawn_blocking(move || radio::airlog::dir().map(|d| radio::airlog::track(&d, &icao, from_ms, to_ms)).unwrap_or_default()).await.unwrap_or_default()
}

/// Every address's track since a time, to start the map's trails with.
#[tauri::command]
async fn air_tracks(since_ms: u64) -> std::collections::HashMap<String, Vec<radio::airlog::Point>> {
    tauri::async_runtime::spawn_blocking(move || radio::airlog::dir().map(|d| radio::airlog::tracks(&d, since_ms)).unwrap_or_default()).await.unwrap_or_default()
}

/// The last `max` aircraft messages from the log, oldest first.
#[tauri::command]
async fn air_messages(days: u64, max: usize) -> Vec<radio::acars::Message> {
    let since = radio::airlog::now_ms().saturating_sub(days * 86_400_000);
    tauri::async_runtime::spawn_blocking(move || radio::airlog::dir().map(|d| radio::airlog::messages(&d, since, max)).unwrap_or_default()).await.unwrap_or_default()
}

/// What a public database knows about a plane, by address; null when offline.
#[tauri::command]
async fn plane_info(icao: String) -> Option<radio::planeinfo::PlaneInfo> {
    tauri::async_runtime::spawn_blocking(move || radio::planeinfo::lookup(&icao)).await.ok().flatten()
}

/// Opens a public web page in the user's browser, best effort.
#[tauri::command]
fn open_link(url: String) {
    if url.starts_with("https://") {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// One elevation tile for the local map, as a data URL; null when it is neither cached nor downloadable.
#[tauri::command]
async fn terrain_tile(z: u8, x: u32, y: u32) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || radio::terrain::tile_url(z, x, y)).await.ok().flatten()
}

/// The local map's shading saved earlier under `key`, as a data URL; null when there is none.
#[tauri::command]
fn ground(key: String) -> Option<String> {
    radio::terrain::ground(&key)
}

/// Saves the local map's shading (a PNG data URL) under `key`; best effort.
#[tauri::command]
fn save_ground(key: String, data_url: String) {
    if let Err(e) = radio::terrain::save_ground(&key, &data_url) {
        eprintln!("saving the map: {e:#}");
    }
}

/// HD Radio program to decode on the tuned FM station (0 = HD1), or null for off.
#[tauri::command]
fn set_hd(app: State<App>, program: Option<u32>) {
    send(&app, Command::SetHd(program));
}

/// P25: only `hold` plays (null: any talkgroup), and nothing in `skip` does.
#[tauri::command]
fn set_p25_picks(app: State<App>, hold: Option<u16>, skip: Vec<u16>) {
    send(&app, Command::P25Picks { hold, skip });
}

/// Start or stop recording the audio; `recording` says what happened.
#[tauri::command]
fn record(app: State<App>, on: bool) {
    send(&app, Command::Record(on));
}

#[tauri::command]
fn recording() -> radio::record::Info {
    radio::record::info()
}

#[tauri::command]
fn set_playing(app: State<App>, playing: bool) {
    send(&app, Command::SetPlaying(playing));
}

fn mtime(p: Option<std::path::PathBuf>) -> Option<SystemTime> {
    std::fs::metadata(p?).ok()?.modified().ok()
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--setup-wake") {
        match radio::wake::setup() {
            Ok(()) => println!("Done: Squelch can wake this computer."),
            Err(e) => {
                eprintln!("Squelch cannot wake this computer: {e:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    tauri::Builder::default()
        .manage(App::default())
        .setup(|app| {
            // Omarchy swaps the theme directory on a switch, and the bands file is hand edited:
            // poll both and tell the UI when either changes.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut theme = radio::theme::stamp();
                let mut bands = mtime(BandPlan::path());
                let mut reps = mtime(Repeaters::path());
                let mut sched = mtime(prefs::schedule_path());
                let mut sets = mtime(prefs::settings_path());
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    let t = radio::theme::stamp();
                    if t != theme {
                        theme = t;
                        let _ = handle.emit("theme", radio::theme::load());
                    }
                    let b = mtime(BandPlan::path());
                    if b != bands {
                        bands = b;
                        let _ = handle.emit("bands", load_bands());
                    }
                    let r = mtime(Repeaters::path());
                    if r != reps {
                        reps = r;
                        let _ = handle.emit("repeaters", ());
                    }
                    let s = mtime(prefs::schedule_path());
                    if s != sched {
                        sched = s;
                        let _ = handle.emit("schedule", load_schedule());
                    }
                    let s = mtime(prefs::settings_path());
                    if s != sets {
                        sets = s;
                        let _ = handle.emit("settings", load_settings());
                    }
                }
            });
            // Space station elements: refresh when the cache is over REFRESH old, trying at most
            // every RETRY (CelesTrak blocks faster polling). Offline, the old cache keeps working.
            const REFRESH: Duration = Duration::from_secs(12 * 3600);
            const RETRY: Duration = Duration::from_secs(2 * 3600);
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                let age = mtime(radio::iss::cache_path()).and_then(|t| t.elapsed().ok());
                if age.map_or(true, |a| a > REFRESH) {
                    match radio::iss::fetch() {
                        Ok(()) => {
                            let _ = handle.emit("iss", ());
                        }
                        Err(e) => eprintln!("space station elements: {e:#}"),
                    }
                }
                std::thread::sleep(RETRY);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_site, air_flights, air_track, air_tracks, air_messages, setup, bands, save_bookmark, delete_bookmark, schedule, save_schedule, set_wake, wake_setup_command, hold_awake, settings, save_settings, read_state, save_state, notify, repeaters, repeaters_map, fm_stations, airports_near, airports_by_code, plane_info, open_link, theme, start, feed, tune, set_mode, set_gain, set_volume, set_playing, record, recording, set_squelch, set_ppm, set_sondes, set_acars, set_adsb, set_air_turns, set_band_look, set_hd, set_p25_picks, iss, iss_track, terrain_tile, ground, save_ground])
        .run(tauri::generate_context!())
        .expect("error while running the receiver");
}
