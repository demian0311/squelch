//! ACARS block framing (ARINC 618): sync, odd parity, CRC, and the fields of a block.
//!
//! On air: pre-key (all ones), bit sync `+*`, SYN SYN, SOH, then mode, address (7), acknowledge, label (2), block id,
//! and either STX, text and ETX/ETB or ETX alone; then two CRC bytes and DEL. Characters are 7 bits with odd parity in
//! bit 7, least significant bit first. The CRC covers everything after SOH through ETX/ETB, parity bits included.

use super::Message;

pub const SYN: u8 = 0x16;
pub const SOH: u8 = 0x01;
pub const STX: u8 = 0x02;
/// ETX and ETB with their parity bits.
pub const ETX: u8 = 0x83;
pub const ETB: u8 = 0x97;
pub const DEL: u8 = 0x7f;
pub const NAK: u8 = 0x15;
/// Mode through block id, then STX or ETX.
const HEADER: usize = 13;
/// ARINC 618 caps a block's text at 220 characters; a block longer than this lost its end.
const MAX_BLOCK: usize = HEADER + 220 + 1;
/// Pre-key bytes the simulator sends; real transmitters send at least this much.
const PRE_KEY: usize = 16;

/// `c` with bit 7 set when that makes the count of ones odd.
pub fn with_parity(c: u8) -> u8 {
    let c = c & 0x7f;
    if c.count_ones() % 2 == 0 {
        c | 0x80
    } else {
        c
    }
}

fn parity_ok(c: u8) -> bool {
    c.count_ones() % 2 == 1
}

/// CRC-16/KERMIT (the CCITT polynomial, reflected, starting at zero), which ACARS sends low byte first.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0x8408 } else { crc >> 1 };
        }
    }
    crc
}

/// A block's bytes after SOH, through ETX/ETB: what the CRC covers. `text` None sends ETX right after the block id.
pub fn block(mode: u8, address: &str, ack: u8, label: &str, block_id: u8, text: Option<&str>, more: bool) -> Vec<u8> {
    let mut b = vec![mode];
    b.extend(format!("{address:.>7}").bytes().take(7));
    b.push(ack);
    b.extend(label.bytes().take(2));
    b.push(block_id);
    match text {
        Some(t) => {
            b.push(STX);
            b.extend(t.bytes());
            b.push(if more { ETB } else { ETX });
        }
        None => b.push(ETX),
    }
    b.iter().map(|&c| with_parity(c)).collect()
}

/// Everything a transmitter sends for `block`, from the pre-key to the closing DEL.
pub fn on_air(block: &[u8]) -> Vec<u8> {
    let mut a = vec![0xff; PRE_KEY];
    a.extend([with_parity(b'+'), with_parity(b'*'), SYN, SYN, SOH]);
    a.extend_from_slice(block);
    a.extend(crc16(block).to_le_bytes());
    a.push(DEL);
    a
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Hunt,
    Syn2,
    Soh,
    Text,
    Crc(usize),
}

/// Finds blocks in a bit stream. The demodulator knows each bit only relative to the one before, so the stream may
/// arrive inverted; a SYN that reads inverted says so.
pub struct Framer {
    state: State,
    reg: u8,
    bits: u8,
    invert: bool,
    block: Vec<u8>,
    bcs: [u8; 2],
    /// Bytes of this block that failed parity.
    bad: usize,
}

impl Default for Framer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framer {
    pub fn new() -> Self {
        Framer { state: State::Hunt, reg: 0, bits: 0, invert: false, block: Vec::new(), bcs: [0; 2], bad: 0 }
    }

    /// Feed one bit; a block that passed its CRC (bytes after SOH through ETX/ETB) is appended to `out`.
    pub fn push(&mut self, bit: bool, out: &mut Vec<Vec<u8>>) {
        self.reg = (self.reg >> 1) | ((bit as u8) << 7);
        if self.state == State::Hunt {
            if self.reg == SYN || self.reg == !SYN {
                self.invert = self.reg != SYN;
                self.state = State::Syn2;
                self.bits = 0;
            }
            return;
        }
        self.bits += 1;
        if self.bits < 8 {
            return;
        }
        self.bits = 0;
        let c = if self.invert { !self.reg } else { self.reg };
        self.state = match self.state {
            State::Syn2 if c == SYN => State::Soh,
            State::Soh if c == SOH => {
                self.block.clear();
                self.bad = 0;
                State::Text
            }
            State::Text => {
                self.block.push(c);
                self.bad += !parity_ok(c) as usize;
                // one damaged byte can be repaired against the CRC; two cannot
                if self.bad > 1 || self.block.len() > MAX_BLOCK {
                    State::Hunt
                } else if c == ETX || c == ETB {
                    State::Crc(0)
                } else {
                    State::Text
                }
            }
            State::Crc(0) => {
                self.bcs[0] = c;
                State::Crc(1)
            }
            State::Crc(_) => {
                self.bcs[1] = c;
                if self.block.len() >= HEADER && repair(&mut self.block, u16::from_le_bytes(self.bcs)) {
                    out.push(std::mem::take(&mut self.block));
                }
                State::Hunt
            }
            _ => State::Hunt,
        };
    }
}

/// True when `block` matches `crc`, after flipping one bit of its one byte with bad parity if there is one.
/// A single bit error always breaks parity, so that byte is where it is.
fn repair(block: &mut [u8], crc: u16) -> bool {
    match block.iter().position(|&c| !parity_ok(c)) {
        None => crc16(block) == crc,
        Some(i) => (0..8).any(|k| {
            block[i] ^= 1 << k;
            if crc16(block) == crc {
                return true;
            }
            block[i] ^= 1 << k;
            false
        }),
    }
}

