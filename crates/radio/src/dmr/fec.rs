//! The error-correcting codes of the DMR air interface (ETSI TS 102 361-1 Annex B), on bits
//! held one per byte. Generator matrices are the spec's Tables B.11–B.17; the interleaves are
//! B.1 (formula B.1) and figure B.3.

use std::sync::OnceLock;

/// A systematic code with one-bit correction: `p[i]` is the parity pattern of info bit `i`,
/// most significant bit first, as the generator matrix prints it.
pub struct Hamming {
    pub k: usize,
    pub p: &'static [u16],
}

/// Hamming (7,4,3), Table B.17: the CACH's TACT field.
pub const H7_4: Hamming = Hamming { k: 4, p: &[0b101, 0b111, 0b110, 0b011] };
/// Hamming (13,9,3), Table B.14: BPTC columns.
pub const H13_9: Hamming = Hamming { k: 9, p: &[0b1111, 0b1110, 0b0111, 0b1010, 0b0101, 0b1011, 0b1100, 0b0110, 0b0011] };
/// Hamming (15,11,3), Table B.15: BPTC rows.
pub const H15_11: Hamming = Hamming { k: 11, p: &[0b1001, 0b1101, 0b1111, 0b1110, 0b0111, 0b1010, 0b0101, 0b1011, 0b1100, 0b0110, 0b0011] };
/// Hamming (16,11,4), Table B.16: embedded link control rows.
pub const H16_11: Hamming = Hamming { k: 11, p: &[0b10011, 0b11010, 0b11111, 0b11100, 0b01110, 0b10101, 0b01011, 0b10110, 0b11001, 0b01101, 0b00111] };

impl Hamming {
    fn syndrome(&self, bits: &[u8]) -> u16 {
        let r = bits.len() - self.k;
        let mut s = 0u16;
        for (i, &b) in bits.iter().enumerate() {
            if b & 1 == 1 {
                s ^= if i < self.k { self.p[i] } else { 1 << (r - 1 - (i - self.k)) };
            }
        }
        s
    }

    /// Correct up to one bit in place. Returns the bits corrected, or None when it cannot.
    pub fn decode(&self, bits: &mut [u8]) -> Option<u32> {
        debug_assert_eq!(bits.len(), self.k + self.r());
        let s = self.syndrome(bits);
        if s == 0 {
            return Some(0);
        }
        if let Some(i) = self.p.iter().position(|&p| p == s) {
            bits[i] ^= 1;
            return Some(1);
        }
        if s.is_power_of_two() {
            let r = self.r();
            bits[self.k + r - 1 - s.trailing_zeros() as usize] ^= 1;
            return Some(1);
        }
        None
    }

    fn r(&self) -> usize {
        16 - self.p.iter().map(|p| p.leading_zeros()).min().unwrap() as usize
    }

    /// Append the parity bits to `info` (tests and the simulator).
    pub fn encode(&self, info: &[u8]) -> Vec<u8> {
        let r = self.r();
        let mut s = 0u16;
        for (i, &b) in info.iter().enumerate() {
            if b & 1 == 1 {
                s ^= self.p[i];
            }
        }
        let mut out = info.to_vec();
        out.extend((0..r).map(|j| ((s >> (r - 1 - j)) & 1) as u8));
        out
    }
}

/// A short block code decoded by a syndrome table: every error pattern up to `t` bits.
struct Table {
    k: usize,
    n: usize,
    /// Parity pattern of each info bit.
    p: &'static [u16],
    t: u32,
    map: OnceLock<Vec<u32>>,
}

impl Table {
    fn r(&self) -> usize {
        self.n - self.k
    }

    fn column(&self, i: usize) -> u16 {
        if i < self.k {
            self.p[i]
        } else {
            1 << (self.r() - 1 - (i - self.k))
        }
    }

    fn map(&self) -> &Vec<u32> {
        self.map.get_or_init(|| {
            let mut m = vec![u32::MAX; 1 << self.r()];
            m[0] = 0;
            let mut fill = |pattern: u32| {
                let s = (0..self.n).filter(|&i| pattern >> i & 1 == 1).fold(0u16, |s, i| s ^ self.column(i));
                if m[s as usize] == u32::MAX {
                    m[s as usize] = pattern;
                }
            };
            for a in 0..self.n {
                fill(1 << a);
                if self.t < 2 {
                    continue;
                }
                for b in a + 1..self.n {
                    fill(1 << a | 1 << b);
                    if self.t < 3 {
                        continue;
                    }
                    for c in b + 1..self.n {
                        fill(1 << a | 1 << b | 1 << c);
                    }
                }
            }
            m
        })
    }

