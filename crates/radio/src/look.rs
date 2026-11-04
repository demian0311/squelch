//! The whole of a wide band at a glance while the radio stays on one channel. One dongle sees `SAMPLE_RATE` at a time, so
//! every `EVERY_SECS` it borrows the dongle and hops across the ranges, about `HOP_SECS` a hop, then goes back. The radio
//! hears and draws nothing during a look, which is why a call in progress holds it off.

use serde::Serialize;

use crate::spectrum::noise_floor;

/// Most of the capture, away from its edges, like the page's view limit.
const HOP_HZ: f64 = 1_920_000.0;
/// One cell of a line: a P25 channel.
pub const CELL_HZ: f64 = 12_500.0;
pub const EVERY_SECS: f64 = 30.0;
/// After a look gave way to tuning.
const RETRY_SECS: f64 = 10.0;
/// Samples just after a hop are the tuner settling.
const SETTLE_SECS: f64 = 0.05;
/// Spectrum rows averaged per hop, about 33 ms each.
const ROWS_PER_HOP: usize = 3;
/// A finished line rides in the status this long, so a page that skipped a status still gets it.
const SEND_FOR_SECS: f64 = 1.0;
/// Bins beside the centre that hold the dongle's own DC spike; each cell has about ten bins, so it loses only these.
const DC_BINS: usize = 3;

/// One continuous range of a line.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Part {
    pub lo_hz: f64,
    pub hi_hz: f64,
    /// Strongest signal per `CELL_HZ`, on the waterfall's byte scale for dB above each hop's floor.
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub seq: u32,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    /// Hopping now: nothing can be heard or drawn.
    pub looking: bool,
    /// The newest line, for `SEND_FOR_SECS` after it is finished.
    pub line: Option<Line>,
}

#[derive(Debug, PartialEq)]
pub enum Hop {
    Stay,
    To(f64),
    /// Finished: go back to where the radio was.
    Home,
}

struct Slice {
    center: f64,
    lo: f64,
    hi: f64,
    part: usize,
}

pub struct BandLook {
    ranges: Vec<(f64, f64)>,
    slices: Vec<Slice>,
    rate: f64,
    bin_scale: (f32, f32),
    /// Samples until the next look.
    wait: i64,
    at: Option<usize>,
    settle: i64,
    rows: usize,
    avg: Vec<f32>,
    parts: Vec<Part>,
    line: Option<Line>,
    seq: u32,
    send_for: i64,
}

fn slices_of(ranges: &[(f64, f64)]) -> Vec<Slice> {
    let mut out = Vec::new();
    for (part, &(lo, hi)) in ranges.iter().enumerate() {
        let n = ((hi - lo) / HOP_HZ).ceil().max(1.0) as usize;
        let step = (hi - lo) / n as f64;
        out.extend((0..n).map(|i| {
            let a = lo + step * i as f64;
            Slice { center: a + step / 2.0, lo: a, hi: a + step, part }
        }));
    }
    out
}

impl BandLook {
    /// `below_floor_db` and `range_db` are the waterfall's byte scale. The first look is due at once.
    pub fn new(ranges: Vec<(f64, f64)>, rate: u32, below_floor_db: f32, range_db: f32) -> Self {
        let slices = slices_of(&ranges);
        BandLook { ranges, slices, rate: rate as f64, bin_scale: (below_floor_db, range_db), wait: 0, at: None, settle: 0, rows: 0, avg: Vec::new(), parts: Vec::new(), line: None, seq: 0, send_for: 0 }
    }

    pub fn ranges(&self) -> &[(f64, f64)] {
        &self.ranges
    }

    pub fn sweeping(&self) -> bool {
        self.at.is_some()
    }

    fn secs(&self, s: f64) -> i64 {
        (s * self.rate) as i64
    }

