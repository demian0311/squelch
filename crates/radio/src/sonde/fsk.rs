//! Two-level FSK symbol recovery from frequency samples.
//!
//! A one-symbol moving average is the matched filter for rectangular symbols. Its zero
//! crossings mark symbol boundaries, so a clock nudged towards them samples each symbol
//! half a symbol after the boundary, where the filter output peaks.

/// How fast the clock follows a crossing: the fraction of the timing error corrected each time.
const TIMING_GAIN: f32 = 0.08;
/// The frequency offset of the transmitter is tracked over this many symbols.
const DC_SYMBOLS: f32 = 64.0;

pub struct Fsk {
    step: f32,
    window: Vec<f32>,
    pos: usize,
    sum: f32,
    dc: f32,
    dc_a: f32,
    phase: f32,
    prev: f32,
}

impl Fsk {
    pub fn new(sample_rate: u32, baud: f32) -> Self {
        let sps = sample_rate as f32 / baud;
        let len = sps.round().max(1.0) as usize;
        Fsk { step: 1.0 / sps, window: vec![0.0; len], pos: 0, sum: 0.0, dc: 0.0, dc_a: 1.0 / (sps * DC_SYMBOLS), phase: 0.0, prev: 0.0 }
    }

    /// Appends one soft value per symbol: positive is the higher tone.
    pub fn process(&mut self, freq: &[f32], out: &mut Vec<f32>) {
        let n = self.window.len() as f32;
        for &f in freq {
            self.dc += self.dc_a * (f - self.dc);
            let v = f - self.dc;
            self.sum += v - self.window[self.pos];
            self.window[self.pos] = v;
            self.pos = (self.pos + 1) % self.window.len();
            let y = self.sum / n;

            let before = self.phase;
            self.phase += self.step;
            if (self.prev < 0.0) != (y < 0.0) {
                // where between the two samples the crossing fell, in symbols
                let frac = self.prev / (self.prev - y);
                let mut at = before + frac * self.step;
                at -= at.round();
                self.phase -= TIMING_GAIN * at;
            }
            if before < 0.5 && self.phase >= 0.5 {
                out.push(y);
            }
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            self.prev = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sonde::testutil::fsk_signal;

    fn bits(n: usize) -> Vec<bool> {
        let mut x = 0xACE1u16;
        (0..n)
            .map(|_| {
                let b = x & 1 == 1;
                x = (x >> 1) ^ if b { 0xB400 } else { 0 };
                b
            })
            .collect()
    }

    #[test]
    fn recovers_bits_despite_offset_and_clock_error() {
        let tx = bits(4000);
        // transmitter clock 0.2 % fast, 1.5 kHz off frequency
        let sig = fsk_signal(&tx, 48_000, 2500.0 * 1.002, 2_000.0, 1_500.0, 2_000.0);
        let mut fsk = Fsk::new(48_000, 2500.0);
        let mut soft = Vec::new();
        fsk.process(&sig, &mut soft);
        let rx: Vec<bool> = soft.iter().map(|&s| s > 0.0).collect();
        // after settling, the received bits must match the sent ones at some small lag
        let tail = &rx[500..rx.len() - 10];
        let best = (0..20)
            .map(|lag| tail.iter().zip(&tx[500 - 10 + lag..]).filter(|(a, b)| a != b).count())
            .min()
            .unwrap();
        assert_eq!(best, 0, "bit errors after lock");
    }
}
