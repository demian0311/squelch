//! DMR (ETSI TS 102 361) conventional voice, as ham repeaters carry it: two time slots on one
//! carrier, each with its own calls. `Dmr` mixes the tuned channel out of the capture, slices
//! bursts, reads the link control (who calls which talkgroup) from headers, terminators and
//! the embedded signalling of voice bursts, and plays one slot's clear voice through mbelib.
//! Encrypted calls (privacy bit, PI header or the EMB's PI flag) are counted and muted.

pub mod fec;
pub mod lc;
pub mod phy;
pub mod voice;

use crate::dsp::{lowpass, Agc, Decimate, Decimator, Mixer, C32};
use crate::p25::mbe;
use lc::FullLc;
use phy::{Burst, Phy, Source};
use serde::Serialize;
use std::collections::VecDeque;

const AUDIO_RATE: f64 = 48_000.0;
/// A call ends this long after its last voice burst.
const HANG_SECS: f64 = 1.0;
/// No sync for this long: nothing DMR on the channel.
const QUIET_SECS: f64 = 2.0;
/// A burst without a sync is voice only this soon after the slot's last voice burst (they come every 60 ms).
const VOICE_GAP_SECS: f64 = 0.2;
/// Codec-corrected bit errors above which a voice frame counts as bad.
const BAD_FRAME_ERRS: u32 = 4;
const RECENT_CALLS: usize = 50;

