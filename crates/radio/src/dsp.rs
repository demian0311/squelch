//! Small DSP building blocks: windowed-sinc filters, decimators, a mixer.

use num_complex::Complex32;
use std::f32::consts::PI;

pub type C32 = Complex32;

/// Convert interleaved unsigned 8-bit I/Q to complex samples in -1..1.
pub fn u8_to_iq(raw: &[u8], out: &mut Vec<C32>) {
    out.clear();
    out.extend(raw.chunks_exact(2).map(|p| C32::new((p[0] as f32 - 127.4) / 128.0, (p[1] as f32 - 127.4) / 128.0)));
}

/// The share of a block's mean that moves the DC estimate: about ten blocks to settle.
const DC_TRACK: f32 = 0.1;

/// Takes the dongle's DC offset out of a capture. The chip's own spike sits at the capture
/// centre, 37–45 dB above the floor in direct sampling (2026-11-06, Nooelec v5), where the
/// tuner path's own blocking is bypassed. A slow mean of I and Q, subtracted per block.
#[derive(Default)]
pub struct DcRemove {
    dc: Option<C32>,
}

impl DcRemove {
    pub fn apply(&mut self, iq: &mut [C32]) {
        if iq.is_empty() {
            return;
        }
        let mean = iq.iter().sum::<C32>() / iq.len() as f32;
        let dc = match self.dc {
            None => mean,
            Some(dc) => dc + (mean - dc) * DC_TRACK,
        };
        self.dc = Some(dc);
        for s in iq.iter_mut() {
            *s -= dc;
        }
    }

    /// Forget the estimate: the next block sets it outright.
    pub fn reset(&mut self) {
        self.dc = None;
    }
}

/// Low-pass taps. `cutoff` and `transition` are fractions of the sample rate.
pub fn lowpass(cutoff: f32, transition: f32) -> Vec<f32> {
    // Hamming window: about 3.3 / transition taps for ~53 dB stopband.
    let mut n = (3.3 / transition).ceil() as usize;
    n = n.clamp(15, 1023) | 1;
    let m = (n - 1) as f32;
    let mut taps: Vec<f32> = (0..n)
        .map(|i| {
            let x = i as f32 - m / 2.0;
            let sinc = if x == 0.0 { 2.0 * cutoff } else { (2.0 * PI * cutoff * x).sin() / (PI * x) };
            let w = 0.54 - 0.46 * (2.0 * PI * i as f32 / m).cos();
            sinc * w
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    for t in taps.iter_mut() {
        *t /= sum;
    }
    taps
}

/// A sample the decimator can filter: one f32 lane for real signals, two for complex ones.
pub trait Sample: Copy + Default {
    const LANES: usize;
    /// Append the samples as lanes.
    fn extend(buf: &mut Vec<f32>, input: &[Self]);
    /// Rebuild a sample from the eight partial sums of `dot8`: lane k holds the terms at indices ≡ k (mod 8).
    fn from_lanes(acc: &[f32; 8]) -> Self;
}

impl Sample for f32 {
    const LANES: usize = 1;
    fn extend(buf: &mut Vec<f32>, input: &[Self]) {
        buf.extend_from_slice(input);
    }
    fn from_lanes(acc: &[f32; 8]) -> Self {
        acc.iter().sum()
    }
}

impl Sample for C32 {
    const LANES: usize = 2;
    fn extend(buf: &mut Vec<f32>, input: &[Self]) {
        // SAFETY: num_complex's Complex<T> is `#[repr(C)]` with fields re, im, so a slice of them is a slice of twice as many f32
        let lanes = unsafe { std::slice::from_raw_parts(input.as_ptr() as *const f32, 2 * input.len()) };
        buf.extend_from_slice(lanes);
    }
    fn from_lanes(acc: &[f32; 8]) -> Self {
        C32::new(acc[0] + acc[2] + acc[4] + acc[6], acc[1] + acc[3] + acc[5] + acc[7])
    }
}

/// Dot product kept in eight independent sums, so the compiler can vectorise it and the adds do not wait on each other.
/// Lane k sums the products at indices ≡ k (mod 8), which keeps interleaved re/im apart.
#[cfg(test)]
fn dot8(a: &[f32], b: &[f32], avx: bool) -> [f32; 8] {
    #[cfg(target_arch = "x86_64")]
    if avx {
        // SAFETY: `avx` is only true when `has_avx` saw AVX2 and FMA on this CPU
        return unsafe { dot8_avx(a, b) };
    }
    let _ = avx;
    dot8_plain(a, b)
}

/// Whether this CPU runs the AVX2 + FMA build of the kernel: one 256-bit fused multiply-add per eight taps.
fn has_avx() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    false
}

/// `a.len()` is a multiple of 8 here (the decimator pads its taps), so there is no scalar tail.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[inline]
unsafe fn dot8_avx(a: &[f32], b: &[f32]) -> [f32; 8] {
    use std::arch::x86_64::*;
    debug_assert_eq!(a.len(), b.len());
    let (pa, pb) = (a.as_ptr(), b.as_ptr());
    // four running sums so consecutive fused multiply-adds do not wait on each other
    let mut acc = [_mm256_setzero_ps(); 4];
    let mut i = 0;
    while i + 32 <= a.len() {
        for (k, acc) in acc.iter_mut().enumerate() {
            *acc = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i + 8 * k)), _mm256_loadu_ps(pb.add(i + 8 * k)), *acc);
        }
        i += 32;
    }
    while i + 8 <= a.len() {
        acc[0] = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)), acc[0]);
        i += 8;
    }
    let sum = _mm256_add_ps(_mm256_add_ps(acc[0], acc[1]), _mm256_add_ps(acc[2], acc[3]));
    let mut out = [0.0f32; 8];
    _mm256_storeu_ps(out.as_mut_ptr(), sum);
    for k in 0..a.len() - i {
        out[k] += a[i + k] * b[i + k];
    }
    out
}

