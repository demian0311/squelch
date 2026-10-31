//! Time each stage of the receive chain on one second of samples.
//! cargo run -p radio --release --example bench

use radio::demod::{Demodulator, Mode};
use radio::device::{IqSource, SimSource};
use radio::dsp::{u8_to_iq, Mixer};
use radio::engine::{demo_signals, FFT_SIZE, ROWS_PER_SEC, SAMPLE_RATE};
use radio::spectrum::Spectrum;
use std::time::Instant;

fn main() {
    let mut src = SimSource::new(SAMPLE_RATE, demo_signals(), false);
    src.set_center(101_400_000).unwrap();
    let mut raw = vec![0u8; SAMPLE_RATE as usize * 2];
    let t = Instant::now();
    src.read(&mut raw).unwrap();
    println!("simulate      {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let mut iq = Vec::new();
    let t = Instant::now();
    u8_to_iq(&raw, &mut iq);
    println!("convert       {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let mut s = Spectrum::new(FFT_SIZE, SAMPLE_RATE, ROWS_PER_SEC);
    let mut rows = Vec::new();
    let t = Instant::now();
    s.process(&iq, &mut rows);
    println!("spectrum      {:>6.0} ms  ({} rows)", t.elapsed().as_secs_f64() * 1e3, rows.len());
    let t = Instant::now();
    let mut chan = iq.clone();
    Mixer::new(-300_000.0, SAMPLE_RATE as f64).process(&mut chan);
    println!("mix           {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);
    for mode in [Mode::Wfm, Mode::Nfm, Mode::Am, Mode::Usb] {
        let mut d = Demodulator::new(mode, SAMPLE_RATE);
        let mut audio = Vec::new();
        let t = Instant::now();
        d.process(&chan, &mut audio);
        println!("demod {:<6}  {:>6.0} ms", format!("{mode:?}"), t.elapsed().as_secs_f64() * 1e3);
    }
}
