//! Trunking signalling blocks (TSBKs): the 12-byte messages a P25 control channel sends.
//!
//! On air each one is 98 dibits: interleaved, then rate-1/2 trellis coded (TIA-102.BAAA).
//! `decode` undoes both and checks the CRC; `Message::parse` reads the opcodes this receiver uses.

/// Dibit order on air: position `i` carries coded dibit `INTERLEAVE[i]`.
const INTERLEAVE: [usize; 98] = [
    0, 1, 8, 9, 16, 17, 24, 25, 32, 33, 40, 41, 48, 49, 56, 57, 64, 65, 72, 73, 80, 81, 88, 89, 96, 97, //
    2, 3, 10, 11, 18, 19, 26, 27, 34, 35, 42, 43, 50, 51, 58, 59, 66, 67, 74, 75, 82, 83, 90, 91, //
    4, 5, 12, 13, 20, 21, 28, 29, 36, 37, 44, 45, 52, 53, 60, 61, 68, 69, 76, 77, 84, 85, 92, 93, //
    6, 7, 14, 15, 22, 23, 30, 31, 38, 39, 46, 47, 54, 55, 62, 63, 70, 71, 78, 79, 86, 87, 94, 95,
];

/// Trellis output nibble for [state][input dibit]; the state is the previous input dibit.
const TRELLIS: [[u8; 4]; 4] = [[0x2, 0xC, 0x1, 0xF], [0xE, 0x0, 0xD, 0x3], [0x9, 0x7, 0xA, 0x4], [0x5, 0xB, 0x6, 0x8]];

/// 48 data dibits plus one flush dibit, two coded dibits each.
const POINTS: usize = 49;

/// Undo the interleave and trellis code of one TSBK. Returns its 12 bytes if the CRC passes.
pub fn decode(dibits: &[u8; 98]) -> Option<[u8; 12]> {
    let mut coded = [0u8; 98];
    for (i, &d) in dibits.iter().enumerate() {
        coded[INTERLEAVE[i]] = d;
    }
    let nibbles: Vec<u8> = coded.chunks(2).map(|p| (p[0] << 2) | p[1]).collect();
    let data = viterbi(&nibbles);
    let mut bytes = [0u8; 12];
    for (i, d) in data.iter().enumerate() {
        bytes[i / 4] |= d << (6 - 2 * (i % 4));
    }
    (crc16(&bytes[..10]) == u16::from_be_bytes([bytes[10], bytes[11]])).then_some(bytes)
}

/// Code 12 bytes the way a transmitter does; the inverse of `decode`, used by tests and the simulator.
pub fn encode(bytes: &[u8; 12]) -> [u8; 98] {
    let mut state = 0usize;
    let mut coded = [0u8; 98];
    for p in 0..POINTS {
        let input = if p < 48 { ((bytes[p / 4] >> (6 - 2 * (p % 4))) & 3) as usize } else { 0 };
        let n = TRELLIS[state][input];
        coded[2 * p] = n >> 2;
        coded[2 * p + 1] = n & 3;
        state = input;
    }
    let mut out = [0u8; 98];
    for (i, o) in out.iter_mut().enumerate() {
        *o = coded[INTERLEAVE[i]];
    }
    out
}

/// Most likely 48 input dibits for 49 received nibbles, by Hamming distance.
fn viterbi(nibbles: &[u8]) -> Vec<u8> {
    let mut cost = [0u32, u32::MAX / 2, u32::MAX / 2, u32::MAX / 2];
    let mut from: Vec<[u8; 4]> = Vec::with_capacity(POINTS);
    for &rx in nibbles {
        let mut next = [u32::MAX; 4];
        let mut back = [0u8; 4];
        for s in 0..4 {
            for input in 0..4 {
                let c = cost[s] + (TRELLIS[s][input] ^ rx).count_ones();
                if c < next[input] {
                    next[input] = c;
                    back[input] = s as u8;
                }
            }
        }
        cost = next;
        from.push(back);
    }
    // the flush dibit leaves the encoder in state 0
    let mut state = 0usize;
    let mut path = vec![0u8; POINTS];
    for p in (0..POINTS).rev() {
        path[p] = state as u8;
        state = from[p][state] as usize;
    }
    path.truncate(48);
    path
}

/// CRC-CCITT over the first 80 bits, inverted.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    !crc
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// A talkgroup call was given a voice channel.
    GroupGrant { channel: u16, group: u16, source: u32, encrypted: bool, emergency: bool },
    /// Calls already in progress, repeated for late joiners.
    GroupUpdate { calls: Vec<(u16, u16)> },
    /// How channel numbers with identifier `iden` turn into frequencies.
    Iden { iden: u8, base_hz: u64, spacing_hz: u32, tdma: bool },
    Network { wacn: u32, system: u16, channel: u16 },
    Site { rfss: u8, site: u8, channel: u16 },
    Other { opcode: u8, mfid: u8 },
}

impl Message {
    pub fn parse(b: &[u8; 12]) -> Message {
        let opcode = b[0] & 0x3F;
        let mfid = b[1];
        let u16_at = |i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
        if mfid != 0 {
            return Message::Other { opcode, mfid };
        }
        match opcode {
            0x00 => Message::GroupGrant {
                channel: u16_at(3),
                group: u16_at(5),
                source: u32::from_be_bytes([0, b[7], b[8], b[9]]),
                encrypted: b[2] & 0x40 != 0,
                emergency: b[2] & 0x80 != 0,
            },
            0x02 => Message::GroupUpdate { calls: vec![(u16_at(2), u16_at(4)), (u16_at(6), u16_at(8))] },
            // identifier update (0x3D), VHF/UHF (0x34) and TDMA (0x33) all put spacing and base in the same bits
            0x33 | 0x34 | 0x3D => {
                let iden = b[2] >> 4;
                let spacing = (u16_at(4) & 0x3FF) as u32;
                let base = u32::from_be_bytes([b[6], b[7], b[8], b[9]]) as u64;
                Message::Iden { iden, base_hz: base * 5, spacing_hz: spacing * 125, tdma: opcode == 0x33 }
            }
            0x3B => Message::Network {
                wacn: (u32::from_be_bytes([0, b[3], b[4], b[5]]) >> 4),
                system: u16_at(5) & 0xFFF,
                channel: u16_at(7),
            },
            0x3A => Message::Site { rfss: b[5], site: b[6], channel: u16_at(7) },
            _ => Message::Other { opcode, mfid },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> [u8; 12] {
        let mut b = [0x80, 0x00, 0x40, 0x10, 0x2A, 0x01, 0x23, 0x45, 0x67, 0x89, 0, 0];
        let crc = crc16(&b[..10]).to_be_bytes();
        b[10] = crc[0];
        b[11] = crc[1];
        b
    }

    #[test]
    fn round_trip() {
        assert_eq!(decode(&encode(&sample())), Some(sample()));
    }

    #[test]
    fn corrects_scattered_errors() {
        let mut d = encode(&sample());
        for i in [3, 30, 60, 90] {
            d[i] ^= 1;
        }
        assert_eq!(decode(&d), Some(sample()));
    }

    #[test]
    fn reads_an_encrypted_grant() {
        let m = Message::parse(&sample());
        assert_eq!(m, Message::GroupGrant { channel: 0x102A, group: 0x0123, source: 0x456789, encrypted: true, emergency: false });
    }
}