    /// Between looks: count a block. Returns the first hop when a look is due and the radio is not `busy`.
    pub fn tick(&mut self, samples: i64, busy: bool) -> Option<f64> {
        self.wait -= samples;
        self.send_for -= samples;
        if self.wait > 0 || busy {
            return None;
        }
        self.parts = self.ranges.iter().map(|&(lo, hi)| Part { lo_hz: lo, hi_hz: hi, cells: vec![0; ((hi - lo) / CELL_HZ).round() as usize] }).collect();
        Some(self.enter(0))
    }

    fn enter(&mut self, i: usize) -> f64 {
        self.at = Some(i);
        self.settle = self.secs(SETTLE_SECS);
        self.rows = 0;
        self.avg.clear();
        self.slices[i].center
    }

    /// During a look: take a block's spectrum rows and its length in samples.
    pub fn sweep(&mut self, rows: &[Vec<f32>], samples: i64) -> Hop {
        let Some(i) = self.at else { return Hop::Stay };
        // the rows of the block that crosses the end of the settling may still hold settling samples
        let settled = self.settle <= 0;
        self.settle -= samples;
        if !settled {
            return Hop::Stay;
        }
        for row in rows {
            if self.avg.is_empty() {
                self.avg = vec![0.0; row.len()];
            }
            for (a, v) in self.avg.iter_mut().zip(row) {
                *a += v;
            }
            self.rows += 1;
        }
        if self.rows < ROWS_PER_HOP {
            return Hop::Stay;
        }
        self.finish_hop(i);
        if i + 1 < self.slices.len() {
            return Hop::To(self.enter(i + 1));
        }
        self.at = None;
        self.wait = self.secs(EVERY_SECS);
        self.seq += 1;
        self.line = Some(Line { seq: self.seq, parts: self.parts.clone() });
        self.send_for = self.secs(SEND_FOR_SECS);
        Hop::Home
    }

    fn finish_hop(&mut self, i: usize) {
        let n = self.rows as f32;
        for a in &mut self.avg {
            *a /= n;
        }
        let Slice { center, lo, hi, part } = self.slices[i];
        let bins = self.avg.len();
        let bin_hz = self.rate / bins as f64;
        let floor = noise_floor(&self.avg);
        let (below, range) = self.bin_scale;
        let bottom = floor - below;
        let p = &mut self.parts[part];
        for (b, &db) in self.avg.iter().enumerate() {
            if b.abs_diff(bins / 2) <= DC_BINS {
                continue;
            }
            let hz = center - self.rate / 2.0 + b as f64 * bin_hz;
            if hz < lo || hz >= hi {
                continue;
            }
            let cell = ((hz - p.lo_hz) / CELL_HZ) as usize;
            if let Some(c) = p.cells.get_mut(cell) {
                *c = (*c).max((((db - bottom) / range) * 255.0).clamp(0.0, 255.0) as u8);
            }
        }
    }

    /// Stop a look in progress; the next one comes `RETRY_SECS` later. True if there was one: the dongle needs to go home.
    pub fn abort(&mut self) -> bool {
        let was = self.at.take().is_some();
        if was {
            self.wait = self.secs(RETRY_SECS);
        }
        was
    }

