//! NXDN (Icom IDAS, Kenwood NEXEDGE) at 2400 symbols/s: finds frames in FM-demodulated audio and
//! reads the link, control and message fields. No voice. Used by `examples/nxdn_probe.rs`, not by
//! the engine.
//!
//! Layouts follow OP25 (`gr-op25_repeater/lib/nxdn.cc`, `rx_sync.cc`) and DSD-FME, GPL like this
//! crate. A frame is 192 symbols in 80 ms: 10 sync, then 182 scrambled: 8 LICH (link information),
//! 30 SACCH (slow control), 144 for voice or FACCH (fast control); or 150 CAC (control channel) or
//! 174 FACCH2 in place of SACCH and the rest.

pub const AUDIO_RATE: usize = 48_000;
pub const SYMBOL_RATE: usize = 2_400;
const SPS: usize = AUDIO_RATE / SYMBOL_RATE;
pub const FRAME_SYMBOLS: usize = 192;
const BODY_DIBITS: usize = FRAME_SYMBOLS - SYNC.len();
/// Frame sync 0xCDF59 as symbols.
const SYNC: [f32; 10] = [-3.0, 1.0, -3.0, 3.0, -3.0, -3.0, 3.0, 3.0, -1.0, 3.0];
/// Normalised correlation that starts a run of frames.
const FIND_CORR: f32 = 0.8;
/// Correlation that keeps a run going at the expected place.
const KEEP_CORR: f32 = 0.7;
/// Samples either side of the expected place to look for the next sync.
const SLIP: usize = 4;

/// Scrambler: dibits at these places have their sign inverted.
const SCRAMBLE: [u8; 92] = [
    2, 5, 6, 7, 10, 12, 14, 16, 17, 22, 23, 25, 26, 27, 28, 30, 33, 34, 36, 37, 38, 41, 45, 47, 52, 54, 56, 57, 59, 62,
    63, 64, 65, 66, 67, 69, 70, 73, 76, 79, 81, 82, 84, 85, 86, 87, 88, 89, 92, 95, 96, 98, 100, 103, 104, 107, 108,
    116, 117, 121, 122, 125, 127, 131, 132, 134, 137, 139, 140, 141, 142, 143, 144, 145, 147, 151, 153, 154, 158, 159,
    160, 162, 164, 165, 168, 170, 171, 174, 175, 176, 177, 181,
];

/// One frame as found: where it is, how well the sync matched and its body, still scrambled.
pub struct Raw {
    pub at: usize,
    pub corr: f32,
    pub dibits: [u8; BODY_DIBITS],
}

fn sync_at(y: &[f32], start: usize) -> Option<(f32, f32, f32)> {
    let end = start + (SYNC.len() - 1) * SPS;
    if end >= y.len() {
        return None;
    }
    let (mut dot, mut pow, mut sum) = (0.0, 0.0, 0.0);
    for (k, s) in SYNC.iter().enumerate() {
        let v = y[start + k * SPS];
        dot += s * v;
        pow += v * v;
        sum += v;
    }
    let dc = sum / SYNC.len() as f32;
    let refs: f32 = SYNC.iter().map(|s| s * s).sum();
    let var = pow - sum * dc;
    if var <= 0.0 {
        return None;
    }
    // The sync's symbols sum to zero, so `dot` does not depend on the DC.
    Some((dot / (var * refs).sqrt(), dot / refs, dc))
}

fn dibit(v: f32) -> u8 {
    match v {
        v if v >= 2.0 => 0b01,
        v if v >= 0.0 => 0b00,
        v if v >= -2.0 => 0b10,
        _ => 0b11,
    }
}

