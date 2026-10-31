//! Record the FM multiplex signal (240 kHz, f32 LE, 1.0 = 75 kHz deviation) of one station.
//! cargo run -p radio --release --example mpx_dump -- 101.1 10 out.f32

use radio::device::{IqSource, RtlSdr};
use radio::dsp::{lowpass, u8_to_iq, Decimator, Mixer};
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let mhz: f64 = a[1].parse()?;
    let secs: f64 = a[2].parse()?;
    let rate = 2_400_000u32;
    let mut src = RtlSdr::open(0, rate)?;
    let center = mhz * 1e6 + 300_000.0;
    src.set_center(center as u32)?;
    let mut mixer = Mixer::new(-300_000.0, rate as f64);
    let fs = rate as f32;
    let mut dec = Decimator::new(lowpass(100_000.0 / fs, 40_000.0 / fs), 10);
    let mut out = std::fs::File::create(&a[3])?;
    let (mut raw, mut iq, mut s1) = (vec![0u8; 262_144], Vec::new(), Vec::new());
    let mut last = num_complex::Complex32::new(1.0, 0.0);
    let mut n = 0usize;
    let k = 240_000.0 / (std::f32::consts::TAU * 75_000.0);
    while (n as f64) < secs * 240_000.0 {
        let got = src.read(&mut raw)?;
        u8_to_iq(&raw[..got], &mut iq);
        mixer.process(&mut iq);
        s1.clear();
        dec.process(&iq, &mut s1);
        for &s in &s1 {
            out.write_all(&((s * last.conj()).arg() * k).to_le_bytes())?;
            last = s;
        }
        n += s1.len();
    }
    eprintln!("{n} samples");
    Ok(())
}
