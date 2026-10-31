//! RDS physical layer: 57 kHz BPSK subcarrier → data bits.
//!
//! The FM multiplex signal is mixed down from 57 kHz and filtered to a few kHz. A Costas
//! loop removes what is left of the carrier offset, and symbol timing runs at the PSK rate
//! (two symbols per bit). Which pairs of symbols form bits is chosen by energy, as the
//! wrong pairing straddles bit edges; differential decoding removes the 180° ambiguity.

use crate::dsp::{lowpass, Decimator, Mixer, C32};
use crate::sonde::fsk::Fsk;

pub const SUBCARRIER_HZ: f64 = 57_000.0;
pub const BIT_RATE: f32 = 1187.5;
/// Baseband rate after decimating the multiplex signal.
const RATE: f32 = 12_000.0;
pub const PILOT_HZ: f64 = 19_000.0;
/// Pilot loop gains per multiplex sample: about 4 Hz loop bandwidth, critically damped.
const PILOT_ALPHA: f64 = 2e-4;
const PILOT_BETA: f64 = 1e-8;
/// The pilot phase detector is low-passed to about 100 Hz before the loop sees it.
const PILOT_LP_HZ: f64 = 100.0;
/// Costas loop gains per baseband sample. The pilot fixes the frequency, so this loop only
/// settles the constant phase between the pilot's third harmonic and the RDS carrier.
const PLL_ALPHA: f32 = 0.002;
const PLL_BETA: f32 = 1e-6;
/// PSK symbols between decisions on which pairs form bits.
const PARITY_WINDOW: usize = 128;

pub struct Phy {
    pilot: Pilot,
    fallback: Mixer,
    stage1: Decimator<C32>,
    stage2: Decimator<C32>,
    phase: f32,
    freq: f32,
    power: f32,
    /// Timing at the PSK symbol rate: two symbols of opposite sign make one biphase bit.
    chips: Fsk,
    prev_chip: f32,
    /// Which of each pair of symbols ends a bit, chosen by where the energy is.
    parity: usize,
    energy: [f32; 2],
    last_energy: [f32; 2],
    count: usize,
    last_bit: bool,
    buf: Vec<C32>,
    s1: Vec<C32>,
    s2: Vec<C32>,
    re: Vec<f32>,
    soft: Vec<f32>,
}

impl Phy {
    /// `mpx_rate` must be a multiple of 24 kHz (240 kHz from the WFM demodulator).
    pub fn new(mpx_rate: u32) -> Self {
        let fs = mpx_rate as f32;
        let d1 = (mpx_rate / 24_000).max(1) as usize;
        let mid_rate = fs / d1 as f32;
        Phy {
            pilot: Pilot::new(mpx_rate as f64),
            fallback: Mixer::new(SUBCARRIER_HZ, mpx_rate as f64),
            stage1: Decimator::new(lowpass(4_000.0 / fs, 8_000.0 / fs), d1),
            stage2: Decimator::new(lowpass(2_600.0 / mid_rate, 2_000.0 / mid_rate), (mid_rate / RATE).round() as usize),
            phase: 0.0,
            freq: 0.0,
            power: 1e-6,
            chips: Fsk::new(RATE as u32, 2.0 * BIT_RATE),
            prev_chip: 0.0,
            parity: 0,
            energy: [0.0; 2],
            last_energy: [0.0; 2],
            count: 0,
            last_bit: false,
            buf: Vec::new(),
            s1: Vec::new(),
            s2: Vec::new(),
            re: Vec::new(),
            soft: Vec::new(),
        }
    }

    pub fn pilot_hz(&self) -> f64 {
        self.pilot.freq_hz()
    }

    /// Carrier loop frequency offset (Hz) and how clearly one bit pairing wins (1 = no preference).
    pub fn lock(&self) -> (f32, f32) {
        let (a, b) = (self.last_energy[0].max(1e-9), self.last_energy[1].max(1e-9));
        (self.freq * RATE / std::f32::consts::TAU, a.max(b) / a.min(b))
    }

