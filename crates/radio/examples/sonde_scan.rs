//! Sweep the weather-balloon band with the sonde decoders on, printing what is heard.
//! cargo run -p radio --release --example sonde_scan -- <seconds per step>

use radio::engine::{Command, Engine};
use radio::demod::Mode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Capture centres covering 400–406 MHz with the 2.4 MHz capture.
const CENTRES_MHZ: [f64; 3] = [401.1, 403.0, 404.9];

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(Mode::None));
    engine.send(Command::SetSondes(true));
    for c in CENTRES_MHZ {
        engine.send(Command::Tune { center_hz: c * 1e6, tuned_hz: c * 1e6 + 60_000.0 });
        let mut seen = Vec::new();
        for _ in 0..secs {
            std::thread::sleep(Duration::from_secs(1));
            if let Some(s) = last.lock().unwrap().clone() {
                for hz in s.decoding_hz {
                    if !seen.contains(&(hz as i64 / 1000)) {
                        seen.push(hz as i64 / 1000);
                    }
                }
                for o in &s.sondes {
                    println!("  SONDE {} {} at {:.4} MHz: {:?} {:?} {:?} alt {:?} climb {:?} ({} frames)", o.kind, o.serial, o.hz / 1e6, o.time, o.lat, o.lon, o.alt_m, o.climb_ms, o.frames);
                }
            }
        }
        let s = last.lock().unwrap().clone().expect("no status");
        println!("{c} MHz ({}): floor {:.1} dBFS, decoders tried on {:?} kHz", s.device, s.floor_db, seen);
    }
}
