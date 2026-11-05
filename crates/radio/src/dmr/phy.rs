//! 4FSK symbols to bursts: FM-demodulate 48 kHz I/Q, find a 24-symbol DMR sync, slice the
//! 132-dibit burst around it (and the 12 CACH dibits before it), then keep slicing a burst
//! every slot (144 symbols) so the voice bursts that carry no sync still come out. Timing
//! re-anchors on every sync seen, from either slot; a blind burst is sliced where the last
//! sync says it should be (a 100 ppm clock drifts under a sample between syncs).
//!
//! Dibit 01 is +3 (+1944 Hz), 00 +1, 10 −1, 11 −3 (ETSI TS 102 361-1 Table 10.3).

use super::voice::{BURST_DIBITS, CACH_DIBITS};
use crate::dsp::C32;

pub const RATE: f64 = 48_000.0;
const SPS: usize = 10;
const SYNC_LEN: usize = 24;
/// Symbols in a slot: a burst and its guard time.
const SLOT_SYMBOLS: usize = 144;
/// Dibits before the sync in a burst.
const BEFORE_SYNC: usize = 54;
/// Normalised correlation a sync must reach.
const MIN_CORR: f32 = 0.85;
/// Sliced back, the 24 sync symbols may miss the pattern by this many. The correlation alone passes
/// noise about once a second (its denominator is the noise's own size, 2026-11-05); a hard match does not.
const MAX_SYNC_ERRS: usize = 3;
/// Lock is dropped after this many bursts without a sync (a voice superframe on both slots
/// goes 5 bursts each without one).
const LOST_AFTER: u32 = 24;
/// Samples either side of the expected sync a locked search looks for the next one.
const SLIP: usize = 3;
/// Frequency offset is tracked over about this many samples.
const DC_SAMPLES: f32 = 9_600.0;
/// Samples each side of a symbol's centre averaged when slicing it.
const HALF_WINDOW: usize = 2;
/// History kept in front of the search position: CACH + the half burst before the sync.
const HISTORY: usize = (CACH_DIBITS + BEFORE_SYNC) * SPS + HALF_WINDOW;

/// Who sent the burst, from its sync pattern (Table 9.2). Voice and data syncs of one source
/// are symbol-wise complements, so each pair is one correlation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A repeater (base station).
    Repeater,
    /// A radio talking to a repeater.
    Mobile,
    /// Direct mode, time slot 1 or 2.
    Direct(u8),
}