    fn decode(&self, bits: &mut [u8]) -> Option<u32> {
        let s = (0..self.n).filter(|&i| bits[i] & 1 == 1).fold(0u16, |s, i| s ^ self.column(i));
        let pattern = self.map()[s as usize];
        if pattern == u32::MAX {
            return None;
        }
        for i in 0..self.n {
            if pattern >> i & 1 == 1 {
                bits[i] ^= 1;
            }
        }
        Some(pattern.count_ones())
    }

    fn encode(&self, info: &[u8]) -> Vec<u8> {
        let s = (0..self.k).filter(|&i| info[i] & 1 == 1).fold(0u16, |s, i| s ^ self.p[i]);
        let r = self.r();
        let mut out = info.to_vec();
        out.extend((0..r).map(|j| ((s >> (r - 1 - j)) & 1) as u8));
        out
    }
}

/// Golay (20,8,7), Table B.11: the slot type. Corrects 3 bits.
static GOLAY_20_8: Table = Table {
    k: 8,
    n: 20,
    p: &[0b0011_1101_1010, 0b1101_1001_1001, 0b0110_1100_1101, 0b0011_0110_0111, 0b1101_1100_0110, 0b1010_1001_0111, 0b1001_0011_1110, 0b1000_1110_1011],
    t: 3,
    map: OnceLock::new(),
};

/// Quadratic residue (16,7,6), Table B.12: the EMB field. Corrects 2 bits.
static QR_16_7: Table = Table {
    k: 7,
    n: 16,
    p: &[0b0_0100_1111, 0b1_0001_1110, 0b1_1011_0111, 0b1_1110_0010, 0b1_1100_1001, 0b0_1110_0101, 0b0_0111_0011],
    t: 2,
    map: OnceLock::new(),
};

pub fn golay_20_8_decode(bits: &mut [u8; 20]) -> Option<u32> {
    GOLAY_20_8.decode(bits)
}

pub fn golay_20_8_encode(info: &[u8; 8]) -> [u8; 20] {
    GOLAY_20_8.encode(info).try_into().unwrap()
}

pub fn qr_16_7_decode(bits: &mut [u8; 16]) -> Option<u32> {
    QR_16_7.decode(bits)
}

pub fn qr_16_7_encode(info: &[u8; 7]) -> [u8; 16] {
    QR_16_7.encode(info).try_into().unwrap()
}

/// BPTC (196,96) as received: deinterleave (formula B.1), correct rows with Hamming (15,11)
/// and columns with Hamming (13,9), twice, and read the 96 information bits out (figure B.1).
/// None when a row or column still fails.
pub fn bptc_196_96_decode(rx: &[u8; 196]) -> Option<[u8; 96]> {
    // bit 0 is the reserved R(3); then 13 rows of 15
    let mut m = [0u8; 196];
    for (i, b) in m.iter_mut().enumerate() {
        *b = rx[i * 181 % 196] & 1;
    }
    let at = |r: usize, c: usize| 1 + r * 15 + c;
    let mut ok = false;
    for _ in 0..2 {
        ok = true;
        for r in 0..9 {
            let mut row: [u8; 15] = std::array::from_fn(|c| m[at(r, c)]);
            if H15_11.decode(&mut row).is_none() {
                ok = false;
            }
            for (c, &b) in row.iter().enumerate() {
                m[at(r, c)] = b;
            }
        }
        for c in 0..15 {
            let mut col: [u8; 13] = std::array::from_fn(|r| m[at(r, c)]);
            if H13_9.decode(&mut col).is_none() {
                ok = false;
            }
            for (r, &b) in col.iter().enumerate() {
                m[at(r, c)] = b;
            }
        }
        if ok {
            break;
        }
    }
    if !ok {
        return None;
    }
    let mut out = [0u8; 96];
    let mut k = 0;
    for c in 3..11 {
        out[k] = m[at(0, c)];
        k += 1;
    }
    for r in 1..9 {
        for c in 0..11 {
            out[k] = m[at(r, c)];
            k += 1;
        }
    }
    Some(out)
}