/// `atan2` to about 1e-5 rad: a polynomial on one octant instead of libm's full-precision series. Four times faster, and
/// the discriminator and the pilot loop call it for every sample.
#[inline]
pub fn fast_atan2(y: f32, x: f32) -> f32 {
    let (ax, ay) = (x.abs(), y.abs());
    let swap = ay > ax;
    let (num, den) = if swap { (ax, ay) } else { (ay, ax) };
    let r = if den == 0.0 { 0.0 } else { num / den };
    let r2 = r * r;
    let mut a = (((((-0.011_721_2 * r2 + 0.052_653_32) * r2 - 0.116_432_87) * r2 + 0.193_543_46) * r2 - 0.332_623_47) * r2 + 0.999_977_26) * r;
    if swap {
        a = std::f32::consts::FRAC_PI_2 - a;
    }
    if x < 0.0 {
        a = PI - a;
    }
    if y < 0.0 {
        -a
    } else {
        a
    }
}

#[inline(always)]
fn dot8_plain(a: &[f32], b: &[f32]) -> [f32; 8] {
    debug_assert_eq!(a.len(), b.len());
    let mut acc = [0.0f32; 8];
    let n8 = a.len() / 8 * 8;
    let (a8, ar) = a.split_at(n8);
    let (b8, br) = b.split_at(n8);
    for (x, y) in a8.chunks_exact(8).zip(b8.chunks_exact(8)) {
        let x: &[f32; 8] = x.try_into().unwrap();
        let y: &[f32; 8] = y.try_into().unwrap();
        for k in 0..8 {
            acc[k] += x[k] * y[k];
        }
    }
    for (k, (x, y)) in ar.iter().zip(br).enumerate() {
        acc[k] += x * y;
    }
    acc
}

