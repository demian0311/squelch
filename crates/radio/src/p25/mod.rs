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
use std::collections::{HashMap, HashSet, VecDeque};
use tsbk::Message;

/// Audio out, matching the other demodulators.
const AUDIO_RATE: f64 = 48_000.0;
/// Seconds without a control frame before a locked control channel counts as lost.
const SEARCH_SECS: f64 = 1.5;
/// Seconds a carrier gets while searching; a control channel syncs within a few frames (0.2–0.4 s on
/// the dongle, Miami 2026-11-04).
const SCAN_TRY_SECS: f64 = 0.6;
/// Carriers tried per capture, strongest first: a crowded 800 MHz capture holds 79 signals above
/// the floor (Miami 2026-11-04), nearly all of them weak; the site's channels were its 8 strongest.
const MAX_CANDIDATES: usize = 8;
/// A control channel is a steady narrow signal: weaker or wider carriers are not tried.
const MIN_SNR_DB: f32 = 12.0;
const MAX_WIDTH_HZ: f64 = 40_000.0;
/// After a hop: the tuner settling and the detector's averaged rows refilling.
const SCAN_SETTLE_SECS: f64 = 0.35;
/// A frequency this close to the control channel is the control channel.
const SAME_CHANNEL_HZ: f64 = 5_000.0;
/// A call ends after this long without a voice frame.
const HANG_SECS: f64 = 1.0;
/// Voice channels closer than this to the capture edge need the dongle moved.
const EDGE_HZ: f64 = 100_000.0;
/// Where the dongle centre goes relative to a voice channel it had to move for.
const RETUNE_OFFSET_HZ: f64 = 300_000.0;
/// Calls are tallied per talkgroup over this window, one bar a minute.
pub const WINDOW_MIN: usize = 10;
const WINDOW_SECS: f64 = WINDOW_MIN as f64 * 60.0;
/// Calls kept for the tally, however busy the window.
const RECENT_CALLS: usize = 400;
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

/// A talkgroup heard on this site in the last `WINDOW_MIN` minutes.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Talkgroup {
    pub group: u16,
    /// Calls in the window.
    pub calls: u32,
    /// Receiver clock of the last call.
    pub last_secs: f64,
    /// Voice channel and talker of the last call.
    pub hz: f64,
    pub source: u32,
    /// The last call was encrypted, or on a Phase 2 channel: this receiver cannot play it.
    pub encrypted: bool,
    pub tdma: bool,
    /// An emergency call in the window.
    pub emergency: bool,
    /// Calls per minute across the window, oldest first.
    pub minutes: Vec<u8>,
}

/// One hop of a scan across the band for a control channel.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Scan {
    /// 1-based.
    pub hop: usize,
    pub hops: usize,
    pub lo_hz: f64,
    pub hi_hz: f64,
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
    /// Talkgroups heard in the last `WINDOW_MIN` minutes, busiest first.
    pub groups: Vec<Talkgroup>,
    /// Why voice cannot play (mbelib missing).
    pub voice_error: Option<String>,
    /// Voice frames the codec found badly damaged, percent, this call.
    pub voice_errors_pct: f32,
    /// Hopping across the band for a control channel.
    pub scan: Option<Scan>,
    /// Carrier under test while searching, dongle frequency.
    pub trying_hz: Option<f64>,
    /// Carriers tested since the search started, dongle frequencies.
    pub tried: Vec<f64>,
}

