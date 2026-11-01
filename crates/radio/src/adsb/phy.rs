//! Mode S physical layer: pulse magnitude at the capture rate, preamble search, PPM bit slicing, CRC-24.
//!
//! A reply is 1 Mbit/s pulse-position modulation: each bit is two 0.5 µs chips, a pulse in the
//! first means 1. The 8 µs preamble has pulses at 0, 1, 3.5 and 4.5 µs. At 2.4 Msps a chip is
//! 1.2 samples, so chips are measured as the area under the magnitude over each chip's span,
//! tried at `PHASES` sub-sample starts.

use crate::dsp::C32;

/// Long replies (DF17 and up) carry 112 bits, the last 24 of them parity.
pub const LONG_BITS: usize = 112;
pub const LONG_BYTES: usize = LONG_BITS / 8;
const PREAMBLE_CHIPS: usize = 16;
const PREAMBLE_HIGH: [usize; 4] = [0, 2, 7, 9];
/// Chips the preamble leaves empty, and where a pulse would show up first if the timing were off.
const PREAMBLE_QUIET: [usize; 8] = [4, 5, 10, 11, 12, 13, 14, 15];
/// Sub-sample starts tried for each candidate. At 1.2 samples per chip the bits go wrong a fifth of a sample off.
const PHASES: usize = 10;
/// A candidate needs its four preamble samples this many times the mean magnitude.
const GATE: f32 = 2.0;
/// The preamble's pulses against its quiet chips, as a magnitude ratio.
const PREAMBLE_RATIO: f64 = 2.0;
/// Mode S parity generator, x^24 + ... + 1 with the x^24 term dropped.
const POLY: u32 = 0xFFF409;

/// One reply that passed its parity check.
#[derive(Clone, Debug)]
pub struct Frame {
    pub bytes: [u8; LONG_BYTES],
    /// Sample index of the preamble, counted from the first sample ever fed.
    pub at: u64,
    /// Preamble pulses over its quiet chips, dB.
    pub snr_db: f32,
    /// One bit was flipped to make the parity check.
    pub corrected: bool,
}

pub struct Demod {
    /// Samples per chip.
    chip: f64,
    mag: Vec<f32>,
    /// Running sum of `mag`; `cum[k]` is the sum of the first `k`.
    cum: Vec<f64>,
    /// First sample of `mag` not yet searched.
    next: usize,
    /// Samples dropped from the front of `mag` so far.
    base: u64,
    /// The syndrome each single-bit error leaves, with the bit.
    syndromes: Vec<(u32, usize)>,
}

impl Demod {
    pub fn new(rate: u32) -> Self {
        let syndromes = (0..LONG_BITS)
            .map(|k| {
                let mut f = [0u8; LONG_BYTES];
                f[k / 8] |= 0x80 >> (k % 8);
                (syndrome(&f), k)
            })
            .collect();
        Demod { chip: rate as f64 / 2e6, mag: Vec::new(), cum: Vec::new(), next: 0, base: 0, syndromes }
    }

    pub fn process(&mut self, iq: &[C32], out: &mut Vec<Frame>) {
        self.mag.extend(iq.iter().map(|c| c.norm()));
        // a candidate reads this far past its start; the extra covers the phase and the last chip's end sample
        let need = ((PREAMBLE_CHIPS + 2 * LONG_BITS) as f64 * self.chip).ceil() as usize + 3;
        if self.mag.len() < self.next + need {
            return;
        }
        self.cum.clear();
        self.cum.reserve(self.mag.len() + 1);
        let mut s = 0.0f64;
        self.cum.push(0.0);
        for &m in &self.mag {
            s += m as f64;
            self.cum.push(s);
        }
        // replies are short and sparse, so the mean is the noise
        let noise = (s / self.mag.len() as f64) as f32;
        let gate = 4.0 * GATE * noise;
        // whatever the phase, each preamble pulse holds one of the two samples from where it would start at phase 0
        let at = |c: usize| (c as f64 * self.chip).ceil() as usize;
        let probe = [at(0), at(2), at(7), at(9)];
        // samples inside chips 4–5 and 10–15 whatever the phase
        let is_quiet = |t: f64| t >= 0.0 && PREAMBLE_QUIET.contains(&((t / self.chip) as usize));
        let quiet: Vec<usize> = (1..(PREAMBLE_CHIPS as f64 * self.chip) as usize).filter(|&k| is_quiet(k as f64) && is_quiet(k as f64 - 0.999)).collect();
        let end = self.mag.len() - need;
        let mut i = self.next;
        while i < end {
            let m = &self.mag;
            let pulses = probe.iter().map(|&k| m[i + k].max(m[i + k + 1])).sum::<f32>();
            // and the gaps that hold no pulse at any phase are well below them; a steady carrier fails here
            let gaps = quiet.iter().map(|&k| m[i + k]).sum::<f32>() / quiet.len() as f32;
            if pulses < gate || pulses < 4.0 * PREAMBLE_RATIO as f32 * gaps {
                i += 1;
                continue;
            }
            match self.try_at(i, noise) {
                Some(f) => {
                    out.push(f);
                    // the reply's own pulses would look like more preambles
                    i += ((PREAMBLE_CHIPS + 2 * LONG_BITS) as f64 * self.chip) as usize;
                }
                None => i += 1,
            }
        }
        self.next = i;
        let drop = self.next.min(self.mag.len());
        self.mag.drain(..drop);
        self.base += drop as u64;
        self.next -= drop;
    }

