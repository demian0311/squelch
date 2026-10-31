//! Turns the tuned channel (already mixed to 0 Hz at the capture rate) into 48 kHz audio.

use crate::dsp::{fast_atan2, lowpass, Agc, DcBlock, Decimate, Decimator, Mixer, C32};
use serde::{Deserialize, Serialize};

pub const AUDIO_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Broadcast FM, 75 kHz deviation, mono.
    Wfm,
    /// Narrow FM, 5 kHz deviation.
    Nfm,
    Am,
    Usb,
    Lsb,
    /// No audio; the band is only for decoders or looking.
    None,
    /// P25 Phase 1 trunked radio; tune the control channel. The engine runs `p25::P25` instead.
    P25,
}

impl Mode {
    /// Default channel width in Hz.
    pub fn bandwidth(self) -> f32 {
        match self {
            Mode::Wfm => 200_000.0,
            Mode::Nfm | Mode::P25 => 12_500.0,
            Mode::Am => 8_000.0,
            Mode::Usb | Mode::Lsb => 2_800.0,
            Mode::None => 10_000.0,
        }
    }
}

const SSB_CENTER: f64 = 1_650.0;

pub struct Demodulator {
    mode: Mode,
    in_rate: f32,
    stage1: Decimate<C32>,
    stage2: Decimator<C32>,
    audio_lp: Decimator<f32>,
    ssb_in: Mixer,
    ssb_out: Mixer,
    last: C32,
    deemph: f32,
    deemph_a: f32,
    dc: DcBlock,
    agc: Agc,
    s1: Vec<C32>,
    s2: Vec<C32>,
    fm: Vec<f32>,
    /// Channel power of the last block, dBFS.
    pub level_db: f32,
    /// Sum and count of the WFM discriminator in Hz; its mean is where the carrier sits from the tuned frequency.
    pub offset_sum: f64,
    pub offset_n: usize,
    /// Station data, decoded in wide FM only.
    pub rds: Option<crate::rds::Rds>,
}