/// The inverse of `bptc_196_96_decode`: 96 information bits to the 196 on-air bits.
pub fn bptc_196_96_encode(info: &[u8; 96]) -> [u8; 196] {
    let mut m = [0u8; 196];
    let at = |r: usize, c: usize| 1 + r * 15 + c;
    let mut k = 0;
    for c in 3..11 {
        m[at(0, c)] = info[k];
        k += 1;
    }
    for r in 1..9 {
        for c in 0..11 {
            m[at(r, c)] = info[k];
            k += 1;
        }
    }
    for r in 0..9 {
        let row: Vec<u8> = (0..11).map(|c| m[at(r, c)]).collect();
        let coded = H15_11.encode(&row);
        for (c, &b) in coded.iter().enumerate() {
            m[at(r, c)] = b;
        }
    }
    for c in 0..15 {
        let col: Vec<u8> = (0..9).map(|r| m[at(r, c)]).collect();
        let coded = H13_9.encode(&col);
        for (r, &b) in coded.iter().enumerate() {
            m[at(r, c)] = b;
        }
    }
    let mut tx = [0u8; 196];
    for (i, &b) in m.iter().enumerate() {
        tx[i * 181 % 196] = b;
    }
    tx
}

/// The embedded link control block (figure B.3): the 32-bit fragments of bursts B–E in order,
/// 128 bits, as 16 columns of 8. Rows 0–6 are Hamming (16,11,4), row 7 is column parity.
/// Returns the 72 LC bits then the 5 checksum bits.
pub fn embedded_lc_decode(bits: &[u8; 128]) -> Option<[u8; 77]> {
    let mut m = [[0u8; 16]; 8];
    for c in 0..16 {
        for r in 0..8 {
            m[r][c] = bits[c * 8 + r] & 1;
        }
    }
    for row in m.iter_mut().take(7) {
        H16_11.decode(row)?;
    }
    for c in 0..16 {
        let ones: u8 = (0..7).map(|r| m[r][c]).sum();
        if ones & 1 != m[7][c] {
            return None;
        }
    }
    let mut out = [0u8; 77];
    let mut k = 0;
    for r in 0..2 {
        for c in 0..11 {
            out[k] = m[r][c];
            k += 1;
        }
    }
    for r in 2..7 {
        for c in 0..10 {
            out[k] = m[r][c];
            k += 1;
        }
    }
    for r in 2..7 {
        out[k] = m[r][10];
        k += 1;
    }
    Some(out)
}

/// The inverse of `embedded_lc_decode`: 72 LC bits and the 5-bit checksum to the 128 bits.
pub fn embedded_lc_encode(lc: &[u8; 72], cs: &[u8; 5]) -> [u8; 128] {
    let mut m = [[0u8; 16]; 8];
    let mut k = 0;
    for r in 0..2 {
        for c in 0..11 {
            m[r][c] = lc[k];
            k += 1;
        }
    }
    for r in 2..7 {
        for c in 0..10 {
            m[r][c] = lc[k];
            k += 1;
        }
        m[r][10] = cs[r - 2];
    }
    for r in 0..7 {
        let coded = H16_11.encode(&m[r][..11]);
        m[r].copy_from_slice(&coded);
    }
    for c in 0..16 {
        m[7][c] = (0..7).map(|r| m[r][c]).sum::<u8>() & 1;
    }
    let mut out = [0u8; 128];
    for c in 0..16 {
        for r in 0..8 {
            out[c * 8 + r] = m[r][c];
        }
    }
    out
}

/// The 5-bit checksum of a 72-bit LC (B.3.11): its nine octets summed, mod 31.
pub fn lc_checksum(lc: &[u8; 72]) -> u8 {
    let sum: u32 = (0..9).map(|i| lc[i * 8..i * 8 + 8].iter().fold(0u32, |b, &x| (b << 1) | x as u32)).sum();
    (sum % 31) as u8
}

// Reed-Solomon (12,9,4) over GF(2^8), field polynomial x^8+x^4+x^3+x^2+1 (B.3.6):
// g(x) = (x + α)(x + α^2)(x + α^3) = x^3 + 0x0e x^2 + 0x38 x + 0x40.
const RS_GEN: [u8; 3] = [0x0e, 0x38, 0x40];

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    while b != 0 {
        if b & 1 == 1 {
            p ^= a;
        }
        let carry = a & 0x80 != 0;
        a <<= 1;
        if carry {
            a ^= 0x1d;
        }
        b >>= 1;
    }
    p
}

