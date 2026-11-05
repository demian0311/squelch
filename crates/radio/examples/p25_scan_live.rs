//! Watch the receiver find a P25 control channel by itself on the real dongle.
//! cargo run -p radio --release --example p25_scan_live -- [secs] [start MHz] [ppm]
//!
//! Tunes the EMS 800 band at `start MHz` (default 864, where the Miami-Dade site has nothing) and
//! prints every change of the receiver's state: each hop, each lock, the site and its calls.
use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() {
    let arg = |n: usize, d: f64| std::env::args().nth(n).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (secs, start_mhz, ppm) = (arg(1, 90.0), arg(2, 864.0), arg(3, 56.0) as i32);
    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetPpm(ppm));
    engine.send(Command::Tune { center_hz: start_mhz * 1e6, tuned_hz: start_mhz * 1e6 });
    engine.send(Command::SetMode(Mode::P25));
    engine.send(Command::BandLook(Some(vec![(769e6, 775e6), (851e6, 869e6)])));
    let t0 = Instant::now();
    let mut shown = String::new();
    let mut listed = false;
    let mut calls = 0;
    while t0.elapsed().as_secs_f64() < secs {
        std::thread::sleep(Duration::from_millis(100));
        let Some(s) = last.lock().unwrap().clone() else { continue };
        let Some(p) = s.p25 else { continue };
        if t0.elapsed().as_secs_f64() > 3.0 && !listed {
            listed = true;
            let mut v: Vec<_> = s.signals.iter().map(|d| (d.hz / 1e6, d.width_hz / 1e3, d.snr_db)).collect();
            v.sort_by(|a, b| b.2.total_cmp(&a.2));
            println!("signals {}: {}", v.len(), v.iter().map(|(f, w, d)| format!("{f:.4}/{w:.0}k/{d:.0}dB")).collect::<Vec<_>>().join(" "));
        }
        let scan = p.scan.as_ref().map(|s| format!("hop {}/{} {:.3}-{:.3}", s.hop, s.hops, s.lo_hz / 1e6, s.hi_hz / 1e6)).unwrap_or_default();
        let site = match (p.rfss, p.site) { (Some(r), Some(s)) => format!("site {r}-{s}"), _ => String::new() };
        let line = format!(
            "{:6.1}s  centre {:.3}  {:9}  {}  tried {:2} trying {}  nac {}  control {}  {}  {} calls",
            t0.elapsed().as_secs_f64(), s.center_hz / 1e6, p.state, scan, p.tried.len(), p.trying_hz.map(|h| format!("{:.4}", h / 1e6)).unwrap_or("-".into()),
            p.nac.map(|n| format!("{n:03X}")).unwrap_or("-".into()),
            p.control_hz.map(|h| format!("{:.4}", h / 1e6)).unwrap_or("-".into()), site, p.groups.iter().map(|g| g.calls).sum::<u32>()
        );
        // one line per change of anything but the clock
        let key = line[8..].to_string();
        if key != shown {
            println!("{line}");
            shown = key;
        }
        calls = p.groups.iter().map(|g| g.calls).sum::<u32>();
    }
    println!("done: {} calls seen", calls);
}
