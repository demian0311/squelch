//! Listen on 1090 MHz with the ADS-B decoder on, printing the aircraft table every few seconds.
//! cargo run -p radio --release --example adsb_probe -- <seconds>

use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(Mode::None));
    // off the replies' centre by the UI's DC offset, as the app tunes it
    engine.send(Command::Tune { center_hz: 1_090_060_000.0, tuned_hz: 1_090_000_000.0 });
    engine.send(Command::SetAdsb(true));
    for n in 1..=secs {
        std::thread::sleep(Duration::from_secs(1));
        if n % 5 != 0 && n != secs {
            continue;
        }
        let Some(s) = last.lock().unwrap().clone() else { continue };
        let Some(info) = s.adsb else { continue };
        println!("{n:>4} s ({}): {:.1} frames/s, {} aircraft", s.device, info.frames_per_sec, info.aircraft.len());
        for a in &info.aircraft {
            let opt = |v: Option<String>| v.unwrap_or_else(|| "-".into());
            println!(
                "  {} {:<8} alt {:>6} ft  {:>4} kt  hdg {:>3}  vr {:>6}  pos {}  {} msgs  {:.0} dB  {:.0} s ago",
                a.icao,
                a.callsign.as_deref().unwrap_or("-"),
                opt(a.alt_ft.map(|v| v.to_string())),
                opt(a.speed_kt.map(|v| format!("{v:.0}"))),
                opt(a.heading_deg.map(|v| format!("{v:.0}"))),
                opt(a.vrate_fpm.map(|v| v.to_string())),
                opt(a.lat.zip(a.lon).map(|(la, lo)| format!("{la:.4}, {lo:.4}"))),
                a.messages,
                a.snr_db,
                a.age_s
            );
        }
    }
}
