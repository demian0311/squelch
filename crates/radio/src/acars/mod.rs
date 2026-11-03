//! ACARS: short text messages between aircraft and airlines, AM with 2400 baud MSK on 25 kHz channels.
//!
//! It listens on the fixed US channels inside the capture rather than on detected signals. A block lasts a fraction
//! of a second and starts with only a short pre-key; by the time the detector has averaged enough rows to report the
//! carrier and a channel is opened, the sync characters are gone. The channels are few and known, so every one in
//! the capture is demodulated all the time.

pub mod frame;
pub mod msk;

use crate::dsp::C32;
use msk::Channel;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// US ACARS channels in the airband (136.7–136.8 MHz are too far from these to share a capture).
pub const US_CHANNELS_HZ: [f64; 6] = [129_125_000.0, 130_025_000.0, 130_425_000.0, 130_450_000.0, 131_125_000.0, 131_550_000.0];
/// Hardware centre that holds the most channels (5 of 6).
pub const CENTER_HZ: f64 = 130_500_000.0;
/// Channels this far from the capture centre are in the dongle's filter roll-off.
const EDGE: f64 = 0.45;
/// Messages kept on the engine side.
const KEEP: usize = 100;
/// A status carries the messages of this long; the page keeps its own list, and pulls statuses far more often.
const SEND_FOR: Duration = Duration::from_secs(10);

/// One decoded block.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Counts up from 1 for each block decoded since decoding started.
    pub id: u64,
    /// When it was decoded, Unix milliseconds.
    pub at_ms: u64,
    pub hz: f64,
    pub mode: char,
    /// Aircraft registration, leading dots dropped.
    pub registration: String,
    /// The block this one acknowledges; `None` is NAK.
    pub ack: Option<char>,
    pub label: String,
    pub block_id: char,
    /// Ended by ETB: more blocks of the same message follow.
    pub more: bool,
    /// Message number and flight id, sent only by aircraft.
    pub msn: String,
    pub flight: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Info {
    /// Channels being decoded now.
    pub channels: Vec<f64>,
    /// Messages decoded in the last `SEND_FOR`, newest last.
    pub messages: Vec<Message>,
    /// Blocks decoded since decoding started.
    pub heard: u64,
}

pub struct Acars {
    rate: u32,
    center: f64,
    channels: Vec<Channel>,
    messages: VecDeque<(Instant, Message)>,
    heard: u64,
    blocks: Vec<Vec<u8>>,
    /// Decoded since `take_fresh`, for the log.
    fresh: Vec<Message>,
}

impl Acars {
    pub fn new(rate: u32) -> Self {
        Acars { rate, center: f64::NAN, channels: Vec::new(), messages: VecDeque::new(), heard: 0, blocks: Vec::new(), fresh: Vec::new() }
    }

    /// Feed a block of the capture centred on `center` Hz; channels follow the centre when it moves.
    pub fn process(&mut self, iq: &[C32], center: f64) {
        if (center - self.center).abs() > 1.0 || self.center.is_nan() {
            self.center = center;
            let half = self.rate as f64 * EDGE;
            self.channels = US_CHANNELS_HZ.iter().filter(|&&hz| (hz - center).abs() < half).map(|&hz| Channel::new(hz, hz - center, self.rate)).collect();
        }
        for ch in &mut self.channels {
            self.blocks.clear();
            ch.process(iq, &mut self.blocks);
            for b in self.blocks.drain(..) {
                let Some(mut m) = frame::parse(&b) else { continue };
                self.heard += 1;
                m.id = self.heard;
                m.hz = ch.hz();
                m.at_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
                self.fresh.push(m.clone());
                self.messages.push_back((Instant::now(), m));
                if self.messages.len() > KEEP {
                    self.messages.pop_front();
                }
            }
        }
    }

    /// Messages decoded since the last call.
    pub fn take_fresh(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.fresh)
    }

    pub fn info(&self) -> Info {
        let now = Instant::now();
        Info {
            channels: self.channels.iter().map(Channel::hz).collect(),
            messages: self.messages.iter().filter(|(t, _)| now - *t < SEND_FOR).map(|(_, m)| m.clone()).collect(),
            heard: self.heard,
        }
    }

    /// Every message still kept, oldest first.
    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter().map(|(_, m)| m)
    }
}

#[cfg(test)]
mod tests {
    use super::frame::{block, on_air, tests::downlink, tests::uplink, NAK};
    use super::msk::tests::msk_audio;
    use super::*;

    /// An AM transmitter on `hz`, keyed `start` seconds in, sending `bytes`, as capture samples around `center`.
    fn add_burst(iq: &mut [C32], rate: u32, center: f64, hz: f64, start: f64, bytes: &[u8], amp: f32) {
        let audio = msk_audio(bytes, rate as f64, msk::BAUD as f64 * 1.00005, 0.0);
        let s0 = (start * rate as f64) as usize;
        let w = std::f64::consts::TAU * (hz - center) / rate as f64;
        for (i, a) in audio.iter().enumerate() {
            let Some(o) = iq.get_mut(s0 + i) else { break };
            let p = w * (s0 + i) as f64 + 0.7;
            // a carrier keyed with the burst, 80 % modulated
            *o += C32::new(p.cos() as f32, p.sin() as f32) * amp * (1.0 + 0.8 * a);
        }
    }

    fn noise(n: usize, sigma: f32) -> Vec<C32> {
        let mut seed = 0x1234_5678u32;
        let mut u = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        (0..n)
            .map(|_| {
                // Box-Muller
                let (a, b) = (u().max(1e-9), u());
                let r = sigma * (-2.0 * a.ln()).sqrt();
                C32::new(r * (std::f32::consts::TAU * b).cos(), r * (std::f32::consts::TAU * b).sin())
            })
            .collect()
    }

    #[test]
    fn decodes_two_channels_at_once_in_noise_and_off_frequency() {
        let rate = 2_400_000u32;
        let center = 130_500_000.0;
        let mut iq = noise(rate as usize * 6 / 10, 0.05);
        let other = block(b'2', ".N501DN", NAK, "Q0", b'7', Some("M12ADL1234"), false);
        // 2.1 kHz off (an uncorrected crystal) on one, 600 Hz on the other; they overlap in time
        add_burst(&mut iq, rate, center, 131_550_000.0 + 2_100.0, 0.02, &on_air(&downlink()), 0.02);
        add_burst(&mut iq, rate, center, 130_025_000.0 - 600.0, 0.10, &on_air(&other), 0.03);
        add_burst(&mut iq, rate, center, 130_025_000.0 - 600.0, 0.40, &on_air(&uplink()), 0.03);
        let mut a = Acars::new(rate);
        for chunk in iq.chunks(65_536) {
            a.process(chunk, center);
        }
        let ch = a.info().channels;
        assert!(ch.contains(&131_550_000.0) && ch.contains(&130_025_000.0) && !ch.contains(&129_125_000.0), "{ch:?}");
        let got: Vec<(f64, String, String)> = a.messages().map(|m| (m.hz, m.flight.clone(), m.label.clone())).collect();
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(got.contains(&(131_550_000.0, "UA0451".into(), "H1".into())), "{got:?}");
        assert!(got.contains(&(130_025_000.0, "DL1234".into(), "Q0".into())), "{got:?}");
        assert!(got.contains(&(130_025_000.0, "".into(), "_d".into())), "{got:?}");
        let ids: Vec<u64> = a.messages().map(|m| m.id).collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(a.info().messages.len(), 3);
    }
}
