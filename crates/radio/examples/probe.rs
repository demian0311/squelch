//! Tune somewhere for a few seconds and print what the engine sees.
//! cargo run -p radio --release --example probe -- 101.1 wfm 4

use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhz: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(101.1);
    let mode: Mode = args.get(2).map(|s| toml::Value::String(s.clone()).try_into().unwrap()).unwrap_or(Mode::Wfm);
    let secs: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);

    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let rows = Arc::new(Mutex::new(0usize));
    let (l, r) = (last.clone(), rows.clone());
    let engine = Engine::start(src, why, move |_| *r.lock().unwrap() += 1, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(mode));
    engine.send(Command::Tune { center_hz: mhz * 1e6 + 300_000.0, tuned_hz: mhz * 1e6 });
    std::thread::sleep(std::time::Duration::from_secs(secs));
    let s = last.lock().unwrap().clone().expect("no status");
    println!("device: {}  error: {:?}", s.device, s.error);
    println!("rows/s: {:.1}  floor: {:.1} dBFS  channel: {:.1} dB above floor  squelch open: {}", *rows.lock().unwrap() as f64 / secs as f64, s.floor_db, s.channel_snr_db, s.squelch_open);
    if let Some(r) = &s.rds {
        println!("rds: {r:?}");
    }
    for d in &s.signals {
        println!("  {:>10.4} MHz  width {:>6.1} kHz  {:>5.1} dB", d.hz / 1e6, d.width_hz / 1e3, d.snr_db);
    }
}
