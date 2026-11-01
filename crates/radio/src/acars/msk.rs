//! One ACARS channel: cut it out of the capture, AM-demodulate it, and recover the 2400 baud MSK bits from the audio.
//!
//! The audio is 1200 Hz and 2400 Hz, continuous in phase, one tone per bit: 2400 Hz when a bit equals the one before
//! it, 1200 Hz when it differs (what acarsdec's coherent demodulator amounts to). Mixed down by 1800 Hz the tones are
//! ±600 Hz, so a frequency discriminator reads them whatever the audio's phase, and the carrier's own offset is gone
//! with the envelope. A one-bit moving average is the matched filter; its zero crossings mark bit boundaries.
//!
//! The pre-key is one steady tone, so a bit clock has nothing to lock to until the sync characters, and a clock that
//! starts half a bit off can miss the first SYN. `PHASES` clocks start evenly spread, each with its own framer; the
//! best placed one decodes, and a block found by several is kept once. Over 40 bit timings in audio noise at the
//! edge of decoding (2026-11-01), one clock decoded 27 blocks and four 32.

use super::frame::Framer;
use crate::dsp::{fast_atan2, lowpass, DcBlock, Decimate, Decimator, Mixer, C32};

pub const BAUD: f32 = 2400.0;
/// The channel is filtered and enveloped at this rate.
const IF_RATE: u32 = 48_000;
/// And the audio worked on at this one: ten samples a bit.
pub const AUDIO_RATE: u32 = 24_000;
const SPS: usize = (AUDIO_RATE as f32 / BAUD) as usize;
/// Midway between the two tones.
const MID_HZ: f64 = 1_800.0;
/// AM voice channels are 25 kHz apart. Pass the carrier and its 3 kHz of audio even with an uncorrected crystal
/// (57 ppm is 7.4 kHz here), stop short of the neighbours' sidebands.
const PASS_HZ: f32 = 11_000.0;
const STOP_HZ: f32 = 14_000.0;
/// Bit clocks started this many fractions of a bit apart.
const PHASES: usize = 4;
/// How fast a clock follows a crossing: the fraction of the timing error corrected each time.
const TIMING_GAIN: f32 = 0.08;
/// Two clocks that decode the same block do so within a few bits of each other.
const SAME_BLOCK_SAMPLES: u64 = AUDIO_RATE as u64 / 10;

struct Clock {
    phase: f32,
    bit: bool,
    framer: Framer,
}

pub struct Channel {
    hz: f64,
    mixer: Mixer,
    filter: Decimate<C32>,
    dc: DcBlock,
    audio: Decimator<f32>,
    tone: Mixer,
    tone_filter: Decimator<C32>,
    last: C32,
    window: [f32; SPS],
    pos: usize,
    sum: f32,
    prev: f32,
    clocks: Vec<Clock>,
    /// Audio samples processed, the clock of `recent`.
    t: u64,
    recent: Vec<(u64, Vec<u8>)>,
    buf: Vec<C32>,
    ifb: Vec<C32>,
    env: Vec<f32>,
    aud: Vec<f32>,
    bb: Vec<C32>,
    bbf: Vec<C32>,
    found: Vec<Vec<u8>>,
}

impl Channel {
    /// `offset_hz` is the channel relative to the capture centre; `in_rate` a multiple of 48 kHz.
    pub fn new(hz: f64, offset_hz: f64, in_rate: u32) -> Self {
        let a = AUDIO_RATE as f32;
        Channel {
            hz,
            mixer: Mixer::new(offset_hz, in_rate as f64),
            filter: Decimate::new(in_rate as f32, PASS_HZ, STOP_HZ, (in_rate / IF_RATE) as usize),
            dc: DcBlock::new(),
            // the envelope's audio is under 3 kHz; halving the rate needs nothing sharp
            audio: Decimator::new(lowpass(5_000.0 / IF_RATE as f32, 6_000.0 / IF_RATE as f32), (IF_RATE / AUDIO_RATE) as usize),
            tone: Mixer::new(MID_HZ, AUDIO_RATE as f64),
            // keeps ±600 Hz and the MSK main lobe, drops the image at -3600 Hz and what DC is left at -1800 Hz
            tone_filter: Decimator::new(lowpass(1_500.0 / a, 1_000.0 / a), 1),
            last: C32::new(1.0, 0.0),
            window: [0.0; SPS],
            pos: 0,
            sum: 0.0,
            prev: 0.0,
            clocks: (0..PHASES).map(|i| Clock { phase: i as f32 / PHASES as f32, bit: false, framer: Framer::new() }).collect(),
            t: 0,
            recent: Vec::new(),
            buf: Vec::new(),
            ifb: Vec::new(),
            env: Vec::new(),
            aud: Vec::new(),
            bb: Vec::new(),
            bbf: Vec::new(),
            found: Vec::new(),
        }
    }

