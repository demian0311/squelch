//! Averaged power spectrum of the whole capture, one row per display frame.

use crate::dsp::C32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

pub struct Spectrum {
    size: usize,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buf: Vec<C32>,
    pending: Vec<C32>,
    acc: Vec<f32>,
    /// frames seen and frames transformed since the last row
    seen: usize,
    frames: usize,
    frames_per_row: usize,
}

/// One frame in this many is transformed; the rest only count toward the row's timing. Each row then averages
/// half the frames (the noise floor's grain grows from about ±0.9 to ±1.3 dB) for half the FFT work.
const EVERY: usize = 2;

impl Spectrum {
    /// `rows_per_sec` sets how many averaged rows come out per second of input.
    pub fn new(size: usize, sample_rate: u32, rows_per_sec: u32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(size);
        let window = (0..size)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos())
            .collect();
        let frames_per_row = ((sample_rate as usize / size) / rows_per_sec as usize).max(1);
        Spectrum { size, fft, window, buf: vec![C32::default(); size], pending: Vec::new(), acc: vec![0.0; size], seen: 0, frames: 0, frames_per_row }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Feed samples; every completed row (dBFS per bin, lowest frequency first) is pushed to `rows`.
    pub fn process(&mut self, input: &[C32], rows: &mut Vec<Vec<f32>>) {
        self.pending.extend_from_slice(input);
        let n = self.size;
        let mut start = 0;
        while self.pending.len() - start >= n {
            if self.seen % EVERY == 0 || self.frames_per_row < EVERY {
                for i in 0..n {
                    self.buf[i] = self.pending[start + i] * self.window[i];
                }
                self.fft.process(&mut self.buf);
                for i in 0..n {
                    self.acc[i] += self.buf[i].norm_sqr();
                }
                self.frames += 1;
            }
            start += n;
            self.seen += 1;
            if self.seen == self.frames_per_row {
                // Hann window coherent gain 0.5 → scale so a full-scale tone reads 0 dB
                let norm = 1.0 / (self.frames as f32 * (n as f32 * 0.5).powi(2));
                let half = n / 2;
                let row = (0..n)
                    .map(|i| {
                        let k = (i + half) % n;
                        10.0 * (self.acc[k] * norm).max(1e-14).log10()
                    })
                    .collect();
                rows.push(row);
                self.acc.iter_mut().for_each(|a| *a = 0.0);
                self.frames = 0;
                self.seen = 0;
            }
        }
        self.pending.drain(..start);
    }
}

/// Noise floor estimate: the 30th percentile of a row.
pub fn noise_floor(row: &[f32]) -> f32 {
    let mut v: Vec<f32> = row.to_vec();
    let k = v.len() * 3 / 10;
    v.select_nth_unstable_by(k, |a, b| a.partial_cmp(b).unwrap());
    v[k]
}

/// Rows after a retune or a gain change that are taken as the floor outright, with the squelch held shut:
/// the tuner's gain is still settling on the new band, and a slow average would lag it by a second.
pub const SETTLE_ROWS: u32 = 6;

/// The capture's noise floor: each row's `noise_floor`, averaged slowly between retunes.
pub struct Floor {
    pub db: f32,
    settle: u32,
}

impl Default for Floor {
    fn default() -> Self {
        Self::new()
    }
}

impl Floor {
    pub fn new() -> Self {
        Self { db: -60.0, settle: SETTLE_ROWS }
    }

    /// The capture moved or the gain changed: the next rows are another band's noise.
    pub fn retuned(&mut self) {
        self.settle = SETTLE_ROWS;
    }

    /// Feed a row. Returns false while settling, when the floor is not yet one to measure a signal against.
    pub fn update(&mut self, row: &[f32]) -> bool {
        let f = noise_floor(row);
        if self.settle > 0 {
            self.settle -= 1;
            self.db = f;
            false
        } else {
            self.db = 0.9 * self.db + 0.1 * f;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retune_takes_the_new_floor_at_once_and_settles_first() {
        let row = |db: f32| vec![db; 64];
        let mut f = Floor::new();
        for _ in 0..SETTLE_ROWS + 10 {
            f.update(&row(-60.0));
        }
        assert_eq!(f.db, -60.0);
        f.retuned();
        assert!(!f.update(&row(-40.0)), "the first row after a retune is still settling");
        assert_eq!(f.db, -40.0, "and the floor is that row's, not an average with the old band");
        for _ in 1..SETTLE_ROWS {
            assert!(!f.update(&row(-40.0)));
        }
        assert!(f.update(&row(-40.0)), "settled after SETTLE_ROWS");
        f.update(&row(-50.0));
        assert!((f.db - -41.0).abs() < 1e-4, "then back to the slow average");
    }

    #[test]
    fn a_tone_lands_in_the_right_bin() {
        let fs = 2_400_000u32;
        let mut s = Spectrum::new(2048, fs, 30);
        let f = 300_000.0f32;
        let input: Vec<C32> = (0..fs as usize / 10).map(|n| C32::from_polar(1.0, std::f32::consts::TAU * f * n as f32 / fs as f32)).collect();
        let mut rows = Vec::new();
        s.process(&input, &mut rows);
        assert!(!rows.is_empty());
        let row = &rows[0];
        let (peak, val) = row.iter().enumerate().fold((0, f32::MIN), |m, (i, &v)| if v > m.1 { (i, v) } else { m });
        let expect = 1024 + (f / fs as f32 * 2048.0).round() as usize;
        assert!((peak as i64 - expect as i64).abs() <= 1, "peak {peak} expected {expect}");
        assert!(val > -3.0 && val < 1.0, "peak level {val}");
        assert!(noise_floor(row) < -60.0);
    }
}
