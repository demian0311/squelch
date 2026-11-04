//! Run the aircraft decoder over a raw 8-bit I/Q recording (`iq_dump`) in engine-sized blocks and time it.
//! cargo run -p radio --release --example adsb_file -- <file.u8> [passes]

use radio::adsb::Adsb;
use radio::dsp::u8_to_iq;
use radio::engine::SAMPLE_RATE;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&args[1]).unwrap();
    let passes: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let secs = raw.len() as f64 / (SAMPLE_RATE as f64 * 2.0);
    for _ in 0..passes {
        let mut a = Adsb::new(SAMPLE_RATE);
        let mut iq = Vec::new();
        let (mut convert, mut decode) = (0.0, 0.0);
        for block in raw.chunks(131_072) {
            let t = Instant::now();
            u8_to_iq(block, &mut iq);
            convert += t.elapsed().as_secs_f64();
            let t = Instant::now();
            a.process(&iq);
            decode += t.elapsed().as_secs_f64();
        }
        let info = a.info();
        println!(
            "{secs:.1} s of capture: convert {:.0} ms, adsb {:.0} ms ({:.1}% of real time), {} aircraft, {} frames",
            convert * 1e3,
            decode * 1e3,
            decode / secs * 100.0,
            info.aircraft.len(),
            info.aircraft.iter().map(|p| p.messages).sum::<u32>(),
        );
    }
}