/// The voice sync of each source; its data sync is the complement.
const VOICE_SYNCS: [(Source, u64); 4] = [(Source::Repeater, 0x755F_D7DF_75F7), (Source::Mobile, 0x7F7D_5DD5_7DFD), (Source::Direct(1), 0x5D57_7F77_57FF), (Source::Direct(2), 0x7DFF_D5F5_5D5F)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sync {
    pub source: Source,
    pub voice: bool,
}

pub struct Burst {
    /// The 132 payload dibits, CACH removed.
    pub dibits: [u8; BURST_DIBITS],
    /// The 12 CACH dibits in front of the burst (meaningful on a repeater's output).
    pub cach: [u8; CACH_DIBITS],
    /// None for a burst sliced on timing alone (voice bursts B–F).
    pub sync: Option<Sync>,
    /// Counts slots since the lock began; consecutive bursts alternate time slots.
    pub seq: u64,
    /// First burst of a fresh lock: `seq` parity means nothing across it.
    pub relocked: bool,
}

struct Lock {
    /// Sample where the next burst's sync should start.
    next: usize,
    /// Discriminator value of a +3 symbol, from the last sync.
    amp: f32,
    misses: u32,
    seq: u64,
}

pub struct Phy {
    syncs: [[f32; SYNC_LEN]; 4],
    last: C32,
    dc: f32,
    fm: Vec<f32>,
    /// Next sample to test for a sync while unlocked.
    pos: usize,
    lock: Option<Lock>,
}

impl Default for Phy {
    fn default() -> Self {
        Self::new()
    }
}

fn levels(sync: u64) -> [f32; SYNC_LEN] {
    let mut out = [0f32; SYNC_LEN];
    for (i, v) in out.iter_mut().enumerate() {
        *v = match (sync >> (46 - 2 * i)) & 3 {
            1 => 3.0,
            0 => 1.0,
            2 => -1.0,
            _ => -3.0,
        };
    }
    out
}

impl Phy {
    pub fn new() -> Self {
        Phy { syncs: VOICE_SYNCS.map(|(_, s)| levels(s)), last: C32::new(1.0, 0.0), dc: 0.0, fm: Vec::new(), pos: 0, lock: None }
    }

    pub fn reset(&mut self) {
        self.fm.clear();
        self.pos = 0;
        self.dc = 0.0;
        self.lock = None;
    }

    pub fn locked(&self) -> bool {
        self.lock.is_some()
    }

    /// Carrier offset from the channel centre, Hz, as the discriminator sees it.
    pub fn offset_hz(&self) -> f64 {
        self.dc as f64 * RATE / std::f64::consts::TAU
    }

    pub fn process(&mut self, iq: &[C32], bursts: &mut Vec<Burst>) {
        for &s in iq {
            let d = (s * self.last.conj()).arg();
            self.last = s;
            self.dc += (d - self.dc) / DC_SAMPLES;
            self.fm.push(d - self.dc);
        }
        // a burst reaches 78 symbols past its sync start
        let tail = (SYNC_LEN + BEFORE_SYNC) * SPS + HALF_WINDOW + SLIP + 1;
        loop {
            if let Some(lock) = &self.lock {
                let (next, mut amp, mut misses, seq) = (lock.next, lock.amp, lock.misses, lock.seq);
                if next + tail >= self.fm.len() {
                    break;
                }
                let (k, c) = self.best_sync(next);
                let (k, c) = (next - SLIP..=next + SLIP).map(|t| self.best_sync(t)).fold((k, c), |a, b| if b.1.abs() > a.1.abs() { b } else { a });
                let t = if c.abs() >= MIN_CORR { self.peak(k, next - SLIP..=next + SLIP) } else { next };
                let voice = self.corr(k, t) > 0.0;
                let (sync, at) = if c.abs() >= MIN_CORR && self.sync_errs(k, t, self.amplitude(t), voice) <= MAX_SYNC_ERRS {
                    amp = self.amplitude(t);
                    misses = 0;
                    (Some(Sync { source: VOICE_SYNCS[k].0, voice }), t)
                } else {
                    misses += 1;
                    (None, next)
                };
                bursts.push(self.slice(at, amp, sync, seq, false));
                if misses > LOST_AFTER {
                    self.lock = None;
                    self.pos = at + SPS;
                } else {
                    self.lock = Some(Lock { next: at + SLOT_SYMBOLS * SPS, amp, misses, seq: seq + 1 });
                }
            } else {
                if self.pos < HISTORY {
                    self.pos = HISTORY;
                }
                if self.pos + tail >= self.fm.len() {
                    break;
                }
                let (k, c) = self.best_sync(self.pos);
                if c.abs() < MIN_CORR {
                    self.pos += 1;
                    continue;
                }
                let t = self.peak(k, self.pos..=self.pos + SPS - 1);
                let c = self.corr(k, t);
                let amp = self.amplitude(t);
                if self.sync_errs(k, t, amp, c > 0.0) > MAX_SYNC_ERRS {
                    self.pos += 1;
                    continue;
                }
                let sync = Some(Sync { source: VOICE_SYNCS[k].0, voice: c > 0.0 });
                bursts.push(self.slice(t, amp, sync, 0, true));
                self.lock = Some(Lock { next: t + SLOT_SYMBOLS * SPS, amp, misses: 0, seq: 1 });
            }
        }
        // keep the unsearched tail, with history enough for the burst in front of the next sync
        let front = self.lock.as_ref().map(|l| l.next - SLIP).unwrap_or(self.pos);
        let keep = front.saturating_sub(HISTORY).min(self.fm.len());
        self.fm.drain(..keep);
        self.pos = self.pos.saturating_sub(keep);
        if let Some(l) = &mut self.lock {
            l.next -= keep;
        }
    }

    /// The sync pair matching best at `t`, and its signed correlation (positive: voice).
    fn best_sync(&self, t: usize) -> (usize, f32) {
        (0..4).map(|k| (k, self.corr(k, t))).max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap()
    }

    /// Where pattern `k` correlates best in `range`: the middle of the plateau when several
    /// samples tie (symbols without pulse shaping are flat), so the slicing window sits
    /// inside the symbol.
    fn peak(&self, k: usize, range: std::ops::RangeInclusive<usize>) -> usize {
        let cs: Vec<(usize, f32)> = range.map(|t| (t, self.corr(k, t).abs())).collect();
        let max = cs.iter().map(|c| c.1).fold(0f32, f32::max);
        let top: Vec<usize> = cs.iter().filter(|c| c.1 >= max * 0.98).map(|c| c.0).collect();
        top[top.len() / 2]
    }

    fn corr(&self, k: usize, t: usize) -> f32 {
        let (mut c, mut e) = (0f32, 0f32);
        for (i, s) in self.syncs[k].iter().enumerate() {
            let x = self.fm[t + i * SPS];
            c += s * x;
            e += x.abs();
        }
        c / (3.0 * e).max(1e-9)
    }

    /// How many of the 24 sync symbols at `t`, sliced at `amp`, miss pattern `k` (inverted for a data sync).
    fn sync_errs(&self, k: usize, t: usize, amp: f32, voice: bool) -> usize {
        let sign = if voice { 1.0 } else { -1.0 };
        (0..SYNC_LEN)
            .filter(|&i| {
                let v = self.symbol(t + i * SPS) / amp * 3.0;
                let level = if v > 2.0 { 3.0 } else if v > 0.0 { 1.0 } else if v > -2.0 { -1.0 } else { -3.0 };
                level != self.syncs[k][i] * sign
            })
            .count()
    }

    /// Discriminator value of a ±3 symbol, from the sync's own symbols.
    fn amplitude(&self, t: usize) -> f32 {
        (0..SYNC_LEN).map(|k| self.symbol(t + k * SPS).abs()).sum::<f32>() / SYNC_LEN as f32
    }

    /// Mean over the middle of a symbol, which averages out noise the single sample carries.
    fn symbol(&self, t: usize) -> f32 {
        let lo = t.saturating_sub(HALF_WINDOW);
        let hi = (t + HALF_WINDOW + 1).min(self.fm.len());
        self.fm[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
    }

    fn slice(&self, sync_at: usize, amp: f32, sync: Option<Sync>, seq: u64, relocked: bool) -> Burst {
        let start = sync_at - (BEFORE_SYNC + CACH_DIBITS) * SPS;
        let dibit = |k: usize| {
            let v = self.symbol(start + k * SPS) / amp * 3.0;
            if v > 2.0 {
                1
            } else if v > 0.0 {
                0
            } else if v > -2.0 {
                2
            } else {
                3
            }
        };
        let cach: [u8; CACH_DIBITS] = std::array::from_fn(dibit);
        let dibits: [u8; BURST_DIBITS] = std::array::from_fn(|k| dibit(k + CACH_DIBITS));
        Burst { dibits, cach, sync, seq, relocked }
    }
}

/// 4FSK-modulate dibits at 48 kHz for tests and the simulator: ±1944 / ±648 Hz deviation.
pub fn modulate(dibits: &[u8]) -> Vec<C32> {
    let mut phase = 0f64;
    let mut out = Vec::with_capacity(dibits.len() * SPS);
    for &d in dibits {
        let level = match d {
            1 => 3.0,
            0 => 1.0,
            2 => -1.0,
            _ => -3.0,
        };
        for _ in 0..SPS {
            phase += std::f64::consts::TAU * 648.0 * level / RATE;
            out.push(C32::new(phase.cos() as f32, phase.sin() as f32));
        }
    }
    out
}

/// The 24 sync dibits of a source, voice or data.
pub fn sync_dibits(source: Source, voice: bool) -> [u8; SYNC_LEN] {
    let (_, s) = VOICE_SYNCS.iter().find(|(src, _)| *src == source).unwrap();
    let s = if voice { *s } else { *s ^ 0xAAAA_AAAA_AAAA };
    std::array::from_fn(|i| ((s >> (46 - 2 * i)) & 3) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_syncs_are_the_complement_of_voice_syncs() {
        // Table 9.2's data patterns, checked against the complement rule
        assert_eq!(VOICE_SYNCS[0].1 ^ 0xAAAA_AAAA_AAAA, 0xDFF5_7D75_DF5D);
        assert_eq!(VOICE_SYNCS[1].1 ^ 0xAAAA_AAAA_AAAA, 0xD5D7_F77F_D757);
        assert_eq!(VOICE_SYNCS[2].1 ^ 0xAAAA_AAAA_AAAA, 0xF7FD_D5DD_FD55);
        assert_eq!(VOICE_SYNCS[3].1 ^ 0xAAAA_AAAA_AAAA, 0xD755_7F5F_F7F5);
    }

    /// Two slots of bursts: slot A with a sync every burst, slot B with a sync only on the first.
    #[test]
    fn locks_on_a_sync_and_keeps_slicing_blind_bursts() {
        let mut raw = vec![0u8; 400];
        let mut want = Vec::new();
        for n in 0..8u32 {
            let mut burst: Vec<u8> = (0..BURST_DIBITS).map(|k| ((k as u32 * 7 + n * 3) % 4) as u8).collect();
            let synced = n % 2 == 0 || n == 1;
            if synced {
                burst[54..78].copy_from_slice(&sync_dibits(Source::Repeater, n % 4 == 0));
            }
            want.push((burst.clone(), synced));
            let cach: Vec<u8> = (0..CACH_DIBITS).map(|k| (k % 3) as u8).collect();
            raw.extend(cach);
            raw.extend(burst);
        }
        raw.extend(vec![0u8; 1200]);
        let mut phy = Phy::new();
        let mut bursts = Vec::new();
        for chunk in modulate(&raw).chunks(3000) {
            phy.process(chunk, &mut bursts);
        }
        assert!(bursts.len() >= 8, "{}", bursts.len());
        for (b, (dibits, synced)) in bursts.iter().zip(&want) {
            assert_eq!(b.dibits.to_vec(), *dibits);
            assert_eq!(b.sync.is_some(), *synced);
            if let Some(s) = b.sync {
                assert_eq!(s.source, Source::Repeater);
            }
            assert_eq!(b.cach.to_vec(), (0..CACH_DIBITS).map(|k| (k % 3) as u8).collect::<Vec<_>>());
        }
        assert!(bursts[0].relocked && !bursts[1].relocked);
        assert_eq!(bursts[1].seq, 1);
    }
}