/// The fields of a block that passed its CRC. Downlinks (block id a digit) start their text with a 4-character
/// message number and a 6-character flight id.
pub fn parse(block: &[u8]) -> Option<Message> {
    if block.len() < HEADER {
        return None;
    }
    let ch = |c: u8| (c & 0x7f) as char;
    let s: Vec<u8> = block.iter().map(|c| c & 0x7f).collect();
    let mut m = Message {
        mode: ch(s[0]),
        registration: s[1..8].iter().map(|&c| c as char).collect::<String>().trim_start_matches('.').trim().to_string(),
        ack: (s[8] != NAK).then(|| ch(s[8])),
        // the general-response label is '_' and DEL, which decoders print as "_d"
        label: s[9..11].iter().map(|&c| if c == DEL { 'd' } else { c as char }).collect(),
        block_id: ch(s[11]),
        more: block[block.len() - 1] == ETB,
        ..Default::default()
    };
    if s[12] == STX {
        let mut text = &s[HEADER..s.len() - 1];
        if m.block_id.is_ascii_digit() && text.len() >= 10 {
            m.msn = String::from_utf8_lossy(&text[..4]).trim().to_string();
            m.flight = String::from_utf8_lossy(&text[4..10]).trim().to_string();
            text = &text[10..];
        }
        m.text = clean(text);
    }
    Some(m)
}

/// Printable text with line breaks; CR LF and lone CR become LF, other control characters go.
fn clean(t: &[u8]) -> String {
    let mut out = String::with_capacity(t.len());
    let mut i = 0;
    while i < t.len() {
        match t[i] {
            b'\r' => {
                out.push('\n');
                if t.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => out.push('\n'),
            c @ 0x20..=0x7e => out.push(c as char),
            _ => {}
        }
        i += 1;
    }
    out.trim_end().to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A position report downlink, as an airliner sends one.
    pub fn downlink() -> Vec<u8> {
        block(b'2', ".N8722Q", NAK, "H1", b'4', Some("M51AUA0451#M1BPOSN39528W104364,DEN,155912,330,\r\nKORD,-47,"), false)
    }

    /// A ground station's general response: no text.
    pub fn uplink() -> Vec<u8> {
        block(b'2', ".N8722Q", b'4', "_\x7f", b'A', None, false)
    }

    fn bits(bytes: &[u8]) -> Vec<bool> {
        bytes.iter().flat_map(|&b| (0..8).map(move |k| b >> k & 1 == 1)).collect()
    }

    fn frame(bits: impl IntoIterator<Item = bool>) -> Vec<Vec<u8>> {
        let mut f = Framer::new();
        let mut out = Vec::new();
        for b in bits {
            f.push(b, &mut out);
        }
        out
    }

    #[test]
    fn crc_is_kermit() {
        assert_eq!(crc16(b"123456789"), 0x2189);
        // with its CRC appended, low byte first, a block checks to zero
        let b = downlink();
        let mut all = b.clone();
        all.extend(crc16(&b).to_le_bytes());
        assert_eq!(crc16(&all), 0);
    }

    #[test]
    fn parity_is_odd() {
        assert_eq!(with_parity(0x03), 0x83);
        assert_eq!(with_parity(0x17), 0x97);
        assert_eq!(with_parity(SYN), SYN);
        assert_eq!(with_parity(b'+'), 0xab);
        assert!(downlink().iter().all(|&c| parity_ok(c)));
    }

    #[test]
    fn parses_a_downlink() {
        let m = parse(&downlink()).unwrap();
        assert_eq!((m.mode, m.registration.as_str(), m.ack, m.label.as_str(), m.block_id), ('2', "N8722Q", None, "H1", '4'));
        assert_eq!((m.msn.as_str(), m.flight.as_str()), ("M51A", "UA0451"));
        assert_eq!(m.text, "#M1BPOSN39528W104364,DEN,155912,330,\nKORD,-47,");
        assert!(!m.more);
    }

    #[test]
    fn parses_an_uplink_without_text() {
        let m = parse(&uplink()).unwrap();
        assert_eq!((m.ack, m.label.as_str(), m.block_id, m.flight.as_str(), m.text.as_str()), (Some('4'), "_d", 'A', "", ""));
    }

    #[test]
    fn frames_blocks_upright_and_inverted() {
        let air = on_air(&downlink());
        // noise first, so the framer starts mid-hunt
        let lead = [true, false, false, true, true, false, true];
        assert_eq!(frame(lead.iter().copied().chain(bits(&air))), vec![downlink()]);
        assert_eq!(frame(lead.iter().copied().chain(bits(&air)).map(|b| !b)), vec![downlink()]);
    }

    #[test]
    fn repairs_one_bit_and_rejects_two() {
        let air = on_air(&downlink());
        let start = PRE_KEY + 5;
        let mut one = bits(&air);
        one[(start + 20) * 8 + 3] ^= true;
        assert_eq!(frame(one.clone()), vec![downlink()]);
        let mut two = one;
        two[(start + 30) * 8 + 6] ^= true;
        assert!(frame(two).is_empty());
        // a wrong CRC byte fails even with clean parity
        let mut crc = bits(&air);
        let at = air.len() - 3;
        crc[at * 8] ^= true;
        assert!(frame(crc).is_empty());
    }
}
