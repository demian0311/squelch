//! IQ sources: the RTL-SDR dongle (through the system librtlsdr) and a
//! synthetic source used by tests and when no dongle is plugged in.

use anyhow::{bail, Result};
use crate::dsp::C32;
use num_complex::Complex;
use std::ffi::{c_char, c_int, c_void, CStr};

/// Tuner gain: hardware AGC, or a fixed value in tenths of a dB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gain {
    Auto,
    Tenths(i32),
}

pub trait IqSource: Send {
    fn name(&self) -> String;
    fn sample_rate(&self) -> u32;
    fn set_center(&mut self, hz: u32) -> Result<()>;
    fn set_gain(&mut self, gain: Gain) -> Result<()>;
    /// Crystal correction in librtlsdr's sense: the crystal runs `ppm` fast. Retunes to correct.
    fn set_ppm(&mut self, ppm: i32) -> Result<()>;
    /// Gains the tuner supports, in tenths of a dB.
    fn gains(&self) -> Vec<i32>;
    /// True while the tuner is bypassed and the antenna is sampled straight (below `DIRECT_BELOW_HZ`).
    fn direct(&self) -> bool {
        false
    }
    /// Fill `buf` with interleaved unsigned 8-bit I/Q. Returns bytes read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
}

#[allow(non_camel_case_types)]
type rtlsdr_dev_t = c_void;

extern "C" {
    fn rtlsdr_get_device_count() -> u32;
    fn rtlsdr_get_device_name(index: u32) -> *const c_char;
    fn rtlsdr_get_device_usb_strings(index: u32, manufact: *mut c_char, product: *mut c_char, serial: *mut c_char) -> c_int;
    fn rtlsdr_open(dev: *mut *mut rtlsdr_dev_t, index: u32) -> c_int;
    fn rtlsdr_close(dev: *mut rtlsdr_dev_t) -> c_int;
    fn rtlsdr_set_center_freq(dev: *mut rtlsdr_dev_t, freq: u32) -> c_int;
    fn rtlsdr_set_freq_correction(dev: *mut rtlsdr_dev_t, ppm: c_int) -> c_int;
    fn rtlsdr_set_sample_rate(dev: *mut rtlsdr_dev_t, rate: u32) -> c_int;
    fn rtlsdr_set_direct_sampling(dev: *mut rtlsdr_dev_t, on: c_int) -> c_int;
    fn rtlsdr_get_tuner_gains(dev: *mut rtlsdr_dev_t, gains: *mut c_int) -> c_int;
    fn rtlsdr_set_tuner_gain_mode(dev: *mut rtlsdr_dev_t, manual: c_int) -> c_int;
    fn rtlsdr_set_tuner_gain(dev: *mut rtlsdr_dev_t, gain: c_int) -> c_int;
    fn rtlsdr_set_agc_mode(dev: *mut rtlsdr_dev_t, on: c_int) -> c_int;
    fn rtlsdr_reset_buffer(dev: *mut rtlsdr_dev_t) -> c_int;
    fn rtlsdr_read_async(dev: *mut rtlsdr_dev_t, cb: extern "C" fn(*mut u8, u32, *mut c_void), ctx: *mut c_void, buf_num: u32, buf_len: u32) -> c_int;
    fn rtlsdr_cancel_async(dev: *mut rtlsdr_dev_t) -> c_int;
}

/// USB transfers librtlsdr keeps queued, and their size. Streaming keeps them all in flight,
/// so the dongle never waits on us; blocking reads left gaps that dropped samples.
const USB_BUFFERS: u32 = 12;
const USB_BUFFER_BYTES: u32 = 262_144;
/// Buffers held between the USB thread and the reader: about 1.6 s at 2.4 Msps.
const QUEUE: usize = 16;

struct Stream {
    tx: std::sync::mpsc::SyncSender<Vec<u8>>,
    dropped: std::sync::atomic::AtomicU64,
}

