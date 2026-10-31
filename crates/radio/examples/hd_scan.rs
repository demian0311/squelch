//! Sweep the FM band and flag stations carrying HD Radio (NRSC-5) digital sidebands.
//! cargo run -p radio --release --example hd_scan -- <seconds per step> [MHz…]
//! Stations named after the seconds also get a 10 kHz-step profile from −250 to +250 kHz.
//!
//! Hybrid HD puts a flat block of OFDM carriers 129–198 kHz either side of the analog
//! carrier. A station is flagged when both blocks stand clear of the noise floor.

use radio::device::{IqSource, RtlSdr};
use radio::dsp::u8_to_iq;
use radio::spectrum::{noise_floor, Spectrum};

const RATE: u32 = 2_400_000;
const FFT: usize = 8192;
/// Capture centres covering 88–108 MHz; stations within `USABLE_HZ` of a centre are measured.
const CENTRES_MHZ: [f64; 11] = [88.6, 90.4, 92.2, 94.0, 95.8, 97.6, 99.4, 101.2, 103.0, 104.8, 106.6];
const USABLE_HZ: f64 = 900_000.0;
/// Analog FM must be this far above the floor before its sidebands are worth judging.
const MIN_STATION_DB: f32 = 20.0;
/// Each sideband block must average this far above the floor to count as HD. Measured 2026-10-31:
/// HD stations 5.7–8.9 dB, analog-only at most 2.9 dB.
const MIN_SIDEBAND_DB: f32 = 4.0;

fn main() -> anyhow::Result<()> {
    let secs: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let profile: Vec<f64> = std::env::args().skip(2).filter_map(|s| s.parse().ok()).collect();
    let mut src = RtlSdr::open(0, RATE)?;
    let bin_hz = RATE as f64 / FFT as f64;
    let (mut raw, mut iq) = (vec![0u8; 262_144], Vec::new());
    println!("station    analog  lower   upper   (dB above floor)");
    for c in CENTRES_MHZ {
        let centre = c * 1e6;
        src.set_center(centre as u32)?;
        // discard samples captured before the retune settled
        for _ in 0..4 {
            src.read(&mut raw)?;
        }
        let mut spec = Spectrum::new(FFT, RATE, 1);
        let mut rows = Vec::new();
        let mut acc = vec![0f64; FFT];
        let mut n = 0usize;
        while (n as f64) < secs * RATE as f64 {
            let got = src.read(&mut raw)?;
            u8_to_iq(&raw[..got], &mut iq);
            n += iq.len();
            spec.process(&iq, &mut rows);
            for r in rows.drain(..) {
                acc.iter_mut().zip(&r).for_each(|(a, &d)| *a += 10f64.powf(d as f64 / 10.0));
            }
        }
        let row: Vec<f32> = acc.iter().map(|&p| 10.0 * p.max(1e-30).log10() as f32).collect();
        let floor = noise_floor(&row);
        // mean power (dB above floor) between two offsets from `hz`
        let band = |hz: f64, lo: f64, hi: f64| -> f32 {
            let bin = |f: f64| (((f - centre) / bin_hz) + FFT as f64 / 2.0).round() as usize;
            let (a, b) = if lo < hi { (bin(hz + lo), bin(hz + hi)) } else { (bin(hz + hi), bin(hz + lo)) };
            let p: f64 = acc[a..=b].iter().sum::<f64>() / (b - a + 1) as f64;
            (10.0 * p.log10()) as f32 - floor
        };
        // US stations sit on odd tenths: 88.1, 88.3, …
        let mut k = ((centre - USABLE_HZ) / 200_000.0).ceil() as i64;
        while (k as f64) * 200_000.0 <= centre + USABLE_HZ {
            let hz = k as f64 * 200_000.0 + 100_000.0;
            k += 1;
            if (hz - centre).abs() > USABLE_HZ {
                continue;
            }
            let analog = band(hz, -50_000.0, 50_000.0);
            if analog < MIN_STATION_DB {
                continue;
            }
            let (lower, upper) = (band(hz, -198_000.0, -129_000.0), band(hz, 129_000.0, 198_000.0));
            let hd = lower >= MIN_SIDEBAND_DB && upper >= MIN_SIDEBAND_DB;
            println!("{:6.1} MHz {analog:6.1} {lower:6.1}  {upper:6.1}  {}", hz / 1e6, if hd { "HD" } else { "" });
            if profile.iter().any(|&m| (m * 1e6 - hz).abs() < 1_000.0) {
                let steps: Vec<String> = (-25..25).map(|i| format!("{:.0}", band(hz, i as f64 * 10_000.0, (i + 1) as f64 * 10_000.0))).collect();
                println!("           −250…+250 kHz: {}", steps.join(" "));
            }
        }
    }
    Ok(())
}