    /// Area under the magnitude from sample position `x` to `x + w`, each sample standing for the half sample either side.
    fn area(&self, x: f64, w: f64) -> f64 {
        let s = |y: f64| {
            let y = y + 0.5;
            let k = y.floor() as usize;
            self.cum[k] + (y - k as f64) * self.mag[k] as f64
        };
        s(x + w) - s(x)
    }

    fn chip(&self, x: f64, c: usize) -> f64 {
        self.area(x + c as f64 * self.chip, self.chip)
    }

    fn try_at(&self, i: usize, noise: f32) -> Option<Frame> {
        let mut cands: Vec<(f64, f64, f32)> = Vec::with_capacity(PHASES);
        for p in 0..PHASES {
            let x = i as f64 + p as f64 / PHASES as f64;
            let e: [f64; PREAMBLE_CHIPS] = std::array::from_fn(|c| self.chip(x, c));
            let high = PREAMBLE_HIGH.map(|c| e[c]);
            let low = high.iter().cloned().fold(f64::MAX, f64::min);
            // each pulse stands above the chips beside it
            if [1, 3, 6, 8, 10].iter().any(|&c| e[c] >= low) {
                continue;
            }
            let hi = high.iter().sum::<f64>() / 4.0;
            let quiet = PREAMBLE_QUIET.iter().map(|&c| e[c]).sum::<f64>() / PREAMBLE_QUIET.len() as f64;
            if hi < PREAMBLE_RATIO * quiet || hi < PREAMBLE_RATIO * noise as f64 * self.chip {
                continue;
            }
            cands.push((hi - quiet, x, 20.0 * (hi / quiet.max(1e-9)).log10() as f32));
        }
        cands.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, x, snr_db) in cands {
            let mut bytes = [0u8; LONG_BYTES];
            let mut ok = true;
            for k in 0..LONG_BITS {
                let c = PREAMBLE_CHIPS + 2 * k;
                if self.chip(x, c) > self.chip(x, c + 1) {
                    bytes[k / 8] |= 0x80 >> (k % 8);
                }
                // only extended squitters are decoded; anything else is dropped as soon as its format shows
                if k == 4 && !matches!(bytes[0] >> 3, 17 | 18) {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            let mut corrected = false;
            let syn = syndrome(&bytes);
            if syn != 0 {
                // never past the format bits, which were already taken as 17 or 18
                match self.syndromes.iter().find(|(s, k)| *s == syn && *k >= 5) {
                    Some(&(_, k)) => {
                        bytes[k / 8] ^= 0x80 >> (k % 8);
                        corrected = true;
                    }
                    None => continue,
                }
            }
            return Some(Frame { bytes, at: self.base + i as u64, snr_db, corrected });
        }
        None
    }
}

/// Mode S CRC-24 of `data`: the remainder of `data`·x^24 over the generator.
pub fn crc24(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in data {
        crc ^= (b as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x100_0000 != 0 {
                crc ^= 0x100_0000 | POLY;
            }
        }
    }
    crc & 0xFF_FFFF
}

/// Parity the message bits imply, against the parity the reply carries; 0 when they agree.
pub fn syndrome(f: &[u8; LONG_BYTES]) -> u32 {
    let parity = (f[11] as u32) << 16 | (f[12] as u32) << 8 | f[13] as u32;
    crc24(&f[..11]) ^ parity
}

#[cfg(test)]
pub mod testutil {
    use super::*;

    pub fn hex(s: &str) -> [u8; LONG_BYTES] {
        let mut f = [0u8; LONG_BYTES];
        for (k, b) in f.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap();
        }
        f
    }