    pub fn hz(&self) -> f64 {
        self.hz
    }

    /// Feed capture samples; blocks that passed their CRC are appended to `out`.
    pub fn process(&mut self, iq: &[C32], out: &mut Vec<Vec<u8>>) {
        self.buf.clear();
        self.buf.extend_from_slice(iq);
        self.mixer.process(&mut self.buf);
        self.ifb.clear();
        self.filter.process(&self.buf, &mut self.ifb);
        self.env.clear();
        self.env.extend(self.ifb.iter().map(|s| s.norm()));
        // the carrier is the envelope's DC
        self.dc.process(&mut self.env);
        self.aud.clear();
        self.audio.process(&self.env, &mut self.aud);
        self.audio_in(out);
    }

    /// Demodulate `self.aud`, audio at `AUDIO_RATE`.
    fn audio_in(&mut self, out: &mut Vec<Vec<u8>>) {
        self.bb.clear();
        self.bb.extend(self.aud.iter().map(|&s| C32::new(s, 0.0)));
        self.tone.process(&mut self.bb);
        self.bbf.clear();
        self.tone_filter.process(&self.bb, &mut self.bbf);
        let k = AUDIO_RATE as f32 / std::f32::consts::TAU;
        let step = 1.0 / SPS as f32;
        for &s in &self.bbf {
            let d = s * self.last.conj();
            self.last = s;
            let f = fast_atan2(d.im, d.re) * k;
            self.sum += f - self.window[self.pos];
            self.window[self.pos] = f;
            self.pos = (self.pos + 1) % SPS;
            let y = self.sum;
            let crossed = (self.prev < 0.0) != (y < 0.0);
            // where between the two samples the crossing fell, in bits
            let frac = if crossed { self.prev / (self.prev - y) } else { 0.0 };
            for c in &mut self.clocks {
                let before = c.phase;
                c.phase += step;
                if crossed {
                    let mut at = before + frac * step;
                    at -= at.round();
                    c.phase -= TIMING_GAIN * at;
                }
                if before < 0.5 && c.phase >= 0.5 {
                    // the lower tone means this bit differs from the last
                    c.bit ^= y < 0.0;
                    c.framer.push(c.bit, &mut self.found);
                }
                if c.phase >= 1.0 {
                    c.phase -= 1.0;
                }
            }
            self.prev = y;
            self.t += 1;
        }
        let t = self.t;
        self.recent.retain(|(at, _)| t - at < SAME_BLOCK_SAMPLES);
        for b in self.found.drain(..) {
            if !self.recent.iter().any(|(_, r)| *r == b) {
                self.recent.push((t, b.clone()));
                out.push(b);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::acars::frame::{on_air, tests::downlink};

    /// The audio tones for `bytes`, least significant bit first, at `rate`, starting `delay` samples in.
    pub fn msk_audio(bytes: &[u8], rate: f64, baud: f64, delay: f64) -> Vec<f32> {
        let bits: Vec<bool> = bytes.iter().flat_map(|&b| (0..8).map(move |k| b >> k & 1 == 1)).collect();
        let n = ((bits.len() as f64 / baud) * rate + delay) as usize + 1;
        let mut phase = 0.0f64;
        let mut prev = true;
        let mut out = vec![0.0f32; n];
        let mut tone = 2_400.0;
        let mut last_bit = usize::MAX;
        for (i, o) in out.iter_mut().enumerate() {
            let t = (i as f64 - delay) / rate * baud;
            if t >= 0.0 {
                let k = t as usize;
                if k != last_bit && k < bits.len() {
                    tone = if bits[k] == prev { 2_400.0 } else { 1_200.0 };
                    prev = bits[k];
                    last_bit = k;
                }
                phase += std::f64::consts::TAU * tone / rate;
                *o = phase.sin() as f32;
            }
        }
        out
    }

    #[test]
    fn decodes_audio_at_any_bit_timing() {
        let air = on_air(&downlink());
        for delay in [0.0, 2.5, 5.0, 7.3] {
            let mut a = vec![0.0f32; 300];
            // a transmitter 100 ppm slow
            a.extend(msk_audio(&air, AUDIO_RATE as f64, BAUD as f64 * 0.9999, delay));
            a.extend(vec![0.0f32; 300]);
            let mut ch = Channel::new(0.0, 0.0, 2_400_000);
            let mut out = Vec::new();
            for chunk in a.chunks(1000) {
                ch.aud.clear();
                ch.aud.extend_from_slice(chunk);
                ch.audio_in(&mut out);
            }
            assert_eq!(out, vec![downlink()], "delay {delay}");
        }
    }
}
