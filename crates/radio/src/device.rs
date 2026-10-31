//! IQ sources: the RTL-SDR dongle (through the system librtlsdr) and a
//! synthetic source used by tests and when no dongle is plugged in.

use anyhow::{bail, Result};
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
    /// Gains the tuner supports, in tenths of a dB.
    fn gains(&self) -> Vec<i32>;
    /// Fill `buf` with interleaved unsigned 8-bit I/Q. Returns bytes read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
}

#[allow(non_camel_case_types)]
type rtlsdr_dev_t = c_void;

extern "C" {
    fn rtlsdr_get_device_count() -> u32;
    fn rtlsdr_get_device_name(index: u32) -> *const c_char;
    fn rtlsdr_open(dev: *mut *mut rtlsdr_dev_t, index: u32) -> c_int;
    fn rtlsdr_close(dev: *mut rtlsdr_dev_t) -> c_int;
    fn rtlsdr_set_center_freq(dev: *mut rtlsdr_dev_t, freq: u32) -> c_int;
    fn rtlsdr_set_sample_rate(dev: *mut rtlsdr_dev_t, rate: u32) -> c_int;
    fn rtlsdr_get_tuner_gains(dev: *mut rtlsdr_dev_t, gains: *mut c_int) -> c_int;
    fn rtlsdr_set_tuner_gain_mode(dev: *mut rtlsdr_dev_t, manual: c_int) -> c_int;
    fn rtlsdr_set_tuner_gain(dev: *mut rtlsdr_dev_t, gain: c_int) -> c_int;
    fn rtlsdr_set_agc_mode(dev: *mut rtlsdr_dev_t, on: c_int) -> c_int;
    fn rtlsdr_reset_buffer(dev: *mut rtlsdr_dev_t) -> c_int;
    fn rtlsdr_read_sync(dev: *mut rtlsdr_dev_t, buf: *mut c_void, len: c_int, n_read: *mut c_int) -> c_int;
}

pub struct RtlSdr {
    dev: *mut rtlsdr_dev_t,
    name: String,
    rate: u32,
    gains: Vec<i32>,
}

// The device handle is only ever used from the one thread that owns the source.
unsafe impl Send for RtlSdr {}

impl RtlSdr {
    pub fn device_count() -> u32 {
        unsafe { rtlsdr_get_device_count() }
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
        let name = unsafe { CStr::from_ptr(rtlsdr_get_device_name(index)) }
            .to_string_lossy()
            .into_owned();
        let mut s = RtlSdr { dev, name, rate, gains: Vec::new() };
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
        Ok(s)
    }
}

impl Drop for RtlSdr {
    fn drop(&mut self) {
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
        if unsafe { rtlsdr_set_center_freq(self.dev, hz) } < 0 {
            bail!("dongle cannot tune to {hz} Hz");
        }
        Ok(())
    }
    fn set_gain(&mut self, gain: Gain) -> Result<()> {
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
        Ok(())
    }
    fn gains(&self) -> Vec<i32> {
        self.gains.clone()
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut n: c_int = 0;
        let r = unsafe { rtlsdr_read_sync(self.dev, buf.as_mut_ptr() as *mut c_void, buf.len() as c_int, &mut n) };
        if r < 0 {
            bail!("dongle read failed (error {r}); was it unplugged?");
        }
        Ok(n as usize)
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
    phases: Vec<f64>,
    rng: u64,
    realtime: bool,
    started: Option<std::time::Instant>,
    produced: u64,
}

impl SimSource {
    pub fn new(rate: u32, signals: Vec<SimSignal>, realtime: bool) -> Self {
        let n = signals.len();
        SimSource { rate, center: 100e6, signals, t: 0, phases: vec![0.0; n], rng: 0x9e3779b97f4a7c15, realtime, started: None, produced: 0 }
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
    fn gains(&self) -> Vec<i32> {
        vec![0, 100, 200, 300, 400, 496]
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let fs = self.rate as f64;
        let tau = std::f64::consts::TAU;
        for chunk in buf.chunks_exact_mut(2) {
            let tsec = self.t as f64 / fs;
            let mut i = 0.02 * self.noise();
            let mut q = 0.02 * self.noise();
            for (k, s) in self.signals.iter().enumerate() {
                let off = s.hz - self.center;
                if off.abs() > fs / 2.0 {
                    continue;
                }
                let m = match &s.symbols {
                    Some(sym) => if sym[(tsec * s.tone_hz) as usize % sym.len()] { 1.0 } else { -1.0 },
                    None => (tau * s.tone_hz * tsec).sin(),
                };
                // instantaneous frequency = off + dev * m
                self.phases[k] += tau * (off + s.fm_dev * m) / fs;
                let a = s.amp * (1.0 + s.am_depth * m as f32) / (1.0 + s.am_depth);
                i += a * self.phases[k].cos() as f32;
                q += a * self.phases[k].sin() as f32;
            }
            self.t += 1;
            chunk[0] = (i * 127.5 + 127.5).clamp(0.0, 255.0) as u8;
            chunk[1] = (q * 127.5 + 127.5).clamp(0.0, 255.0) as u8;
        }
        for p in self.phases.iter_mut() {
            *p %= tau;
        }
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
