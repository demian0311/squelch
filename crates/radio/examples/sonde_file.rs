//! Decode radiosondes from a WAV recording.
//! `sonde_file <file.wav>`: mono = FM-demodulated audio, stereo = I/Q.

use radio::dsp::{lowpass, Decimator, Mixer};
use radio::sonde::{all_decoders, fsk::Fsk};
use radio::spectrum::Spectrum;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: sonde_file <file.wav>");
    let data = std::fs::read(&path)?;
    let (mut pos, mut rate, mut channels, mut bits) = (12usize, 0u32, 0u16, 0u16);
    let mut samples: &[u8] = &[];
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = u32::from_le_bytes(data[pos + 4..pos + 8].try_into()?) as usize;
        let body = &data[pos + 8..(pos + 8 + len).min(data.len())];
        if id == b"fmt " {
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into()?);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            samples = body;
        }
        pos += 8 + len + (len & 1);
    }
    let vals: Vec<f32> = match bits {
        8 => samples.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        16 => samples.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
        b => anyhow::bail!("{b}-bit WAV not handled"),
    };
    let freq: Vec<f32> = if channels == 2 {
        let k = rate as f32 / std::f32::consts::TAU;
        let mut iq: Vec<num_complex::Complex32> = vals.chunks_exact(2).map(|c| num_complex::Complex32::new(c[0], c[1])).collect();
        // find the strongest narrow signal and bring it to the centre
        let mut rows = Vec::new();
        Spectrum::new(1000, rate, 10).process(&iq, &mut rows);
        let mut avg = vec![0f32; 1000];
        for r in &rows {
            for (a, v) in avg.iter_mut().zip(r) {
                *a += 10f32.powf(v / 10.0);
            }
        }
        let peak = (0..1000).filter(|&i| !(495..=505).contains(&i)).max_by(|&a, &b| avg[a].total_cmp(&avg[b])).unwrap();
        let offset = (peak as f64 - 500.0) * rate as f64 / 1000.0;
        println!("strongest signal {offset:+.0} Hz from centre");
        Mixer::new(offset, rate as f64).process(&mut iq);
        let mut narrow = Vec::new();
        Decimator::new(lowpass(10_000.0 / rate as f32, 4_000.0 / rate as f32), 1).process(&iq, &mut narrow);
        narrow.windows(2).map(|w| (w[1] * w[0].conj()).arg() * k).collect()
    } else {
        vals
    };
    println!("{path}: {rate} Hz, {channels} channel(s), {:.1} s", freq.len() as f32 / rate as f32);
    for mut dec in all_decoders() {
        let mut soft = Vec::new();
        Fsk::new(rate, dec.baud()).process(&freq, &mut soft);
        let mut out = Vec::new();
        dec.push(&soft, &mut out);
        println!("{}: {} frames", dec.kind(), out.len());
        for u in out.iter().take(6).chain(out.iter().last()) {
            println!("  {u:?}");
        }
    }
    Ok(())
}
