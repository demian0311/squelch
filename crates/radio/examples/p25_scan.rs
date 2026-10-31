//! Sweep the 700/800 MHz public-safety bands and list narrow carriers with how often each is on.
//! cargo run -p radio --release --example p25_scan -- <seconds per step>
//!
//! A trunked site's control channel transmits all the time, so it shows near 100% duty;
//! voice channels come and go. Frequencies are as the dongle reads them, uncorrected for its
//! crystal error (eagle's reads ~105 ppm, about +90 kHz at 860 MHz).

use radio::device::{Gain, IqSource, RtlSdr};
use radio::dsp::u8_to_iq;
use radio::spectrum::{noise_floor, Spectrum};

const RATE: u32 = 2_400_000;
const FFT: usize = 2048;
const ROWS_PER_SEC: u32 = 20;
/// Capture centres; each covers ±`USABLE_HZ`.
const CENTRES_MHZ: [f64; 12] = [770.0, 772.0, 774.0, 852.0, 854.0, 856.0, 858.0, 860.0, 862.0, 864.0, 866.0, 868.0];
const USABLE_HZ: f64 = 1_000_000.0;
/// A 12.5 kHz channel is "on" in a row when it stands this far above the floor.
const ON_DB: f32 = 8.0;

fn main() -> anyhow::Result<()> {
    let secs: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let mut src = RtlSdr::open(0, RATE)?;
    src.set_gain(Gain::Tenths(*src.gains().last().unwrap_or(&400)))?;
    let bin_hz = RATE as f64 / FFT as f64;
    let width = (12_500.0 / bin_hz).round() as usize;
    let step = width / 2;
    let (mut raw, mut iq) = (vec![0u8; 262_144], Vec::new());
    println!("MHz (raw)   avg dB  duty");
    for c in CENTRES_MHZ {
        let centre = c * 1e6;
        src.set_center(centre as u32)?;
        for _ in 0..4 {
            src.read(&mut raw)?;
        }
        let mut spec = Spectrum::new(FFT, RATE, ROWS_PER_SEC);
        let mut rows = Vec::new();
        let mut n = 0usize;
        while (n as f64) < secs * RATE as f64 {
            let got = src.read(&mut raw)?;
            u8_to_iq(&raw[..got], &mut iq);
            n += iq.len();
            spec.process(&iq, &mut rows);
        }
        let lo = ((FFT as f64 / 2.0) - USABLE_HZ / bin_hz) as usize;
        let hi = ((FFT as f64 / 2.0) + USABLE_HZ / bin_hz) as usize;
        // channel power (dB) of each window start in each row
        let win = |r: &[f32], a: usize| -> f32 {
            let p: f64 = r[a..a + width].iter().map(|&d| 10f64.powf(d as f64 / 10.0)).sum::<f64>() / width as f64;
            (10.0 * p.log10()) as f32
        };
        let floors: Vec<f32> = rows.iter().map(|r| noise_floor(r)).collect();
        let starts: Vec<usize> = (lo..hi - width).step_by(step).collect();
        let stats: Vec<(f32, f32)> = starts
            .iter()
            .map(|&a| {
                let above: Vec<f32> = rows.iter().zip(&floors).map(|(r, f)| win(r, a) - f).collect();
                let duty = above.iter().filter(|&&d| d >= ON_DB).count() as f32 / above.len().max(1) as f32;
                (above.iter().sum::<f32>() / above.len().max(1) as f32, duty)
            })
            .collect();
        // report local maxima of duty×level among windows that are on at least 5% of the time
        for i in 0..stats.len() {
            let (db, duty) = stats[i];
            if duty < 0.05 {
                continue;
            }
            let score = |j: usize| stats[j].1 * 100.0 + stats[j].0;
            let left = i == 0 || score(i - 1) <= score(i);
            let right = i + 1 >= stats.len() || score(i + 1) < score(i);
            if left && right {
                let hz = centre + ((starts[i] + width / 2) as f64 - FFT as f64 / 2.0) * bin_hz;
                println!("{:9.4}   {db:6.1}  {:3.0}%", hz / 1e6, duty * 100.0);
            }
        }
    }
    Ok(())
}
