//! Cuts one narrow channel out of the capture and FM-demodulates it for a data decoder.

use crate::dsp::{lowpass, Decimator, Mixer, C32};

/// Decoders run on frequency samples at this rate.
pub const RATE: u32 = 48_000;

pub struct Channel {
    offset_hz: f64,
    mixer: Mixer,
    stage1: Decimator<C32>,
    stage2: Decimator<C32>,
    last: C32,
    buf: Vec<C32>,
    s1: Vec<C32>,
    s2: Vec<C32>,
}

impl Channel {
    /// `offset_hz` is the channel centre relative to the capture centre; `in_rate` a multiple of 240 kHz.
    pub fn new(offset_hz: f64, in_rate: u32, bandwidth_hz: f32) -> Self {
        let fs = in_rate as f32;
        let d1 = (in_rate / 240_000).max(1) as usize;
        let mid = fs / d1 as f32;
        let half = bandwidth_hz / 2.0;
        Channel {
            offset_hz,
            mixer: Mixer::new(offset_hz, in_rate as f64),
            // Only what folds onto the channel after decimating matters, so the first filter
            // may pass everything up to the next alias: a short filter with a wide transition.
            stage1: Decimator::new(lowpass(mid / 2.0 / fs, (mid - 2.0 * half) / fs), d1),
            stage2: Decimator::new(lowpass((half + 4_000.0) / mid, 8_000.0 / mid), (mid as u32 / RATE) as usize),
            last: C32::new(1.0, 0.0),
            buf: Vec::new(),
            s1: Vec::new(),
            s2: Vec::new(),
        }
    }

    pub fn offset_hz(&self) -> f64 {
        self.offset_hz
    }

    /// Appends the instantaneous frequency, in Hz from the channel centre, at `RATE`.
    pub fn process(&mut self, iq: &[C32], out: &mut Vec<f32>) {
        self.buf.clear();
        self.buf.extend_from_slice(iq);
        self.mixer.process(&mut self.buf);
        self.s1.clear();
        self.stage1.process(&self.buf, &mut self.s1);
        self.s2.clear();
        self.stage2.process(&self.s1, &mut self.s2);
        let k = RATE as f32 / std::f32::consts::TAU;
        for &s in &self.s2 {
            out.push((s * self.last.conj()).arg() * k);
            self.last = s;
        }
    }
}
