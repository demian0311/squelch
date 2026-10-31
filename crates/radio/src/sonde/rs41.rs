//! Vaisala RS41: 4800 baud GFSK, one 320- or 518-byte frame a second.
//!
//! Bytes go out LSB first and are XOR-scrambled with a 64-byte mask. Two interleaved
//! RS(255,231) codewords protect the frame; the payload is a chain of CRC-checked blocks.

use super::geo::{ecef_to_geodetic, ecef_velocity};
use super::rs::Rs;
use super::{FrameDecoder, Fix, Update};

pub const BAUD: f32 = 4800.0;
const HEADER: [u8; 8] = [0x10, 0xB6, 0xCA, 0x11, 0x22, 0x96, 0x12, 0xF8];
const MASK: [u8; 64] = [
    0x96, 0x83, 0x3E, 0x51, 0xB1, 0x49, 0x08, 0x98, 0x32, 0x05, 0x59, 0x0E, 0xF9, 0x44, 0xC6, 0x26, //
    0x21, 0x60, 0xC2, 0xEA, 0x79, 0x5D, 0x6D, 0xA1, 0x54, 0x69, 0x47, 0x0C, 0xDC, 0xE8, 0x5C, 0xF1, //
    0xF7, 0x76, 0x82, 0x7F, 0x07, 0x99, 0xA2, 0x2C, 0x93, 0x7C, 0x30, 0x63, 0xF5, 0x10, 0x2E, 0x61, //
    0xD0, 0xBC, 0xB4, 0xB6, 0x06, 0xAA, 0xF4, 0x23, 0x78, 0x6E, 0x3B, 0xAE, 0xBF, 0x7B, 0x4C, 0xC1,
];
const SHORT: usize = 320;
const LONG: usize = 518;
/// Byte holding 0x0F (short frame) or 0xF0 (long frame), after descrambling.
const FRAME_TYPE: usize = 0x38;
const FIRST_BLOCK: usize = 0x39;
/// Header bits that may be wrong and still count as a sync.
const SYNC_ERRORS: u32 = 4;
/// GPS time runs ahead of UTC by this many seconds (since 2017).
pub const GPS_LEAP_SECONDS: i64 = 18;

pub struct Rs41 {
    sync: u64,
    pattern: u64,
    invert: bool,
    /// Bits still wanted for the frame being collected; 0 when hunting for a header.
    frame: Vec<u8>,
    byte: u8,
    nbits: u32,
    collecting: bool,
    rs: Rs,
}

impl Default for Rs41 {
    fn default() -> Self {
        Self::new()
    }
}

impl Rs41 {
    pub fn new() -> Self {
        let pattern = HEADER.iter().flat_map(|&b| (0..8).map(move |i| (b >> i) & 1)).fold(0u64, |p, bit| (p << 1) | bit as u64);
        Rs41 { sync: 0, pattern, invert: false, frame: Vec::with_capacity(LONG), byte: 0, nbits: 0, collecting: false, rs: Rs::new(24) }
    }

    fn bit(&mut self, b: bool, out: &mut Vec<Update>) {
        if !self.collecting {
            self.sync = (self.sync << 1) | b as u64;
            if (self.sync ^ self.pattern).count_ones() <= SYNC_ERRORS {
                self.start(false);
            } else if (self.sync ^ !self.pattern).count_ones() <= SYNC_ERRORS {
                self.start(true);
            }
            return;
        }
        let b = b ^ self.invert;
        self.byte |= (b as u8) << self.nbits;
        self.nbits += 1;
        if self.nbits < 8 {
            return;
        }
        self.frame.push(self.byte);
        self.byte = 0;
        self.nbits = 0;
        let n = self.frame.len();
        let want = if n > FRAME_TYPE && self.frame[FRAME_TYPE] ^ MASK[FRAME_TYPE % 64] == 0xF0 { LONG } else { SHORT };
        if n >= want {
            self.collecting = false;
            let mut f = std::mem::take(&mut self.frame);
            if let Some(u) = self.decode(&mut f) {
                out.push(u);
            }
            f.clear();
            self.frame = f;
        }
    }

