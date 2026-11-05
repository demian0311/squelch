//! Full link control (ETSI TS 102 361-1 Table 9.7, 361-2 Tables 7.1 and 7.11): who is
//! calling whom, in a voice LC header, a terminator, or the embedded signalling of a call.

/// Voice LC header, data type 1: its Reed-Solomon parity is masked with this.
pub const MASK_HEADER: u32 = 0x96_96_96;
/// Terminator with LC, data type 2.
pub const MASK_TERMINATOR: u32 = 0x99_99_99;

pub const FLCO_GROUP: u8 = 0;
pub const FLCO_PRIVATE: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullLc {
    pub protect: bool,
    /// Full LC opcode: 0 group voice, 3 unit to unit.
    pub flco: u8,
    /// Feature set id: 0 standard, 16 Motorola, 6 Hytera…
    pub fid: u8,
    pub emergency: bool,
    /// Privacy: the voice is encrypted.
    pub privacy: bool,
    pub broadcast: bool,
    pub priority: u8,
    /// Talkgroup, or the called radio.
    pub dst: u32,
    pub src: u32,
}

fn field(bits: &[u8], at: usize, n: usize) -> u32 {
    bits[at..at + n].iter().fold(0u32, |b, &x| (b << 1) | (x & 1) as u32)
}

impl FullLc {
    /// The 72 LC bits, most significant first.
    pub fn parse(bits: &[u8]) -> FullLc {
        FullLc {
            protect: bits[0] & 1 == 1,
            flco: field(bits, 2, 6) as u8,
            fid: field(bits, 8, 8) as u8,
            emergency: bits[16] & 1 == 1,
            privacy: bits[17] & 1 == 1,
            broadcast: bits[20] & 1 == 1,
            priority: field(bits, 22, 2) as u8,
            dst: field(bits, 24, 24),
            src: field(bits, 48, 24),
        }
    }

    pub fn is_voice(&self) -> bool {
        matches!(self.flco, FLCO_GROUP | FLCO_PRIVATE)
    }

    /// Back to 72 bits (tests and the simulator).
    pub fn to_bits(&self) -> [u8; 72] {
        let mut b = [0u8; 72];
        let mut put = |at: usize, n: usize, v: u32| {
            for i in 0..n {
                b[at + i] = ((v >> (n - 1 - i)) & 1) as u8;
            }
        };
        put(0, 1, self.protect as u32);
        put(2, 6, self.flco as u32);
        put(8, 8, self.fid as u32);
        put(16, 1, self.emergency as u32);
        put(17, 1, self.privacy as u32);
        put(20, 1, self.broadcast as u32);
        put(22, 2, self.priority as u32);
        put(24, 24, self.dst);
        put(48, 24, self.src);
        b
    }

    /// 72 LC bits followed by the 24 masked Reed-Solomon parity bits: the 96 bits of a header
    /// or terminator before BPTC coding.
    pub fn to_block(&self, mask: u32) -> [u8; 96] {
        let lc = self.to_bits();
        let bytes: [u8; 9] = std::array::from_fn(|i| field(&lc, i * 8, 8) as u8);
        let p = super::fec::rs_12_9_parity(&bytes);
        let masked = (u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2])) ^ mask;
        let mut out = [0u8; 96];
        out[..72].copy_from_slice(&lc);
        for i in 0..24 {
            out[72 + i] = ((masked >> (23 - i)) & 1) as u8;
        }
        out
    }

    /// Read a header or terminator block; None when the parity fails.
    pub fn from_block(bits: &[u8; 96], mask: u32) -> Option<FullLc> {
        let mut block: [u8; 12] = std::array::from_fn(|i| field(bits, i * 8, 8) as u8);
        for (i, b) in block[9..].iter_mut().enumerate() {
            *b ^= (mask >> (16 - 8 * i)) as u8;
        }
        super::fec::rs_12_9_check(&block).then(|| FullLc::parse(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_block_round_trips_and_a_wrong_mask_fails() {
        let lc = FullLc { protect: false, flco: FLCO_GROUP, fid: 0, emergency: true, privacy: false, broadcast: false, priority: 1, dst: 31_088, src: 3_104_321 };
        let block = lc.to_block(MASK_HEADER);
        assert_eq!(FullLc::from_block(&block, MASK_HEADER), Some(lc));
        assert_eq!(FullLc::from_block(&block, MASK_TERMINATOR), None);
        assert_eq!(FullLc::parse(&lc.to_bits()), lc);
    }
}
