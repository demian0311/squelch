//! RDS block layer: 26-bit blocks (16 data + 10 check bits) into groups of four.
//!
//! The check bits are a shortened cyclic code with a different offset word added for each
//! block position (A, B, C, C', D), so the remainder of a received block names its position.
//! Burst errors up to `MAX_BURST` bits are corrected from a syndrome table.

use std::collections::VecDeque;

use std::collections::HashMap;

/// g(x) = x¹⁰ + x⁸ + x⁷ + x⁵ + x⁴ + x³ + 1
const POLY: u32 = 0x5B9;
pub const OFFSET_A: u32 = 0x0FC;
pub const OFFSET_B: u32 = 0x198;
pub const OFFSET_C: u32 = 0x168;
pub const OFFSET_C2: u32 = 0x350;
pub const OFFSET_D: u32 = 0x1B4;
/// Longer bursts are correctable in theory but miscorrect too often on noise.
const MAX_BURST: usize = 2;
/// Sync is dropped when more than `LOSE_AT` of the last `WINDOW` blocks were bad.
const WINDOW: usize = 50;
const LOSE_AT: usize = 42;
/// A second valid block must follow within this many blocks, in the right position, to sync.
const SYNC_WITHIN: u64 = 6;

/// Remainder of a 26-bit word divided by g(x). For a valid block it equals the offset word.
pub fn remainder(word: u32) -> u32 {
    let mut r = word;
    for bit in (10..26).rev() {
        if r & (1 << bit) != 0 {
            r ^= POLY << (bit - 10);
        }
    }
    r & 0x3FF
}

/// Check bits for 16 data bits at a block position.
pub fn encode(data: u16, offset: u32) -> u32 {
    let w = (data as u32) << 10;
    w | (remainder(w) ^ offset)
}

/// One group; a block is `None` when it could not be received or corrected.
#[derive(Clone, Copy, Debug, Default)]
pub struct Group {
    pub a: Option<u16>,
    pub b: Option<u16>,
    pub c: Option<u16>,
    /// True when the third block carried offset C' (a version B group).
    pub c_prime: bool,
    pub d: Option<u16>,
}

/// Counters for diagnosing reception.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub good: u64,
    pub fixed: u64,
    pub bad: u64,
    pub syncs: u64,
}

pub struct Blocks {
    pub stats: Stats,
    word: u32,
    nbits: u32,
    synced: bool,
    /// Position (0 = A … 3 = D) of the block being collected.
    pos: usize,
    recent: VecDeque<bool>,
    /// Bit count and block position of the last valid block seen while hunting.
    hit: Option<(u64, usize)>,
    count: u64,
    group: Group,
    fixes: HashMap<u32, u32>,
}

impl Default for Blocks {
    fn default() -> Self {
        Self::new()
    }
}

impl Blocks {
    pub fn new() -> Self {
        let mut fixes = HashMap::new();
        for len in 1..=MAX_BURST {
            for start in 0..=(26 - len) {
                // bursts start and end with an error bit
                let span = len.saturating_sub(2);
                for inner in 0..(1u32 << span) {
                    let e = if len == 1 { 1 } else { 1 | (inner << 1) | (1 << (len - 1)) };
                    let e = e << start;
                    fixes.entry(remainder(e)).or_insert(e);
                }
            }
        }
        Blocks { stats: Stats::default(), word: 0, nbits: 0, synced: false, pos: 0, recent: VecDeque::new(), hit: None, count: 0, group: Group::default(), fixes }
    }

    pub fn synced(&self) -> bool {
        self.synced
    }

