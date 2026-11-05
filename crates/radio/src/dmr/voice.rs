//! The three AMBE+2 voice frames of a DMR voice burst (ETSI TS 102 361-1 Table E.5).
//!
//! Burst dibits count from the first payload dibit after the CACH. Interleave tables are
//! DSD's `rW rX rY rZ`: the 36 dibits of a 72-bit frame into the codec's `ambe_fr[4][24]`
//! rows C0 (24 bits), C1 (23), C2 (11) and C3 (14).

/// Dibits in a burst without its CACH.
pub const BURST_DIBITS: usize = 132;
/// Dibits of the common announcement channel in front of an outbound burst.
pub const CACH_DIBITS: usize = 12;
/// The sync or embedded-signalling field: 24 dibits in the middle.
pub const CENTRE: std::ops::Range<usize> = 54..78;

const W: [u8; 36] = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2];
const X: [u8; 36] = [23, 10, 22, 9, 21, 8, 20, 7, 19, 6, 18, 5, 17, 4, 16, 3, 15, 2, 14, 1, 13, 0, 12, 10, 11, 9, 10, 8, 9, 7, 8, 6, 7, 5, 6, 4];
const Y: [u8; 36] = [0, 2, 0, 2, 0, 2, 0, 2, 0, 3, 0, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3, 1, 3];
const Z: [u8; 36] = [5, 3, 4, 2, 3, 1, 2, 0, 1, 13, 0, 12, 22, 11, 21, 10, 20, 9, 19, 8, 18, 7, 17, 6, 16, 5, 15, 4, 14, 3, 13, 2, 12, 1, 11, 0];

/// Spread the 36 dibits of one voice frame into the codec's bit layout.
pub fn deinterleave(dibits: &[u8]) -> [[u8; 24]; 4] {
    let mut fr = [[0u8; 24]; 4];
    for (j, &d) in dibits.iter().take(36).enumerate() {
        fr[W[j] as usize][X[j] as usize] = (d >> 1) & 1;
        fr[Y[j] as usize][Z[j] as usize] = d & 1;
    }
    fr
}

/// The inverse of `deinterleave`: a codec frame to the 36 on-air dibits (tests and the simulator).
pub fn interleave(fr: &[[u8; 24]; 4]) -> [u8; 36] {
    std::array::from_fn(|j| (fr[W[j] as usize][X[j] as usize] << 1) | fr[Y[j] as usize][Z[j] as usize])
}

// mbelib's Golay (23,12) generator (ecc_const.h): parity for each data bit, most significant first.
const GOLAY23: [u32; 12] = [0x63a, 0x31d, 0x7b4, 0x3da, 0x1ed, 0x6cc, 0x366, 0x1b3, 0x6e3, 0x54b, 0x49f, 0x475];

/// 23-bit Golay codeword as mbelib lays it out: bit 22 down to 11 the data, 10 down to 0 the parity.
fn golay23(data: u32) -> u32 {
    let parity = (0..12).filter(|&i| (data >> (11 - i)) & 1 == 1).fold(0u32, |p, i| p ^ GOLAY23[i]);
    (data << 11) | parity
}

/// Code 49 vocoder bits the way the codec expects them (mbelib `ambe3600x2450.c`): C0 and C1
/// Golay (23,12) coded, C1 scrambled by a sequence seeded from C0, C2 and C3 plain.
pub fn encode_frame(d: &[u8; 49]) -> [[u8; 24]; 4] {
    let bits = |at: usize, n: usize| d[at..at + n].iter().fold(0u32, |b, &x| (b << 1) | (x & 1) as u32);
    let mut fr = [[0u8; 24]; 4];
    let c0 = golay23(bits(0, 12));
    for j in 0..23 {
        fr[0][j + 1] = ((c0 >> j) & 1) as u8;
    }
    let mut c1 = golay23(bits(12, 12));
    // the pseudo-random modulator
    let mut pr = 16 * bits(0, 12);
    for k in 1..24 {
        pr = (173 * pr + 13849) % 65536;
        let j = 23 - k;
        c1 ^= (pr >> 15) << j;
    }
    for j in 0..23 {
        fr[1][j] = ((c1 >> j) & 1) as u8;
    }
    for j in 0..11 {
        fr[2][10 - j] = d[24 + j];
    }
    for j in 0..14 {
        fr[3][13 - j] = d[35 + j];
    }
    fr
}

/// The three frames of a voice burst, as 36 dibits each: the second is split around the centre.
pub fn frames(burst: &[u8]) -> [Vec<u8>; 3] {
    let mut second = burst[36..54].to_vec();
    second.extend_from_slice(&burst[78..96]);
    [burst[..36].to_vec(), second, burst[96..132].to_vec()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bit_lands_once() {
        let mut seen = [[0u8; 24]; 4];
        for j in 0..36 {
            seen[W[j] as usize][X[j] as usize] += 1;
            seen[Y[j] as usize][Z[j] as usize] += 1;
        }
        let widths = [24, 23, 11, 14];
        for (row, w) in seen.iter().zip(widths) {
            assert!(row[..w].iter().all(|&n| n == 1), "{row:?}");
            assert!(row[w..].iter().all(|&n| n == 0));
        }
    }

    #[test]
    fn interleave_round_trips() {
        let d: [u8; 49] = std::array::from_fn(|i| ((i * 7 + 3) % 5 == 0) as u8);
        let fr = encode_frame(&d);
        assert_eq!(deinterleave(&interleave(&fr)), fr);
    }

    #[test]
    fn a_coded_frame_decodes_without_corrections() {
        let Ok(mut dec) = crate::p25::mbe::Decoder::new() else { return };
        for seed in 0..5usize {
            let d: [u8; 49] = std::array::from_fn(|i| ((i * 7 + seed) % 3 == 0) as u8);
            let (_, errs) = dec.decode_ambe(&encode_frame(&d));
            assert_eq!(errs, 0, "seed {seed}");
        }
    }

    #[test]
    fn the_frames_cover_the_payload() {
        let burst: Vec<u8> = (0..132).map(|i| (i % 4) as u8).collect();
        let f = frames(&burst);
        assert!(f.iter().all(|x| x.len() == 36));
        let mut all: Vec<u8> = f.concat();
        all.sort_unstable();
        let mut want: Vec<u8> = burst.iter().enumerate().filter(|(i, _)| !CENTRE.contains(i)).map(|(_, &d)| d).collect();
        want.sort_unstable();
        assert_eq!(all, want);
    }
}