    /// I/Q at `rate` with each reply starting `gap_us` after the last, plus a carrier offset and Gaussian noise
    /// (`noise` is the standard deviation of each component). Starts are off the sample grid by varying amounts.
    pub fn synth(frames: &[[u8; LONG_BYTES]], rate: u32, gap_us: f64, amp: f32, noise: f32, offset_hz: f64) -> Vec<C32> {
        let us = rate as f64 / 1e6;
        let total = ((frames.len() as f64 + 1.0) * gap_us * us) as usize;
        // each sample is the share of its span (half a sample either side) under a pulse, as the tuner's filter blurs it
        let mut level = vec![0f32; total];
        for (n, f) in frames.iter().enumerate() {
            let t0 = (n as f64 + 0.5) * gap_us + 0.137 * n as f64 % 1.0;
            let mut c: Vec<bool> = [1, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0].iter().map(|&b| b == 1).collect();
            for k in 0..LONG_BITS {
                let one = f[k / 8] & (0x80 >> (k % 8)) != 0;
                c.push(one);
                c.push(!one);
            }
            for (j, &on) in c.iter().enumerate() {
                if on {
                    let (a, b) = ((t0 + j as f64 * 0.5) * us, (t0 + j as f64 * 0.5 + 0.5) * us);
                    for s in (a - 0.5).floor().max(0.0) as usize..((b + 0.5).ceil() as usize).min(total) {
                        let lap = (b.min(s as f64 + 0.5) - a.max(s as f64 - 0.5)).max(0.0);
                        level[s] += lap as f32;
                    }
                }
            }
        }
        let mut seed = 0x1234_5678u32;
        let mut uni = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((seed >> 8) as f32 + 0.5) / (1u32 << 24) as f32
        };
        let mut gauss = || {
            let (u, v) = (uni(), uni());
            (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
        };
        (0..total)
            .map(|s| {
                let ph = std::f64::consts::TAU * offset_hz * s as f64 / rate as f64;
                let a = amp * level[s].min(1.0);
                C32::new(a * ph.cos() as f32 + noise * gauss(), a * ph.sin() as f32 + noise * gauss())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    const KNOWN: [&str; 4] = ["8D4840D6202CC371C32CE0576098", "8D40621D58C382D690C8AC2863A7", "8D40621D58C386435CC412692AD6", "8D485020994409940838175B284F"];

    #[test]
    fn parity_of_published_frames() {
        for s in KNOWN {
            assert_eq!(syndrome(&hex(s)), 0, "{s}");
        }
        assert_eq!(crc24(&hex(KNOWN[0])[..11]), 0x576098);
    }

    #[test]
    fn recovers_frames_from_noisy_iq() {
        let frames: Vec<_> = KNOWN.iter().cycle().take(40).map(|s| hex(s)).collect();
        // pulses 17 dB above the noise per sample; at 1.2 samples per chip the blur costs more than the noise below about 13 dB
        let iq = synth(&frames, 2_400_000, 300.0, 0.4, 0.04, 310_000.0);
        let mut d = Demod::new(2_400_000);
        let mut out = Vec::new();
        // in pieces, so replies straddle blocks
        for piece in iq.chunks(10_007) {
            d.process(piece, &mut out);
        }
        assert_eq!(out.len(), frames.len());
        for (f, want) in out.iter().zip(&frames) {
            assert_eq!(&f.bytes, want);
            assert!(f.snr_db > 8.0, "{}", f.snr_db);
        }
        // in order, about 300 µs apart
        for w in out.windows(2) {
            let dt = (w[1].at - w[0].at) as f64 / 2.4;
            assert!((dt - 300.0).abs() < 2.0, "{dt}");
        }
    }

    #[test]
    fn corrects_one_bad_bit() {
        let mut f = hex(KNOWN[1]);
        f[7] ^= 0x10;
        let iq = synth(&[f], 2_400_000, 300.0, 0.4, 0.02, 0.0);
        let mut d = Demod::new(2_400_000);
        let mut out = Vec::new();
        d.process(&iq, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].corrected);
        assert_eq!(out[0].bytes, hex(KNOWN[1]));
    }

    #[test]
    fn noise_alone_gives_nothing() {
        let iq = synth(&[], 2_400_000, 2_000_000.0, 0.0, 0.1, 0.0);
        let mut d = Demod::new(2_400_000);
        let mut out = Vec::new();
        for piece in iq.chunks(65_536) {
            d.process(piece, &mut out);
        }
        assert!(out.is_empty(), "{} false frames", out.len());
    }
}

