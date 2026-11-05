//! Tune a DMR repeater output on the dongle and print what the receiver hears: syncs, colour
//! code, every call as it starts and ends, and the bursts per second. Close the app first.
//! cargo run -p radio --release --example dmr_probe -- <MHz> [seconds, default 60]

use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let mhz: f64 = args.next().and_then(|s| s.parse().ok()).expect("frequency in MHz");
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let hz = mhz * 1e6;
    let (src, why) = Engine::open_source();
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(Mode::Dmr));
    engine.send(Command::Tune { center_hz: hz, tuned_hz: hz });
    engine.send(Command::SetPlaying(true));
    let mut seen = 0usize;
    let mut live: [Option<String>; 2] = [None, None];
    for t in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        let Some(s) = last.lock().unwrap().clone() else { continue };
        let Some(d) = s.dmr else { continue };
        if t % 5 == 0 {
            println!(
                "{t:>4} s  {}  snr {:>5.1} dB  heard {}  source {:?}  cc {:?}  {:.0} bursts/s  encrypted {}",
                s.device,
                s.channel_snr_db,
                d.heard,
                d.source,
                d.colour_code,
                d.bursts_per_sec,
                d.encrypted_calls
            );
        }
        for (i, slot) in d.slots.iter().enumerate() {
            let now = slot.call.as_ref().map(|c| format!("{} {:?} <- {:?}{}{}", if c.group { "talkgroup" } else { "radio" }, c.dst, c.src, if c.encrypted { " encrypted" } else { "" }, if slot.playing { " playing" } else { "" }));
            if now != live[i] {
                if let Some(n) = &now {
                    println!("{t:>4} s  slot {}: {n}", i + 1);
                }
                live[i] = now;
            }
        }
        for c in d.recent.iter().take(d.recent.len().saturating_sub(seen)).rev() {
            println!(
                "{t:>4} s  ended slot {} {} {:?} <- {:?}: {:.1} s, {} frames, {} bad{}",
                c.slot,
                if c.group { "talkgroup" } else { "radio" },
                c.dst,
                c.src,
                c.last_secs - c.start_secs,
                c.frames,
                c.bad_frames,
                if c.encrypted { ", encrypted" } else { "" }
            );
        }
        seen = d.recent.len();
    }
}
