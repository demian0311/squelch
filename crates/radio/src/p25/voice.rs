//! The nine IMBE voice frames inside an LDU1 or LDU2 (TIA-102.BAAA).
//!
//! Positions count dibits from the frame sync with status symbols already removed: sync and
//! NID take 56, then voice frames of 72 dibits with link-control words (20 dibits) between
//! frames 2–8 and low-speed data (16) before frame 9. Interleave tables are DSD's `iW iX iY iZ`.

/// Start of each voice frame.
pub const FRAMES: [usize; 9] = [56, 128, 220, 312, 404, 496, 588, 680, 768];
/// Dibits in an LDU once status symbols are removed.
pub const LDU_DIBITS: usize = 840;

const W: [u8; 72] = [
    0, 2, 4, 1, 3, 5, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, //
    0, 2, 5, 1, 3, 6, 0, 2, 5, 1, 3, 6, 0, 2, 5, 1, 3, 7, 0, 2, 5, 1, 3, 7, 0, 2, 5, 1, 4, 7, 0, 3, 5, 2, 4, 7,
];
const X: [u8; 72] = [
    22, 20, 10, 20, 18, 0, 20, 18, 8, 18, 16, 13, 18, 16, 6, 16, 14, 11, 16, 14, 4, 14, 12, 9, 14, 12, 2, 12, 10, 7, 12, 10, 0, 10, 8, 5, //
    10, 8, 13, 8, 6, 3, 8, 6, 11, 6, 4, 1, 6, 4, 9, 4, 2, 6, 4, 2, 7, 2, 0, 4, 2, 0, 5, 0, 13, 2, 0, 21, 3, 21, 11, 0,
];
const Y: [u8; 72] = [
    1, 3, 5, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 4, 1, 3, 6, 0, 2, 5, //
    1, 3, 6, 0, 2, 5, 1, 3, 6, 0, 2, 5, 1, 3, 6, 0, 2, 5, 1, 3, 7, 0, 2, 5, 1, 4, 7, 0, 3, 5, 2, 4, 7, 1, 3, 5,
];
const Z: [u8; 72] = [
    21, 19, 1, 21, 19, 9, 19, 17, 14, 19, 17, 7, 17, 15, 12, 17, 15, 5, 15, 13, 10, 15, 13, 3, 13, 11, 8, 13, 11, 1, 11, 9, 6, 11, 9, 14, //
    9, 7, 4, 9, 7, 12, 7, 5, 2, 7, 5, 10, 5, 3, 0, 5, 3, 8, 3, 1, 5, 3, 1, 6, 1, 14, 3, 1, 22, 4, 22, 12, 1, 22, 20, 2,
];

/// Spread 72 dibits into the codec's `imbe_fr[8][23]` bit layout.
pub fn deinterleave(dibits: &[u8]) -> [[u8; 23]; 8] {
    let mut fr = [[0u8; 23]; 8];
    for (j, &d) in dibits.iter().take(72).enumerate() {
        fr[W[j] as usize][X[j] as usize] = (d >> 1) & 1;
        fr[Y[j] as usize][Z[j] as usize] = d & 1;
    }
    fr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fills_the_ldu() {
        assert_eq!(FRAMES[8] + 72, LDU_DIBITS);
    }

    #[test]
    fn every_bit_lands_once() {
        // 144 bits fill c0–c3 (23 each), c4–c6 (15 each) and c7 (7)
        let mut seen = [[0u8; 23]; 8];
        for j in 0..72 {
            seen[W[j] as usize][X[j] as usize] += 1;
            seen[Y[j] as usize][Z[j] as usize] += 1;
        }
        let widths = [23, 23, 23, 23, 15, 15, 15, 7];
        for (row, w) in seen.iter().zip(widths) {
            assert!(row[..w].iter().all(|&n| n == 1), "{row:?}");
            assert!(row[w..].iter().all(|&n| n == 0));
        }
    }
}