const DT_PI_HEADER: u8 = 0;
const DT_VOICE_HEADER: u8 = 1;
const DT_TERMINATOR: u8 = 2;
const DT_IDLE: u8 = 9;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Call {
    /// Time slot 1 or 2.
    pub slot: u8,
    /// A talkgroup call; false for radio to radio.
    pub group: bool,
    /// Talkgroup or called radio; None until a header or the embedded LC is read.
    pub dst: Option<u32>,
    pub src: Option<u32>,
    pub emergency: bool,
    pub encrypted: bool,
    pub start_secs: f64,
    pub last_secs: f64,
    pub frames: u32,
    pub bad_frames: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SlotInfo {
    pub call: Option<Call>,
    /// This slot's voice goes to the speaker.
    pub playing: bool,
    pub calls: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Info {
    pub now_secs: f64,
    /// A sync was seen within `QUIET_SECS`.
    pub heard: bool,
    /// "repeater", "radio" or "direct", from the last sync.
    pub source: Option<&'static str>,
    pub colour_code: Option<u8>,
    pub slots: [SlotInfo; 2],
    /// Calls that ended, newest first.
    pub recent: Vec<Call>,
    /// Why voice cannot play (mbelib missing).
    pub voice_error: Option<String>,
    pub bursts_per_sec: f32,
    pub encrypted_calls: u32,
}

#[derive(Default)]
struct Slot {
    call: Option<Call>,
    last_voice: f64,
    in_voice: bool,
    /// Embedded LC fragments collected so far in this superframe.
    emb: Vec<[u8; 32]>,
    calls: u32,
}

pub struct Dmr {
    rate: f64,
    mixer: Mixer,
    mixed: (f64, f64),
    d1: Decimate<C32>,
    d2: Decimator<C32>,
    chan: Vec<C32>,
    s1: Vec<C32>,
    s2: Vec<C32>,
    phy: Phy,
    bursts: Vec<Burst>,
    now: f64,
    tuned_hz: f64,
    slots: [Slot; 2],
    /// `(seq + parity) & 1` is a burst's slot, once a CACH or a direct-mode sync said so.
    parity: Option<u64>,
    last_sync: f64,
    source: Option<Source>,
    cc: Option<u8>,
    recent: VecDeque<Call>,
    /// Slot whose voice plays.
    playing: Option<usize>,
    decoder: Option<mbe::Decoder>,
    voice_error: Option<String>,
    up: Decimator<f32>,
    agc: Agc,
    audio8k: Vec<f32>,
    burst_count: u32,
    rate_since: f64,
    bursts_per_sec: f32,
    encrypted_calls: u32,
}

fn bits_of(dibits: &[u8]) -> Vec<u8> {
    dibits.iter().flat_map(|&d| [(d >> 1) & 1, d & 1]).collect()
}

fn field(bits: &[u8], at: usize, n: usize) -> u32 {
    bits[at..at + n].iter().fold(0u32, |b, &x| (b << 1) | (x & 1) as u32)
}

impl Dmr {
    pub fn new(rate: u32, tuned_hz: f64) -> Self {
        let fs = rate as f32;
        let mid = (AUDIO_RATE * 5.0) as f32;
        Dmr {
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
            bursts: Vec::new(),
            now: 0.0,
            tuned_hz,
            slots: Default::default(),
            parity: None,
            last_sync: f64::NEG_INFINITY,
            source: None,
            cc: None,
            recent: VecDeque::new(),
            playing: None,
            decoder: None,
            voice_error: mbe::available().err(),
            up: Decimator::new(lowpass(3_600.0 / AUDIO_RATE as f32, 800.0 / AUDIO_RATE as f32), 1),
            agc: Agc::new(),
            audio8k: Vec::new(),
            burst_count: 0,
            rate_since: 0.0,
            bursts_per_sec: 0.0,
            encrypted_calls: 0,
        }
    }

    /// A new channel: forget the calls of the old one.
    pub fn set_tuned(&mut self, hz: f64) {
        if hz == self.tuned_hz {
            return;
        }
        self.tuned_hz = hz;
        self.phy.reset();
        self.slots = Default::default();
        self.parity = None;
        self.cc = None;
        self.source = None;
        self.playing = None;
        self.decoder = None;
        self.recent.clear();
        self.audio8k.clear();
        self.last_sync = f64::NEG_INFINITY;
    }

    pub fn info(&self) -> Info {
        Info {
            now_secs: self.now,
            heard: self.now - self.last_sync < QUIET_SECS,
            source: self.source.map(|s| match s {
                Source::Repeater => "repeater",
                Source::Mobile => "radio",
                Source::Direct(_) => "direct",
            }),
            colour_code: self.cc,
            slots: std::array::from_fn(|i| SlotInfo { call: self.slots[i].call.clone(), playing: self.playing == Some(i), calls: self.slots[i].calls }),
            recent: self.recent.iter().cloned().collect(),
            voice_error: self.voice_error.clone(),
            bursts_per_sec: self.bursts_per_sec,
            encrypted_calls: self.encrypted_calls,
        }
    }

    pub fn in_call(&self) -> bool {
        self.slots.iter().any(|s| s.call.is_some())
    }

    pub fn process(&mut self, iq: &[C32], center: f64, pcm: &mut Vec<f32>) {
        pcm.clear();
        self.now += iq.len() as f64 / self.rate;
        if (self.tuned_hz, center) != self.mixed {
            self.mixer.set(self.tuned_hz - center, self.rate);
            self.mixed = (self.tuned_hz, center);
        }
        self.chan.clear();
        self.chan.extend_from_slice(iq);
        self.mixer.process(&mut self.chan);
        self.s1.clear();
        self.d1.process(&self.chan, &mut self.s1);
        self.s2.clear();
        self.d2.process(&self.s1, &mut self.s2);
        let mut bursts = std::mem::take(&mut self.bursts);
        bursts.clear();
        self.phy.process(&self.s2, &mut bursts);
        for b in &bursts {
            self.burst(b);
        }
        self.burst_count += bursts.len() as u32;
        self.bursts = bursts;
        for i in 0..2 {
            if self.slots[i].call.as_ref().is_some_and(|_| self.now - self.slots[i].last_voice > HANG_SECS) {
                self.end_call(i);
            }
        }
        if self.now - self.rate_since >= 2.0 {
            self.bursts_per_sec = self.burst_count as f32 / (self.now - self.rate_since) as f32;
            self.burst_count = 0;
            self.rate_since = self.now;
        }
        self.play(pcm);
    }

    /// Which slot a burst belongs to: the CACH says on a repeater's output, a direct-mode sync
    /// says, and otherwise consecutive bursts alternate.
    fn slot_of(&mut self, b: &Burst) -> usize {
        if b.relocked {
            self.parity = None;
        }
        let repeater = b.sync.map(|s| s.source == Source::Repeater).unwrap_or(self.source == Some(Source::Repeater));
        if repeater {
            let cach = bits_of(&b.cach);
            let mut tact = [cach[0], cach[4], cach[8], cach[12], cach[14], cach[18], cach[22]];
            if fec::H7_4.decode(&mut tact).is_some() {
                let slot = tact[1] as u64;
                self.parity = Some((slot + b.seq) & 1);
                return slot as usize;
            }
        }
        if let Some(phy::Sync { source: Source::Direct(ts), .. }) = b.sync {
            let slot = (ts - 1) as u64;
            self.parity = Some((slot + b.seq) & 1);
            return slot as usize;
        }
        ((b.seq + self.parity.unwrap_or(0)) & 1) as usize
    }

    fn burst(&mut self, b: &Burst) {
        let i = self.slot_of(b);
        match b.sync {
            Some(s) => {
                self.last_sync = self.now;
                self.source = Some(s.source);
                if s.voice {
                    self.slots[i].emb.clear();
                    self.voice_burst(i, b);
                } else {
                    self.data_burst(i, b);
                }
            }
            None => {
                let s = &self.slots[i];
                if s.in_voice && self.now - s.last_voice < VOICE_GAP_SECS {
                    self.embedded(i, b);
                    self.voice_burst(i, b);
                }
            }
        }
    }

    fn data_burst(&mut self, i: usize, b: &Burst) {
        let bits = bits_of(&b.dibits);
        let mut slot_type = [0u8; 20];
        slot_type[..10].copy_from_slice(&bits[98..108]);
        slot_type[10..].copy_from_slice(&bits[156..166]);
        let Some(_) = fec::golay_20_8_decode(&mut slot_type) else { return };
        self.cc = Some(field(&slot_type, 0, 4) as u8);
        let dt = field(&slot_type, 4, 4) as u8;
        let mut info = [0u8; 196];
        info[..98].copy_from_slice(&bits[..98]);
        info[98..].copy_from_slice(&bits[166..264]);
        let now = self.now;
        match dt {
            DT_VOICE_HEADER => {
                let Some(block) = fec::bptc_196_96_decode(&info) else { return };
                if let Some(lc) = FullLc::from_block(&block, lc::MASK_HEADER).filter(FullLc::is_voice) {
                    let slot = &mut self.slots[i];
                    apply_lc(slot.call.get_or_insert_with(|| new_call(i, now)), &lc);
                    slot.in_voice = true;
                    slot.last_voice = now;
                    slot.emb.clear();
                    self.pick_playing(i);
                }
            }
            DT_PI_HEADER => {
                let slot = &mut self.slots[i];
                slot.call.get_or_insert_with(|| new_call(i, now)).encrypted = true;
                slot.in_voice = true;
                slot.last_voice = now;
            }
            DT_TERMINATOR => {
                if let Some(block) = fec::bptc_196_96_decode(&info) {
                    if let (Some(lc), Some(call)) = (FullLc::from_block(&block, lc::MASK_TERMINATOR), self.slots[i].call.as_mut()) {
                        apply_lc(call, &lc);
                    }
                }
                self.end_call(i);
            }
            DT_IDLE => self.slots[i].in_voice = false,
            _ => self.slots[i].in_voice = false,
        }
    }

    /// The EMB field and embedded LC fragment of voice bursts B–F.
    fn embedded(&mut self, i: usize, b: &Burst) {
        let centre = bits_of(&b.dibits[voice::CENTRE]);
        let mut emb = [0u8; 16];
        emb[..8].copy_from_slice(&centre[..8]);
        emb[8..].copy_from_slice(&centre[40..48]);
        let Some(_) = fec::qr_16_7_decode(&mut emb) else { return };
        self.cc = Some(field(&emb, 0, 4) as u8);
        let pi = emb[4] == 1;
        let lcss = field(&emb, 5, 2) as u8;
        let slot = &mut self.slots[i];
        if pi {
            if let Some(c) = slot.call.as_mut() {
                c.encrypted = true;
            }
        }
        let frag: [u8; 32] = centre[8..40].try_into().unwrap();
        match lcss {
            1 => {
                slot.emb.clear();
                slot.emb.push(frag);
            }
            3 if !slot.emb.is_empty() => slot.emb.push(frag),
            2 if slot.emb.len() == 3 => {
                slot.emb.push(frag);
                let all: Vec<u8> = slot.emb.concat();
                slot.emb.clear();
                let Some(block) = fec::embedded_lc_decode(all[..].try_into().unwrap()) else { return };
                let lc_bits: [u8; 72] = block[..72].try_into().unwrap();
                if fec::lc_checksum(&lc_bits) != field(&block, 72, 5) as u8 {
                    return;
                }
                let lc = FullLc::parse(&lc_bits);
                if lc.is_voice() {
                    if let Some(c) = slot.call.as_mut() {
                        apply_lc(c, &lc);
                    }
                }
            }
            _ => slot.emb.clear(),
        }
    }

    fn voice_burst(&mut self, i: usize, b: &Burst) {
        let now = self.now;
        let encrypted = {
            let slot = &mut self.slots[i];
            let call = slot.call.get_or_insert_with(|| new_call(i, now));
            call.last_secs = now;
            slot.in_voice = true;
            slot.last_voice = now;
            call.encrypted
        };
        self.pick_playing(i);
        if self.playing != Some(i) {
            return;
        }
        if self.decoder.is_none() {
            self.decoder = mbe::Decoder::new().ok();
        }
        let Some(dec) = self.decoder.as_mut() else { return };
        let mut pcm = Vec::with_capacity(3 * mbe::FRAME_SAMPLES);
        let mut bad = 0;
        for f in voice::frames(&b.dibits) {
            let (p, errs) = dec.decode_ambe(&voice::deinterleave(&f));
            if errs > BAD_FRAME_ERRS {
                bad += 1;
            }
            pcm.extend_from_slice(&p);
        }
        let call = self.slots[i].call.as_mut().unwrap();
        call.frames += 3;
        call.bad_frames += bad;
        // a mostly damaged burst is noise; an encrypted one is nothing anyone should hear
        if bad < 2 && !encrypted {
            self.audio8k.extend_from_slice(&pcm);
        }
    }

    fn pick_playing(&mut self, i: usize) {
        if self.playing.is_none() {
            self.playing = Some(i);
        }
    }

    fn end_call(&mut self, i: usize) {
        let slot = &mut self.slots[i];
        slot.in_voice = false;
        slot.emb.clear();
        let Some(call) = slot.call.take() else { return };
        slot.calls += 1;
        if call.encrypted {
            self.encrypted_calls += 1;
        }
        self.recent.push_front(call);
        self.recent.truncate(RECENT_CALLS);
        if self.playing == Some(i) {
            self.playing = (0..2).find(|&j| self.slots[j].call.is_some());
            self.decoder = None;
        }
    }

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

fn new_call(i: usize, now: f64) -> Call {
    Call { slot: i as u8 + 1, group: true, dst: None, src: None, emergency: false, encrypted: false, start_secs: now, last_secs: now, frames: 0, bad_frames: 0 }
}

fn apply_lc(call: &mut Call, lc: &FullLc) {
    call.group = lc.flco == lc::FLCO_GROUP;
    call.dst = Some(lc.dst);
    call.src = Some(lc.src);
    call.emergency |= lc.emergency;
    call.encrypted |= lc.privacy;
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use phy::{modulate, sync_dibits};

    fn dibits_of(bits: &[u8]) -> Vec<u8> {
        bits.chunks(2).map(|p| (p[0] << 1) | p[1]).collect()
    }

    /// The 12 CACH dibits in front of a repeater's burst for time slot `slot` (0 or 1).
    pub fn cach(slot: u8) -> Vec<u8> {
        let tact = fec::H7_4.encode(&[0, slot, 0, 0]);
        let mut bits = [0u8; 24];
        for (k, &at) in [0, 4, 8, 12, 14, 18, 22].iter().enumerate() {
            bits[at] = tact[k];
        }
        dibits_of(&bits)
    }

    /// A data burst with a repeater sync: slot type (colour code, data type) and 196 BPTC bits.
    pub fn data_burst(cc: u8, dt: u8, block: &[u8; 96]) -> Vec<u8> {
        let info = fec::bptc_196_96_encode(block);
        let st = fec::golay_20_8_encode(&[(cc >> 3) & 1, (cc >> 2) & 1, (cc >> 1) & 1, cc & 1, (dt >> 3) & 1, (dt >> 2) & 1, (dt >> 1) & 1, dt & 1]);
        let mut bits = Vec::with_capacity(264);
        bits.extend_from_slice(&info[..98]);
        bits.extend_from_slice(&st[..10]);
        bits.extend_from_slice(&bits_of(&sync_dibits(Source::Repeater, false)));
        bits.extend_from_slice(&st[10..]);
        bits.extend_from_slice(&info[98..]);
        dibits_of(&bits)
    }

    /// A voice burst: `centre` is the 48-bit sync or EMB field; three coded vocoder frames around it.
    pub fn voice_burst(centre: &[u8], seed: u8) -> Vec<u8> {
        let frame = |n: usize| {
            let d: [u8; 49] = std::array::from_fn(|i| ((i * 7 + seed as usize * 3 + n) % 3 == 0) as u8);
            voice::interleave(&voice::encode_frame(&d))
        };
        let (f1, f2, f3) = (frame(0), frame(1), frame(2));
        let mut out = Vec::with_capacity(132);
        out.extend_from_slice(&f1);
        out.extend_from_slice(&f2[..18]);
        out.extend(dibits_of(centre));
        out.extend_from_slice(&f2[18..]);
        out.extend_from_slice(&f3);
        out
    }

    /// EMB (colour code, PI, LCSS) around a 32-bit fragment.
    pub fn emb_centre(cc: u8, pi: bool, lcss: u8, frag: &[u8]) -> Vec<u8> {
        let emb = fec::qr_16_7_encode(&[(cc >> 3) & 1, (cc >> 2) & 1, (cc >> 1) & 1, cc & 1, pi as u8, (lcss >> 1) & 1, lcss & 1]);
        let mut c = Vec::with_capacity(48);
        c.extend_from_slice(&emb[..8]);
        c.extend_from_slice(frag);
        c.extend_from_slice(&emb[8..]);
        c
    }

    /// A repeater's output: slot 1 carries `lc`'s call (header, one voice superframe with the
    /// embedded LC, terminator), slot 2 idles. Returns on-air dibits.
    pub fn repeater_call(lc: &FullLc, with_header: bool) -> Vec<u8> {
        let idle = data_burst(1, DT_IDLE, &[0u8; 96]);
        let mut out = vec![0u8; 300];
        let mut push = |slot: u8, burst: &[u8]| {
            out.extend(cach(slot));
            out.extend_from_slice(burst);
        };
        for _ in 0..2 {
            push(0, &idle);
            push(1, &idle);
        }
        if with_header {
            push(0, &data_burst(1, DT_VOICE_HEADER, &lc.to_block(lc::MASK_HEADER)));
            push(1, &idle);
        }
        let lc_bits = lc.to_bits();
        let cs = fec::lc_checksum(&lc_bits);
        let csb: [u8; 5] = std::array::from_fn(|i| (cs >> (4 - i)) & 1);
        let emb_lc = fec::embedded_lc_encode(&lc_bits, &csb);
        for n in 0..6u8 {
            let centre: Vec<u8> = match n {
                0 => bits_of(&sync_dibits(Source::Repeater, true)),
                1..=4 => emb_centre(1, false, [1, 3, 3, 2][n as usize - 1], &emb_lc[(n as usize - 1) * 32..n as usize * 32]),
                _ => emb_centre(1, false, 0, &[0u8; 32]),
            };
            push(0, &voice_burst(&centre, n));
            push(1, &idle);
        }
        push(0, &data_burst(1, DT_TERMINATOR, &lc.to_block(lc::MASK_TERMINATOR)));
        push(1, &idle);
        for _ in 0..3 {
            push(0, &idle);
            push(1, &idle);
        }
        out.extend(vec![0u8; 2000]);
        out
    }

    /// Runs the receiver at 240 kHz: the 48 kHz modulation held for 5 samples each, which the
    /// front end's first filter cleans up.
    fn run(raw: &[u8]) -> (Dmr, Vec<f32>) {
        let iq: Vec<C32> = modulate(raw).into_iter().flat_map(|s| [s; 5]).collect();
        let mut dmr = Dmr::new(240_000, 0.0);
        let mut audio = Vec::new();
        let mut pcm = Vec::new();
        for chunk in iq.chunks(24_000) {
            dmr.process(chunk, 0.0, &mut pcm);
            audio.extend_from_slice(&pcm);
        }
        (dmr, audio)
    }

    fn test_lc() -> FullLc {
        FullLc { protect: false, flco: lc::FLCO_GROUP, fid: 0, emergency: false, privacy: false, broadcast: false, priority: 0, dst: 31_088, src: 3_104_321 }
    }

    #[test]
    fn a_call_with_a_header_is_read_played_and_ended() {
        let (dmr, audio) = run(&repeater_call(&test_lc(), true));
        let info = dmr.info();
        assert_eq!(info.colour_code, Some(1));
        assert_eq!(info.source, Some("repeater"));
        assert!(info.slots.iter().all(|s| s.call.is_none()), "{:?}", info.slots);
        let call = info.recent.first().expect("one call ended");
        assert_eq!((call.slot, call.group, call.dst, call.src), (1, true, Some(31_088), Some(3_104_321)));
        assert_eq!((call.frames, call.bad_frames), (18, 0));
        assert!(!call.encrypted);
        assert_eq!(info.slots[0].calls, 1);
        if info.voice_error.is_none() {
            // 18 frames of 20 ms at 48 kHz, give or take the filter's delay
            assert!(audio.len() > 15_000, "{}", audio.len());
        }
    }

    #[test]
    fn without_the_header_the_embedded_lc_names_the_call() {
        let (dmr, _) = run(&repeater_call(&test_lc(), false));
        let call = dmr.info().recent.first().cloned().expect("one call ended");
        assert_eq!((call.dst, call.src), (Some(31_088), Some(3_104_321)));
    }

    #[test]
    fn an_encrypted_call_stays_silent() {
        let lc = FullLc { privacy: true, ..test_lc() };
        let (dmr, audio) = run(&repeater_call(&lc, true));
        let info = dmr.info();
        assert_eq!(info.encrypted_calls, 1);
        assert!(info.recent[0].encrypted);
        assert!(audio.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn plain_bursts_decode_their_slot_from_the_cach() {
        let (dmr, _) = run(&repeater_call(&test_lc(), true));
        assert_eq!(dmr.recent[0].slot, 1);
        assert_eq!(dmr.slots[1].calls, 0);
    }
}
