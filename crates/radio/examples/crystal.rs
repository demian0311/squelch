//! Measure the dongle's crystal error on a strong FM station, apply it, and measure again.
//! cargo run -p radio --release --example crystal -- 101.1

use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let mhz: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(101.1);
    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(Mode::Wfm));
    engine.send(Command::Tune { center_hz: mhz * 1e6 + 300_000.0, tuned_hz: mhz * 1e6 });
    let measure = |label: &str| {
        *last.lock().unwrap() = None;
        std::thread::sleep(Duration::from_secs(7));
        let s = last.lock().unwrap().clone().expect("no status");
        println!("{label}: applied {} ppm, channel {:.1} dB, measured {:?}", s.ppm, s.channel_snr_db, s.crystal);
        s.crystal
    };
    let Some(c) = measure("uncorrected") else { return };
    engine.send(Command::SetPpm(c.ppm.round() as i32));
    measure("corrected");
}
