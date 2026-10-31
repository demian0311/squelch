//! Check narrow carriers for P25 Phase 1 frames and print each one's network code and frame types.
//! cargo run -p radio --release --example p25_probe -- <seconds each> <MHz>…
//!
//! FM-demodulates to 48 kHz (10 samples per 4800-baud symbol) and correlates against the
//! 24-symbol frame sync. After each sync the next 8 symbols carry NAC (12 bits) and DUID (4 bits).

use num_complex::Complex32;
use radio::device::{Gain, IqSource, RtlSdr};
use radio::dsp::{lowpass, u8_to_iq, Decimator, Mixer};
use radio::p25::tsbk::{self, Message};
use std::collections::BTreeMap;

const RATE: u32 = 2_400_000;
const SPS: usize = 10;
/// Frame sync 0x5575F5FF77FF as C4FM symbols: dibit 01 → +3, 11 → −3.
const SYNC: u64 = 0x5575_F5FF_77FF;
/// Normalised correlation a sync must reach.
const MIN_CORR: f32 = 0.85;
/// Sync, NID and three TSBKs with their status symbols.
const FRAME_DIBITS: usize = 370;

fn sync_symbols() -> [f32; 24] {
    let mut s = [0f32; 24];
    for (i, v) in s.iter_mut().enumerate() {
        *v = if (SYNC >> (46 - 2 * i)) & 3 == 1 { 3.0 } else { -3.0 };
    }
    s
}

fn duid_name(d: u8) -> &'static str {
    match d {
        0x0 => "HDU voice header",
        0x3 => "TDU end",
        0x5 => "LDU1 voice",
        0x7 => "TSDU control",
        0xA => "LDU2 voice",
        0xC => "PDU data",
        0xF => "TDULC end",
        _ => "?",
    }
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let secs: f64 = a[1].parse()?;
    let mut src = RtlSdr::open(0, RATE)?;
    src.set_gain(Gain::Tenths(*src.gains().last().unwrap_or(&400)))?;
    let sync = sync_symbols();
    for m in &a[2..] {
        let mhz: f64 = m.parse()?;
        // tune 300 kHz away so the dongle's DC spike stays off the channel
        src.set_center((mhz * 1e6 + 300_000.0) as u32)?;
        let mut mixer = Mixer::new(-300_000.0, RATE as f64);
        let fs = RATE as f32;
        let mut d1 = Decimator::new(lowpass(30_000.0 / fs, 60_000.0 / fs), 10);
        let f2 = 240_000.0;
        let mut d2 = Decimator::new(lowpass(7_000.0 / f2, 4_000.0 / f2), 5);
        let (mut raw, mut iq, mut s1, mut s2) = (vec![0u8; 262_144], Vec::new(), Vec::new(), Vec::new());
        for _ in 0..4 {
            src.read(&mut raw)?;
        }
        let mut fm = Vec::new();
        let mut last = Complex32::new(1.0, 0.0);
        while (fm.len() as f64) < secs * 48_000.0 {
            let got = src.read(&mut raw)?;
            u8_to_iq(&raw[..got], &mut iq);
            mixer.process(&mut iq);
            s1.clear();
            d1.process(&iq, &mut s1);
            s2.clear();
            d2.process(&s1, &mut s2);
            for &s in &s2 {
                fm.push((s * last.conj()).arg());
                last = s;
            }
        }
        // remove the offset left by tuning error, in 0.5 s blocks
        for blk in fm.chunks_mut(24_000) {
            let mean = blk.iter().sum::<f32>() / blk.len() as f32;
            blk.iter_mut().for_each(|x| *x -= mean);
        }
        let span = 24 * SPS;
        let (mut syncs, mut inverted) = (0usize, 0usize);
        let mut nacs: BTreeMap<u16, usize> = BTreeMap::new();
        let mut duids: BTreeMap<u8, usize> = BTreeMap::new();
        let (mut tsbks, mut good) = (0usize, 0usize);
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let mut t = 0usize;
        while t + FRAME_DIBITS * SPS < fm.len() {
            let corr: f32 = (0..24).map(|k| sync[k] * fm[t + k * SPS]).sum();
            let energy: f32 = (0..24).map(|k| fm[t + k * SPS].abs()).sum::<f32>() * 3.0;
            let c = corr / energy.max(1e-9);
            if c.abs() < MIN_CORR {
                t += 1;
                continue;
            }
            // step to the best-correlating sample within this symbol
            let score = |u: usize| (0..24).map(|k| sync[k] * fm[u + k * SPS]).sum::<f32>().abs();
            let best = (t..t + SPS).max_by(|&x, &y| score(x).total_cmp(&score(y))).unwrap();
            let sign = if c < 0.0 { -1.0 } else { 1.0 };
            if c < 0.0 {
                inverted += 1;
            }
            let amp = (0..24).map(|k| fm[best + k * SPS].abs()).sum::<f32>() / 24.0;
            // dibits from the frame start, status symbols (every 36th) dropped
            let data: Vec<u8> = (0..FRAME_DIBITS)
                .filter(|k| k % 36 != 35)
                .take_while(|k| best + k * SPS < fm.len())
                .map(|k| {
                    let v = sign * fm[best + k * SPS] / amp * 3.0;
                    if v > 2.0 { 1 } else if v > 0.0 { 0 } else if v > -2.0 { 2 } else { 3 }
                })
                .collect();
            let bits = data[24..32].iter().fold(0u16, |b, &d| (b << 2) | d as u16);
            if bits & 0xF == 7 {
                for blk in 0..3 {
                    let at = 56 + 98 * blk;
                    let Some(chunk) = data.get(at..at + 98) else { break };
                    tsbks += 1;
                    let Some(b) = tsbk::decode(chunk.try_into().unwrap()) else { break };
                    good += 1;
                    let m = Message::parse(&b);
                    if !matches!(m, Message::Other { .. }) || seen.len() < 40 {
                        let key = format!("{m:?}");
                        *seen.entry(key).or_default() += 1;
                    }
                    if b[0] & 0x80 != 0 {
                        break;
                    }
                }
            }
            syncs += 1;
            *nacs.entry(bits >> 4).or_default() += 1;
            *duids.entry((bits & 0xF) as u8).or_default() += 1;
            t = best + span;
        }
        println!("{mhz:.4} MHz: {syncs} syncs in {secs} s{}", if inverted > 0 { format!(" ({inverted} inverted)") } else { String::new() });
        let mut top: Vec<_> = nacs.into_iter().collect();
        top.sort_by(|x, y| y.1.cmp(&x.1));
        for (nac, n) in top.iter().take(3) {
            println!("    NAC {nac:03X} ×{n}");
        }
        if tsbks > 0 {
            println!("    TSBK {good}/{tsbks} pass CRC");
        }
        for (m, n) in &seen {
            println!("      {m} ×{n}");
        }
        for (d, n) in duids {
            if n * 20 >= syncs {
                println!("    DUID {d:X} {} ×{n}", duid_name(d));
            }
        }
    }
    Ok(())
}
