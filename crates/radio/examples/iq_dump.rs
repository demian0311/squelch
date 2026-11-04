//! Record raw 8-bit I/Q from the dongle, as the engine reads it, for offline benchmarks.
//! cargo run -p radio --release --example iq_dump -- <centre MHz> <secs> <out.u8>

use radio::engine::{Engine, SAMPLE_RATE};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhz: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1090.06);
    let secs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let path = args.get(3).cloned().unwrap_or_else(|| "iq.u8".into());
    let (mut src, why) = Engine::open_source();
    if let Some(w) = why {
        eprintln!("{w}");
    }
    src.set_center((mhz * 1e6) as u32).unwrap();
    let mut buf = vec![0u8; 131_072];
    let total = SAMPLE_RATE as usize * 2 * secs;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    // the first second after a tune settles
    let mut skip = SAMPLE_RATE as usize * 2;
    let mut written = 0;
    while written < total {
        let n = src.read(&mut buf).unwrap();
        if skip > 0 {
            skip = skip.saturating_sub(n);
            continue;
        }
        let take = n.min(total - written);
        out.write_all(&buf[..take]).unwrap();
        written += take;
    }
    out.flush().unwrap();
    println!("{} ({written} bytes, {secs} s at {mhz} MHz)", path);
}
