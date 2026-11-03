//! Time each stage of the receive chain on one second of samples.
//! cargo run -p radio --release --example bench

use radio::adsb::Adsb;
use radio::demod::{Demodulator, Mode};
use radio::device::{IqSource, SimSource};
use radio::dsp::{u8_to_iq, Mixer};
use radio::engine::{demo_signals, FFT_SIZE, ROWS_PER_SEC, SAMPLE_RATE};
use radio::detect::Detected;
use radio::sonde::{all_decoders, Scanner, MAX_CHANNELS};
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
    let mut sc = Scanner::new(SAMPLE_RATE, all_decoders);
    let fake: Vec<Detected> = (0..MAX_CHANNELS).map(|i| Detected { hz: 101_000_000.0 + i as f64 * 100_000.0, width_hz: 10_000.0, snr_db: 20.0, hd: None }).collect();
    sc.retarget(101_400_000.0, &fake);
    let t = Instant::now();
    sc.process(&iq);
    println!("sondes x{}     {:>6.0} ms", sc.channels().len(), t.elapsed().as_secs_f64() * 1e3);
    let mut ac = radio::acars::Acars::new(SAMPLE_RATE);
    let t = Instant::now();
    ac.process(&iq, 130_500_000.0);
    println!("acars x{}      {:>6.0} ms", ac.info().channels.len(), t.elapsed().as_secs_f64() * 1e3);
    let mut a = Adsb::new(SAMPLE_RATE);
    let t = Instant::now();
    a.process(&iq);
    println!("adsb          {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);
}
