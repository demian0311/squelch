//! Direct sampling below 24 MHz: switch the dongle's tuner out, tune the chip's own
//! down-converter, and print what the engine sees (floor and every signal above it).
//! cargo run -p radio --release --example hf_probe -- [centre MHz] [secs] [branch 1|2] [mode]
//! The whole 0.5–14.4 MHz span is six captures; a run with no centre walks them all.

use radio::demod::Mode;
use radio::device::{IqSource, RtlSdr};
use radio::engine::{Command, Engine, BELOW_FLOOR_DB, FFT_SIZE, RANGE_DB, SAMPLE_RATE};
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let centre: Option<f64> = args.get(1).and_then(|s| s.parse().ok());
    let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
    let branch: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
    let mode: Mode = args.get(4).map(|s| toml::Value::String(s.clone()).try_into().unwrap()).unwrap_or(Mode::Am);

    let mut dongle = RtlSdr::open(0, SAMPLE_RATE).expect("no dongle (close the app first)");
    dongle.set_direct_sampling(branch).expect("direct sampling");
    println!("{}  direct sampling branch {branch}", dongle.name());

    let last = Arc::new(Mutex::new(None));
    let rows = Arc::new(Mutex::new(0usize));
    // the rows summed, so the raw spectrum can be read without the detector's judgement
    let sum = Arc::new(Mutex::new(vec![0f32; FFT_SIZE]));
    let (l, r, su) = (last.clone(), rows.clone(), sum.clone());
    let engine = Engine::start(
        Box::new(dongle),
        None,
        move |row| {
            *r.lock().unwrap() += 1;
            let mut su = su.lock().unwrap();
            for (a, b) in su.iter_mut().zip(&row.bins) {
                *a += *b as f32;
            }
        },
        move |s| *l.lock().unwrap() = Some(s),
    );
    engine.send(Command::SetMode(mode));
    engine.send(Command::SetPlaying(false));

    // a 2.4 Msps capture is usable over about 2 MHz; six of them cover 0.5–14.4 MHz
    let centres: Vec<f64> = match centre {
        Some(c) => vec![c],
        None => (0..6).map(|i| 1.5 + 2.1 * i as f64).collect(),
    };
    for c in centres {
        engine.send(Command::Tune { center_hz: c * 1e6, tuned_hz: c * 1e6 });
        *rows.lock().unwrap() = 0;
        sum.lock().unwrap().iter_mut().for_each(|a| *a = 0.0);
        std::thread::sleep(std::time::Duration::from_secs(secs));
        let s = last.lock().unwrap().clone().expect("no status");
        let mut v: Vec<_> = s.signals.iter().collect();
        v.sort_by(|a, b| b.snr_db.total_cmp(&a.snr_db));
        println!(
            "centre {c:.3} MHz  rows/s {:.1}  floor {:.1} dBFS  {} signals{}",
            *rows.lock().unwrap() as f64 / secs as f64,
            s.floor_db,
            v.len(),
            s.error.map(|e| format!("  error: {e}")).unwrap_or_default()
        );
        for d in v.iter().take(12) {
            println!("  {:>8.4} MHz  width {:>6.1} kHz  {:>5.1} dB", d.hz / 1e6, d.width_hz / 1e3, d.snr_db);
        }
        // the averaged row's peaks: dB above the floor, 10 kHz apart at least
        let n = (*rows.lock().unwrap()).max(1) as f32;
        let bin_hz = SAMPLE_RATE as f64 / FFT_SIZE as f64;
        let lo = c * 1e6 - SAMPLE_RATE as f64 / 2.0;
        let mut peaks: Vec<(f64, f32)> = sum
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, a)| (lo + i as f64 * bin_hz, a / n * RANGE_DB / 255.0 - BELOW_FLOOR_DB))
            .collect();
        peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut kept: Vec<(f64, f32)> = Vec::new();
        for p in peaks {
            if kept.len() >= 14 || p.1 < 6.0 {
                break;
            }
            if kept.iter().all(|k| (k.0 - p.0).abs() > 10_000.0) {
                kept.push(p);
            }
        }
        kept.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!("  raw peaks: {}", kept.iter().map(|(f, d)| format!("{:.3}/{d:.0}", f / 1e6)).collect::<Vec<_>>().join("  "));
    }
}
