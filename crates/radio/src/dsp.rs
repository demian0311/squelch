//! Small DSP building blocks: windowed-sinc filters, decimators, a mixer.

use num_complex::Complex32;
use std::f32::consts::PI;

pub type C32 = Complex32;

/// Convert interleaved unsigned 8-bit I/Q to complex samples in -1..1.
pub fn u8_to_iq(raw: &[u8], out: &mut Vec<C32>) {
    out.clear();
    out.extend(raw.chunks_exact(2).map(|p| C32::new((p[0] as f32 - 127.4) / 128.0, (p[1] as f32 - 127.4) / 128.0)));
}

/// Low-pass taps. `cutoff` and `transition` are fractions of the sample rate.
pub fn lowpass(cutoff: f32, transition: f32) -> Vec<f32> {
    // Hamming window: about 3.3 / transition taps for ~53 dB stopband.
    let mut n = (3.3 / transition).ceil() as usize;
    n = n.clamp(15, 1023) | 1;
    let m = (n - 1) as f32;
    let mut taps: Vec<f32> = (0..n)
        .map(|i| {
            let x = i as f32 - m / 2.0;
            let sinc = if x == 0.0 { 2.0 * cutoff } else { (2.0 * PI * cutoff * x).sin() / (PI * x) };
            let w = 0.54 - 0.46 * (2.0 * PI * i as f32 / m).cos();
            sinc * w
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    for t in taps.iter_mut() {
        *t /= sum;
    }
    taps
}

/// FIR filter that keeps one output for every `factor` inputs.
pub struct Decimator<T> {
    taps: Vec<f32>,
    // history written twice so the last `n` samples are always one contiguous slice
    hist: Vec<T>,
    pos: usize,
    factor: usize,
    phase: usize,
}

impl<T: Copy + Default + std::ops::Mul<f32, Output = T> + std::ops::Add<Output = T>> Decimator<T> {
    pub fn new(taps: Vec<f32>, factor: usize) -> Self {
        let n = taps.len();
        Decimator { taps, hist: vec![T::default(); 2 * n], pos: 0, factor, phase: 0 }
    }

    pub fn process(&mut self, input: &[T], out: &mut Vec<T>) {
        let n = self.taps.len();
        for &x in input {
            self.pos = (self.pos + 1) % n;
            self.hist[self.pos] = x;
            self.hist[self.pos + n] = x;
            self.phase += 1;
            if self.phase == self.factor {
                self.phase = 0;
                // taps are symmetric, so oldest-to-newest order does not matter
                let window = &self.hist[self.pos + 1..self.pos + 1 + n];
                let mut acc = T::default();
                for (h, t) in window.iter().zip(self.taps.iter()) {
                    acc = acc + *h * *t;
                }
                out.push(acc);
            }
        }
    }
}

/// Multiplies by e^{-j 2π f t}, moving `shift_hz` down to 0 Hz.
pub struct Mixer {
    phase: f64,
    step: f64,
}

impl Mixer {
    pub fn new(shift_hz: f64, rate: f64) -> Self {
        Mixer { phase: 0.0, step: -std::f64::consts::TAU * shift_hz / rate }
    }

    pub fn set(&mut self, shift_hz: f64, rate: f64) {
        self.step = -std::f64::consts::TAU * shift_hz / rate;
    }

    pub fn process(&mut self, buf: &mut [C32]) {
        let rot = C32::new(self.step.cos() as f32, self.step.sin() as f32);
        let mut osc = C32::new(self.phase.cos() as f32, self.phase.sin() as f32);
        for (k, s) in buf.iter_mut().enumerate() {
            *s *= osc;
            osc *= rot;
            if k % 1024 == 1023 {
                osc /= osc.norm();
            }
        }
        self.phase = (self.phase + self.step * buf.len() as f64) % std::f64::consts::TAU;
    }
}

/// One-pole high-pass that removes DC from audio.
pub struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    pub fn new() -> Self {
        DcBlock { x1: 0.0, y1: 0.0 }
    }
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let y = *s - self.x1 + 0.995 * self.y1;
            self.x1 = *s;
            self.y1 = y;
            *s = y;
        }
    }
}

impl Default for DcBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Slow automatic gain control aiming for a fixed audio peak level.
pub struct Agc {
    gain: f32,
}

impl Agc {
    pub fn new() -> Self {
        Agc { gain: 1.0 }
    }
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let y = *s * self.gain;
            let a = y.abs();
            if a > 0.5 {
                self.gain *= 0.995;
            } else {
                self.gain *= 1.0001;
            }
            self.gain = self.gain.clamp(0.01, 5000.0);
            *s = y.clamp(-1.0, 1.0);
        }
    }
}

impl Default for Agc {
    fn default() -> Self {
        Self::new()
    }
}

/// Mean power of a block in dB (full scale = 0 dB).
pub fn power_db(buf: &[C32]) -> f32 {
    if buf.is_empty() {
        return -120.0;
    }
    let p: f32 = buf.iter().map(|c| c.norm_sqr()).sum::<f32>() / buf.len() as f32;
    10.0 * (p.max(1e-12)).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowpass_has_unity_dc_gain() {
        let t = lowpass(0.1, 0.05);
        assert!((t.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert_eq!(t.len() % 2, 1);
    }

    #[test]
    fn decimator_passes_low_and_blocks_high_tones() {
        let fs = 48_000.0;
        let tone = |f: f32| -> Vec<f32> { (0..9600).map(|n| (2.0 * PI * f * n as f32 / fs).sin()).collect() };
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let mut d = Decimator::<f32>::new(lowpass(2000.0 / fs, 1000.0 / fs), 4);
        let mut out = Vec::new();
        d.process(&tone(500.0), &mut out);
        assert!(rms(&out[200..]) > 0.65, "pass band attenuated: {}", rms(&out[200..]));
        let mut d = Decimator::<f32>::new(lowpass(2000.0 / fs, 1000.0 / fs), 4);
        out.clear();
        d.process(&tone(5000.0), &mut out);
        assert!(rms(&out[200..]) < 0.01, "stop band leaked: {}", rms(&out[200..]));
    }

    #[test]
    fn mixer_moves_a_tone_to_dc() {
        let fs = 240_000.0;
        let mut buf: Vec<C32> = (0..4800).map(|n| C32::from_polar(1.0, 2.0 * PI * 10_000.0 * n as f32 / fs)).collect();
        Mixer::new(10_000.0, fs as f64).process(&mut buf);
        let mean = buf.iter().sum::<C32>() / buf.len() as f32;
        assert!(mean.norm() > 0.99);
    }
}
