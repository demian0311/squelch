//! P25 Phase 1 (TIA-102): the digital trunked radio most US police, fire and EMS use.
//!
//! `P25` listens to a site's control channel, and when it grants an unencrypted Phase 1 call,
//! moves to that voice channel and plays it until the call ends. `phy` turns I/Q into frames,
//! `tsbk` reads control messages, `voice` unpacks the IMBE frames and `mbe` decodes them.
//!
//! The dongle's crystal error is measured against the control channel's own announced
//! frequency, and applied to every voice channel it grants.

pub mod mbe;
pub mod phy;
pub mod tsbk;
pub mod voice;

use crate::detect::Detected;
use crate::dsp::{lowpass, Agc, C32, Decimate, Decimator, Mixer};
use phy::{Frame, Phy};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use tsbk::Message;

/// Audio out, matching the other demodulators.
const AUDIO_RATE: f64 = 48_000.0;
/// Seconds without a control frame before trying the next nearby carrier.
const SEARCH_SECS: f64 = 1.5;
/// How far from the tuned frequency a control channel may sit: crystal error, ppm.
const SEARCH_PPM: f64 = 150.0;
/// A call ends after this long without a voice frame.
const HANG_SECS: f64 = 1.0;
/// Voice channels closer than this to the capture edge need the dongle moved.
const EDGE_HZ: f64 = 100_000.0;
/// Where the dongle centre goes relative to a voice channel it had to move for.
const RETUNE_OFFSET_HZ: f64 = 300_000.0;
const RECENT_CALLS: usize = 30;
/// After a terminator, how long to stay for a reply on the same channel.
const AFTER_TDU_SECS: f64 = 0.5;
/// Updates for a call that just ended are ignored this long; a new grant still follows.
const REFOLLOW_SECS: f64 = 4.0;
/// A call heard again within this many seconds is the same call.
const SAME_CALL_SECS: f64 = 10.0;
/// Codec bit errors above which a voice frame counts as bad.
const BAD_FRAME_ERRS: u32 = 4;
/// Bits a voice frame's NAC may differ from the site's and still count as this site.
const NAC_SLACK_BITS: u32 = 1;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Call {
    pub group: u16,
    /// Radio ID of the talker; 0 until a grant names it.
    pub source: u32,
    /// Voice channel, true frequency.
    pub hz: f64,
    pub encrypted: bool,
    /// Phase 2 (TDMA) channel, which this receiver cannot play.
    pub tdma: bool,
    pub emergency: bool,
    /// Seconds since the receiver started that the call was granted.
    pub at_secs: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Info {
    /// "searching", "control" or "voice".
    pub state: &'static str,
    /// Seconds since the receiver started, the clock `Call::at_secs` uses.
    pub now_secs: f64,
    pub nac: Option<u16>,
    pub wacn: Option<u32>,
    pub system: Option<u16>,
    pub rfss: Option<u8>,
    pub site: Option<u8>,
    /// Control channel, true frequency.
    pub control_hz: Option<f64>,
    /// Dongle frequency error, measured on the control channel.
    pub ppm: Option<f64>,
    /// Control messages that passed CRC, per second.
    pub messages_per_sec: f32,
    /// The call being played.
    pub call: Option<Call>,
    pub recent: Vec<Call>,
    /// Why voice cannot play (mbelib missing).
    pub voice_error: Option<String>,
    /// Voice frames the codec found badly damaged, percent, this call.
    pub voice_errors_pct: f32,
}

#[derive(Clone, Copy)]
struct Iden {
    base_hz: f64,
    spacing_hz: f64,
    tdma: bool,
}

struct Voice {
    call: Call,
    /// Dongle frequency of the voice channel.
    raw_hz: f64,
    decoder: Option<mbe::Decoder>,
    last_frame: f64,
    frames: u32,
    bad: u32,
}

pub struct P25 {
    rate: f64,
    mixer: Mixer,
    mixed: (f64, f64),
    d1: Decimate<C32>,
    d2: Decimator<C32>,
    chan: Vec<C32>,
    s1: Vec<C32>,
    s2: Vec<C32>,
    phy: Phy,
    frames: Vec<Frame>,
    /// Seconds of input processed.
    now: f64,
    /// Dongle frequency the user tuned; the control channel is searched for near it.
    tuned_hz: f64,
    /// Dongle frequency of the control channel being tried or locked.
    control_raw: f64,
    candidates: Vec<f64>,
    last_control: f64,
    locked: bool,
    idens: [Option<Iden>; 16],
    control_channel: Option<u16>,
    info: Info,
    good_msgs: u32,
    rate_since: f64,
    /// Talkgroups seen in grants, and whether they were in the clear.
    clear_groups: HashMap<u16, bool>,
    recent: VecDeque<Call>,
    voice: Option<Voice>,
    /// Talkgroup, channel and time of the call that ended last.
    ended: Option<(u16, f64, f64)>,
    /// How often each NAC was read; the most common is the site's, the rest are bit errors.
    nacs: HashMap<u16, u32>,
    /// Dongle centre to restore when a call that needed a retune ends.
    home_center: Option<f64>,
    up: Decimator<f32>,
    agc: Agc,
    audio8k: Vec<f32>,
}

impl P25 {
    pub fn new(rate: u32, tuned_hz: f64) -> Self {
        let fs = rate as f32;
        let mid = (AUDIO_RATE * 5.0) as f32;
        P25 {
            rate: rate as f64,
            mixer: Mixer::new(0.0, rate as f64),
            mixed: (f64::NAN, f64::NAN),
            // capture → 240 kHz → 48 kHz
            d1: Decimate::new(fs, 30_000.0, 90_000.0, (fs / mid).round() as usize),
            d2: Decimator::new(lowpass(7_000.0 / mid, 4_000.0 / mid), 5),
            chan: Vec::new(),
            s1: Vec::new(),
            s2: Vec::new(),
            phy: Phy::new(),
            frames: Vec::new(),
            now: 0.0,
            tuned_hz,
            control_raw: tuned_hz,
            candidates: Vec::new(),
            last_control: 0.0,
            locked: false,
            idens: [None; 16],
            control_channel: None,
            info: Info { voice_error: mbe::available().err(), ..Info::default() },
            good_msgs: 0,
            rate_since: 0.0,
            clear_groups: HashMap::new(),
            recent: VecDeque::new(),
            voice: None,
            ended: None,
            nacs: HashMap::new(),
            home_center: None,
            up: Decimator::new(lowpass(3_600.0 / AUDIO_RATE as f32, 800.0 / AUDIO_RATE as f32), 1),
            agc: Agc::new(),
            audio8k: Vec::new(),
        }
    }

    pub fn info(&self) -> Info {
        let mut i = self.info.clone();
        i.now_secs = self.now;
        i.recent = self.recent.iter().cloned().collect();
        i.call = self.voice.as_ref().map(|v| v.call.clone());
        i.state = match (&self.voice, self.locked) {
            (Some(_), _) => "voice",
            (None, true) => "control",
            (None, false) => "searching",
        };
        if let Some(v) = self.voice.as_ref().filter(|v| v.frames > 0) {
            i.voice_errors_pct = 100.0 * v.bad as f32 / v.frames as f32;
        }
        i
    }

    /// Feed one block of capture I/Q at dongle centre `center`; voice goes into `pcm` at 48 kHz.
    /// Returns a new dongle centre when a voice channel lies outside the capture, and the old
    /// one when that call ends.
    /// Following a call now.
    pub fn in_call(&self) -> bool {
        self.voice.is_some()
    }

    pub fn process(&mut self, iq: &[C32], center: f64, signals: &[Detected], pcm: &mut Vec<f32>) -> Option<f64> {
        pcm.clear();
        self.now += iq.len() as f64 / self.rate;
        let listen = self.voice.as_ref().map(|v| v.raw_hz).unwrap_or(self.control_raw);
        if (listen, center) != self.mixed {
            self.mixer.set(listen - center, self.rate);
            self.mixed = (listen, center);
        }
        self.chan.clear();
        self.chan.extend_from_slice(iq);
        self.mixer.process(&mut self.chan);
        self.s1.clear();
        self.d1.process(&self.chan, &mut self.s1);
        self.s2.clear();
        self.d2.process(&self.s1, &mut self.s2);
        let mut frames = std::mem::take(&mut self.frames);
        frames.clear();
        self.phy.process(&self.s2, &mut frames);

        let mut retune = None;
        for f in &frames {
            if let Some(r) = self.frame(f, center) {
                retune = Some(r);
            }
        }
        self.frames = frames;
        self.play(pcm);

        match &self.voice {
            Some(v) if self.now - v.last_frame > HANG_SECS => retune = self.end_call(),
            None if self.now - self.last_control > SEARCH_SECS => self.next_candidate(signals),
            _ => {}
        }
        if self.now - self.rate_since >= 2.0 {
            self.info.messages_per_sec = self.good_msgs as f32 / (self.now - self.rate_since) as f32;
            self.good_msgs = 0;
            self.rate_since = self.now;
        }
        retune.filter(|&c| (c - center).abs() > 1.0)
    }

    fn frame(&mut self, f: &Frame, center: f64) -> Option<f64> {
        if self.voice.is_some() {
            return self.voice_frame(f);
        }
        if f.duid != phy::DUID_TSDU {
            return None;
        }
        let mut any = false;
        let mut retune = None;
        for blk in 0..3 {
            let at = 56 + 98 * blk;
            let Some(chunk) = f.dibits.get(at..at + 98) else { break };
            let Some(b) = tsbk::decode(chunk.try_into().unwrap()) else { break };
            any = true;
            self.good_msgs += 1;
            retune = self.message(Message::parse(&b), center);
            if retune.is_some() || self.voice.is_some() || b[0] & 0x80 != 0 {
                break;
            }
        }
        if any && self.voice.is_none() {
            self.last_control = self.now;
            self.locked = true;
            *self.nacs.entry(f.nac).or_default() += 1;
            self.info.nac = self.nacs.iter().max_by_key(|(_, &n)| n).map(|(&nac, _)| nac);
            // follow the dongle's slow drift
            self.control_raw += self.phy.offset_hz() * 0.05;
        }
        retune
    }

    fn freq(&self, channel: u16) -> Option<(f64, bool)> {
        let iden = self.idens[(channel >> 12) as usize]?;
        Some((iden.base_hz + (channel & 0xFFF) as f64 * iden.spacing_hz, iden.tdma))
    }

    /// Dongle frequency per true frequency, once the control channel's own frequency is known.
    fn scale(&self) -> Option<f64> {
        let (cc, _) = self.freq(self.control_channel?)?;
        Some(self.control_raw / cc)
    }

    fn message(&mut self, m: Message, center: f64) -> Option<f64> {
        match m {
            Message::Iden { iden, base_hz, spacing_hz, tdma } => {
                self.idens[iden as usize & 15] = Some(Iden { base_hz: base_hz as f64, spacing_hz: spacing_hz as f64, tdma });
            }
            Message::Network { wacn, system, channel } => {
                self.info.wacn = Some(wacn);
                self.info.system = Some(system);
                self.set_control(channel);
            }
            Message::Site { rfss, site, channel } => {
                self.info.rfss = Some(rfss);
                self.info.site = Some(site);
                self.set_control(channel);
            }
            Message::GroupGrant { channel, group, source, encrypted, emergency } => {
                self.clear_groups.insert(group, !encrypted);
                return self.grant(channel, group, source, encrypted, emergency, true, center);
            }
            Message::GroupUpdate { calls } => {
                for (channel, group) in calls {
                    // an update does not say whether the call is encrypted; the grant did
                    let Some(&clear) = self.clear_groups.get(&group) else { continue };
                    let r = self.grant(channel, group, 0, !clear, false, false, center);
                    if r.is_some() || self.voice.is_some() {
                        return r;
                    }
                }
            }
            Message::Other { .. } => {}
        }
        None
    }

    fn set_control(&mut self, channel: u16) {
        self.control_channel = Some(channel);
        if let (Some((cc, _)), Some(s)) = (self.freq(channel), self.scale()) {
            self.info.control_hz = Some(cc);
            self.info.ppm = Some((s - 1.0) * 1e6);
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// `granted` is false for an update, which repeats a call already granted.
    fn grant(&mut self, channel: u16, group: u16, source: u32, encrypted: bool, emergency: bool, granted: bool, center: f64) -> Option<f64> {
        let (hz, tdma) = self.freq(channel)?;
        let now = self.now;
        let call = Call { group, source, hz, encrypted, tdma, emergency, at_secs: now };
        match self.recent.iter_mut().find(|c| c.group == group && c.hz == hz && now - c.at_secs < SAME_CALL_SECS) {
            Some(c) => {
                c.at_secs = now;
                if source != 0 {
                    c.source = source;
                }
            }
            None => {
                if self.recent.len() == RECENT_CALLS {
                    self.recent.pop_back();
                }
                self.recent.push_front(call.clone());
            }
        }
        // updates keep announcing a call for a moment after it ends
        let just_ended = self.ended.is_some_and(|(g, h, at)| g == group && h == hz && now - at < REFOLLOW_SECS);
        if encrypted || tdma || self.info.voice_error.is_some() || (just_ended && !granted) {
            return None;
        }
        let raw_hz = hz * self.scale()?;
        self.voice = Some(Voice { call, raw_hz, decoder: mbe::Decoder::new().ok(), last_frame: now, frames: 0, bad: 0 });
        self.phy.reset();
        self.agc = Agc::new();
        if (raw_hz - center).abs() > self.rate / 2.0 - EDGE_HZ {
            self.home_center = Some(center);
            return Some(raw_hz + RETUNE_OFFSET_HZ);
        }
        None
    }

    fn voice_frame(&mut self, f: &Frame) -> Option<f64> {
        let v = self.voice.as_mut()?;
        match f.duid {
            phy::DUID_LDU1 | phy::DUID_LDU2 => {
                // a distant site reusing the channel has its own NAC; reading errors flip a bit or two
                let ours = self.info.nac.is_none_or(|n| (n ^ f.nac).count_ones() <= NAC_SLACK_BITS);
                let dec = v.decoder.as_mut().filter(|_| ours)?;
                let mut pcm = Vec::with_capacity(9 * mbe::FRAME_SAMPLES);
                let mut bad = 0;
                for &at in &voice::FRAMES {
                    let (p, errs) = dec.decode(&voice::deinterleave(&f.dibits[at..at + 72]));
                    if errs > BAD_FRAME_ERRS {
                        bad += 1;
                    }
                    pcm.extend_from_slice(&p);
                }
                v.frames += voice::FRAMES.len() as u32;
                v.bad += bad;
                // a mostly damaged frame is noise or someone else's call: mute it, and let the call end
                if bad * 2 < voice::FRAMES.len() as u32 {
                    v.last_frame = self.now;
                    self.audio8k.extend_from_slice(&pcm);
                }
                None
            }
            phy::DUID_HDU => {
                v.last_frame = self.now;
                None
            }
            // the talker let go; the channel often carries the reply, so wait a little
            phy::DUID_TDU | phy::DUID_TDULC => {
                v.last_frame = v.last_frame.min(self.now - HANG_SECS + AFTER_TDU_SECS);
                None
            }
            _ => None,
        }
    }

    fn end_call(&mut self) -> Option<f64> {
        if let Some(v) = self.voice.take() {
            self.ended = Some((v.call.group, v.call.hz, self.now));
        }
        self.phy.reset();
        self.last_control = self.now;
        self.home_center.take()
    }

    /// Try the next carrier near the tuned frequency, nearest first, then the tuned one again.
    fn next_candidate(&mut self, signals: &[Detected]) {
        self.locked = false;
        self.last_control = self.now;
        if self.candidates.is_empty() {
            let reach = self.tuned_hz * SEARCH_PPM * 1e-6;
            let mut c: Vec<f64> = signals.iter().map(|s| s.hz).filter(|hz| (hz - self.tuned_hz).abs() <= reach).collect();
            c.sort_by(|a, b| (b - self.tuned_hz).abs().total_cmp(&(a - self.tuned_hz).abs()));
            c.insert(0, self.tuned_hz);
            self.candidates = c;
        }
        if let Some(hz) = self.candidates.pop() {
            self.control_raw = hz;
            self.control_channel = None;
            self.phy.reset();
        }
    }

    /// 8 kHz codec audio → 48 kHz, levelled.
    fn play(&mut self, pcm: &mut Vec<f32>) {
        if self.audio8k.is_empty() {
            return;
        }
        let mut stuffed = Vec::with_capacity(self.audio8k.len() * 6);
        for &s in &self.audio8k {
            stuffed.push(s * 6.0);
            stuffed.extend_from_slice(&[0.0; 5]);
        }
        self.audio8k.clear();
        self.up.process(&stuffed, pcm);
        self.agc.process(pcm);
    }
}