/// Where the search for a control channel is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Search {
    /// The capture the user tuned; `refilled` once its carriers are listed.
    Home { refilled: bool },
    /// Hop `at` of the band's hops; `filled` once its carriers are listed.
    Hop { at: usize, filled: bool },
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
    search: Search,
    /// Centre, low and high edge of each scan hop; empty without band ranges.
    hops: Vec<(f64, f64, f64)>,
    /// The hops in the order a scan takes them: nearest the user first, so their own band comes first.
    order: Vec<usize>,
    /// Dongle centre to go back to after a scan finds nothing.
    scan_home: f64,
    tried: Vec<f64>,
    idens: [Option<Iden>; 16],
    control_channel: Option<u16>,
    info: Info,
    good_msgs: u32,
    rate_since: f64,
    /// Talkgroups seen in grants, and whether they were in the clear.
    clear_groups: HashMap<u16, bool>,
    recent: VecDeque<Call>,
    /// Only this talkgroup plays.
    hold: Option<u16>,
    /// Talkgroups that never play.
    skip: HashSet<u16>,
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
            search: Search::Home { refilled: false },
            hops: Vec::new(),
            order: Vec::new(),
            scan_home: tuned_hz,
            tried: Vec::new(),
            locked: false,
            idens: [None; 16],
            control_channel: None,
            info: Info { voice_error: mbe::available().err(), ..Info::default() },
            good_msgs: 0,
            rate_since: 0.0,
            clear_groups: HashMap::new(),
            recent: VecDeque::new(),
            hold: None,
            skip: HashSet::new(),
            voice: None,
            ended: None,
            nacs: HashMap::new(),
            home_center: None,
            up: Decimator::new(lowpass(3_600.0 / AUDIO_RATE as f32, 800.0 / AUDIO_RATE as f32), 1),
            agc: Agc::new(),
            audio8k: Vec::new(),
        }
    }

    /// The band's frequency ranges: when the capture holds no control channel, the search hops across them.
    pub fn set_ranges(&mut self, ranges: &[(f64, f64)]) {
        self.hops = crate::look::slices_of(ranges).iter().map(|s| (s.center, s.lo, s.hi)).collect();
    }

    /// Hopping across the band now: the dongle is not where the user left it.
    pub fn scanning(&self) -> bool {
        matches!(self.search, Search::Hop { .. })
    }

    /// `hz` is the control channel being followed, so tuning it changes nothing.
    pub fn on_control(&self, hz: f64) -> bool {
        self.locked && (hz - self.info.control_hz.unwrap_or(self.control_raw)).abs() < SAME_CHANNEL_HZ
    }

    pub fn set_tuned(&mut self, hz: f64) {
        self.tuned_hz = hz;
    }

    fn try_secs(&self) -> f64 {
        if self.locked {
            SEARCH_SECS
        } else {
            SCAN_TRY_SECS
        }
    }

    /// The carriers worth trying in a capture, strongest first (`first`, the user's own frequency,
    /// ahead of them), ready to `pop`.
    fn candidates_in(&self, signals: &[Detected], center: f64, first: Option<f64>) -> Vec<f64> {
        let usable = self.rate * 0.4;
        let mut c: Vec<&Detected> = signals.iter().filter(|s| (s.hz - center).abs() <= usable && s.width_hz <= MAX_WIDTH_HZ && s.snr_db >= MIN_SNR_DB).collect();
        c.sort_by(|a, b| b.snr_db.total_cmp(&a.snr_db));
        c.truncate(MAX_CANDIDATES);
        let mut out: Vec<f64> = c.iter().map(|s| s.hz).collect();
        if let Some(f) = first {
            out.retain(|h| (h - f).abs() > SAME_CHANNEL_HZ);
            out.insert(0, f);
        }
        out.reverse();
        out
    }

    pub fn info(&self) -> Info {
        let mut i = self.info.clone();
        i.now_secs = self.now;
        i.tried = self.tried.clone();
        i.trying_hz = (self.voice.is_none() && !self.locked).then_some(self.control_raw);
        i.groups = self.groups();
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

    /// Only `hold` plays (None: any talkgroup), and nothing in `skip` does. A call that is no
    /// longer allowed ends now; the returned centre is where the dongle goes back to, as in `process`.
    pub fn set_picks(&mut self, hold: Option<u16>, skip: &[u16]) -> Option<f64> {
        self.hold = hold;
        self.skip = skip.iter().copied().collect();
        match &self.voice {
            Some(v) if !self.allowed(v.call.group) => self.end_call(),
            _ => None,
        }
    }

    fn allowed(&self, group: u16) -> bool {
        !self.skip.contains(&group) && self.hold.is_none_or(|h| h == group)
    }

    /// The window's calls folded per talkgroup, busiest first; the last call gives the fields.
    fn groups(&self) -> Vec<Talkgroup> {
        let mut by: Vec<Talkgroup> = Vec::new();
        for c in self.recent.iter().rev() {
            let age = self.now - c.at_secs;
            if age >= WINDOW_SECS {
                continue;
            }
            let minute = WINDOW_MIN - 1 - ((age.max(0.0) / 60.0) as usize).min(WINDOW_MIN - 1);
            let g = match by.iter_mut().position(|g| g.group == c.group) {
                Some(i) => &mut by[i],
                None => {
                    by.push(Talkgroup { group: c.group, calls: 0, last_secs: c.at_secs, hz: c.hz, source: c.source, encrypted: c.encrypted, tdma: c.tdma, emergency: false, minutes: vec![0; WINDOW_MIN] });
                    by.last_mut().unwrap()
                }
            };
            g.calls += 1;
            g.minutes[minute] = g.minutes[minute].saturating_add(1);
            g.emergency |= c.emergency;
            if c.at_secs >= g.last_secs {
                g.last_secs = c.at_secs;
                g.hz = c.hz;
                g.source = c.source;
                g.encrypted = c.encrypted;
                g.tdma = c.tdma;
            }
        }
        by.sort_by(|a, b| b.calls.cmp(&a.calls).then(b.last_secs.total_cmp(&a.last_secs)));
        by
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
            None if self.now - self.last_control > self.try_secs() => {
                if let Some(c) = self.next_candidate(signals, center) {
                    retune = Some(c);
                }
            }
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
            if !self.locked {
                self.found();
            }
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
                while self.recent.len() >= RECENT_CALLS || self.recent.back().is_some_and(|c| now - c.at_secs >= WINDOW_SECS) {
                    self.recent.pop_back();
                }
                self.recent.push_front(call.clone());
            }
        }
        // updates keep announcing a call for a moment after it ends
        let just_ended = self.ended.is_some_and(|(g, h, at)| g == group && h == hz && now - at < REFOLLOW_SECS);
        if encrypted || tdma || !self.allowed(group) || self.info.voice_error.is_some() || (just_ended && !granted) {
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

    /// A control channel locked: the search is over.
    fn found(&mut self) {
        self.search = Search::Home { refilled: false };
        self.candidates.clear();
        self.tried.clear();
        self.info.scan = None;
    }

    /// Start a hop, or the way home: the first try comes right when the tuner has settled.
    fn enter(&mut self, s: Search) {
        self.search = s;
        self.last_control = self.now + SCAN_SETTLE_SECS - self.try_secs();
    }

    /// The next carrier to try. In the capture the user tuned: their frequency, then the strongest
    /// carriers (a site's control channel is any one of its channels; Miami-Dade's downtown site
    /// spreads 20 across 2.75 MHz). When that finds nothing and the band's ranges are known: each
    /// hop across them nearest the user first, strongest carriers first, then home again.
    /// Returns a dongle centre to move to.
    fn next_candidate(&mut self, signals: &[Detected], center: f64) -> Option<f64> {
        self.locked = false;
        self.last_control = self.now;
        match self.search {
            Search::Home { refilled: false } => {
                self.search = Search::Home { refilled: true };
                self.candidates = self.candidates_in(signals, center, Some(self.tuned_hz));
            }
            Search::Hop { at, filled: false } => {
                self.search = Search::Hop { at, filled: true };
                self.candidates = self.candidates_in(signals, center, None);
            }
            _ => {}
        }
        if let Some(hz) = self.candidates.pop() {
            self.control_raw = hz;
            self.control_channel = None;
            self.phy.reset();
            self.tried.push(hz);
            return None;
        }
        if self.hops.is_empty() {
            self.search = Search::Home { refilled: false };
            return None;
        }
        let next = match self.search {
            Search::Home { .. } => {
                self.scan_home = center;
                self.tried.clear();
                self.order = (0..self.hops.len()).collect();
                self.order.sort_by(|&a, &b| (self.hops[a].0 - center).abs().total_cmp(&(self.hops[b].0 - center).abs()));
                0
            }
            Search::Hop { at, .. } => at + 1,
        };
        if next < self.hops.len() {
            let (c, lo, hi) = self.hops[self.order[next]];
            self.enter(Search::Hop { at: next, filled: false });
            self.info.scan = Some(Scan { hop: next + 1, hops: self.hops.len(), lo_hz: lo, hi_hz: hi });
            Some(c)
        } else {
            self.enter(Search::Home { refilled: false });
            self.info.scan = None;
            Some(self.scan_home)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(hz: f64) -> Detected {
        Detected { hz, width_hz: 12_500.0, snr_db: 20.0, hd: None }
    }

    fn snr(hz: f64, db: f32) -> Detected {
        Detected { snr_db: db, ..seen(hz) }
    }

    #[test]
    fn the_tuned_frequency_then_the_strongest_carriers_in_the_capture() {
        let mut p = P25::new(2_400_000, 852.5e6);
        let signals = [snr(853.3e6, 25.0), snr(851.7e6, 35.0), snr(852.6e6, 30.0), snr(852.9e6, 11.0), Detected { width_hz: 80_000.0, ..snr(852.2e6, 40.0) }];
        let mut tried = Vec::new();
        for _ in 0..4 {
            assert_eq!(p.next_candidate(&signals, 852.5e6), None);
            tried.push(p.control_raw);
        }
        // the weak one and the wide one are never tried
        assert_eq!(tried, vec![852.5e6, 851.7e6, 852.6e6, 853.3e6]);
        // without band ranges the capture is searched again, never left
        assert_eq!(p.next_candidate(&signals, 852.5e6), None);
        assert!(!p.scanning());
    }

    #[test]
    fn a_crowded_capture_gets_only_its_strongest_carriers_tried() {
        let p = P25::new(2_400_000, 852.5e6);
        let signals: Vec<Detected> = (0..40).map(|i| snr(851.5e6 + i as f64 * 25e3, 12.0 + i as f32)).collect();
        let c = p.candidates_in(&signals, 852.5e6, None);
        assert_eq!(c.len(), MAX_CANDIDATES);
        assert_eq!(*c.last().unwrap(), 851.5e6 + 39.0 * 25e3, "the strongest pops first");
    }

    #[test]
    fn an_empty_capture_sends_the_dongle_across_the_band_and_home() {
        let mut p = P25::new(2_400_000, 852.5e6);
        p.set_ranges(&[(851e6, 869e6)]);
        let mut centre = 852.5e6;
        let mut moves = Vec::new();
        for _ in 0..30 {
            p.now += 2.0;
            if let Some(c) = p.next_candidate(&[], centre) {
                centre = c;
                moves.push(c);
            }
        }
        let hops = 10;
        // nearest the user first, so a scan from 864 reaches 851 before 769
        assert!((moves[0] - 852.5e6).abs() < 1e6, "{moves:?}");
        assert!(moves[..hops].windows(2).all(|w| (w[0] - 852.5e6).abs() <= (w[1] - 852.5e6).abs()), "{moves:?}");
        assert!(moves[..hops].iter().all(|c| (851e6..869e6).contains(c)), "{moves:?}");
        let mut sorted = moves[..hops].to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted.dedup();
        assert_eq!(sorted.len(), hops, "every hop once: {moves:?}");
        assert_eq!(moves[hops], 852.5e6, "home after the last hop");
        assert_eq!(moves[hops + 1], moves[0], "then round again");
        assert!(p.info().scan.is_some_and(|s| (1..=hops).contains(&s.hop) && s.hops == hops));
    }

    #[test]
    fn the_scan_strongest_carrier_first_and_the_found_channel_is_tuned_without_a_restart() {
        let mut p = P25::new(2_400_000, 864e6);
        p.set_ranges(&[(851e6, 869e6)]);
        p.now = 10.0;
        p.next_candidate(&[], 864e6);
        let hop = p.next_candidate(&[], 864e6).unwrap();
        assert!(p.scanning());
        p.now += 1.0;
        let weak = snr(hop + 0.2e6, 14.0);
        let strong = snr(hop - 0.3e6, 30.0);
        assert_eq!(p.next_candidate(&[weak, strong.clone()], hop), None);
        assert_eq!(p.control_raw, strong.hz);
        assert_eq!(p.info().tried, vec![strong.hz]);
        p.locked = true;
        assert!(p.on_control(strong.hz + 1_000.0));
        assert!(!p.on_control(strong.hz + 12_500.0));
    }

    #[test]
    fn talkgroups_are_tallied_over_the_window_and_hold_and_skip_pick_what_plays() {
        let mut p = P25::new(2_400_000, 852.5e6);
        p.idens[0] = Some(Iden { base_hz: 851e6, spacing_hz: 12_500.0, tdma: false });
        p.control_channel = Some(0);
        p.control_raw = 851e6;
        p.info.voice_error = None;
        p.now = 100.0;
        p.grant(10, 2024, 1, false, false, true, 852.5e6);
        assert!(p.in_call(), "a clear grant inside the capture plays");
        p.end_call();
        p.now = 130.0;
        p.grant(11, 2001, 2, true, false, true, 852.5e6);
        assert!(!p.in_call(), "an encrypted grant is tallied, not played");
        p.now = 400.0;
        p.grant(10, 2024, 3, false, true, true, 852.5e6);
        let g = p.groups();
        assert_eq!(g.iter().map(|g| (g.group, g.calls, g.encrypted, g.emergency, g.source)).collect::<Vec<_>>(), vec![(2024, 2, false, true, 3), (2001, 1, true, false, 2)]);
        assert_eq!(g[0].minutes.len(), WINDOW_MIN);
        assert_eq!(g[0].minutes[WINDOW_MIN - 1], 1, "the call just now is in the newest minute");
        assert_eq!(g[0].minutes[WINDOW_MIN - 1 - 5], 1, "the call 300 s ago is five minutes back");
        // skipping the talkgroup playing ends its call, and it never plays again
        assert!(p.in_call());
        p.set_picks(None, &[2024]);
        assert!(!p.in_call());
        p.now = 420.0;
        p.grant(10, 2024, 3, false, false, true, 852.5e6);
        assert!(!p.in_call());
        // a hold lets only that talkgroup play
        p.set_picks(Some(2001), &[]);
        p.now = 440.0;
        p.grant(10, 2024, 3, false, false, true, 852.5e6);
        assert!(!p.in_call());
        p.grant(12, 2001, 4, false, false, true, 852.5e6);
        assert!(p.in_call());
        assert_eq!(p.groups()[0].calls, 4, "skipped and held calls are still tallied");
        // calls older than the window leave the tally
        p.now = 440.0 + WINDOW_SECS + 1.0;
        assert!(p.groups().is_empty());
    }
}