impl Demodulator {
    /// `in_rate` must be 2.4 MHz or another multiple of 240 kHz.
    pub fn new(mode: Mode, in_rate: u32) -> Self {
        let fs = in_rate as f32;
        let d1 = (in_rate / 240_000).max(1) as usize;
        let mid = fs / d1 as f32;
        let stage1 = match mode {
            Mode::Wfm => Decimate::new(fs, 100_000.0, 140_000.0, d1),
            _ => Decimate::new(fs, 30_000.0, 110_000.0, d1),
        };
        let half = mode.bandwidth() / 2.0;
        let stage2 = match mode {
            Mode::Usb | Mode::Lsb => Decimator::new(lowpass((half - 250.0) / mid, 500.0 / mid), 5),
            _ => Decimator::new(lowpass(half / mid, 3_000.0 / mid), 5),
        };
        let ssb_shift = if mode == Mode::Lsb { -SSB_CENTER } else { SSB_CENTER };
        // 75 µs de-emphasis (Americas)
        let deemph_a = 1.0 - (-1.0 / (mid * 75e-6)).exp();
        Demodulator {
            mode,
            in_rate: fs,
            stage1,
            stage2,
            audio_lp: Decimator::new(lowpass(15_000.0 / mid, 6_000.0 / mid), 5),
            ssb_in: Mixer::new(ssb_shift, mid as f64),
            ssb_out: Mixer::new(-ssb_shift, AUDIO_RATE as f64),
            last: C32::new(1.0, 0.0),
            deemph: 0.0,
            deemph_a,
            dc: DcBlock::new(),
            agc: Agc::new(),
            s1: Vec::new(),
            s2: Vec::new(),
            fm: Vec::new(),
            level_db: -120.0,
            offset_sum: 0.0,
            offset_n: 0,
            rds: (mode == Mode::Wfm).then(|| crate::rds::Rds::new(mid as u32)),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Phase difference between consecutive samples, scaled so `dev` Hz maps to 1.0.
    fn discriminate(&mut self, input: &[C32], rate: f32, dev: f32, out: &mut Vec<f32>) {
        let k = rate / (std::f32::consts::TAU * dev);
        for &s in input {
            let d = s * self.last.conj();
            self.last = s;
            out.push(fast_atan2(d.im, d.re) * k);
        }
    }

    pub fn process(&mut self, input: &[C32], audio: &mut Vec<f32>) {
        audio.clear();
        if matches!(self.mode, Mode::None | Mode::P25) {
            return;
        }
        let mid = self.in_rate / (self.in_rate / 240_000.0).round().max(1.0);
        self.s1.clear();
        let mut s1 = std::mem::take(&mut self.s1);
        self.stage1.process(input, &mut s1);
        match self.mode {
            Mode::Wfm => {
                self.level_db = crate::dsp::power_db(&s1);
                let mut fm = std::mem::take(&mut self.fm);
                fm.clear();
                self.discriminate(&s1, mid, 75_000.0, &mut fm);
                self.offset_sum += fm.iter().map(|&v| v as f64 * 75_000.0).sum::<f64>();
                self.offset_n += fm.len();
                if let Some(r) = &mut self.rds {
                    r.process(&fm);
                }
                for v in fm.iter_mut() {
                    self.deemph += self.deemph_a * (*v - self.deemph);
                    *v = self.deemph;
                }
                self.audio_lp.process(&fm, audio);
                for v in audio.iter_mut() {
                    *v *= 0.6;
                }
                self.fm = fm;
            }
            Mode::Usb | Mode::Lsb => {
                self.ssb_in.process(&mut s1);
                self.s2.clear();
                let mut s2 = std::mem::take(&mut self.s2);
                self.stage2.process(&s1, &mut s2);
                self.level_db = crate::dsp::power_db(&s2);
                self.ssb_out.process(&mut s2);
                audio.extend(s2.iter().map(|c| c.re));
                self.agc.process(audio);
                self.s2 = s2;
            }
            Mode::Nfm | Mode::Am => {
                self.s2.clear();
                let mut s2 = std::mem::take(&mut self.s2);
                self.stage2.process(&s1, &mut s2);
                self.level_db = crate::dsp::power_db(&s2);
                if self.mode == Mode::Nfm {
                    self.discriminate(&s2, AUDIO_RATE as f32, 5_000.0, audio);
                    self.dc.process(audio);
                    for v in audio.iter_mut() {
                        *v *= 0.5;
                    }
                } else {
                    audio.extend(s2.iter().map(|c| c.norm()));
                    self.dc.process(audio);
                    self.agc.process(audio);
                }
                self.s2 = s2;
            }
            Mode::None | Mode::P25 => {}
        }
        self.s1 = s1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{IqSource, SimSignal, SimSource};
    use crate::dsp::u8_to_iq;

    /// Demodulate one second of a single simulated signal at the capture centre and
    /// return the dominant audio frequency found by zero crossings.
    fn demod_tone(mode: Mode, sig: SimSignal) -> (f32, f32) {
        let rate = 2_400_000;
        let mut src = SimSource::new(rate, vec![sig], false);
        src.set_center(100_000_000).unwrap();
        let mut raw = vec![0u8; rate as usize * 2];
        src.read(&mut raw).unwrap();
        let mut iq = Vec::new();
        u8_to_iq(&raw, &mut iq);
        let mut d = Demodulator::new(mode, rate);
        let mut audio = Vec::new();
        d.process(&iq, &mut audio);
        let tail = &audio[audio.len() / 2..];
        let crossings = tail.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let secs = tail.len() as f32 / AUDIO_RATE as f32;
        let rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt();
        (crossings as f32 / secs, rms)
    }

    fn sig(fm_dev: f64, am_depth: f32, offset: f64) -> SimSignal {
        SimSignal { hz: 100_000_000.0 + offset, amp: 0.5, fm_dev, am_depth, tone_hz: 1_000.0, symbols: None }
    }

    #[test]
    fn audio_rate_is_48k() {
        let mut d = Demodulator::new(Mode::Nfm, 2_400_000);
        let mut audio = Vec::new();
        d.process(&vec![C32::new(0.1, 0.0); 240_000], &mut audio);
        assert_eq!(audio.len(), 4_800);
    }

    #[test]
    fn nfm_recovers_the_tone() {
        let (f, rms) = demod_tone(Mode::Nfm, sig(3_000.0, 0.0, 0.0));
        assert!((f - 1_000.0).abs() < 30.0, "got {f} Hz");
        assert!(rms > 0.1);
    }

    #[test]
    fn wfm_recovers_the_tone() {
        let (f, rms) = demod_tone(Mode::Wfm, sig(50_000.0, 0.0, 0.0));
        assert!((f - 1_000.0).abs() < 30.0, "got {f} Hz");
        assert!(rms > 0.05);
    }

    #[test]
    fn am_recovers_the_tone() {
        let (f, rms) = demod_tone(Mode::Am, sig(0.0, 0.8, 0.0));
        assert!((f - 1_000.0).abs() < 30.0, "got {f} Hz");
        assert!(rms > 0.05);
    }

    #[test]
    fn usb_turns_a_carrier_above_the_dial_into_a_tone() {
        // a plain carrier 1 kHz above the tuned frequency is heard as 1 kHz
        let (f, _) = demod_tone(Mode::Usb, sig(0.0, 0.0, 1_000.0));
        assert!((f - 1_000.0).abs() < 30.0, "got {f} Hz");
    }

    #[test]
    fn lsb_rejects_a_carrier_above_the_dial() {
        let (_, usb) = demod_tone(Mode::Usb, sig(0.0, 0.0, 1_000.0));
        let mut d = Demodulator::new(Mode::Lsb, 2_400_000);
        // feed the same carrier without AGC influence by checking channel level instead
        let rate = 2_400_000;
        let mut src = SimSource::new(rate, vec![sig(0.0, 0.0, 1_000.0)], false);
        src.set_center(100_000_000).unwrap();
        let mut raw = vec![0u8; rate as usize];
        src.read(&mut raw).unwrap();
        let mut iq = Vec::new();
        u8_to_iq(&raw, &mut iq);
        let mut audio = Vec::new();
        d.process(&iq, &mut audio);
        let mut u = Demodulator::new(Mode::Usb, rate);
        u.process(&iq, &mut audio);
        assert!(u.level_db - d.level_db > 20.0, "usb {} lsb {}", u.level_db, d.level_db);
        assert!(usb > 0.0);
    }
}
