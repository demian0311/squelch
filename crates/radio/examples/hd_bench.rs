//! Time the HD Radio decoder's stages on one second of noise: the front end and libnrsc5 itself.
//! cargo run -p radio --release --example hd_bench

use radio::dsp::C32;
use radio::engine::SAMPLE_RATE;
use radio::hd::{ffi, FrontEnd};
use std::time::Instant;

fn main() {
    let mut seed = 1u32;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    let n = SAMPLE_RATE as usize;
    let mut iq: Vec<C32> = (0..n).map(|_| C32 { re: noise(), im: noise() }).collect();
    let mut front = FrontEnd::new(300_000.0, SAMPLE_RATE);
    let mut flat = Vec::new();
    let t = Instant::now();
    for block in iq.chunks_mut(1 << 16) {
        let mut tmp = Vec::new();
        front.process(block, &mut tmp);
        flat.extend_from_slice(&tmp);
    }
    println!("front end     {:>6.0} ms  ({} samples out)", t.elapsed().as_secs_f64() * 1e3, flat.len() / 2);
    match ffi::Session::open(|_| {}) {
        Ok(mut s) => {
            let t = Instant::now();
            for block in flat.chunks(1 << 16) {
                s.push(block);
            }
            println!("nrsc5         {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);
        }
        Err(e) => println!("nrsc5: {e}"),
    }
}