/// The three parity bytes of nine data bytes, before the data-type mask.
pub fn rs_12_9_parity(data: &[u8; 9]) -> [u8; 3] {
    let mut rem = [0u8; 3];
    for &d in data {
        let fb = d ^ rem[0];
        rem[0] = rem[1] ^ gf_mul(fb, RS_GEN[0]);
        rem[1] = rem[2] ^ gf_mul(fb, RS_GEN[1]);
        rem[2] = gf_mul(fb, RS_GEN[2]);
    }
    rem
}

/// Whether twelve bytes (nine data, three parity with the mask already removed) are a
/// codeword: the polynomial is zero at α, α^2 and α^3.
pub fn rs_12_9_check(block: &[u8; 12]) -> bool {
    (1..=3u8).all(|j| {
        let root = (0..j).fold(1u8, |a, _| gf_mul(a, 2));
        block.iter().fold(0u8, |acc, &b| gf_mul(acc, root) ^ b) == 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(n: usize, seed: u32) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (s >> 16) as u8 & 1
            })
            .collect()
    }

    #[test]
    fn hamming_codes_fix_one_bit_anywhere() {
        for h in [&H7_4, &H13_9, &H15_11, &H16_11] {
            let info = bits(h.k, 7);
            let coded = h.encode(&info);
            for i in 0..coded.len() {
                let mut rx = coded.clone();
                rx[i] ^= 1;
                assert_eq!(h.decode(&mut rx), Some(1), "k={} bit {i}", h.k);
                assert_eq!(&rx[..h.k], &info[..]);
            }
            let mut clean = coded.clone();
            assert_eq!(h.decode(&mut clean), Some(0));
        }
    }

    #[test]
    fn golay_fixes_three_bits_and_qr_two() {
        let info: [u8; 8] = bits(8, 3).try_into().unwrap();
        let coded = golay_20_8_encode(&info);
        let mut rx = coded;
        rx[0] ^= 1;
        rx[9] ^= 1;
        rx[19] ^= 1;
        assert_eq!(golay_20_8_decode(&mut rx), Some(3));
        assert_eq!(rx, coded);
        let info: [u8; 7] = bits(7, 5).try_into().unwrap();
        let coded = qr_16_7_encode(&info);
        let mut rx = coded;
        rx[2] ^= 1;
        rx[15] ^= 1;
        assert_eq!(qr_16_7_decode(&mut rx), Some(2));
        assert_eq!(rx, coded);
    }

    #[test]
    fn bptc_round_trip_survives_scattered_errors() {
        let info: [u8; 96] = bits(96, 11).try_into().unwrap();
        let mut tx = bptc_196_96_encode(&info);
        assert_eq!(bptc_196_96_decode(&tx), Some(info));
        for i in [3, 40, 77, 120, 150, 190] {
            tx[i] ^= 1;
        }
        assert_eq!(bptc_196_96_decode(&tx), Some(info));
    }

    #[test]
    fn embedded_lc_round_trip_and_checksum() {
        let lc: [u8; 72] = bits(72, 13).try_into().unwrap();
        let cs = lc_checksum(&lc);
        let csb: [u8; 5] = std::array::from_fn(|i| (cs >> (4 - i)) & 1);
        let mut tx = embedded_lc_encode(&lc, &csb);
        tx[17] ^= 1;
        tx[100] ^= 1;
        let out = embedded_lc_decode(&tx).expect("decodes");
        assert_eq!(&out[..72], &lc[..]);
        assert_eq!(&out[72..], &csb[..]);
    }

    #[test]
    fn reed_solomon_parity_is_a_codeword_and_a_damaged_one_is_not() {
        let data: [u8; 9] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x39, 0x12, 0xD6];
        let p = rs_12_9_parity(&data);
        let mut block = [0u8; 12];
        block[..9].copy_from_slice(&data);
        block[9..].copy_from_slice(&p);
        assert!(rs_12_9_check(&block));
        block[4] ^= 0x10;
        assert!(!rs_12_9_check(&block));
    }
}
