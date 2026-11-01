//! Listen to the US ACARS channels for a while and print every block decoded.
//! cargo run -p radio --release --example acars_probe -- <seconds> [centre MHz, default 130.5]

use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let centre: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(130.5) * 1e6;
    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(Mode::None));
    engine.send(Command::Tune { center_hz: centre, tuned_hz: centre });
    engine.send(Command::SetAcars(true));
    let mut shown = 0u64;
    let mut told = false;
    for _ in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        let Some(s) = last.lock().unwrap().clone() else { continue };
        let Some(a) = s.acars else { continue };
        if !told {
            told = true;
            eprintln!("{}: floor {:.1} dBFS, channels {:?} MHz", s.device, s.floor_db, a.channels.iter().map(|h| h / 1e6).collect::<Vec<_>>());
        }
        let from = shown;
        for m in a.messages.iter().filter(|m| m.id > from) {
            println!(
                "{:.3} MHz mode {} reg {:<7} flight {:<6} label {} block {} ack {:?} msn {}{}\n  {}",
                m.hz / 1e6,
                m.mode,
                m.registration,
                m.flight,
                m.label,
                m.block_id,
                m.ack,
                m.msn,
                if m.more { " (more follows)" } else { "" },
                m.text.replace('\n', "\n  ")
            );
            shown = m.id;
        }
    }
    let heard = last.lock().unwrap().as_ref().and_then(|s| s.acars.as_ref().map(|a| a.heard)).unwrap_or(0);
    println!("{heard} blocks in {secs} s");
}