/// FIR filter that keeps one output for every `factor` inputs. The filter runs only on the kept samples, and a block
/// is copied once behind the `n - 1` samples before it, so the per-sample work is that copy.
pub struct Decimator<T> {
    /// each tap repeated `T::LANES` times, to line up with the interleaved samples, then zeros up to a multiple of
    /// eight lanes (a delay of a few samples, nothing more)
    taps: Vec<f32>,
    /// the last `n - 1` samples of the previous block, then the current block, as lanes
    buf: Vec<f32>,
    n: usize,
    factor: usize,
    /// inputs taken since the last kept one
    phase: usize,
    avx: bool,
    _t: std::marker::PhantomData<T>,
}

impl<T: Sample> Decimator<T> {
    pub fn new(taps: Vec<f32>, factor: usize) -> Self {
        let l = T::LANES;
        let n = (taps.len() * l).div_ceil(8) * 8 / l;
        let mut taps: Vec<f32> = taps.iter().flat_map(|&t| std::iter::repeat_n(t, l)).collect();
        taps.resize(n * l, 0.0);
        Decimator { taps, buf: vec![0.0; (n - 1) * l], n, factor, phase: 0, avx: has_avx(), _t: std::marker::PhantomData }
    }

    pub fn process(&mut self, input: &[T], out: &mut Vec<T>) {
        T::extend(&mut self.buf, input);
        self.filter(input.len(), out);
    }

    /// Filters the block of `len` samples just appended to `buf`, then keeps only what the next block's windows reach back to.
    fn filter(&mut self, len: usize, out: &mut Vec<T>) {
        let (n, l) = (self.n, T::LANES);
        // input sample p is kept when it is the `factor`-th since the last kept one; its window is the n samples ending at it
        let first = (self.factor - 1 - self.phase) % self.factor;
        #[cfg(target_arch = "x86_64")]
        if self.avx {
            // SAFETY: `avx` is only true when `has_avx` saw AVX2 and FMA on this CPU
            unsafe { self.block_avx(first, len, out) };
        } else {
            self.block_plain(first, len, out);
        }
        #[cfg(not(target_arch = "x86_64"))]
        self.block_plain(first, len, out);
        self.phase = (self.phase + len) % self.factor;
        let keep = (n - 1) * l;
        let total = self.buf.len();
        self.buf.copy_within(total - keep.., 0);
        self.buf.truncate(keep);
    }

