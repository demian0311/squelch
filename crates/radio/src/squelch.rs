//! Squelch: silences the audio while the tuned channel is weak.

/// Stays open this long after the signal drops, so pauses between words aren't cut and a signal at the mark doesn't chatter.
pub const HANG_SECS: f32 = 0.6;

pub struct Squelch {
    /// Opening level in dB above the noise floor; `None` means always open.
    threshold: Option<f32>,
    open: bool,
    below_for: f32,
    /// Gain at the end of the last `apply`, so open and close fade instead of clicking.
    gain: f32,
}

impl Default for Squelch {
    fn default() -> Self {
        Self::new()
    }
}

impl Squelch {
    pub fn new() -> Self {
        Self { threshold: None, open: true, below_for: 0.0, gain: 1.0 }
    }

    pub fn set(&mut self, threshold: Option<f32>) {
        self.threshold = threshold;
        if threshold.is_none() {
            self.open = true;
        }
    }

    /// Start closed on a new frequency, so the old station's level can't hold it open.
    pub fn reset(&mut self) {
        self.open = self.threshold.is_none();
        self.below_for = 0.0;
    }

    /// Feed the channel level (dB above the floor) measured over the last `dt` seconds.
    pub fn update(&mut self, level_db: f32, dt: f32) {
        let Some(t) = self.threshold else {
            self.open = true;
            return;
        };
        if level_db >= t {
            self.open = true;
            self.below_for = 0.0;
        } else if self.open {
            self.below_for += dt;
            if self.below_for >= HANG_SECS {
                self.open = false;
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Scale `pcm` by the squelch, ramping from the previous gain across the block.
    pub fn apply(&mut self, pcm: &mut [f32]) {
        let target = if self.open { 1.0 } else { 0.0 };
        if self.gain == target {
            if target == 0.0 {
                pcm.fill(0.0);
            }
            return;
        }
        let n = pcm.len().max(1) as f32;
        for (i, s) in pcm.iter_mut().enumerate() {
            *s *= self.gain + (target - self.gain) * (i + 1) as f32 / n;
        }
        self.gain = target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 30.0;

    fn run(q: &mut Squelch, level: f32, secs: f32) {
        for _ in 0..(secs / DT).round() as usize {
            q.update(level, DT);
        }
    }

    #[test]
    fn off_is_always_open() {
        let mut q = Squelch::new();
        run(&mut q, -20.0, 2.0);
        assert!(q.is_open());
    }

    #[test]
    fn opens_on_a_signal_and_closes_after_the_hang() {
        let mut q = Squelch::new();
        q.set(Some(10.0));
        q.reset();
        assert!(!q.is_open());
        run(&mut q, 15.0, 0.1);
        assert!(q.is_open());
        run(&mut q, 2.0, HANG_SECS / 2.0);
        assert!(q.is_open(), "short pauses stay open");
        run(&mut q, 2.0, HANG_SECS);
        assert!(!q.is_open());
    }

    #[test]
    fn just_below_the_mark_closes_after_the_hang() {
        let mut q = Squelch::new();
        q.set(Some(20.0));
        run(&mut q, 18.0, HANG_SECS + 0.1);
        assert!(!q.is_open(), "the meter reads below the mark, so it must be muted");
    }

    #[test]
    fn dips_shorter_than_the_hang_do_not_chatter() {
        let mut q = Squelch::new();
        q.set(Some(10.0));
        for _ in 0..10 {
            run(&mut q, 12.0, 0.1);
            run(&mut q, 8.0, HANG_SECS / 2.0);
            assert!(q.is_open());
        }
    }

    #[test]
    fn fades_instead_of_clicking() {
        let mut q = Squelch::new();
        q.set(Some(10.0));
        q.reset();
        let mut pcm = vec![1.0f32; 100];
        q.apply(&mut pcm);
        assert!(pcm[0] > 0.95 && pcm[99] == 0.0, "closing fades out");
        let mut pcm = vec![1.0f32; 100];
        q.apply(&mut pcm);
        assert!(pcm.iter().all(|&s| s == 0.0));
        run(&mut q, 20.0, 0.1);
        let mut pcm = vec![1.0f32; 100];
        q.apply(&mut pcm);
        assert!(pcm[0] < 0.05 && pcm[99] == 1.0 && pcm.windows(2).all(|w| w[0] <= w[1]));
    }
}
