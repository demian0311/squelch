// The window shell: wires the radio engine to the web UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use radio::bands::BandPlan;
use radio::demod::Mode;
use radio::device::Gain;
use radio::engine::{Command, Engine, Status, BELOW_FLOOR_DB, FFT_SIZE, RANGE_DB, SAMPLE_RATE};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{Emitter, State};

#[derive(Default)]
struct App {
    engine: Mutex<Option<Engine>>,
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

#[tauri::command]
fn theme() -> BTreeMap<String, String> {
    radio::theme::load()
}

/// Start the engine. Spectrum rows arrive on `rows` as bytes: an f64 centre (little endian) then one byte per bin.
#[tauri::command]
fn start(app: State<App>, rows: Channel<InvokeResponseBody>, status: Channel<Status>) {
    let mut slot = app.engine.lock().unwrap();
    *slot = None; // stop any earlier engine first so the dongle is free
    let (src, why) = Engine::open_source();
    let engine = Engine::start(
        src,
        why,
        move |row| {
            let mut bytes = Vec::with_capacity(8 + row.bins.len());
            bytes.extend_from_slice(&row.center_hz.to_le_bytes());
            bytes.extend_from_slice(&row.bins);
            let _ = rows.send(InvokeResponseBody::Raw(bytes));
        },
        move |s| {
            let _ = status.send(s);
        },
    );
    *slot = Some(engine);
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
    send(&app, Command::SetGain(tenths.map(Gain::Tenths).unwrap_or(Gain::Auto)));
}

#[tauri::command]
fn set_volume(app: State<App>, volume: f32) {
    send(&app, Command::SetVolume(volume));
}

#[tauri::command]
fn set_squelch(app: State<App>, db: Option<f32>) {
    send(&app, Command::SetSquelch(db));
}

#[tauri::command]
fn set_sondes(app: State<App>, on: bool) {
    send(&app, Command::SetSondes(on));
}

#[tauri::command]
fn set_playing(app: State<App>, playing: bool) {
    send(&app, Command::SetPlaying(playing));
}

fn mtime(p: Option<std::path::PathBuf>) -> Option<SystemTime> {
    std::fs::metadata(p?).ok()?.modified().ok()
}

fn main() {
    tauri::Builder::default()
        .manage(App::default())
        .setup(|app| {
            // Omarchy swaps the theme directory on a switch, and the bands file is hand edited:
            // poll both and tell the UI when either changes.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut theme = radio::theme::stamp();
                let mut bands = mtime(BandPlan::path());
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
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![setup, bands, theme, start, tune, set_mode, set_gain, set_volume, set_playing, set_squelch, set_sondes])
        .run(tauri::generate_context!())
        .expect("error while running the receiver");
}