    fn start(&mut self, invert: bool) {
        self.invert = invert;
        self.collecting = true;
        self.frame.clear();
        self.frame.extend_from_slice(&HEADER);
        self.byte = 0;
        self.nbits = 0;
        self.sync = 0;
    }

    /// Descramble, correct and parse one frame (header included, still scrambled).
    pub fn decode(&self, f: &mut [u8]) -> Option<Update> {
        for (i, b) in f.iter_mut().enumerate() {
            *b ^= MASK[i % 64];
        }
        self.correct(f);
        parse(f)
    }

    fn correct(&self, f: &mut [u8]) {
        let msg = (f.len() - 56) / 2;
        for k in 0..2 {
            let mut cw = [0u8; 255];
            for i in 0..24 {
                cw[i] = f[8 + 24 * k + i];
            }
            for j in 0..msg {
                cw[24 + j] = f[56 + 2 * j + k];
            }
            if self.rs.decode(&mut cw, 24 + msg).is_some() {
                for i in 0..24 {
                    f[8 + 24 * k + i] = cw[i];
                }
                for j in 0..msg {
                    f[56 + 2 * j + k] = cw[24 + j];
                }
            }
        }
    }
}

impl FrameDecoder for Rs41 {
    fn kind(&self) -> &'static str {
        "RS41"
    }
    fn baud(&self) -> f32 {
        BAUD
    }
    fn push(&mut self, soft: &[f32], out: &mut Vec<Update>) {
        for &s in soft {
            self.bit(s > 0.0, out);
        }
    }
}

/// CRC-16/CCITT-FALSE.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn le16(d: &[u8]) -> u16 {
    u16::from_le_bytes([d[0], d[1]])
}
fn le32(d: &[u8]) -> u32 {
    u32::from_le_bytes([d[0], d[1], d[2], d[3]])
}