/// Finds every frame in 48 kHz FM audio. Either polarity.
pub fn find_frames(audio: &[f32]) -> Vec<Raw> {
    // A symbol-long average stands in for the receive filter.
    let mut y = vec![0.0f32; audio.len()];
    let mut run = 0.0f32;
    for i in 0..audio.len() {
        run += audio[i];
        if i >= SPS {
            run -= audio[i - SPS];
        }
        // y[i] is centred on sample i.
        if i >= SPS / 2 {
            y[i - SPS / 2] = run / SPS as f32;
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    let mut expect: Option<usize> = None;
    while i < y.len() {
        let found = match expect {
            Some(e) => (e.saturating_sub(SLIP)..=e + SLIP)
                .filter_map(|p| sync_at(&y, p).map(|c| (p, c)))
                .max_by(|a, b| a.1 .0.abs().total_cmp(&b.1 .0.abs()))
                .filter(|(_, c)| c.0.abs() >= KEEP_CORR),
            None => sync_at(&y, i).filter(|c| c.0.abs() >= FIND_CORR).and_then(|_| {
                // The strongest place within a symbol.
                let best = (i..i + SPS)
                    .filter_map(|p| sync_at(&y, p).map(|c| (p, c)))
                    .max_by(|a, b| a.1 .0.abs().total_cmp(&b.1 .0.abs()))?;
                // Ten symbols match by chance often enough: a run starts only with a second sync
                // one frame later.
                let next = best.0 + FRAME_SYMBOLS * SPS;
                (next.saturating_sub(SLIP)..=next + SLIP)
                    .filter_map(|p| sync_at(&y, p))
                    .any(|c| c.0.abs() >= FIND_CORR)
                    .then_some(best)
            }),
        };
        let Some((p, (corr, gain, dc))) = found else {
            if expect.take().is_none() {
                i += 1;
            }
            continue;
        };
        let last = p + (FRAME_SYMBOLS - 1) * SPS;
        if last >= y.len() {
            break;
        }
        let mut dibits = [0u8; BODY_DIBITS];
        for (k, d) in dibits.iter_mut().enumerate() {
            *d = dibit((y[p + (SYNC.len() + k) * SPS] - dc) / gain);
        }
        out.push(Raw { at: p, corr: corr.abs(), dibits });
        expect = Some(p + FRAME_SYMBOLS * SPS);
        i = p + FRAME_SYMBOLS * SPS;
    }
    out
}

fn descramble(d: &mut [u8]) {
    for &p in &SCRAMBLE {
        if let Some(v) = d.get_mut(p as usize) {
            *v ^= 0b10;
        }
    }
}

fn bits_of(d: &[u8]) -> Vec<u8> {
    d.iter().flat_map(|v| [v >> 1, v & 1]).collect()
}

/// Reverses the block interleaver: written by rows, read by columns.
fn deinterleave(bits: &[u8], rows: usize, cols: usize) -> Vec<u8> {
    let mut out = vec![0; rows * cols];
    for r in 0..rows {
        for c in 0..cols {
            out[r + c * rows] = bits[r * cols + c];
        }
    }
    out
}

/// An erased (punctured) bit for `viterbi`.
const ERASED: u8 = 2;

fn parity(v: u32) -> u8 {
    (v.count_ones() & 1) as u8
}

/// Rate-1/2 code, memory 4, ending with four zero bits. `coded` may hold `ERASED`.
/// Returns the data bits without the tail.
fn viterbi(coded: &[u8]) -> Vec<u8> {
    const STATES: usize = 16;
    let steps = coded.len() / 2;
    let mut cost = [u32::MAX / 2; STATES];
    cost[0] = 0;
    let mut from = vec![[0u8; STATES]; steps];
    for t in 0..steps {
        let mut next = [u32::MAX / 2; STATES];
        for (s, &c) in cost.iter().enumerate() {
            for bit in 0..2u32 {
                let reg = ((s as u32) << 1) | bit;
                let mut add = 0;
                for (out, mask) in [(coded[t * 2], 0x19), (coded[t * 2 + 1], 0x17)] {
                    if out != ERASED && out != parity(reg & mask) {
                        add += 1;
                    }
                }
                let n = (reg & 15) as usize;
                if c + add < next[n] {
                    next[n] = c + add;
                    from[t][n] = s as u8;
                }
            }
        }
        cost = next;
    }
    let mut s = 0usize;
    let mut bits = vec![0u8; steps];
    for t in (0..steps).rev() {
        bits[t] = (s & 1) as u8;
        s = from[t][s] as usize;
    }
    bits.truncate(steps - 4);
    bits
}

/// A shift-register CRC; zero when the bits include a good check. `taps` are the places that
/// take the feedback, the last place always does.
fn crc_register(bits: &[u8], len: usize, taps: &[usize]) -> bool {
    let mut s = vec![1u8; len];
    for &b in bits {
        let a = b ^ s[0];
        s.rotate_left(1);
        s[len - 1] = a;
        for &t in taps {
            s[t] ^= a;
        }
    }
    s.iter().all(|&v| v == 0)
}

fn crc6_ok(b: &[u8]) -> bool {
    crc_register(b, 6, &[0, 3, 4])
}
fn crc12_ok(b: &[u8]) -> bool {
    crc_register(b, 12, &[8, 9, 10])
}
fn crc15_ok(b: &[u8]) -> bool {
    crc_register(b, 15, &[0, 3, 4, 7, 8, 12])
}
fn crc16_ok(b: &[u8]) -> bool {
    let mut crc = 0xc3eeu32;
    for &v in b {
        crc = ((crc << 1) | v as u32) & 0x1ffff;
        if crc & 0x10000 != 0 {
            crc = (crc & 0xffff) ^ ((1 << 12) | (1 << 5) | 1);
        }
    }
    (crc ^ 0xffff) & 0xffff == 0
}

fn field(bits: &[u8], at: usize, len: usize) -> u32 {
    bits[at..at + len].iter().fold(0, |v, &b| (v << 1) | b as u32)
}

fn bytes(bits: &[u8]) -> Vec<u8> {
    bits.chunks_exact(8).map(|c| field(c, 0, 8) as u8).collect()
}

fn sacch(d: &[u8]) -> Option<Vec<u8>> {
    let p = deinterleave(&bits_of(d), 12, 5);
    let mut c = Vec::with_capacity(72);
    for g in p.chunks_exact(10) {
        c.extend_from_slice(&g[..5]);
        c.push(ERASED);
        c.extend_from_slice(&g[5..]);
        c.push(ERASED);
    }
    let b = viterbi(&c);
    crc6_ok(&b).then(|| b[..26].to_vec())
}

fn facch(d: &[u8]) -> Option<Vec<u8>> {
    let p = deinterleave(&bits_of(d), 16, 9);
    let mut c = Vec::with_capacity(192);
    for g in p.chunks_exact(3) {
        c.extend_from_slice(&[g[0], ERASED, g[1], g[2]]);
    }
    let b = viterbi(&c);
    crc12_ok(&b).then(|| bytes(&b[..80]))
}

fn twelve(d: &[u8], cols: usize) -> Vec<u8> {
    let p = deinterleave(&bits_of(d), 12, cols);
    let mut c = Vec::with_capacity(cols * 14);
    for g in p.chunks_exact(12) {
        c.extend_from_slice(&g[..3]);
        c.push(ERASED);
        c.extend_from_slice(&g[3..10]);
        c.push(ERASED);
        c.extend_from_slice(&g[10..]);
    }
    viterbi(&c)
}

fn cac(d: &[u8]) -> Option<Vec<u8>> {
    let b = twelve(d, 25);
    crc16_ok(&b).then(|| bytes(&b[..152]))
}

fn facch2(d: &[u8]) -> Option<Vec<u8>> {
    let b = twelve(d, 29);
    crc15_ok(&b).then(|| bytes(&b[..184]))
}

/// What one frame carries.
#[derive(Default, Debug, PartialEq)]
pub struct Frame {
    /// None when its parity failed; nothing else is read then.
    pub lich: Option<u8>,
    /// Voice blocks in this frame, each half of the 144.
    pub voice: u8,
    /// Slow control: 2 bits of segment number, 6 of radio access number, 18 of content.
    pub sacch: Option<Vec<u8>>,
    /// True when the frame should carry SACCH and its check failed.
    pub sacch_bad: bool,
    /// Radio access number, when a control channel block carried it.
    pub ran: Option<u8>,
    /// Whole messages with a good check, bytes.
    pub messages: Vec<Vec<u8>>,
    /// Control blocks that failed their check.
    pub bad: u32,
}

/// Descrambles and decodes the fields of one frame.
pub fn read_frame(raw: &Raw) -> Frame {
    let mut d = raw.dibits;
    descramble(&mut d);
    let full = d[..8].iter().fold(0u8, |v, x| (v << 1) | (x >> 1));
    let sum = (full >> 7) + (full >> 6 & 1) + (full >> 5 & 1) + (full >> 4 & 1);
    if sum & 1 != full & 1 {
        return Frame::default();
    }
    let lich = full >> 1;
    let mut f = Frame { lich: Some(lich), ..Frame::default() };
    let read = |v: Option<Vec<u8>>, f: &mut Frame| match v {
        Some(m) => f.messages.push(m),
        None => f.bad += 1,
    };
    // Which fields each link value carries (OP25 `rx_sync::nxdn_frame`).
    let (voice, fast, has_sacch) = match lich {
        0x32 | 0x33 | 0x52 | 0x53 | 0x73 => (2, 1, true),
        0x34 | 0x35 | 0x54 | 0x55 | 0x75 => (1, 2, true),
        0x36 | 0x37 | 0x56 | 0x57 | 0x77 => (3, 0, true),
        0x20 | 0x21 | 0x30 | 0x31 | 0x40 | 0x41 | 0x50 | 0x51 | 0x61 | 0x71 => (0, 3, true),
        0x38 | 0x39 => (0, 0, true),
        0x01 | 0x05 => {
            match cac(&d[8..158]) {
                // The first byte is segment bits and the radio access number.
                Some(m) => {
                    f.ran = Some(m[0] & 0x3f);
                    f.messages.push(m[1..].to_vec());
                }
                None => f.bad += 1,
            }
            return f;
        }
        0x28 | 0x29 | 0x2e | 0x2f | 0x48 | 0x49 | 0x4e | 0x4f | 0x69 | 0x6f => {
            read(facch2(&d[8..182]), &mut f);
            return f;
        }
        _ => return f,
    };
    f.voice = voice;
    if has_sacch {
        f.sacch = sacch(&d[8..38]);
        f.sacch_bad = f.sacch.is_none();
    }
    if fast & 1 != 0 {
        read(facch(&d[38..110]), &mut f);
    }
    if fast & 2 != 0 && !(fast & 1 != 0 && d[38..110] == d[110..182]) {
        read(facch(&d[110..182]), &mut f);
    }
    f
}

/// Joins the four SACCH segments of a superframe. Segment number counts down from 3.
#[derive(Default)]
pub struct Sacch {
    want: Option<u32>,
    ran: u32,
    bits: Vec<u8>,
}

impl Sacch {
    /// Returns (radio access number, 9 bytes) when a superframe completes.
    pub fn push(&mut self, seg: &[u8]) -> Option<(u32, Vec<u8>)> {
        let n = field(seg, 0, 2);
        let ran = field(seg, 2, 6);
        if n == 3 {
            self.bits.clear();
            self.ran = ran;
        } else if self.want != Some(n) || self.ran != ran {
            self.want = None;
            return None;
        }
        self.bits.extend_from_slice(&seg[8..26]);
        if n == 0 {
            self.want = None;
            return Some((ran, bytes(&self.bits)));
        }
        self.want = Some(n - 1);
        None
    }
}

/// Link value in words: channel type, then what the frame is for (DSD-FME `nxdn_frame.c`).
pub fn lich_name(lich: u8) -> String {
    let rf = ["control", "traffic", "data", "composite"][(lich >> 5 & 3) as usize];
    let dir = if lich & 1 == 1 { "outbound" } else { "inbound" };
    format!("{rf} channel, {dir}, function {}, option {}", lich >> 3 & 3, lich >> 1 & 3)
}

/// Message type (low 6 bits of the first byte), names from DSD-FME `nxdn_message_type`.
pub fn message_name(first: u8) -> &'static str {
    match first & 0x3f {
        0x00 => "call response",
        0x01 => "voice call",
        0x02 => "voice call receive request",
        0x03 => "voice call, encryption vector",
        0x04 => "voice channel assignment",
        0x05 => "voice channel assignment, repeat",
        0x06 => "call connection response",
        0x07 => "transmission release, extended",
        0x08 => "transmission release",
        0x09 => "data call header",
        0x0a => "data call receive request",
        0x0b => "data call data",
        0x0c => "data call acknowledge",
        0x0d => "data channel assignment, repeat",
        0x0e => "data channel assignment",
        0x0f => "header delay",
        0x10 => "idle",
        0x11 => "disconnect",
        0x17 => "destination ID information",
        0x18 => "site information",
        0x19 => "service information",
        0x1a => "control channel information",
        0x1b => "adjacent site information",
        0x1c => "failure status information",
        0x20 => "registration response",
        0x22 => "registration clear response",
        0x23 => "registration command",
        0x24 => "group registration response",
        0x28 => "authentication request",
        0x29 => "authentication response",
        0x2a => "authentication request 2",
        0x2b => "authentication response 2",
        0x30 => "status inquiry request",
        0x31 => "status inquiry response",
        0x32 => "status request",
        0x33 => "status response",
        0x34 => "remote control request",
        0x35 => "remote control response",
        0x36 => "remote control request, extended",
        0x37 => "remote control response, extended",
        0x38 => "short data request header",
        0x39 => "short data request data",
        0x3a => "short data, encryption vector",
        0x3b => "short data response",
        0x3f => "proprietary",
        _ => "?",
    }
}

/// A site's location: category, system and site (DSD-FME `nxdn_location_id_handler`).
pub fn location(id: u32) -> String {
    let body = id & 0x3f_ffff;
    let (cat, sys, site) = match id >> 22 {
        0 => ("global", body >> 12, body & 0x3ff),
        1 => ("local", body >> 5, body & 0x1f),
        2 => ("regional", body >> 8, body & 0xff),
        _ => ("reserved", 0, 0),
    };
    format!("{cat} system {sys} site {site}")
}

/// The services a site offers (Service Information, bits per DSD-FME `nxdn_srv_info_handler`).
pub fn services(bits: u16) -> String {
    const NAMES: [&str; 12] = [
        "multi-site",
        "multi-system",
        "location registration",
        "group registration",
        "authentication",
        "composite control channel",
        "voice",
        "data",
        "short data",
        "status and remote control",
        "telephone network",
        "IP network",
    ];
    let on: Vec<&str> = (0..12).filter(|i| bits & (0x8000 >> i) != 0).map(|i| NAMES[i]).collect();
    on.join(", ")
}

fn be24(b: &[u8]) -> u32 {
    (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32
}

/// The fields of the messages this probe reads, in words. Layouts from DSD-FME
/// `NXDN_decode_srv_info`, `_site_info`, `_adj_site` (channel version). None for anything else.
pub fn describe(m: &[u8]) -> Option<String> {
    match m.first()? & 0x3f {
        0x19 if m.len() >= 9 => {
            Some(format!("{}; offers {}", location(be24(&m[1..])), services(u16::from_be_bytes([m[4], m[5]]))))
        }
        0x18 if m.len() >= 17 => {
            let adj = m[15] >> 4;
            let c1 = (m[15] as u16 & 15) << 6 | (m[16] as u16) >> 2;
            Some(format!("{}; {adj} adjacent sites; control channel number {c1}", location(be24(&m[1..]))))
        }
        0x1b => {
            let sites: Vec<String> = m[1..]
                .chunks_exact(5)
                .take(3)
                .filter(|a| a[3] != 0 || a[4] != 0)
                .map(|a| {
                    let v = u16::from_be_bytes([a[3], a[4]]);
                    format!("{} on channel number {}", location(be24(a)), v & 0x3ff)
                })
                .collect();
            Some(sites.join("; "))
        }
        _ => None,
    }
}

/// Voice call fields: (source unit, destination, cipher type, key).
pub fn voice_call(m: &[u8]) -> Option<(u16, u16, u8, u8)> {
    if m.len() < 8 || m[0] & 0x3f != 0x01 {
        return None;
    }
    Some((
        u16::from_be_bytes([m[3], m[4]]),
        u16::from_be_bytes([m[5], m[6]]),
        m[7] >> 6,
        m[7] & 0x3f,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(data: &[u8]) -> Vec<u8> {
        let mut reg = 0u32;
        let mut out = Vec::new();
        for &b in data.iter().chain([0u8; 4].iter()) {
            reg = ((reg << 1) | b as u32) & 0x1f;
            out.push(parity(reg & 0x19));
            out.push(parity(reg & 0x17));
        }
        out
    }

    fn interleave(bits: &[u8], rows: usize, cols: usize) -> Vec<u8> {
        let mut out = vec![0; rows * cols];
        for r in 0..rows {
            for c in 0..cols {
                out[r * cols + c] = bits[r + c * rows];
            }
        }
        out
    }

    fn dibits(bits: &[u8]) -> Vec<u8> {
        bits.chunks_exact(2).map(|c| c[0] << 1 | c[1]).collect()
    }

    /// The check bits that make `crc_register` come out zero: what the register holds after the data.
    fn crc_tail(data: &[u8], len: usize, taps: &[usize]) -> Vec<u8> {
        let mut s = vec![1u8; len];
        for &b in data {
            let a = b ^ s[0];
            s.rotate_left(1);
            s[len - 1] = a;
            for &t in taps {
                s[t] ^= a;
            }
        }
        s
    }

    fn sacch_dibits(data: &[u8]) -> Vec<u8> {
        let mut with = data.to_vec();
        with.extend(crc_tail(data, 6, &[0, 3, 4]));
        let coded = encode(&with);
        let mut kept = Vec::new();
        for (i, b) in coded.iter().enumerate() {
            if i % 6 != 5 {
                kept.push(*b);
            }
        }
        dibits(&interleave(&kept, 12, 5))
    }

    fn facch_dibits(data: &[u8]) -> Vec<u8> {
        let mut with = data.to_vec();
        with.extend(crc_tail(data, 12, &[8, 9, 10]));
        let coded = encode(&with);
        let mut kept = Vec::new();
        for (i, b) in coded.iter().enumerate() {
            if i % 4 != 1 {
                kept.push(*b);
            }
        }
        dibits(&interleave(&kept, 16, 9))
    }

    fn lich_dibits(lich: u8) -> Vec<u8> {
        let p = (lich >> 6 & 1) + (lich >> 5 & 1) + (lich >> 4 & 1) + (lich >> 3 & 1);
        let full = lich << 1 | (p & 1);
        // The low bit of each LICH dibit is always one (DSD-FME's "off bits").
        (0..8).map(|i| (full >> (7 - i) & 1) << 1 | 1).collect()
    }

    fn body(lich: u8, sacch: &[u8], fast: &[u8]) -> Vec<u8> {
        let mut d = lich_dibits(lich);
        d.extend_from_slice(sacch);
        d.extend_from_slice(fast);
        d.extend_from_slice(fast);
        descramble(&mut d);
        d
    }

    fn level(d: u8) -> f32 {
        match d {
            0b01 => 3.0,
            0b00 => 1.0,
            0b10 => -1.0,
            _ => -3.0,
        }
    }

    /// FM audio for frames, symbols held flat, DC and scale added.
    fn modulate(frames: &[Vec<u8>], scale: f32, dc: f32) -> Vec<f32> {
        let mut out = vec![dc; 777];
        for b in frames {
            for s in SYNC.iter().copied().chain(b.iter().map(|&d| level(d))) {
                out.extend(std::iter::repeat(s * scale + dc).take(SPS));
            }
        }
        out.extend(std::iter::repeat(dc).take(SPS * 40));
        out
    }

    fn call_message() -> Vec<u8> {
        let mut m = vec![0u8; 10];
        m[0] = 0x01;
        m[3..5].copy_from_slice(&1234u16.to_be_bytes());
        m[5..7].copy_from_slice(&77u16.to_be_bytes());
        m[7] = 0b01_000101;
        m
    }

    fn to_bits(m: &[u8]) -> Vec<u8> {
        m.iter().flat_map(|b| (0..8).rev().map(move |i| b >> i & 1)).collect()
    }

    fn segment(n: u8, ran: u8, content: &[u8]) -> Vec<u8> {
        let mut b: Vec<u8> = (0..2).rev().map(|i| n >> i & 1).collect();
        b.extend((0..6).rev().map(|i| ran >> i & 1));
        b.extend_from_slice(content);
        b
    }

    #[test]
    fn frames_decode_from_audio_with_dc_and_either_polarity() {
        let msg = to_bits(&call_message());
        let frames: Vec<Vec<u8>> = (0..4u8)
            .map(|i| body(0x51, &sacch_dibits(&segment(3 - i, 21, &msg[i as usize * 18..][..18])), &facch_dibits(&msg)))
            .collect();
        for scale in [0.1f32, -0.1] {
            let found = find_frames(&modulate(&frames, scale, 0.03));
            assert_eq!(found.len(), 4, "scale {scale}");
            let mut join = Sacch::default();
            let mut whole = None;
            for raw in &found {
                let f = read_frame(raw);
                assert_eq!(f.lich, Some(0x51));
                assert_eq!(f.bad, 0);
                assert_eq!(f.messages.len(), 1);
                assert_eq!(voice_call(&f.messages[0]), Some((1234, 77, 1, 5)));
                whole = join.push(f.sacch.as_ref().unwrap()).or(whole);
            }
            let (ran, m) = whole.unwrap();
            assert_eq!(ran, 21);
            assert_eq!(&m[..], &bytes(&msg[..72])[..]);
        }
    }

    #[test]
    fn viterbi_repairs_errors_and_crc_catches_what_it_cannot() {
        let msg = to_bits(&call_message());
        let mut d = facch_dibits(&msg);
        assert_eq!(facch(&d).unwrap(), call_message());
        d[5] ^= 1;
        d[40] ^= 2;
        assert_eq!(facch(&d).unwrap(), call_message(), "two bit errors repaired");
        for v in d.iter_mut().take(40) {
            *v ^= 3;
        }
        assert_eq!(facch(&d), None);
    }

    #[test]
    fn bad_link_parity_reads_nothing() {
        let mut b = body(0x51, &[0; 30], &[0; 72]);
        b[0] ^= 2;
        let raw = Raw { at: 0, corr: 1.0, dibits: b.try_into().unwrap() };
        assert_eq!(read_frame(&raw), Frame::default());
    }

    fn from_hex(h: &str) -> Vec<u8> {
        (0..h.len() / 2).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap()).collect()
    }

    // Blocks heard on 462.7455 MHz on 2026-11-03, each past its CRC-16; first byte (radio access
    // number 42) already removed.
    #[test]
    fn reads_control_channel_messages_heard_on_air() {
        let srv = from_hex("1980602abfc0000000000000000000000000");
        assert_eq!(message_name(srv[0]), "service information");
        let text = describe(&srv).unwrap();
        assert!(text.starts_with("regional system 96 site 42; offers multi-site,"), "{text}");
        assert!(text.contains("voice, data"));
        let site = from_hex("1880602a4914bfc0000000000000014e1c00");
        assert_eq!(describe(&site).unwrap(), "regional system 96 site 42; 4 adjacent sites; control channel number 903");
        let adj = from_hex("1b80600105d1806006097580600b0cf50000");
        assert_eq!(
            describe(&adj).unwrap(),
            "regional system 96 site 1 on channel number 465; regional system 96 site 6 on channel number 373; regional system 96 site 11 on channel number 245"
        );
    }

    #[test]
    fn noise_gives_no_messages() {
        let mut x = 12345u32;
        let noise: Vec<f32> = (0..48_000 * 5)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                (x >> 8) as f32 / 8388608.0 - 1.0
            })
            .collect();
        // Chance frames may appear; no message may pass its check.
        let found = find_frames(&noise);
        assert!(found.len() < 40, "{}", found.len());
        assert!(found.iter().map(read_frame).all(|f| f.messages.is_empty()));
    }

    #[test]
    fn crc16_accepts_a_good_block() {
        // Brute force: only one 16-bit tail satisfies the check.
        let data = vec![1u8, 0, 1, 1, 0, 0, 1, 0];
        let good = (0..65536u32).filter(|t| {
            let mut b = data.clone();
            b.extend((0..16).rev().map(|i| (t >> i & 1) as u8));
            crc16_ok(&b)
        });
        assert_eq!(good.count(), 1);
    }
}