extern "C" fn on_samples(buf: *mut u8, len: u32, ctx: *mut c_void) {
    let stream = unsafe { &*(ctx as *const Stream) };
    let data = unsafe { std::slice::from_raw_parts(buf, len as usize) }.to_vec();
    if stream.tx.try_send(data).is_err() {
        stream.dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

struct DevPtr(*mut rtlsdr_dev_t);
// librtlsdr allows control calls from other threads while one thread streams.
unsafe impl Send for DevPtr {}

/// Below this the tuner cannot reach, so the dongle samples the antenna straight (direct
/// sampling): the RTL2832U's own down-converter tunes 0–`DIRECT_TOP_HZ` and the band above that
/// folds back under it. The R820T's own floor is 24 MHz.
pub const DIRECT_BELOW_HZ: u32 = 24_000_000;
/// Half the chip's 28.8 MHz clock: the top of what direct sampling reads true.
pub const DIRECT_TOP_HZ: u32 = 14_400_000;
/// The input wired to the antenna for HF on the RTL-SDR v3 and the Nooelec NESDR SMArt v5
/// (checked 2026-11-06 on the v5: the I branch hears only clock spurs).
const HF_BRANCH: u32 = 2;

pub struct RtlSdr {
    dev: *mut rtlsdr_dev_t,
    name: String,
    rate: u32,
    gains: Vec<i32>,
    /// Tuner bypassed, antenna sampled straight.
    direct: bool,
    /// The gain asked for, re-applied when the tuner comes back after direct sampling.
    gain: Gain,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    stream: Option<Box<Stream>>,
    usb: Option<std::thread::JoinHandle<()>>,
    pending: Vec<u8>,
    taken: usize,
}

// The handle is used by the owning thread for control and by the USB thread for streaming.
unsafe impl Send for RtlSdr {}

impl RtlSdr {
    /// USB buffers lost because the reader fell more than `QUEUE` buffers behind.
    pub fn dropped(&self) -> u64 {
        self.stream.as_ref().map_or(0, |s| s.dropped.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn device_count() -> u32 {
        unsafe { rtlsdr_get_device_count() }
    }

    /// What the dongle says about itself over USB: maker, product and serial.
    /// librtlsdr's own name comes from a table of USB ids, where every maker that kept
    /// Realtek's id (Nooelec among them) reads "Generic RTL2832U OEM".
    /// `None` when the strings cannot be read or say nothing.
    fn usb_name(index: u32) -> Option<String> {
        // librtlsdr writes at most 256 bytes into each, NUL-terminated
        let mut maker = [0 as c_char; 256];
        let mut product = [0 as c_char; 256];
        let mut serial = [0 as c_char; 256];
        let r = unsafe { rtlsdr_get_device_usb_strings(index, maker.as_mut_ptr(), product.as_mut_ptr(), serial.as_mut_ptr()) };
        if r != 0 {
            return None;
        }
        let read = |b: &[c_char]| unsafe { CStr::from_ptr(b.as_ptr()) }.to_string_lossy().trim().to_string();
        Some(usb_name(&read(&maker), &read(&product), &read(&serial)))
            .filter(|n| !n.is_empty())
    }

    /// Direct sampling: the tuner is bypassed and the RTL2832U samples the antenna at its
    /// 28.8 MHz clock, so `set_center` moves the chip's own down-converter over 0–14.4 MHz
    /// (above that the band folds back). `branch` 1 = I, 2 = Q (the RTL-SDR v3 and the
    /// Nooelec v5 wire HF to Q), 0 = back to the tuner. No tuner gain in this mode.
    pub fn set_direct_sampling(&mut self, branch: u32) -> Result<()> {
        if unsafe { rtlsdr_set_direct_sampling(self.dev, branch as c_int) } < 0 {
            bail!("dongle refused direct sampling {branch}");
        }
        self.direct = branch != 0;
        if !self.direct {
            // librtlsdr re-initialises the tuner on the way back; its gain is whatever that left
            self.apply_gain(self.gain);
        }
        while self.rx.try_recv().is_ok() {}
        self.taken = self.pending.len();
        Ok(())
    }

    fn apply_gain(&self, gain: Gain) {
        unsafe {
            match gain {
                Gain::Auto => {
                    rtlsdr_set_tuner_gain_mode(self.dev, 0);
                    rtlsdr_set_agc_mode(self.dev, 0);
                }
                Gain::Tenths(g) => {
                    rtlsdr_set_tuner_gain_mode(self.dev, 1);
                    rtlsdr_set_tuner_gain(self.dev, g);
                }
            }
        }
    }

    pub fn open(index: u32, rate: u32) -> Result<Self> {
        if Self::device_count() <= index {
            bail!("no RTL-SDR dongle found");
        }
        let mut dev: *mut rtlsdr_dev_t = std::ptr::null_mut();
        let r = unsafe { rtlsdr_open(&mut dev, index) };
        if r < 0 || dev.is_null() {
            bail!("could not open the dongle (error {r}); is another app using it?");
        }
        let name = Self::usb_name(index).unwrap_or_else(|| {
            unsafe { CStr::from_ptr(rtlsdr_get_device_name(index)) }
                .to_string_lossy()
                .into_owned()
        });
        let (tx, rx) = std::sync::mpsc::sync_channel(QUEUE);
        let mut s = RtlSdr { dev, name, rate, gains: Vec::new(), direct: false, gain: Gain::Auto, rx, stream: None, usb: None, pending: Vec::new(), taken: 0 };
        unsafe {
            if rtlsdr_set_sample_rate(dev, rate) < 0 {
                bail!("dongle refused sample rate {rate}");
            }
            let n = rtlsdr_get_tuner_gains(dev, std::ptr::null_mut());
            if n > 0 {
                let mut g = vec![0 as c_int; n as usize];
                rtlsdr_get_tuner_gains(dev, g.as_mut_ptr());
                s.gains = g.into_iter().map(|v| v as i32).collect();
            }
            rtlsdr_reset_buffer(dev);
        }
        s.set_gain(Gain::Auto)?;
        let stream = Box::new(Stream { tx, dropped: std::sync::atomic::AtomicU64::new(0) });
        let ctx = &*stream as *const Stream as usize;
        let d = DevPtr(dev);
        s.stream = Some(stream);
        s.usb = Some(std::thread::spawn(move || {
            let d = d;
            unsafe { rtlsdr_read_async(d.0, on_samples, ctx as *mut c_void, USB_BUFFERS, USB_BUFFER_BYTES) };
        }));
        Ok(s)
    }
}

impl Drop for RtlSdr {
    fn drop(&mut self) {
        unsafe {
            rtlsdr_cancel_async(self.dev);
        }
        if let Some(t) = self.usb.take() {
            let _ = t.join();
        }
        unsafe {
            rtlsdr_close(self.dev);
        }
    }
}

impl IqSource for RtlSdr {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn set_center(&mut self, hz: u32) -> Result<()> {
        // the tuner stops at 24 MHz; below it the antenna is sampled straight, and the
        // chip's own down-converter takes the same centre call
        let want = hz < DIRECT_BELOW_HZ;
        if want != self.direct {
            self.set_direct_sampling(if want { HF_BRANCH } else { 0 })?;
        }
        if unsafe { rtlsdr_set_center_freq(self.dev, hz) } < 0 {
            bail!("dongle cannot tune to {hz} Hz");
        }
        // samples already queued were taken at the old frequency
        while self.rx.try_recv().is_ok() {}
        self.taken = self.pending.len();
        Ok(())
    }
    fn set_ppm(&mut self, ppm: i32) -> Result<()> {
        // -2 means it already had this value
        let r = unsafe { rtlsdr_set_freq_correction(self.dev, ppm) };
        if r < 0 && r != -2 {
            bail!("dongle refused a {ppm} ppm correction");
        }
        while self.rx.try_recv().is_ok() {}
        self.taken = self.pending.len();
        Ok(())
    }
    fn set_gain(&mut self, gain: Gain) -> Result<()> {
        self.gain = gain;
        // no tuner in the path while direct sampling: the antenna sets the level
        if !self.direct {
            self.apply_gain(gain);
        }
        Ok(())
    }
    fn gains(&self) -> Vec<i32> {
        self.gains.clone()
    }
    fn direct(&self) -> bool {
        self.direct
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut filled = 0;
        while filled < buf.len() {
            if self.taken == self.pending.len() {
                self.pending = match self.rx.recv_timeout(std::time::Duration::from_secs(2)) {
                    Ok(v) => v,
                    Err(_) => bail!("the dongle stopped sending samples; was it unplugged?"),
                };
                self.taken = 0;
            }
            let n = (buf.len() - filled).min(self.pending.len() - self.taken);
            buf[filled..filled + n].copy_from_slice(&self.pending[self.taken..self.taken + n]);
            filled += n;
            self.taken += n;
        }
        Ok(filled)
    }
}

/// A carrier the synthetic source transmits, at an absolute frequency.
#[derive(Clone, Debug)]
pub struct SimSignal {
    pub hz: f64,
    /// Peak amplitude, 0..1 of full scale.
    pub amp: f32,
    /// FM deviation in Hz; 0 for a plain carrier.
    pub fm_dev: f64,
    /// AM modulation depth 0..1.
    pub am_depth: f32,
    /// Tone frequency used for the modulation.
    pub tone_hz: f64,
    /// Data symbols sent instead of the tone, looping, at `tone_hz` baud (FSK of ±`fm_dev`).
    pub symbols: Option<std::sync::Arc<[bool]>>,
}

/// Generates noise plus the given signals, paced to real time when `realtime` is set.
pub struct SimSource {
    rate: u32,
    center: f64,
    signals: Vec<SimSignal>,
    t: u64,
    /// Each signal's oscillator, carrier and modulation together.
    phasors: Vec<Complex<f64>>,
    /// The signals summed for the block in hand, before noise.
    sum: Vec<C32>,
    rng: u64,
    realtime: bool,
    started: Option<std::time::Instant>,
    produced: u64,
}

impl SimSource {
    pub fn new(rate: u32, signals: Vec<SimSignal>, realtime: bool) -> Self {
        let n = signals.len();
        SimSource { rate, center: 100e6, signals, t: 0, phasors: vec![Complex::new(1.0, 0.0); n], sum: Vec::new(), rng: 0x9e3779b97f4a7c15, realtime, started: None, produced: 0 }
    }

    fn noise(&mut self) -> f32 {
        // xorshift, roughly uniform in -1..1
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
}

impl IqSource for SimSource {
    fn name(&self) -> String {
        "Simulated receiver".into()
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn set_center(&mut self, hz: u32) -> Result<()> {
        self.center = hz as f64;
        Ok(())
    }
    fn set_gain(&mut self, _gain: Gain) -> Result<()> {
        Ok(())
    }
    fn set_ppm(&mut self, _ppm: i32) -> Result<()> {
        Ok(())
    }
    fn gains(&self) -> Vec<i32> {
        vec![0, 100, 200, 300, 400, 496]
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let fs = self.rate as f64;
        let tau = std::f64::consts::TAU;
        let n = buf.len() / 2;
        self.sum.clear();
        self.sum.resize(n, C32::default());
        for (k, s) in self.signals.iter().enumerate() {
            let off = s.hz - self.center;
            if off.abs() > fs / 2.0 {
                continue;
            }
            // Oscillators turn by a complex multiply a sample, not a sine and cosine of an angle: the carrier by a fixed
            // step, the tone likewise, and the modulation by a small angle whose sine and cosine come from short series.
            // Nothing drifts: the tone restarts from the clock each block, the carrier is normalised after it.
            let carrier = Complex::from_polar(1.0, tau * off / fs);
            let tone = Complex::from_polar(1.0, tau * s.tone_hz / fs);
            let mut tone_ph = Complex::from_polar(1.0, tau * s.tone_hz * (self.t as f64 / fs));
            let mut ph = self.phasors[k];
            let dev = tau * s.fm_dev / fs;
            let gain = s.amp / (1.0 + s.am_depth);
            let per_sample = s.tone_hz / fs;
            for (j, acc) in self.sum.iter_mut().enumerate() {
                let m = match &s.symbols {
                    Some(sym) => {
                        if sym[((self.t + j as u64) as f64 * per_sample) as usize % sym.len()] {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    None => {
                        tone_ph *= tone;
                        tone_ph.im
                    }
                };
                // instantaneous frequency = off + dev * m
                let x = dev * m;
                let x2 = x * x;
                let sin = x * (1.0 - x2 * (1.0 / 6.0 - x2 * (1.0 / 120.0 - x2 / 5040.0)));
                let cos = 1.0 - x2 * (0.5 - x2 * (1.0 / 24.0 - x2 * (1.0 / 720.0 - x2 / 40320.0)));
                ph *= carrier * Complex::new(cos, sin);
                let a = gain * (1.0 + s.am_depth * m as f32);
                acc.re += a * ph.re as f32;
                acc.im += a * ph.im as f32;
            }
            self.phasors[k] = ph / ph.norm();
        }
        let sum = std::mem::take(&mut self.sum);
        for (chunk, s) in buf.chunks_exact_mut(2).zip(&sum) {
            let i = s.re + 0.02 * self.noise();
            let q = s.im + 0.02 * self.noise();
            chunk[0] = (i * 127.5 + 127.5).clamp(0.0, 255.0) as u8;
            chunk[1] = (q * 127.5 + 127.5).clamp(0.0, 255.0) as u8;
        }
        self.sum = sum;
        self.t += n as u64;
        if self.realtime {
            let start = *self.started.get_or_insert_with(std::time::Instant::now);
            self.produced += (buf.len() / 2) as u64;
            let due = std::time::Duration::from_secs_f64(self.produced as f64 / fs);
            let el = start.elapsed();
            if due > el {
                std::thread::sleep(due - el);
            }
        }
        Ok(buf.len())
    }
}


/// One line from the USB strings: the product, led by the maker unless the product
/// already names it, then the serial. Empty when the dongle says nothing.
pub fn usb_name(maker: &str, product: &str, serial: &str) -> String {
    let mut name = if product.is_empty() {
        maker.to_string()
    } else if maker.is_empty() || product.to_lowercase().contains(&maker.to_lowercase()) {
        product.to_string()
    } else {
        format!("{maker} {product}")
    };
    if !serial.is_empty() && !name.is_empty() {
        name.push_str(&format!(" · serial {serial}"));
    }
    name
}

#[cfg(test)]
mod usb_name_tests {
    use super::usb_name;

    #[test]
    fn nooelec_v5_reads_maker_product_and_serial() {
        assert_eq!(usb_name("Nooelec", "NESDR SMArt v5", "28058249"), "Nooelec NESDR SMArt v5 · serial 28058249");
    }

    #[test]
    fn a_maker_already_in_the_product_is_not_repeated() {
        assert_eq!(usb_name("Realtek", "Realtek RTL2838UHIDIR", "00000001"), "Realtek RTL2838UHIDIR · serial 00000001");
    }

    #[test]
    fn silent_strings_give_nothing_so_the_table_name_is_used() {
        assert_eq!(usb_name("", "", "00000001"), "");
        assert_eq!(usb_name("", "RTL2838", ""), "RTL2838");
    }
}