    pub fn push(&mut self, bit: bool, out: &mut Vec<Group>) {
        self.word = ((self.word << 1) | bit as u32) & 0x3FF_FFFF;
        self.nbits += 1;
        self.count += 1;
        if !self.synced {
            // hunt bit by bit: two valid blocks a whole number of blocks apart, in order
            if self.nbits >= 26 {
                if let Some(p) = position(remainder(self.word)) {
                    let slot = p.min(3) - usize::from(p == 4);
                    if let Some((at, q)) = self.hit {
                        let gap = self.count - at;
                        let n = gap / 26;
                        if gap % 26 == 0 && n >= 1 && n <= SYNC_WITHIN && (q as u64 + n) % 4 == slot as u64 {
                            self.synced = true;
                            self.stats.syncs += 1;
                            self.recent.clear();
                            self.group = Group::default();
                            self.store(p, (self.word >> 10) as u16, out);
                            self.pos = (slot + 1) % 4;
                            self.nbits = 0;
                            return;
                        }
                    }
                    self.hit = Some((self.count, slot));
                }
            }
            return;
        }
        if self.nbits < 26 {
            return;
        }
        self.nbits = 0;
        let rem = remainder(self.word);
        let expect: &[u32] = match self.pos {
            0 => &[OFFSET_A],
            1 => &[OFFSET_B],
            2 => &[OFFSET_C, OFFSET_C2],
            _ => &[OFFSET_D],
        };
        let mut data = None;
        for (k, &off) in expect.iter().enumerate() {
            let syn = rem ^ off;
            let fixed = if syn == 0 { Some(self.word) } else { self.fixes.get(&syn).map(|e| self.word ^ e) };
            if let Some(w) = fixed {
                if syn == 0 {
                    self.stats.good += 1;
                } else {
                    self.stats.fixed += 1;
                }
                data = Some(((w >> 10) as u16, k == 1));
                break;
            }
        }
        self.recent.push_back(data.is_none());
        if self.recent.len() > WINDOW {
            self.recent.pop_front();
        }
        match data {
            Some((d, prime)) => {
                self.store(if prime { 4 } else { self.pos }, d, out);
            }
            None => {
                self.stats.bad += 1;
                if self.recent.iter().filter(|&&b| b).count() > LOSE_AT {
                    self.synced = false;
                    self.hit = None;
                    self.nbits = 0;
                    return;
                }
                if self.pos == 3 {
                    out.push(std::mem::take(&mut self.group));
                }
            }
        }
        self.pos = (self.pos + 1) % 4;
    }

    /// `p` is the block position, with 4 meaning C'.
    fn store(&mut self, p: usize, d: u16, out: &mut Vec<Group>) {
        match p {
            0 => self.group.a = Some(d),
            1 => self.group.b = Some(d),
            2 => self.group.c = Some(d),
            4 => {
                self.group.c = Some(d);
                self.group.c_prime = true;
            }
            _ => {
                self.group.d = Some(d);
                out.push(std::mem::take(&mut self.group));
            }
        }
    }
}

/// Which block position an exact remainder belongs to (4 = C').
fn position(rem: u32) -> Option<usize> {
    [OFFSET_A, OFFSET_B, OFFSET_C, OFFSET_D, OFFSET_C2].iter().position(|&o| o == rem)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn bits_of(groups: &[[u16; 4]]) -> Vec<bool> {
        let offs = [OFFSET_A, OFFSET_B, OFFSET_C, OFFSET_D];
        groups.iter().flat_map(|g| (0..4).flat_map(move |i| { let w = encode(g[i], offs[i]); (0..26).rev().map(move |b| (w >> b) & 1 == 1) })).collect()
    }

    #[test]
    fn valid_blocks_leave_the_offset_as_remainder() {
        for off in [OFFSET_A, OFFSET_B, OFFSET_C, OFFSET_C2, OFFSET_D] {
            assert_eq!(remainder(encode(0xBEEF, off)), off);
        }
    }

    #[test]
    fn syncs_and_corrects_a_burst() {
        let groups: Vec<[u16; 4]> = (0..6).map(|i| [0x54A8, 0x0400 | i, 0x1234, 0x4142]).collect();
        let mut bits = vec![true, false, true];
        bits.extend(bits_of(&groups));
        // a 2-bit burst inside the fourth group's block C
        for k in 0..2 {
            let i = 3 + 3 * 104 + 52 + 7 + k;
            bits[i] = !bits[i];
        }
        let mut b = Blocks::new();
        let mut out = Vec::new();
        for bit in bits {
            b.push(bit, &mut out);
        }
        let full: Vec<&Group> = out.iter().filter(|g| g.a.is_some() && g.d.is_some()).collect();
        assert!(full.len() >= 5, "{out:?}");
        assert!(full.iter().all(|g| g.a == Some(0x54A8) && g.c == Some(0x1234) && g.d == Some(0x4142)));
    }
}
