//! Tune an FM station with HD Radio decoding on and print what arrives; pictures go to <dir>.
//! cargo run -p radio --release --example hd_probe -- 106.7 0 60 /tmp/hd

use radio::demod::Mode;
use radio::engine::{Command, Engine};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mhz: f64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(106.7);
    let program: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let secs: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
    let dir = a.get(4).cloned().unwrap_or_else(|| "hd_images".into());
    std::fs::create_dir_all(&dir).expect("create picture dir");
    let last = Arc::new(Mutex::new(None));
    let l = last.clone();
    let d = dir.clone();
    let (src, why) = Engine::open_source();
    let engine = Engine::start(src, why, |_| {}, move |s| {
        if let Some(imgs) = &s.hd_images {
            for (i, img) in imgs.iter().enumerate() {
                let b64 = img.data_url.split(',').nth(1).unwrap_or("");
                let ext = if img.data_url.starts_with("data:image/png") { "png" } else { "jpg" };
                let _ = std::fs::write(format!("{d}/{i:02}_{}_{}_{}.{ext}.b64", img.kind, img.row, img.col), b64);
            }
            println!("  {} pictures: {:?}", imgs.len(), imgs.iter().map(|i| format!("{} {}", i.kind, i.name)).collect::<Vec<_>>());
        }
        *l.lock().unwrap() = Some(s);
    });
    engine.send(Command::SetMode(Mode::Wfm));
    engine.send(Command::Tune { center_hz: mhz * 1e6 + 300_000.0, tuned_hz: mhz * 1e6 });
    engine.send(Command::SetHd(Some(program)));
    let mut prev = None;
    for t in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        let Some(s) = last.lock().unwrap().clone() else { continue };
        let hd = s.hd.clone().map(|mut h| {
            h.mer_db = h.mer_db.map(|m| m.round());
            h.ber = None;
            h
        });
        if hd != prev {
            println!("{t:3}s sidebands {} {:?}", s.hd_sidebands, hd);
            prev = hd;
        }
    }
}