    fn block_plain(&self, first: usize, len: usize, out: &mut Vec<T>) {
        let (n, l) = (self.n, T::LANES);
        for p in (first..len).step_by(self.factor) {
            // taps are symmetric, so oldest-to-newest order does not matter
            out.push(T::from_lanes(&dot8_plain(&self.buf[p * l..(p + n) * l], &self.taps)));
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn block_avx(&self, first: usize, len: usize, out: &mut Vec<T>) {
        let (n, l) = (self.n, T::LANES);
        for p in (first..len).step_by(self.factor) {
            out.push(T::from_lanes(&dot8_avx(&self.buf[p * l..(p + n) * l], &self.taps)));
        }
    }
}

impl Decimator<C32> {
    /// `process` of `input` after `mixer`, with the mixer writing straight into the filter's buffer: no copy of the
    /// block and one pass over it less, which is most of a narrow channel's cost at the capture rate.
    pub fn process_mixed(&mut self, input: &[C32], mixer: &mut Mixer, out: &mut Vec<C32>) {
        let at = self.buf.len();
        self.buf.resize(at + 2 * input.len(), 0.0);
        // SAFETY: as in `Sample::extend`, a `#[repr(C)]` Complex<f32> is two f32 lanes with the same 4-byte alignment,
        // and `buf[at..]` holds exactly `input.len()` of them
        let dst = unsafe { std::slice::from_raw_parts_mut(self.buf[at..].as_mut_ptr() as *mut C32, input.len()) };
        mixer.process_into(input, dst);
        self.filter(input.len(), out);
    }
}

/// Pass everything below `pass` Hz, stop everything above `stop` Hz, and decimate by `factor`, in two stages where that is cheaper:
/// a short filter takes most of the decimation, then a sharp one at the lower rate finishes it. One filter at the
/// input rate needs its full length for every kept sample; the short first stage needs only to keep out what would
/// alias onto `0..stop` after it, so its transition band can be wide.
pub struct Decimate<T> {
    a: Decimator<T>,
    b: Option<Decimator<T>>,
    mid: Vec<T>,
}

impl<T: Sample> Decimate<T> {
    pub fn new(rate: f32, pass: f32, stop: f32, factor: usize) -> Self {
        // `lowpass` puts its half-power point at the cutoff, so the band edges go either side of it
        let taps = |pass: f32, stop: f32, rate: f32| lowpass((pass + stop) / 2.0 / rate, (stop - pass) / rate);
        let single = Decimator::new(taps(pass, stop, rate), factor);
        if factor % 2 != 0 || factor < 4 {
            return Decimate { a: single, b: None, mid: Vec::new() };
        }
        let rate_a = rate / (factor / 2) as f32;
        let stop_a = rate_a - stop;
        if stop_a - pass <= stop - pass {
            return Decimate { a: single, b: None, mid: Vec::new() };
        }
        let a = Decimator::new(taps(pass, stop_a, rate), factor / 2);
        let b = Decimator::new(taps(pass, stop, rate_a), 2);
        Decimate { a, b: Some(b), mid: Vec::new() }
    }

    pub fn process(&mut self, input: &[T], out: &mut Vec<T>) {
        match &mut self.b {
            None => self.a.process(input, out),
            Some(b) => {
                self.mid.clear();
                self.a.process(input, &mut self.mid);
                b.process(&self.mid, out);
            }
        }
    }
}

impl Decimate<C32> {
    /// `process` of `input` after `mixer`, without a copy of the block (see `Decimator::process_mixed`).
    pub fn process_mixed(&mut self, input: &[C32], mixer: &mut Mixer, out: &mut Vec<C32>) {
        match &mut self.b {
            None => self.a.process_mixed(input, mixer, out),
            Some(b) => {
                self.mid.clear();
                self.a.process_mixed(input, mixer, &mut self.mid);
                b.process(&self.mid, out);
            }
        }
    }
}

/// Multiplies by e^{-j 2π f t}, moving `shift_hz` down to 0 Hz.
pub struct Mixer {
    phase: f64,
    step: f64,
    avx: bool,
}

impl Mixer {
    pub fn new(shift_hz: f64, rate: f64) -> Self {
        Mixer { phase: 0.0, step: -std::f64::consts::TAU * shift_hz / rate, avx: has_avx() }
    }

    pub fn set(&mut self, shift_hz: f64, rate: f64) {
        self.step = -std::f64::consts::TAU * shift_hz / rate;
    }

    pub fn process(&mut self, buf: &mut [C32]) {
        let (p, n) = (buf.as_mut_ptr(), buf.len());
        // SAFETY: one pointer for source and destination; each sample is read and then written, in turn
        unsafe { self.mix(p, p, n) };
    }

    /// `dst` becomes `src` mixed; the two must be the same length. Saves a copy when the mixed block lands in another buffer.
    pub fn process_into(&mut self, src: &[C32], dst: &mut [C32]) {
        assert_eq!(src.len(), dst.len());
        // SAFETY: both slices hold `len` samples
        unsafe { self.mix(src.as_ptr(), dst.as_mut_ptr(), src.len()) };
    }

    /// `len` samples at `src`, mixed, written to `dst`; the two may be the same place.
    unsafe fn mix(&mut self, src: *const C32, dst: *mut C32, len: usize) {
        #[cfg(target_arch = "x86_64")]
        if self.avx {
            // SAFETY: `avx` is only true when `has_avx` saw AVX2 and FMA on this CPU
            unsafe { self.mix_avx(src, dst, len) };
            return;
        }
        unsafe { self.mix_plain(src, dst, len) };
    }

    /// Four samples a vector: the oscillator for the first of them times a table of the next four steps, then the samples
    /// times that. Two complex multiplies a sample, but eight at a time and none waiting on the one before.
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn mix_avx(&mut self, src: *const C32, dst: *mut C32, len: usize) {
        use std::arch::x86_64::*;
        // (a + jb)(c + jd) for four pairs: re of a spread, times b; im of a spread, times b with re/im swapped; then add/sub
        #[inline]
        unsafe fn cmul(a: __m256, b: __m256) -> __m256 {
            let ar = _mm256_moveldup_ps(a);
            let ai = _mm256_movehdup_ps(a);
            let bs = _mm256_permute_ps(b, 0b10_11_00_01);
            _mm256_fmaddsub_ps(ar, b, _mm256_mul_ps(ai, bs))
        }
        let step = |k: f64| C32::new((k * self.step).cos() as f32, (k * self.step).sin() as f32);
        let table = [step(0.0), step(1.0), step(2.0), step(3.0)];
        let table = _mm256_loadu_ps(table.as_ptr() as *const f32);
        let rot4 = step(4.0);
        let mut osc = C32::new(self.phase.cos() as f32, self.phase.sin() as f32);
        let quads = len / 4;
        for k in 0..quads {
            let o = _mm256_set_ps(osc.im, osc.re, osc.im, osc.re, osc.im, osc.re, osc.im, osc.re);
            let s = src.add(4 * k) as *const f32;
            let d = dst.add(4 * k) as *mut f32;
            _mm256_storeu_ps(d, cmul(_mm256_loadu_ps(s), cmul(o, table)));
            osc *= rot4;
            if k % 256 == 255 {
                osc /= osc.norm();
            }
        }
        let rot = step(1.0);
        for i in 4 * quads..len {
            *dst.add(i) = *src.add(i) * osc;
            osc *= rot;
        }
        self.phase = (self.phase + self.step * len as f64) % std::f64::consts::TAU;
    }

    unsafe fn mix_plain(&mut self, src: *const C32, dst: *mut C32, len: usize) {
        // four oscillators a step apart, each advancing four steps, so the multiplies do not wait on one another
        let rot = C32::new(self.step.cos() as f32, self.step.sin() as f32);
        let rot4 = C32::new((4.0 * self.step).cos() as f32, (4.0 * self.step).sin() as f32);
        let o = C32::new(self.phase.cos() as f32, self.phase.sin() as f32);
        let mut osc = [o, o * rot, o * rot * rot, o * rot * rot * rot];
        let quads = len / 4;
        for k in 0..quads {
            for i in 0..4 {
                *dst.add(4 * k + i) = *src.add(4 * k + i) * osc[i];
                osc[i] *= rot4;
            }
            if k % 256 == 255 {
                for o in &mut osc {
                    *o /= o.norm();
                }
            }
        }
        let mut o = osc[0];
        for i in 4 * quads..len {
            *dst.add(i) = *src.add(i) * o;
            o *= rot;
        }
        self.phase = (self.phase + self.step * len as f64) % std::f64::consts::TAU;
    }
}

/// One-pole high-pass that removes DC from audio.
pub struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    pub fn new() -> Self {
        DcBlock { x1: 0.0, y1: 0.0 }
    }
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let y = *s - self.x1 + 0.995 * self.y1;
            self.x1 = *s;
            self.y1 = y;
            *s = y;
        }
    }
}

