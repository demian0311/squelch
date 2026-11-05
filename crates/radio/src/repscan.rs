//! The Repeaters screen's receiver: every repeater output in the capture watched at once, the
//! DMR ones each with their own `Dmr`, the FM ones by their carrier among the detected signals.
//! One plays: the held one, else whoever started talking first (a scanner), kept until it goes
//! quiet. The engine moves `tuned` to what plays, so the status and the top bar say which.

use crate::detect::Detected;
use crate::dmr::{Dmr, Info as DmrInfo};
use crate::dsp::C32;
use serde::{Deserialize, Serialize};

/// A plain FM carrier counts as on air this long after it was last seen (a talker's pause).
const FM_HANG_SECS: f64 = 2.0;
/// A detected signal this close to the output frequency is the repeater's.
const SAME_HZ: f64 = 6_000.0;
/// Without a squelch level, a carrier this far above the floor is on air.
const DEFAULT_ON_DB: f32 = 8.0;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct RepeaterChannel {
    pub hz: f64,
    pub dmr: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RepeaterState {
    pub hz: f64,
    /// The DMR receiver's state, on a DMR repeater in the capture.
    pub dmr: Option<DmrInfo>,
    /// FM: a carrier is on now.
    pub on_air: bool,
    /// Inside the capture, so it can be heard.
    pub heard: bool,
    /// Seconds (engine clock) when it last carried something.
    pub last_on_secs: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub now_secs: f64,
    /// The repeater playing now.
    pub playing: Option<f64>,
    pub hold: Option<f64>,
    pub channels: Vec<RepeaterState>,
}

/// What the engine should put on the speaker this block.
pub enum Play {
    /// Demodulate this frequency as narrow FM.
    Fm(f64),
    /// This DMR repeater's voice, already decoded.
    Dmr(f64),
    Quiet,
}

struct Chan {
    hz: f64,
    dmr: Option<Dmr>,
    on_air: bool,
    last_on: Option<f64>,
    pcm: Vec<f32>,
}

pub struct RepeaterScan {
    rate: f64,
    now: f64,
    channels: Vec<Chan>,
    hold: Option<f64>,
    playing: Option<f64>,
}

impl RepeaterScan {
    pub fn new(rate: u32, channels: &[RepeaterChannel], hold: Option<f64>) -> Self {
        let mut s = RepeaterScan { rate: rate as f64, now: 0.0, channels: Vec::new(), hold, playing: None };
        s.set(rate, channels, hold);
        s
    }

    /// A new list keeps the receivers of frequencies already on it.
    pub fn set(&mut self, rate: u32, channels: &[RepeaterChannel], hold: Option<f64>) {
        let old = std::mem::take(&mut self.channels);
        let mut old: Vec<Option<Chan>> = old.into_iter().map(Some).collect();
        for c in channels {
            let kept = old.iter_mut().find(|o| o.as_ref().is_some_and(|o| (o.hz - c.hz).abs() < 1.0 && o.dmr.is_some() == c.dmr)).and_then(Option::take);
            self.channels.push(kept.unwrap_or_else(|| Chan { hz: c.hz, dmr: c.dmr.then(|| Dmr::new(rate, c.hz)), on_air: false, last_on: None, pcm: Vec::new() }));
        }
        self.hold = hold;
        if self.playing.is_some_and(|p| !self.channels.iter().any(|c| (c.hz - p).abs() < 1.0)) {
            self.playing = None;
        }
    }

    pub fn hold(&self) -> Option<f64> {
        self.hold
    }

    pub fn playing(&self) -> Option<f64> {
        self.playing
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    /// Run the DMR receivers in the capture, read the FM carriers, and say what plays.
    pub fn process(&mut self, iq: &[C32], center: f64, signals: &[Detected], squelch_db: Option<f32>) -> Play {
        self.now += iq.len() as f64 / self.rate;
        let half = self.rate * 0.45;
        let on_db = squelch_db.unwrap_or(DEFAULT_ON_DB).max(3.0);
        let now = self.now;
        for c in &mut self.channels {
            let heard = (c.hz - center).abs() < half;
            match &mut c.dmr {
                Some(d) if heard => {
                    d.process(iq, center, &mut c.pcm);
                    if d.in_call() {
                        c.last_on = Some(now);
                    }
                }
                Some(_) => c.pcm.clear(),
                None => {
                    c.on_air = heard && signals.iter().any(|s| (s.hz - c.hz).abs() < SAME_HZ && s.snr_db >= on_db);
                    if c.on_air {
                        c.last_on = Some(now);
                    }
                }
            }
        }
        let active = |c: &Chan| match &c.dmr {
            Some(d) => d.in_call(),
            None => c.on_air || c.last_on.is_some_and(|t| now - t < FM_HANG_SECS),
        };
        let hearable = |c: &Chan| (c.hz - center).abs() < half;
        let next = match self.hold {
            Some(h) => Some(h),
            None => {
                let still = self.playing.and_then(|p| self.channels.iter().find(|c| (c.hz - p).abs() < 1.0)).filter(|c| hearable(c) && active(c)).map(|c| c.hz);
                still.or_else(|| self.channels.iter().find(|c| hearable(c) && active(c) && c.dmr.as_ref().is_none_or(Dmr::clear_call)).map(|c| c.hz))
            }
        };
        self.playing = next;
        match next.and_then(|hz| self.channels.iter().find(|c| (c.hz - hz).abs() < 1.0)) {
            Some(c) if c.dmr.is_some() => Play::Dmr(c.hz),
            Some(c) => Play::Fm(c.hz),
            None => Play::Quiet,
        }
    }

    /// The playing DMR repeater's audio for this block.
    pub fn take_pcm(&mut self, hz: f64, pcm: &mut Vec<f32>) {
        pcm.clear();
        if let Some(c) = self.channels.iter_mut().find(|c| (c.hz - hz).abs() < 1.0) {
            pcm.append(&mut c.pcm);
        }
    }

    pub fn info(&self, center: f64) -> Info {
        let half = self.rate * 0.45;
        Info {
            now_secs: self.now,
            playing: self.playing,
            hold: self.hold,
            channels: self
                .channels
                .iter()
                .map(|c| RepeaterState { hz: c.hz, dmr: c.dmr.as_ref().map(Dmr::info), on_air: c.on_air, heard: (c.hz - center).abs() < half, last_on_secs: c.last_on })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(hz: f64, snr: f32) -> Detected {
        Detected { hz, width_hz: 12_500.0, snr_db: snr, hd: None }
    }

    #[test]
    fn an_fm_carrier_plays_and_holds_through_a_pause_then_scanning_resumes() {
        let chans = [RepeaterChannel { hz: 443_000_000.0, dmr: false }, RepeaterChannel { hz: 443_500_000.0, dmr: false }];
        let mut s = RepeaterScan::new(2_400_000, &chans, None);
        let iq = vec![C32::new(0.0, 0.0); 240_000];
        let centre = 443_250_000.0;
        assert!(matches!(s.process(&iq, centre, &[], Some(10.0)), Play::Quiet));
        // the second repeater keys up: it plays, and a louder first one later does not take over
        assert!(matches!(s.process(&iq, centre, &[seen(443_500_500.0, 20.0)], Some(10.0)), Play::Fm(hz) if hz == 443_500_000.0));
        assert!(matches!(s.process(&iq, centre, &[seen(443_500_500.0, 20.0), seen(443_000_000.0, 30.0)], Some(10.0)), Play::Fm(hz) if hz == 443_500_000.0));
        // a one-second pause keeps it; after the hang the other, still on, gets its turn
        for _ in 0..9 {
            assert!(matches!(s.process(&iq, centre, &[seen(443_000_000.0, 30.0)], Some(10.0)), Play::Fm(hz) if hz == 443_500_000.0));
        }
        for _ in 0..12 {
            s.process(&iq, centre, &[seen(443_000_000.0, 30.0)], Some(10.0));
        }
        assert!(matches!(s.process(&iq, centre, &[seen(443_000_000.0, 30.0)], Some(10.0)), Play::Fm(hz) if hz == 443_000_000.0));
        let info = s.info(centre);
        assert_eq!(info.channels.iter().map(|c| c.on_air).collect::<Vec<_>>(), [true, false]);
        assert!(info.channels[1].last_on_secs.is_some());
    }

    #[test]
    fn a_hold_wins_and_a_repeater_outside_the_capture_is_not_heard() {
        let chans = [RepeaterChannel { hz: 443_000_000.0, dmr: false }, RepeaterChannel { hz: 446_000_000.0, dmr: true }];
        let mut s = RepeaterScan::new(2_400_000, &chans, Some(446_000_000.0));
        let iq = vec![C32::new(0.0, 0.0); 24_000];
        assert!(matches!(s.process(&iq, 443_250_000.0, &[seen(443_000_000.0, 30.0)], None), Play::Dmr(hz) if hz == 446_000_000.0));
        let info = s.info(443_250_000.0);
        assert_eq!(info.channels.iter().map(|c| c.heard).collect::<Vec<_>>(), [true, false]);
        assert!(info.channels[1].dmr.is_some());
        s.set(2_400_000, &chans, None);
        assert!(matches!(s.process(&iq, 443_250_000.0, &[seen(443_000_000.0, 30.0)], None), Play::Fm(_)));
    }
}
