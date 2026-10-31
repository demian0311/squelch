//! Follow a P25 site's calls from its control channel, printing state every 2 s.
//! cargo run -p radio --release --example p25_live -- <control MHz> <seconds> [voice.f32]
//! Voice is written as 48 kHz mono f32 LE when a file is named.

use radio::detect::detect;
use radio::device::{Gain, IqSource, RtlSdr};
use radio::dsp::u8_to_iq;
use radio::engine::{DETECT_DB, FFT_SIZE, SAMPLE_RATE};
use radio::p25::P25;
use radio::spectrum::{noise_floor, Spectrum};
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let mhz: f64 = a[1].parse()?;
    let secs: f64 = a[2].parse()?;
    let mut out = a.get(3).map(std::fs::File::create).transpose()?;
    let mut src = RtlSdr::open(0, SAMPLE_RATE)?;
    src.set_gain(Gain::Tenths(*src.gains().last().unwrap_or(&400)))?;
    let mut center = mhz * 1e6 + 300_000.0;
    src.set_center(center as u32)?;
    let mut p25 = P25::new(SAMPLE_RATE, mhz * 1e6);
    let mut spec = Spectrum::new(FFT_SIZE, SAMPLE_RATE, 5);
    let (mut raw, mut iq, mut rows, mut pcm) = (vec![0u8; 131_072], Vec::new(), Vec::new(), Vec::new());
    let mut signals = Vec::new();
    let (mut t, mut last_print, mut audio_secs) = (0.0, 0.0, 0.0);
    while t < secs {
        let n = src.read(&mut raw)?;
        u8_to_iq(&raw[..n], &mut iq);
        t += iq.len() as f64 / SAMPLE_RATE as f64;
        rows.clear();
        spec.process(&iq, &mut rows);
        if let Some(r) = rows.last() {
            signals = detect(r, noise_floor(r), DETECT_DB, center, SAMPLE_RATE as f64, 3);
        }
        if let Some(c) = p25.process(&iq, center, &signals, &mut pcm) {
            center = c;
            src.set_center(center as u32)?;
            println!("{t:6.1} s  dongle → {:.4} MHz", center / 1e6);
        }
        audio_secs += pcm.len() as f64 / 48_000.0;
        if let Some(f) = &mut out {
            for s in &pcm {
                f.write_all(&s.to_le_bytes())?;
            }
        }
        if t - last_print >= 2.0 {
            last_print = t;
            let i = p25.info();
            let call = i.call.as_ref().map(|c| format!("TG {} from {} on {:.5} MHz, {:.0}% bad frames", c.group, c.source, c.hz / 1e6, i.voice_errors_pct)).unwrap_or_default();
            println!(
                "{t:6.1} s  {:9} NAC {:?} sys {:?} site {:?}-{:?} ppm {:.1} msgs/s {:.1} audio {audio_secs:.1} s  {call}",
                i.state,
                i.nac.map(|n| format!("{n:03X}")),
                i.system.map(|n| format!("{n:03X}")),
                i.rfss,
                i.site,
                i.ppm.unwrap_or(f64::NAN),
                i.messages_per_sec
            );
        }
    }
    for c in p25.info().recent {
        println!("call TG {} src {} {:.5} MHz enc {} tdma {} at {:.0} s", c.group, c.source, c.hz / 1e6, c.encrypted, c.tdma, c.at_secs);
    }
    if let Some(e) = p25.info().voice_error {
        println!("voice: {e}");
    }
    Ok(())
}