    pub fn info(&self) -> Info {
        Info { looking: self.sweeping(), line: if self.send_for > 0 { self.line.clone() } else { None } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 2_400_000;
    const BINS: usize = 2048;

    fn look() -> BandLook {
        BandLook::new(vec![(769e6, 775e6), (851e6, 869e6)], RATE, 8.0, 68.0)
    }

    /// A noise-floor row with a strong signal at `hz`, as seen with the dongle at `center`.
    fn row(center: f64, hz: f64) -> Vec<f32> {
        let mut r = vec![-60.0f32; BINS];
        let bin = ((hz - center + RATE as f64 / 2.0) / (RATE as f64 / BINS as f64)).round() as isize;
        if bin >= 0 && (bin as usize) < BINS {
            r[bin as usize] = -20.0;
        }
        r
    }

    /// Runs a whole look with a signal at `hz`, returning the line and each hop's centre.
    fn whole_look(l: &mut BandLook, hz: f64) -> (Line, Vec<f64>) {
        let mut at = l.tick(1, false).expect("due");
        let mut hops = vec![at];
        let block = 65_536;
        for _ in 0..1000 {
            match l.sweep(&[row(at, hz)], block) {
                Hop::Stay => {}
                Hop::To(c) => {
                    at = c;
                    hops.push(c);
                }
                Hop::Home => return (l.info().line.expect("line sent"), hops),
            }
        }
        panic!("the look never ended");
    }

    #[test]
    fn hops_cover_both_ranges_and_nothing_between() {
        let (_, hops) = whole_look(&mut look(), 860e6);
        assert_eq!(hops.len(), 14);
        for pair in hops.windows(2) {
            let step = pair[1] - pair[0];
            assert!(step <= HOP_HZ + 1.0 || step > 70e6, "a hole: {step}");
        }
        assert!(hops.iter().all(|&c| (769e6..775e6).contains(&c) || (851e6..869e6).contains(&c)));
    }

    #[test]
    fn a_signal_lands_in_its_own_cell_and_only_there() {
        let hz = 853.9375e6;
        let (line, _) = whole_look(&mut look(), hz);
        let p = &line.parts[1];
        assert_eq!(p.cells.len(), 1440);
        let want = ((hz - 851e6) / CELL_HZ) as usize;
        let strongest = (0..p.cells.len()).max_by_key(|&i| p.cells[i]).unwrap();
        assert!(strongest.abs_diff(want) <= 1, "{strongest} vs {want}");
        assert!(p.cells[strongest] > 100, "{}", p.cells[strongest]);
        assert!(line.parts[0].cells.iter().all(|&c| c < 50), "signal leaked into the other range");
    }

    #[test]
    fn the_dc_spike_at_a_hop_centre_is_left_out() {
        let mut l = look();
        let mut at = l.tick(1, false).unwrap();
        let line = loop {
            // a spike that follows the dongle, at every hop's own centre
            match l.sweep(&[row(at, at)], 65_536) {
                Hop::Stay => {}
                Hop::To(c) => at = c,
                Hop::Home => break l.info().line.unwrap(),
            }
        };
        assert!(line.parts.iter().all(|p| p.cells.iter().all(|&c| c < 50)));
    }

    #[test]
    fn settling_samples_are_not_used() {
        let mut l = look();
        let at = l.tick(1, false).unwrap();
        // a signal seen only while settling
        assert_eq!(l.sweep(&[row(at, at + 400e3)], (SETTLE_SECS * RATE as f64) as i64 + 1), Hop::Stay);
        let mut next = None;
        for _ in 0..ROWS_PER_HOP {
            if let Hop::To(c) = l.sweep(&[row(at, 0.0)], 65_536) {
                next = Some(c);
            }
        }
        assert!(next.is_some());
        assert!(l.parts[0].cells.iter().all(|&c| c < 50));
    }

    #[test]
    fn a_call_holds_a_look_off_until_it_ends() {
        let mut l = look();
        assert_eq!(l.tick(1, true), None);
        assert!(!l.sweeping());
        assert!(l.tick(1, false).is_some());
    }

    #[test]
    fn looks_come_every_thirty_seconds_and_lines_are_sent_briefly() {
        let mut l = look();
        whole_look(&mut l, 860e6);
        assert_eq!(l.info().line.as_ref().map(|x| x.seq), Some(1));
        let s = RATE as i64;
        assert_eq!(l.tick(2 * s, false), None);
        assert!(l.info().line.is_none());
        assert_eq!(l.tick(27 * s, false), None);
        assert!(l.tick(2 * s, false).is_some());
    }

    #[test]
    fn tuning_ends_a_look_and_pushes_the_next_one_back() {
        let mut l = look();
        l.tick(1, false).unwrap();
        assert!(l.abort());
        assert!(!l.sweeping());
        assert!(!l.abort());
        assert_eq!(l.tick(9 * RATE as i64, false), None);
        assert!(l.tick(2 * RATE as i64, false).is_some());
    }
}