    /// Feed FM multiplex samples; decoded data bits are appended to `bits`.
    pub fn process(&mut self, mpx: &[f32], bits: &mut Vec<bool>) {
        self.buf.clear();
        if self.pilot.locked() || self.pilot.warming() {
            // mix down by three times the pilot phase
            self.pilot.run(mpx, Some(&mut self.buf));
        } else {
            self.pilot.run(mpx, None);
            self.buf.extend(mpx.iter().map(|&v| C32::new(v, 0.0)));
            self.fallback.process(&mut self.buf);
        }
        self.s1.clear();
        self.stage1.process(&self.buf, &mut self.s1);
        self.s2.clear();
        self.stage2.process(&self.s1, &mut self.s2);
        self.re.clear();
        for i in 0..self.s2.len() {
            // Costas loop for BPSK: the error is I·Q, normalised by signal power
            let z = self.s2[i] * C32::from_polar(1.0, -self.phase);
            self.power += 0.001 * (z.norm_sqr() - self.power);
            let err = (z.re * z.im / self.power.max(1e-12)).clamp(-1.0, 1.0);
            self.freq += PLL_BETA * err;
            self.phase = (self.phase + self.freq + PLL_ALPHA * err) % std::f32::consts::TAU;
            self.re.push(z.re);
        }
        self.soft.clear();
        self.chips.process(&self.re, &mut self.soft);
        for i in 0..self.soft.len() {
            let c = self.soft[i];
            // a biphase bit is the difference of its two halves
            let v = c - self.prev_chip;
            self.prev_chip = c;
            let k = self.count % 2;
            self.energy[k] += v.abs();
            self.count += 1;
            if self.count % PARITY_WINDOW == 0 {
                self.parity = if self.energy[0] >= self.energy[1] { 0 } else { 1 };
                self.last_energy = self.energy;
                self.energy = [0.0; 2];
            }
            if k == self.parity {
                let bit = v > 0.0;
                bits.push(bit != self.last_bit);
                self.last_bit = bit;
            }
        }
    }
}

/// Phase-locked loop on the 19 kHz stereo pilot. The signal is mixed down by the loop's own
/// phase and low-passed twice, so programme audio near 19 kHz cannot disturb the phase error.
/// The error is read once per `BLOCK` samples and the oscillator for the block comes from a table of
/// nominal steps plus a short series for the small departure, so each sample costs a few independent
/// multiplies instead of a chain through a sine, a cosine and an arctangent. The loop is far too slow
/// (a few hertz wide) to notice an error held for 16 samples.
struct Pilot {
    fs: f64,
    /// e^{jφ} at the start of the next block
    osc: (f64, f64),
    /// e^{jk·w0} for k in 0..=BLOCK: the nominal pilot step
    table: Vec<(f64, f64)>,
    w0: f64,
    w: f64,
    z1: (f64, f64),
    z2: (f64, f64),
    noise: f64,
    lp: f64,
    samples: u64,
}

/// Samples between phase-error updates of the pilot loop.
const BLOCK: usize = 16;

impl Pilot {
    fn new(fs: f64) -> Self {
        let w = std::f64::consts::TAU * PILOT_HZ / fs;
        let table = (0..=BLOCK)
            .map(|k| {
                let a = k as f64 * w;
                (a.cos(), a.sin())
            })
            .collect();
        Pilot { fs, osc: (1.0, 0.0), table, w0: w, w, z1: (0.0, 0.0), z2: (0.0, 0.0), noise: 1e-9, lp: 1.0 - (-std::f64::consts::TAU * PILOT_LP_HZ / fs).exp(), samples: 0 }
    }