impl Default for DcBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Slow automatic gain control aiming for a fixed audio peak level.
pub struct Agc {
    gain: f32,
}

impl Agc {
    pub fn new() -> Self {
        Agc { gain: 1.0 }
    }
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let y = *s * self.gain;
            let a = y.abs();
            if a > 0.5 {
                self.gain *= 0.995;
            } else {
                self.gain *= 1.0001;
            }
            self.gain = self.gain.clamp(0.01, 5000.0);
            *s = y.clamp(-1.0, 1.0);
        }
    }
}

impl Default for Agc {
    fn default() -> Self {
        Self::new()
    }
}

/// Mean power of a block in dB (full scale = 0 dB).
pub fn power_db(buf: &[C32]) -> f32 {
    if buf.is_empty() {
        return -120.0;
    }
    let p: f32 = buf.iter().map(|c| c.norm_sqr()).sum::<f32>() / buf.len() as f32;
    10.0 * (p.max(1e-12)).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_remove_takes_the_offset_and_keeps_the_tone() {
        let mut dc = DcRemove::default();
        let tone = |i: usize| C32::from_polar(0.3, i as f32 * 0.2);
        let mut block: Vec<C32> = (0..4096).map(|i| tone(i) + C32::new(0.25, -0.1)).collect();
        dc.apply(&mut block);
        let mean = block.iter().sum::<C32>() / block.len() as f32;
        assert!(mean.norm() < 1e-3, "offset left: {mean}");
        let err = block.iter().enumerate().map(|(i, s)| (s - tone(i)).norm()).fold(0.0, f32::max);
        assert!(err < 0.02, "the tone changed by {err}");
        // a later block with the same offset settles further, never overshoots
        let mut again: Vec<C32> = (0..4096).map(|i| tone(i) + C32::new(0.25, -0.1)).collect();
        dc.apply(&mut again);
        let mean = again.iter().sum::<C32>() / again.len() as f32;
        assert!(mean.norm() < 1e-3, "offset left on the second block: {mean}");
    }

    #[test]
    fn lowpass_has_unity_dc_gain() {
        let t = lowpass(0.1, 0.05);
        assert!((t.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert_eq!(t.len() % 2, 1);
    }

    #[test]
    fn decimator_passes_low_and_blocks_high_tones() {
        let fs = 48_000.0;
        let tone = |f: f32| -> Vec<f32> { (0..9600).map(|n| (2.0 * PI * f * n as f32 / fs).sin()).collect() };
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let mut d = Decimator::<f32>::new(lowpass(2000.0 / fs, 1000.0 / fs), 4);
        let mut out = Vec::new();
        d.process(&tone(500.0), &mut out);
        assert!(rms(&out[200..]) > 0.65, "pass band attenuated: {}", rms(&out[200..]));
        let mut d = Decimator::<f32>::new(lowpass(2000.0 / fs, 1000.0 / fs), 4);
        out.clear();
        d.process(&tone(5000.0), &mut out);
        assert!(rms(&out[200..]) < 0.01, "stop band leaked: {}", rms(&out[200..]));
    }

    #[test]
    fn two_stage_decimation_matches_one_stage() {
        let fs = 2_400_000.0;
        let n = 240_000;
        let tone = |f: f32| -> Vec<C32> { (0..n).map(|k| C32::from_polar(1.0, 2.0 * PI * f * k as f32 / fs)).collect() };
        let rms = |v: &[C32]| (v.iter().map(|x| x.norm_sqr()).sum::<f32>() / v.len() as f32).sqrt();
        let mut two = Decimate::<C32>::new(fs, 100_000.0, 140_000.0, 10);
        assert!(two.b.is_some());
        let one = Decimate::<C32>::new(fs, 100_000.0, 140_000.0, 5);
        assert!(one.b.is_none());
        let mut out = Vec::new();
        two.process(&tone(80_000.0), &mut out);
        assert_eq!(out.len(), n / 10);
        assert!(rms(&out[2_000..]) > 0.95, "pass band attenuated: {}", rms(&out[2_000..]));
        // 400 kHz would alias onto 80 kHz after the first stage if that stage let it through
        for f in [150_000.0, 400_000.0, 1_000_000.0] {
            out.clear();
            two.process(&tone(f), &mut out);
            assert!(rms(&out[2_000..]) < 0.01, "{f} Hz leaked: {}", rms(&out[2_000..]));
        }
    }

    #[test]
    fn mixer_handles_a_length_that_is_not_a_multiple_of_four() {
        let fs = 240_000.0;
        let mut buf: Vec<C32> = (0..4803).map(|n| C32::from_polar(1.0, 2.0 * PI * 10_000.0 * n as f32 / fs)).collect();
        let mut m = Mixer::new(10_000.0, fs as f64);
        let (first, rest) = buf.split_at_mut(1001);
        m.process(first);
        m.process(rest);
        let mean = buf.iter().sum::<C32>() / buf.len() as f32;
        assert!(mean.norm() > 0.99, "{}", mean.norm());
    }

    #[test]
    fn both_dot_kernels_agree() {
        let a: Vec<f32> = (0..208).map(|i| (i as f32 * 0.37).sin()).collect();
        let b: Vec<f32> = (0..208).map(|i| (i as f32 * 0.11).cos()).collect();
        let plain = dot8(&a, &b, false);
        let fast = dot8(&a, &b, has_avx());
        for k in 0..8 {
            assert!((plain[k] - fast[k]).abs() < 1e-4, "lane {k}: {} vs {}", plain[k], fast[k]);
        }
    }

    #[test]
    fn fast_atan2_is_close_everywhere() {
        let mut worst = 0.0f32;
        for i in -40..=40 {
            for j in -40..=40 {
                let (y, x) = (i as f32 * 0.3, j as f32 * 0.3);
                worst = worst.max((fast_atan2(y, x) - y.atan2(x)).abs());
            }
        }
        assert!(worst < 2e-5, "worst error {worst}");
        assert_eq!(fast_atan2(0.0, 0.0), 0.0);
    }

    #[test]
    fn mixer_moves_a_tone_to_dc() {
        let fs = 240_000.0;
        let mut buf: Vec<C32> = (0..4800).map(|n| C32::from_polar(1.0, 2.0 * PI * 10_000.0 * n as f32 / fs)).collect();
        Mixer::new(10_000.0, fs as f64).process(&mut buf);
        let mean = buf.iter().sum::<C32>() / buf.len() as f32;
        assert!(mean.norm() > 0.99);
    }

    #[test]
    fn both_mixer_paths_agree() {
        let fs = 240_000.0;
        let tone: Vec<C32> = (0..4803).map(|n| C32::from_polar(0.7, 2.0 * PI * 31_000.0 * n as f32 / fs + 0.4)).collect();
        let mut fast = tone.clone();
        let mut plain = tone.clone();
        let mut m = Mixer::new(-12_345.0, fs as f64);
        m.process(&mut fast);
        let mut m = Mixer::new(-12_345.0, fs as f64);
        m.avx = false;
        m.process(&mut plain);
        let worst = fast.iter().zip(&plain).map(|(a, b)| (a - b).norm()).fold(0.0f32, f32::max);
        assert!(worst < 1e-4, "paths differ by {worst}");
    }

    #[test]
    fn mixing_into_the_decimator_matches_mixing_first() {
        let rate = 2_400_000.0;
        let mut rng = 0x9e3779b97f4a7c15u64;
        let mut noise = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 40) as f32 / (1u64 << 23) as f32 - 1.0
        };
        let blocks: Vec<Vec<C32>> = [4_099, 777, 65_536].iter().map(|&n| (0..n).map(|_| C32::new(noise(), noise())).collect()).collect();
        let (mut d1, mut d2) = (Decimate::<C32>::new(rate, 11_000.0, 14_000.0, 50), Decimate::<C32>::new(rate, 11_000.0, 14_000.0, 50));
        let (mut m1, mut m2) = (Mixer::new(-475_000.0, rate as f64), Mixer::new(-475_000.0, rate as f64));
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for block in &blocks {
            let mut mixed = block.clone();
            m1.process(&mut mixed);
            d1.process(&mixed, &mut a);
            d2.process_mixed(block, &mut m2, &mut b);
        }
        assert_eq!(a.len(), b.len());
        let worst = a.iter().zip(&b).map(|(x, y)| (x - y).norm()).fold(0.0f32, f32::max);
        assert!(a.len() > 1_000 && worst < 1e-5, "{} samples, paths differ by {worst}", a.len());
    }
}
