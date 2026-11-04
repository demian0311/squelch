//! Compare the dongle staying on aircraft broadcasts with taking turns between them and aircraft messages.
//! cargo run -p radio --release --example air_turns -- <seconds planes only> [planes secs] [messages secs] [seconds in turns]

use radio::engine::{Command, Engine, ADSB_CENTER_HZ};
use radio::acars::CENTER_HZ;
use radio::demod::Mode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() {
    let arg = |n: usize, d: f64| std::env::args().nth(n).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (only_s, planes_s, messages_s) = (arg(1, 120.0) as u64, arg(2, 20.0), arg(3, 10.0));
    let turns_s = arg(4, only_s as f64) as u64;
    let (mut src, why) = Engine::open_source();
    let mut ms = Vec::new();
    for hz in [CENTER_HZ, ADSB_CENTER_HZ].repeat(5) {
        let t = Instant::now();
        src.set_center(hz as u32).unwrap();
        ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("retune: {:?} ms", ms.iter().map(|m| m.round() as i64).collect::<Vec<_>>());
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let engine = Engine::start(src, why, |_| {}, move |s| *l.lock().unwrap() = Some(s));
    engine.send(Command::SetMode(Mode::None));
    engine.send(Command::SetAdsb(true));
    engine.send(Command::SetAcars(true));
    for turns in [None, Some((planes_s, messages_s))] {
        engine.send(Command::AirTurns(turns));
        if turns.is_none() {
            engine.send(Command::Tune { center_hz: ADSB_CENTER_HZ, tuned_hz: ADSB_CENTER_HZ });
        }
        println!("--- {}", if turns.is_some() { format!("turns {planes_s}/{messages_s} s") } else { "planes only".into() });
        let (mut planes, mut with_pos, mut fix_age, mut n, mut acars) = (0.0, 0.0, 0.0, 0.0, 0);
        let secs = if turns.is_some() { turns_s } else { only_s };
        for at in 0..secs {
            std::thread::sleep(Duration::from_secs(1));
            let Some(s) = last.lock().unwrap().clone() else { continue };
            let Some(info) = s.adsb else { continue };
            let pos: Vec<_> = info.aircraft.iter().filter(|a| a.lat.is_some()).collect();
            planes += info.aircraft.len() as f64;
            with_pos += pos.len() as f64;
            fix_age += pos.iter().map(|a| a.age_s as f64).sum::<f64>() / pos.len().max(1) as f64;
            n += 1.0;
            acars = s.acars.map_or(acars, |a| a.heard);
            if at % 30 == 29 {
                println!("{:4} s: {} aircraft, {} with position, {acars} messages, centre {:.1} MHz", at + 1, info.aircraft.len(), pos.len(), s.center_hz / 1e6);
            }
        }
        println!("mean aircraft {:.1}, with position {:.1}, mean last-heard age {:.1} s, messages heard so far {acars}", planes / n, with_pos / n, fix_age / n);
    }
}
