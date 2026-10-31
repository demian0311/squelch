//! GRAW DFM-09 / DFM-17: 2500 baud FSK, Manchester coded, a 280-bit frame every 224 ms.
//!
//! After a 16-bit sync, each frame holds one configuration block (7 nibbles) and two
//! data packets (13 nibbles), every nibble protected by an extended Hamming (8,4) code
//! and the bits of each block interleaved. Packets 0–8 cycle about once a second.

use super::{FrameDecoder, Fix, Update};

pub const BAUD: f32 = 2500.0;
/// The sync word as 32 line symbols.
const SYNC: &str = "10011010100110010101101001010101";
const SYNC_ERRORS: u32 = 2;
/// Line symbols after the sync: 264 data bits, two symbols each.
const BODY_SYMBOLS: usize = 528;
const CONF: (usize, usize) = (0, 7);
const DAT1: (usize, usize) = (56, 13);
const DAT2: (usize, usize) = (160, 13);
/// Serial numbers from this one up are DFM-17s, whatever the type nibble says.
const DFM17_FROM: u32 = 23_000_000;

/// Syndrome (first parity bit most significant) produced by an error in each codeword bit.
const SINGLE_ERROR: [u8; 8] = [0x7, 0xB, 0xD, 0xE, 0x8, 0x4, 0x2, 0x1];

fn parity(d: [bool; 4]) -> [bool; 4] {
    [d[1] ^ d[2] ^ d[3], d[0] ^ d[2] ^ d[3], d[0] ^ d[1] ^ d[3], d[0] ^ d[1] ^ d[2]]
}

/// Decode one 8-bit codeword (data first, MSB first) to its nibble; `None` on a double error.
pub fn hamming(mut cw: [bool; 8]) -> Option<u8> {
    let p = parity([cw[0], cw[1], cw[2], cw[3]]);
    let s = (0..4).fold(0u8, |acc, i| (acc << 1) | (p[i] ^ cw[4 + i]) as u8);
    if s != 0 {
        let k = SINGLE_ERROR.iter().position(|&e| e == s)?;
        cw[k] = !cw[k];
    }
    Some((0..4).fold(0u8, |acc, i| (acc << 1) | cw[i] as u8))
}

/// Undo the interleave of an `l`-codeword block and decode its nibbles.
fn block(bits: &[bool], (start, l): (usize, usize)) -> Option<Vec<u8>> {
    (0..l)
        .map(|i| {
            let mut cw = [false; 8];
            for (j, c) in cw.iter_mut().enumerate() {
                *c = bits[start + l * j + i];
            }
            hamming(cw)
        })
        .collect()
}

/// `len` bits starting at bit `at` of a nibble string, most significant first.
fn field(n: &[u8], at: usize, len: usize) -> u64 {
    (at..at + len).fold(0u64, |acc, b| (acc << 1) | ((n[b / 4] >> (3 - b % 4)) & 1) as u64)
}

#[derive(Default)]
struct Cycle {
    frame: Option<u32>,
    ms: Option<u64>,
    lat: Option<f64>,
    lon: Option<f64>,
    alt: Option<f64>,
    speed: f32,
    heading: f32,
    climb: f32,
}

pub struct Dfm {
    sync: u32,
    pattern: u32,
    invert: bool,
    body: Vec<f32>,
    collecting: bool,
    cycle: Cycle,
    max_ch: u8,
    /// Type byte of the all-zero configuration channel.
    nul: Option<u8>,
    dfm06: Option<(String, bool)>,
    hi: Option<(u16, bool)>,
    lo: Option<(u16, bool)>,
    serial: Option<(String, String)>,
    serial_sent: bool,
}

impl Default for Dfm {
    fn default() -> Self {
        Self::new()
    }
}

impl Dfm {
    pub fn new() -> Self {
        let pattern = SYNC.bytes().fold(0u32, |p, c| (p << 1) | (c == b'1') as u32);
        Dfm {
            sync: 0,
            pattern,
            invert: false,
            body: Vec::with_capacity(BODY_SYMBOLS),
            collecting: false,
            cycle: Cycle::default(),
            max_ch: 0,
            nul: None,
            dfm06: None,
            hi: None,
            lo: None,
            serial: None,
            serial_sent: false,
        }
    }

    fn symbol(&mut self, s: f32, out: &mut Vec<Update>) {
        if self.collecting {
            self.body.push(s);
            if self.body.len() == BODY_SYMBOLS {
                self.collecting = false;
                self.frame(out);
            }
            return;
        }
        self.sync = (self.sync << 1) | (s > 0.0) as u32;
        let (normal, inverted) = ((self.sync ^ self.pattern).count_ones(), (!self.sync ^ self.pattern).count_ones());
        if normal <= SYNC_ERRORS || inverted <= SYNC_ERRORS {
            self.invert = inverted < normal;
            self.collecting = true;
            self.body.clear();
            self.sync = 0;
        }
    }

