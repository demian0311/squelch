//! C4FM symbols to frames: FM-demodulate 48 kHz I/Q, find the 24-symbol frame sync, and
//! slice the dibits after it using levels measured on the sync itself.
//!
//! Timing comes from each sync (10 samples per 4800-baud symbol); a frame is at most 864
//! dibits, short enough that a 100 ppm clock error drifts under one sample.

use crate::dsp::C32;

pub const RATE: f64 = 48_000.0;
const SPS: usize = 10;
/// Frame sync 0x5575F5FF77FF; dibit 01 is +3, 11 is −3.
const SYNC: u64 = 0x5575_F5FF_77FF;
const SYNC_LEN: usize = 24;
/// Normalised correlation a sync must reach.
const MIN_CORR: f32 = 0.85;
/// Raw dibits kept after a sync: the length of an LDU, the longest frame.
const FRAME_RAW: usize = 864;
/// Frequency offset is tracked over about this many samples.
const DC_SAMPLES: f32 = 9_600.0;
/// Samples each side of a symbol's centre averaged when slicing it.
const HALF_WINDOW: usize = 2;

pub const DUID_HDU: u8 = 0x0;
pub const DUID_TDU: u8 = 0x3;
pub const DUID_LDU1: u8 = 0x5;
pub const DUID_TSDU: u8 = 0x7;
pub const DUID_LDU2: u8 = 0xA;
pub const DUID_TDULC: u8 = 0xF;

pub struct Frame {
    pub nac: u16,
    pub duid: u8,
    /// Dibits from the sync on, status symbols removed.
    pub dibits: Vec<u8>,
}

pub struct Phy {
    sync: [f32; SYNC_LEN],
    last: C32,
    dc: f32,
    fm: Vec<f32>,
    /// Next sample to test for a sync.
    pos: usize,
}

impl Default for Phy {
    fn default() -> Self {
        Self::new()
    }
}

impl Phy {
    pub fn new() -> Self {
        let mut sync = [0f32; SYNC_LEN];
        for (i, v) in sync.iter_mut().enumerate() {
            *v = if (SYNC >> (46 - 2 * i)) & 3 == 1 { 3.0 } else { -3.0 };
        }
        Phy { sync, last: C32::new(1.0, 0.0), dc: 0.0, fm: Vec::new(), pos: 0 }
    }

    pub fn reset(&mut self) {
        self.fm.clear();
        self.pos = 0;
        self.dc = 0.0;
    }

    /// Carrier offset from the channel centre, Hz, as the discriminator sees it.
    pub fn offset_hz(&self) -> f64 {
        self.dc as f64 * RATE / std::f64::consts::TAU
    }

    pub fn process(&mut self, iq: &[C32], frames: &mut Vec<Frame>) {
        for &s in iq {
            let d = (s * self.last.conj()).arg();
            self.last = s;
            self.dc += (d - self.dc) / DC_SAMPLES;
            self.fm.push(d - self.dc);
        }
        let need = FRAME_RAW * SPS;
        while self.pos + need < self.fm.len() {
            let t = self.pos;
            let c = self.corr(t);
            if c.abs() < MIN_CORR {
                self.pos += 1;
                continue;
            }
            let best = (t..t + SPS).max_by(|&x, &y| self.corr(x).abs().total_cmp(&self.corr(y).abs())).unwrap();
            let frame = self.slice(best, self.corr(best) < 0.0);
            // an LDU fills its whole span; anything shorter may be followed by another sync
            let len = if matches!(frame.duid, DUID_LDU1 | DUID_LDU2) { FRAME_RAW - 4 } else { SYNC_LEN };
            self.pos = best + len * SPS;
            frames.push(frame);
        }
        // keep the unsearched tail
        let keep = self.pos.min(self.fm.len());
        self.fm.drain(..keep);
        self.pos -= keep;
    }