/// Read the CRC-valid blocks of a descrambled, corrected frame.
fn parse(f: &[u8]) -> Option<Update> {
    let mut u = Update::default();
    let mut any = false;
    let mut pos = FIRST_BLOCK;
    while pos + 2 <= f.len() {
        let (id, len) = (f[pos], f[pos + 1] as usize);
        if pos + len + 4 > f.len() {
            break;
        }
        let d = &f[pos + 2..pos + 2 + len];
        if crc16(d) == le16(&f[pos + 2 + len..]) {
            any = true;
            match id {
                0x79 if len >= 10 => {
                    u.frame = Some(le16(d) as u32);
                    let serial: String = d[2..10].iter().map(|&c| c as char).collect();
                    if serial.chars().all(|c| c.is_ascii_alphanumeric()) {
                        u.serial = Some(serial);
                    }
                }
                0x7C if len >= 6 => {
                    let tow_s = le32(&d[2..]) as i64 / 1000 - GPS_LEAP_SECONDS;
                    let t = tow_s.rem_euclid(86_400);
                    u.time = Some(format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60));
                }
                0x7B if len >= 18 => {
                    let p = |o: usize| le32(&d[o..]) as i32 as f64 / 100.0;
                    let v = |o: usize| le16(&d[o..]) as i16 as f64 / 100.0;
                    let (lat, lon, alt) = ecef_to_geodetic(p(0), p(4), p(8));
                    let (speed, heading, climb) = ecef_velocity(lat, lon, v(12), v(14), v(16));
                    if (-1000.0..80_000.0).contains(&alt) {
                        u.fix = Some(Fix { lat, lon, alt_m: alt, speed_ms: speed as f32, heading_deg: heading as f32, climb_ms: climb as f32 });
                    }
                }
                _ => {}
            }
        }
        pos += len + 4;
    }
    any.then_some(u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sonde::fsk::Fsk;
    use crate::sonde::testutil::fsk_signal;

    fn block(f: &mut Vec<u8>, id: u8, data: &[u8]) {
        f.push(id);
        f.push(data.len() as u8);
        f.extend_from_slice(data);
        f.extend_from_slice(&crc16(data).to_le_bytes());
    }

    /// A scrambled, RS-protected short frame, as it goes on air.
    fn frame() -> Vec<u8> {
        let mut f = vec![0u8; FIRST_BLOCK];
        f[..8].copy_from_slice(&[0x86, 0x35, 0xF4, 0x40, 0x93, 0xDF, 0x1A, 0x60]);
        f[FRAME_TYPE] = 0x0F;
        let mut status = vec![0u8; 0x28];
        status[..2].copy_from_slice(&4242u16.to_le_bytes());
        status[2..10].copy_from_slice(b"T1234567");
        block(&mut f, 0x79, &status);
        block(&mut f, 0x7A, &[0u8; 0x2A]);
        let mut info = vec![0u8; 0x1E];
        // 12:00:18 GPS = 12:00:00 UTC
        info[2..6].copy_from_slice(&((12 * 3600 + 18) * 1000u32).to_le_bytes());
        block(&mut f, 0x7C, &info);
        block(&mut f, 0x7D, &[0u8; 0x59]);
        // Denver at 10 km, rising 5 m/s
        let (x, y, z) = (-1_272_316.12f64, -4_751_565.96f64, 4_062_153.74f64);
        let up = (39.7392f64.to_radians(), (-104.9903f64).to_radians());
        let mut pos = vec![0u8; 0x15];
        for (i, v) in [x, y, z].iter().enumerate() {
            pos[i * 4..i * 4 + 4].copy_from_slice(&((v * 100.0).round() as i32).to_le_bytes());
        }
        let vu = [up.0.cos() * up.1.cos(), up.0.cos() * up.1.sin(), up.0.sin()];
        for (i, v) in vu.iter().enumerate() {
            pos[12 + i * 2..14 + i * 2].copy_from_slice(&((v * 500.0).round() as i16).to_le_bytes());
        }
        block(&mut f, 0x7B, &pos);
        block(&mut f, 0x76, &[0u8; 0x11]);
        assert_eq!(f.len(), SHORT);
        let rs = Rs::new(24);
        let msg = (SHORT - 56) / 2;
        for k in 0..2 {
            let mut cw = [0u8; 255];
            for j in 0..msg {
                cw[24 + j] = f[56 + 2 * j + k];
            }
            rs.encode(&mut cw);
            f[8 + 24 * k..32 + 24 * k].copy_from_slice(&cw[..24]);
        }
        for (i, b) in f.iter_mut().enumerate() {
            *b ^= MASK[i % 64];
        }
        f
    }

    fn check(u: &Update) {
        assert_eq!(u.serial.as_deref(), Some("T1234567"));
        assert_eq!(u.frame, Some(4242));
        assert_eq!(u.time.as_deref(), Some("12:00:00"));
        let fix = u.fix.as_ref().unwrap();
        assert!((fix.lat - 39.7392).abs() < 0.01 && (fix.lon + 104.9903).abs() < 0.01, "{fix:?}");
        assert!((fix.climb_ms - 5.0).abs() < 0.1, "{fix:?}");
    }

    #[test]
    fn scrambled_header_matches_the_spec() {
        assert_eq!(frame()[..8], HEADER);
    }

    #[test]
    fn decodes_a_frame_with_byte_errors() {
        let mut f = frame();
        for i in [60, 90, 150, 200, 260, 300] {
            f[i] ^= 0xA5;
        }
        check(&Rs41::new().decode(&mut f).unwrap());
    }

    #[test]
    fn decodes_from_the_air() {
        let bits: Vec<bool> = std::iter::repeat_n(false, 200)
            .chain([frame(), frame()].concat().iter().flat_map(|&b| (0..8).map(move |i| (b >> i) & 1 == 1)))
            .chain(std::iter::repeat_n(false, 50))
            .collect();
        for invert in [false, true] {
            let bits: Vec<bool> = bits.iter().map(|&b| b ^ invert).collect();
            let sig = fsk_signal(&bits, 48_000, BAUD * 0.999, 2_400.0, -800.0, 1_500.0);
            let mut soft = Vec::new();
            Fsk::new(48_000, BAUD).process(&sig, &mut soft);
            let mut out = Vec::new();
            Rs41::new().push(&soft, &mut out);
            assert!(!out.is_empty(), "no frame (invert {invert})");
            check(&out[0]);
        }
    }
}