    fn frame(&mut self, out: &mut Vec<Update>) {
        // symbol pair "01" is a one: compare the halves rather than slicing each
        let bits: Vec<bool> = self.body.chunks(2).map(|p| (p[1] > p[0]) ^ self.invert).collect();
        if let Some(c) = block(&bits, CONF) {
            self.config(&c);
        }
        for d in [DAT1, DAT2] {
            if let Some(n) = block(&bits, d) {
                self.packet(&n, out);
            }
        }
        if let (Some((serial, model)), false) = (&self.serial, self.serial_sent) {
            self.serial_sent = true;
            out.push(Update { serial: Some(serial.clone()), model: Some(model.clone()), ..Default::default() });
        }
    }

    /// Configuration channels cycle. The channel after the all-zero one carries the serial:
    /// six BCD digits on a DFM-06, otherwise two 16-bit halves on a DFM-09 or DFM-17.
    fn config(&mut self, c: &[u8]) {
        let ch = c[0];
        if c[2..].iter().all(|&n| n == 0) {
            self.nul = Some(ch << 4 | c[1]);
            return;
        }
        self.max_ch = self.max_ch.max(ch);
        let Some(nul) = self.nul else { return };
        if ch != (nul >> 4) + 1 && ch != self.max_ch {
            return;
        }
        let found = if nul & 0x58 == 0x58 {
            let s: String = c[1..7].iter().map(|n| format!("{n:X}")).collect();
            self.dfm06 = Some((s.clone(), self.dfm06.as_ref().is_some_and(|(v, _)| *v == s)));
            self.dfm06.clone().filter(|(_, twice)| *twice).map(|(s, _)| (s, "DFM-06"))
        } else if c[6] <= 1 {
            let half = field(c, 8, 16) as u16;
            let slot = if c[6] == 0 { &mut self.hi } else { &mut self.lo };
            // a half counts once it arrives twice with the same value
            *slot = Some((half, slot.is_some_and(|(v, _)| v == half)));
            match (self.hi, self.lo) {
                (Some((h, true)), Some((l, true))) => {
                    let sn = (h as u32) << 16 | l as u32;
                    Some((sn.to_string(), if ch == 0xA && sn < DFM17_FROM { "DFM-09" } else { "DFM-17" }))
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some((s, model)) = found {
            if self.serial.as_ref().is_none_or(|(old, _)| *old != s) {
                self.serial = Some((s, model.to_string()));
                self.serial_sent = false;
            }
        }
    }

    fn packet(&mut self, n: &[u8], out: &mut Vec<Update>) {
        let c = &mut self.cycle;
        let i32f = |at| field(n, at, 32) as u32 as i32 as f64;
        let i16f = |at| field(n, at, 16) as u16 as i16 as f32;
        match n[12] {
            0 => c.frame = Some(field(n, 24, 8) as u32),
            1 => c.ms = Some(field(n, 32, 16)),
            2 => {
                c.lat = Some(i32f(0) / 1e7);
                c.speed = i16f(32) / 100.0;
            }
            3 => {
                c.lon = Some(i32f(0) / 1e7);
                c.heading = field(n, 32, 16) as f32 / 100.0;
            }
            4 => {
                c.alt = Some(i32f(0) / 100.0);
                c.climb = i16f(32) / 100.0;
            }
            8 => {
                let (hour, minute) = (field(n, 21, 5), field(n, 26, 6));
                let c = std::mem::take(&mut self.cycle);
                let time = c.ms.filter(|_| hour < 24 && minute < 60).map(|ms| format!("{hour:02}:{minute:02}:{:02}", ms / 1000 % 60));
                let fix = match (c.lat, c.lon, c.alt) {
                    (Some(lat), Some(lon), Some(alt_m)) if lat.abs() <= 90.0 && lon.abs() <= 180.0 && (-1000.0..80_000.0).contains(&alt_m) => {
                        Some(Fix { lat, lon, alt_m, speed_ms: c.speed, heading_deg: c.heading, climb_ms: c.climb })
                    }
                    _ => None,
                };
                if fix.is_some() || time.is_some() {
                    let (serial, model) = self.serial.clone().unzip();
                    out.push(Update { serial, model, frame: c.frame, time, fix });
                }
            }
            _ => {}
        }
    }
}

impl FrameDecoder for Dfm {
    fn kind(&self) -> &'static str {
        "DFM"
    }
    fn baud(&self) -> f32 {
        BAUD
    }
    fn push(&mut self, soft: &[f32], out: &mut Vec<Update>) {
        for &s in soft {
            self.symbol(s, out);
        }
    }
}

// ── transmitter, for the simulator and tests ──

fn nibbles(fields: &[(usize, usize, u64)], len: usize) -> Vec<u8> {
    let mut n = vec![0u8; len];
    for &(at, w, v) in fields {
        for b in 0..w {
            let bit = (v >> (w - 1 - b)) & 1;
            n[(at + b) / 4] |= (bit as u8) << (3 - (at + b) % 4);
        }
    }
    n
}

fn encode(n: &[u8]) -> Vec<bool> {
    let l = n.len();
    let cws: Vec<[bool; 8]> = n
        .iter()
        .map(|&x| {
            let d = [x & 8 != 0, x & 4 != 0, x & 2 != 0, x & 1 != 0];
            let p = parity(d);
            [d[0], d[1], d[2], d[3], p[0], p[1], p[2], p[3]]
        })
        .collect();
    let mut bits = vec![false; 8 * l];
    for (i, cw) in cws.iter().enumerate() {
        for j in 0..8 {
            bits[l * j + i] = cw[j];
        }
    }
    bits
}

/// Line symbols for three cycles of frames from a DFM-17 hanging at one position, for the simulator.
pub fn simulate(sn: u32, lat: f64, lon: f64, alt_m: f64) -> Vec<bool> {
    let pk = |id: u64, f: &[(usize, usize, u64)]| {
        let mut all = f.to_vec();
        all.push((48, 4, id));
        nibbles(&all, 13)
    };
    let lat = (lat * 1e7) as i32 as u32 as u64;
    let lon = (lon * 1e7) as i32 as u32 as u64;
    let alt = (alt_m * 100.0) as i32 as u32 as u64;
    let packets = [
        pk(0, &[(16, 8, 2), (24, 8, 77)]),
        pk(1, &[(32, 16, 34_500)]),
        pk(2, &[(0, 32, lat), (32, 16, 1234)]),
        pk(3, &[(0, 32, lon), (32, 16, 9000)]),
        pk(4, &[(0, 32, alt), (32, 16, (-250i16) as u16 as u64)]),
        pk(5, &[]),
        pk(6, &[]),
        pk(7, &[]),
        pk(8, &[(0, 12, 2026), (12, 4, 10), (16, 5, 30), (21, 5, 19), (26, 6, 45)]),
    ];
    let confs = [
        nibbles(&[(0, 4, 0xA), (4, 4, 0xC)], 7),
        nibbles(&[(0, 4, 0xB), (4, 4, 0xC), (8, 16, (sn >> 16) as u64), (24, 4, 0)], 7),
        nibbles(&[(0, 4, 0xB), (4, 4, 0xC), (8, 16, (sn & 0xFFFF) as u64), (24, 4, 1)], 7),
    ];
    let mut sym = vec![false; 100];
    for round in 0..3 {
        for f in 0..5 {
            sym.extend(SYNC.bytes().map(|c| c == b'1'));
            let mut bits = encode(&confs[(round * 5 + f) % 3]);
            bits.extend(encode(&packets[(2 * f) % 9]));
            bits.extend(encode(&packets[(2 * f + 1) % 9]));
            sym.extend(bits.iter().flat_map(|&b| [!b, b]));
        }
    }
    sym
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::sonde::fsk::Fsk;
    use crate::sonde::testutil::fsk_signal;

    pub fn air() -> Vec<bool> {
        simulate(23_038_743, 39.7392, -104.9903, 10_000.0)
    }

    #[test]
    fn hamming_fixes_one_bit_and_flags_two() {
        let good = [true, false, true, true, false, true, false, false];
        assert_eq!(parity([true, false, true, true]), [false, true, false, false]);
        let mut one = good;
        one[2] = !one[2];
        assert_eq!(hamming(one), Some(0b1011));
        let mut two = good;
        two[0] = !two[0];
        two[5] = !two[5];
        assert_eq!(hamming(two), None);
    }

    #[test]
    fn decodes_position_and_serial_from_the_air() {
        for invert in [false, true] {
            let sym: Vec<bool> = air().iter().map(|&s| s ^ invert).collect();
            let sig = fsk_signal(&sym, 48_000, BAUD * 1.001, 3_000.0, 1_200.0, 2_000.0);
            let mut soft = Vec::new();
            Fsk::new(48_000, BAUD).process(&sig, &mut soft);
            let mut out = Vec::new();
            Dfm::new().push(&soft, &mut out);
            let fix = out.iter().find_map(|u| u.fix.clone()).expect("no fix");
            assert!((fix.lat - 39.7392).abs() < 1e-6 && (fix.lon + 104.9903).abs() < 1e-6, "{fix:?}");
            assert!((fix.alt_m - 10_000.0).abs() < 0.01 && (fix.climb_ms + 2.5).abs() < 0.01, "{fix:?}");
            assert!(out.iter().any(|u| u.time.as_deref() == Some("19:45:34")));
            let u = out.iter().find(|u| u.serial.is_some()).expect("no serial");
            assert_eq!((u.serial.as_deref(), u.model.as_deref()), (Some("23038743"), Some("DFM-17")));
        }
    }
}