    fn corr(&self, t: usize) -> f32 {
        let (mut c, mut e) = (0f32, 0f32);
        for k in 0..SYNC_LEN {
            let x = self.fm[t + k * SPS];
            c += self.sync[k] * x;
            e += x.abs();
        }
        c / (3.0 * e).max(1e-9)
    }

    /// Mean over the middle of a symbol, which averages out noise the single sample carries.
    fn symbol(&self, t: usize) -> f32 {
        let lo = t.saturating_sub(HALF_WINDOW);
        let hi = (t + HALF_WINDOW + 1).min(self.fm.len());
        self.fm[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
    }

    fn slice(&self, at: usize, inverted: bool) -> Frame {
        let sign = if inverted { -1.0 } else { 1.0 };
        let amp = (0..SYNC_LEN).map(|k| self.symbol(at + k * SPS).abs()).sum::<f32>() / SYNC_LEN as f32;
        let dibits: Vec<u8> = (0..FRAME_RAW)
            .filter(|k| k % 36 != 35)
            .map(|k| {
                let v = sign * self.symbol(at + k * SPS) / amp * 3.0;
                if v > 2.0 {
                    1
                } else if v > 0.0 {
                    0
                } else if v > -2.0 {
                    2
                } else {
                    3
                }
            })
            .collect();
        let nid = dibits[24..32].iter().fold(0u16, |b, &d| (b << 2) | d as u16);
        Frame { nac: nid >> 4, duid: (nid & 0xF) as u8, dibits }
    }
}

/// C4FM-modulate dibits at 48 kHz for tests and the simulator: ±1.8 kHz / ±600 Hz deviation.
pub fn modulate(dibits: &[u8]) -> Vec<C32> {
    let mut phase = 0f64;
    let mut out = Vec::with_capacity(dibits.len() * SPS);
    for &d in dibits {
        let level = match d {
            1 => 3.0,
            0 => 1.0,
            2 => -1.0,
            _ => -3.0,
        };
        for _ in 0..SPS {
            phase += std::f64::consts::TAU * 600.0 * level / RATE;
            out.push(C32::new(phase.cos() as f32, phase.sin() as f32));
        }
    }
    out
}

/// Raw on-air dibits of a frame: sync, NID (NAC, DUID, no BCH parity), then `body`, with a
/// status symbol inserted after every 35.
pub fn frame_dibits(nac: u16, duid: u8, body: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    for i in 0..SYNC_LEN {
        data.push(((SYNC >> (46 - 2 * i)) & 3) as u8);
    }
    let nid = (nac << 4) | duid as u16;
    for i in 0..8 {
        data.push(((nid >> (14 - 2 * i)) & 3) as u8);
    }
    data.extend(std::iter::repeat_n(0, 24));
    data.extend_from_slice(body);
    let mut raw = Vec::new();
    for d in data {
        if raw.len() % 36 == 35 {
            raw.push(2);
        }
        raw.push(d);
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p25::tsbk;

    #[test]
    fn finds_a_control_frame_and_its_message() {
        let mut msg = [0x80u8, 0x00, 0x00, 0x10, 0x2A, 0x01, 0x23, 0x45, 0x67, 0x89, 0, 0];
        let crc = tsbk::crc16(&msg[..10]).to_be_bytes();
        msg[10..].copy_from_slice(&crc);
        let mut raw = vec![0u8; 50];
        raw.extend(frame_dibits(0xD17, DUID_TSDU, &tsbk::encode(&msg)));
        raw.extend(vec![0u8; FRAME_RAW]);
        let mut phy = Phy::new();
        let mut frames = Vec::new();
        phy.process(&modulate(&raw), &mut frames);
        let f = frames.first().expect("a frame");
        assert_eq!((f.nac, f.duid), (0xD17, DUID_TSDU));
        let body: [u8; 98] = f.dibits[56..154].try_into().unwrap();
        assert_eq!(tsbk::decode(&body), Some(msg));
    }
}