    /// Track the pilot through `mpx`. With `out`, also append every sample mixed down by three times the
    /// pilot phase, which puts the 57 kHz RDS subcarrier at 0 Hz.
    fn run(&mut self, mpx: &[f32], mut out: Option<&mut Vec<C32>>) {
        let k = self.lp;
        for block in mpx.chunks(BLOCK) {
            let n = block.len();
            let e = crate::dsp::fast_atan2(self.z2.1 as f32, self.z2.0 as f32) as f64;
            self.w += PILOT_BETA * e * n as f64;
            let step = self.w + PILOT_ALPHA * e;
            // e^{j·i·step} = e^{j·i·w0} · e^{j·i·d} with d tiny, so cos(i·d) ≈ 1 - (i·d)²/2 and sin(i·d) ≈ i·d
            let d = step - self.w0;
            let (bc, bs) = self.osc;
            let at = |i: usize| {
                let (tc, ts) = self.table[i];
                let a = i as f64 * d;
                let (dc, ds) = (1.0 - a * a / 2.0, a);
                let (rc, rs) = (tc * dc - ts * ds, tc * ds + ts * dc);
                (bc * rc - bs * rs, bc * rs + bs * rc)
            };
            let (mut z1, mut z2, mut noise) = (self.z1, self.z2, self.noise);
            for (i, &x) in block.iter().enumerate() {
                let (c, s) = at(i);
                let x = x as f64;
                z1.0 += k * (x * c - z1.0);
                z1.1 += k * (-x * s - z1.1);
                z2.0 += k * (z1.0 - z2.0);
                z2.1 += k * (z1.1 - z2.1);
                noise += k * (x * x - noise);
                if let Some(out) = out.as_deref_mut() {
                    let p = C32::new(c as f32, s as f32);
                    out.push(C32::new(x as f32, 0.0) * (p * p * p).conj());
                }
            }
            (self.z1, self.z2, self.noise) = (z1, z2, noise);
            // where the oscillator stands for the next block, kept on the unit circle
            let (c, s) = at(n);
            let m = (c * c + s * s).sqrt();
            self.osc = (c / m, s / m);
            self.samples += n as u64;
        }
    }

    fn warming(&self) -> bool {
        self.samples < self.fs as u64
    }

    /// In phase with a pilot that stands clear of the rest of the signal.
    fn locked(&self) -> bool {
        let (re, im) = self.z2;
        re > 0.0 && im.abs() < re && re * re > 0.002 * self.noise
    }

    fn freq_hz(&self) -> f64 {
        self.w * self.fs / std::f64::consts::TAU
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// FM multiplex at `rate` carrying `data` as RDS, plus a pilot and an audio tone.
    pub fn mpx(data: &[bool], rate: u32) -> Vec<f32> {
        // differential encoding, then each bit as a +/- biphase symbol
        let mut level = false;
        let sent: Vec<bool> = data
            .iter()
            .map(|&d| {
                level ^= d;
                level
            })
            .collect();
        let fs = rate as f64;
        let n = (sent.len() as f64 * fs / BIT_RATE as f64) as usize;
        let tau = std::f64::consts::TAU;
        (0..n)
            .map(|i| {
                let t = i as f64 / fs;
                let pos = t * BIT_RATE as f64;
                let k = pos as usize;
                let sym = if sent[k] { 1.0 } else { -1.0 };
                let chip = if pos.fract() < 0.5 { sym } else { -sym };
                let rds = 0.05 * chip * (tau * SUBCARRIER_HZ * t + 0.7).cos();
                let pilot = 0.09 * (tau * 19_000.0 * t).sin();
                let audio = 0.5 * (tau * 1_000.0 * t).sin();
                (rds + pilot + audio) as f32
            })
            .collect()
    }

    pub fn pattern(n: usize) -> Vec<bool> {
        let mut x = 0x1D3u16;
        (0..n)
            .map(|_| {
                let b = x & 1 == 1;
                x = (x >> 1) ^ if b { 0x8016 } else { 0 };
                b
            })
            .collect()
    }

    #[test]
    fn recovers_bits_from_the_subcarrier() {
        let data = pattern(3_000);
        let sig = mpx(&data, 240_000);
        let mut bits = Vec::new();
        Phy::new(240_000).process(&sig, &mut bits);
        assert!(bits.len() > 2_900, "{} bits", bits.len());
        let tail = &bits[1_000..bits.len() - 20];
        let best = (0..40).map(|lag| tail.iter().zip(&data[980 + lag..]).filter(|(a, b)| a != b).count()).min().unwrap();
        assert!(best < 5, "{best} bit errors after lock");
    }
}
