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

pub struct RtlSdr {
    dev: *mut rtlsdr_dev_t,
    name: String,
    rate: u32,
    gains: Vec<i32>,
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
        let (tx, rx) = std::sync::mpsc::sync_channel(QUEUE);
        let mut s = RtlSdr { dev, name, rate, gains: Vec::new(), rx, stream: None, usb: None, pending: Vec::new(), taken: 0 };
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
        if unsafe { rtlsdr_set_center_freq(self.dev, hz) } < 0 {
            bail!("dongle cannot tune to {hz} Hz");
        }
        // samples already queued were taken at the old frequency
        while self.rx.try_recv().is_ok() {}
        self.taken = self.pending.len();
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
