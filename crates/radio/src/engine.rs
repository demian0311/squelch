//! The receive loop: reads the dongle, produces spectrum rows, finds signals and plays audio.

use crate::audio::AudioOut;
use crate::demod::{Demodulator, Mode};
use crate::detect::{detect, Detected};
use crate::device::{Gain, IqSource, RtlSdr, SimSignal, SimSource};
use crate::dsp::{u8_to_iq, Mixer, C32};
use crate::hd::{Hd, HdImage, HdInfo};
use crate::p25::P25;
use crate::spectrum::{noise_floor, Spectrum};
use crate::sonde::{all_decoders, Scanner, Sonde};
use crate::squelch::Squelch;
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const SAMPLE_RATE: u32 = 2_400_000;
pub const FFT_SIZE: usize = 2048;
pub const ROWS_PER_SEC: u32 = 30;
/// Spectrum rows are sent as bytes: 0 is `floor - BELOW_FLOOR_DB`, 255 is `RANGE_DB` above that.
pub const BELOW_FLOOR_DB: f32 = 8.0;
pub const RANGE_DB: f32 = 60.0;
/// Signals must stand this far above the noise floor to be reported.
pub const DETECT_DB: f32 = 8.0;
const READ_BYTES: usize = 131_072;
/// Seconds of a strong FM station that make one crystal measurement.
const MEASURE_SECS: f64 = 5.0;
/// An FM station must be this far above the floor to measure the crystal on.
const MEASURE_SNR_DB: f32 = 20.0;
/// Broadcast FM channel spacing; a station tuned on this grid is a known frequency.
const FM_GRID_HZ: f64 = 100_000.0;

#[derive(Debug, Clone)]
pub enum Command {
    /// Hardware centre and the frequency being listened to, both in Hz.
    Tune { center_hz: f64, tuned_hz: f64 },
    SetMode(Mode),
    SetGain(Gain),
    SetVolume(f32),
    SetPlaying(bool),
    /// Mute below this many dB above the floor; `None` turns squelch off.
    SetSquelch(Option<f32>),
    /// Run the radiosonde decoders on every narrow signal in the capture.
    SetSondes(bool),
    /// Decode HD Radio on the tuned FM station: `Some(0)` is HD1, `Some(1)` HD2, `None` off.
    SetHd(Option<u32>),
    /// Crystal correction, librtlsdr's sign: positive means the crystal runs fast.
    SetPpm(i32),
    Stop,
}

/// The dongle's crystal error as last measured, in `Command::SetPpm`'s sign.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Crystal {
    pub ppm: f64,
    /// The known frequency it was measured on: an FM station or a P25 control channel.
    pub hz: f64,
}

pub struct Row {
    pub center_hz: f64,
    pub bins: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub device: String,
    pub simulated: bool,
    pub error: Option<String>,
    pub sample_rate: u32,
    pub center_hz: f64,
    pub tuned_hz: f64,
    pub mode: Mode,
    pub playing: bool,
    pub gains: Vec<i32>,
    pub floor_db: f32,
    /// Strongest bin inside the tuned channel, dB above the floor, lightly smoothed.
    pub channel_snr_db: f32,
    pub squelch_db: Option<f32>,
    /// False while the squelch is muting the audio.
    pub squelch_open: bool,
    /// Station data of the tuned FM station.
    pub rds: Option<crate::rds::Info>,
    /// Weather balloons heard recently, while sonde decoding is on.
    pub sondes: Vec<Sonde>,
    /// Frequencies the sonde decoders are listening to.
    pub decoding_hz: Vec<f64>,
    pub signals: Vec<Detected>,
    /// The tuned FM station shows HD Radio sidebands.
    pub hd_sidebands: bool,
    /// HD Radio decoding of the tuned station, while it is on.
    pub hd: Option<HdInfo>,
    /// The HD pictures, sent only when they change.
    pub hd_images: Option<Vec<HdImage>>,
    /// Trunked radio state, in P25 mode.
    pub p25: Option<crate::p25::Info>,
    /// Correction applied now.
    pub ppm: i32,
    pub crystal: Option<Crystal>,
}

pub struct Engine {
    tx: Sender<Command>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    /// Open the first dongle, or a simulated receiver if none answers or `SQUELCH_SIM` is set.
    pub fn open_source() -> (Box<dyn IqSource>, Option<String>) {
        if std::env::var_os("SQUELCH_SIM").is_none() {
            match RtlSdr::open(0, SAMPLE_RATE) {
                Ok(d) => return (Box::new(d), None),
                Err(e) => return (Box::new(SimSource::new(SAMPLE_RATE, demo_signals(), true)), Some(e.to_string())),
            }
        }
        (Box::new(SimSource::new(SAMPLE_RATE, demo_signals(), true)), None)
    }

    pub fn start(
        source: Box<dyn IqSource>,
        simulated_because: Option<String>,
        on_row: impl Fn(Row) + Send + 'static,
        on_status: impl Fn(Status) + Send + 'static,
    ) -> Self {
        let (tx, rx) = channel();
        let handle = std::thread::Builder::new()
            .name("radio".into())
            .spawn(move || run(source, simulated_because, rx, on_row, on_status))
            .expect("spawn radio thread");
        Engine { tx, handle: Some(handle) }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(
    mut src: Box<dyn IqSource>,
    simulated_because: Option<String>,
    rx: Receiver<Command>,
    on_row: impl Fn(Row),
    on_status: impl Fn(Status),
) {
    let rate = src.sample_rate();
    let simulated = src.name() == "Simulated receiver";
    let device = match (&simulated_because, simulated) {
        (Some(why), _) => format!("Simulated receiver ({why})"),
        _ => src.name(),
    };
    let mut center = 101_100_000.0f64 + 300_000.0;
    let mut tuned = 101_100_000.0f64;
    let mut mode = Mode::Wfm;
    let mut playing = true;
    let mut volume = 0.7f32;
    let mut ppm = 0i32;
    let mut crystal: Option<Crystal> = None;
    let mut error: Option<String> = None;
    let _ = src.set_center(center as u32);

    let audio = match AudioOut::open() {
        Ok(a) => Some(a),
        Err(e) => {
            error = Some(format!("no audio: {e}"));
            None
        }
    };
    let mut demod = Demodulator::new(mode, rate);
    let mut mixer = Mixer::new(tuned - center, rate as f64);
    let mut spectrum = Spectrum::new(FFT_SIZE, rate, ROWS_PER_SEC);
    let mut raw = vec![0u8; READ_BYTES];
    let mut iq: Vec<C32> = Vec::new();
    let mut chan: Vec<C32> = Vec::new();
    let mut pcm: Vec<f32> = Vec::new();
    let mut rows: Vec<Vec<f32>> = Vec::new();
    let mut floor = -60.0f32;
    let mut recent: Vec<Vec<f32>> = Vec::new();
    let mut signals: Vec<Detected> = Vec::new();
    let mut snr = 0.0f32;
    let mut squelch = Squelch::new();
    let mut squelch_db: Option<f32> = None;
    let mut scanner: Option<Scanner> = None;
    let mut hd_program: Option<u32> = None;
    let mut hd: Option<Hd> = None;
    let mut hd_sidebands = false;
    let hd_for = |program: Option<u32>, mode: Mode, center: f64, tuned: f64| -> Option<Hd> {
        program.filter(|_| mode == Mode::Wfm).map(|p| Hd::new(tuned - center, rate, p))
    };
    let p25_for = |mode: Mode, tuned: f64| (mode == Mode::P25).then(|| P25::new(rate, tuned));
    let mut p25: Option<P25> = None;
    let mut last_status = Instant::now() - Duration::from_secs(1);

    loop {
        loop {
            match rx.try_recv() {
                Ok(Command::Stop) | Err(TryRecvError::Disconnected) => return,
                Ok(Command::Tune { center_hz, tuned_hz }) => {
                    let moved = (center_hz - center).abs() > 1.0 || (tuned_hz - tuned).abs() > 1.0;
                    if (center_hz - center).abs() > 1.0 {
                        center = center_hz;
                        if let Err(e) = src.set_center(center as u32) {
                            error = Some(e.to_string());
                        }
                        recent.clear();
                    }
                    tuned = tuned_hz;
                    mixer.set(tuned - center, rate as f64);
                    if let Some(a) = &audio {
                        a.clear();
                    }
                    snr = 0.0;
                    squelch.reset();
                    if let Some(r) = &mut demod.rds {
                        r.clear();
                    }
                    if moved {
                        demod.offset_sum = 0.0;
                        demod.offset_n = 0;
                        p25 = p25_for(mode, tuned);
                        hd = hd_for(hd_program, mode, center, tuned);
                        hd_sidebands = false;
                    }
                }
                Ok(Command::SetMode(m)) => {
                    mode = m;
                    demod = Demodulator::new(mode, rate);
                    p25 = p25_for(mode, tuned);
                    hd = hd_for(hd_program, mode, center, tuned);
                }
                Ok(Command::SetGain(g)) => {
                    if let Err(e) = src.set_gain(g) {
                        error = Some(e.to_string());
                    }
                }
                Ok(Command::SetSquelch(t)) => {
                    squelch_db = t;
                    squelch.set(t);
                }
                Ok(Command::SetSondes(on)) => {
                    if on != scanner.is_some() {
                        scanner = on.then(|| Scanner::new(rate, all_decoders));
                    }
                }
                Ok(Command::SetHd(p)) => {
                    match (&hd, p) {
                        (Some(h), Some(p)) => h.set_program(p),
                        _ => {
                            hd = hd_for(p, mode, center, tuned);
                        }
                    }
                    hd_program = p;
                }
                Ok(Command::SetPpm(p)) => {
                    if p != ppm {
                        ppm = p;
                        if let Err(e) = src.set_ppm(p) {
                            error = Some(e.to_string());
                        }
                        recent.clear();
                        demod.offset_sum = 0.0;
                        demod.offset_n = 0;
                        // its own measurement was against the old correction
                        if p25.is_some() {
                            p25 = p25_for(mode, tuned);
                        }
                    }
                }
                Ok(Command::SetVolume(v)) => volume = v.clamp(0.0, 1.5),
                Ok(Command::SetPlaying(p)) => {
                    playing = p;
                    if let (false, Some(a)) = (p, &audio) {
                        a.clear();
                    }
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        let n = match src.read(&mut raw) {
            Ok(n) => n,
            Err(e) => {
                error = Some(e.to_string());
                send_status(&on_status, &device, simulated, &error, rate, center, tuned, mode, playing, src.gains(), floor, 0.0, squelch_db, squelch.is_open(), &scanner, None, &[], hd_sidebands, None, None, None, ppm, crystal);
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        u8_to_iq(&raw[..n], &mut iq);
        if let Some(h) = &hd {
            h.feed(&iq);
        }

        rows.clear();
        spectrum.process(&iq, &mut rows);
        for row in rows.drain(..) {
            floor = 0.9 * floor + 0.1 * noise_floor(&row);
            let lo = floor - BELOW_FLOOR_DB;
            let bins = row.iter().map(|&db| (((db - lo) / RANGE_DB) * 255.0).clamp(0.0, 255.0) as u8).collect();
            on_row(Row { center_hz: center, bins });
            let peak = channel_peak(&row, center, tuned, mode, rate) - floor;
            // rise fast so the squelch opens on the first syllable, fall slower to ride out fades
            snr += (peak - snr) * if peak > snr { 0.6 } else { 0.25 };
            squelch.update(snr, 1.0 / ROWS_PER_SEC as f32);
            recent.push(row);
        }
        if recent.len() >= 6 {
            let n = recent[0].len();
            let mut avg = vec![0.0f32; n];
            for r in &recent {
                for (a, v) in avg.iter_mut().zip(r) {
                    *a += v / recent.len() as f32;
                }
            }
            signals = detect(&avg, floor, DETECT_DB, center, rate as f64, 3);
            hd_sidebands = mode == Mode::Wfm && crate::hd::sidebands(&avg, floor, center, tuned, rate as f64);
            if let Some(sc) = &mut scanner {
                sc.retarget(center, &signals);
            }
            recent.clear();
        }

        if let Some(sc) = &mut scanner {
            sc.process(&iq);
        }

        if let Some(p) = &mut p25 {
            // the receiver opens and closes on calls itself, so the squelch stays out of it
            if let Some(c) = p.process(&iq, center, &signals, &mut pcm) {
                center = c;
                if let Err(e) = src.set_center(center as u32) {
                    error = Some(e.to_string());
                }
                recent.clear();
            }
        } else {
            chan.clear();
            chan.extend_from_slice(&iq);
            mixer.process(&mut chan);
            demod.process(&chan, &mut pcm);
            // a strong station on the FM grid shows how far off the crystal is: its carrier is the discriminator's mean
            let on_grid = mode == Mode::Wfm && ((tuned / FM_GRID_HZ).round() * FM_GRID_HZ - tuned).abs() < 1.0;
            if !on_grid || snr < MEASURE_SNR_DB {
                demod.offset_sum = 0.0;
                demod.offset_n = 0;
            } else if demod.offset_n as f64 >= 240_000.0 * MEASURE_SECS {
                let off_ppm = demod.offset_sum / demod.offset_n as f64 / tuned * 1e6;
                crystal = Some(Crystal { ppm: ppm as f64 - off_ppm, hz: tuned });
                demod.offset_sum = 0.0;
                demod.offset_n = 0;
            }
            if let Some(h) = &hd {
                h.fill(&mut pcm);
            }
            squelch.apply(&mut pcm);
        }
        if let (true, Some(a)) = (playing, &audio) {
            a.push(&pcm, volume);
        }

        if last_status.elapsed() >= Duration::from_millis(200) {
            last_status = Instant::now();
            let p25_info = p25.as_ref().map(P25::info);
            // P25's ppm is where the control channel was heard against where it says it is, so already this sign's residual
            if let Some(i) = &p25_info {
                if let (Some(p), Some(hz)) = (i.ppm, i.control_hz) {
                    crystal = Some(Crystal { ppm: ppm as f64 - p, hz });
                }
            }
            send_status(&on_status, &device, simulated, &error, rate, center, tuned, mode, playing, src.gains(), floor, snr, squelch_db, squelch.is_open(), &scanner, demod.rds.as_ref().map(|r| r.info().clone()), &signals, hd_sidebands, hd.as_ref().map(Hd::info), hd.as_mut().and_then(Hd::new_images), p25_info, ppm, crystal);
        }
    }
}

/// Strongest bin (dBFS) inside the channel around `tuned`.
fn channel_peak(row: &[f32], center: f64, tuned: f64, mode: Mode, rate: u32) -> f32 {
    let n = row.len();
    let bin_hz = rate as f64 / n as f64;
    let half = (mode.bandwidth() as f64 / 2.0).max(bin_hz);
    let start = center - rate as f64 / 2.0;
    let lo = ((tuned - half - start) / bin_hz).floor().max(0.0) as usize;
    let hi = (((tuned + half - start) / bin_hz).ceil().max(0.0) as usize).min(n - 1);
    if lo > hi {
        return f32::MIN;
    }
    row[lo..=hi].iter().cloned().fold(f32::MIN, f32::max)
}

#[allow(clippy::too_many_arguments)]
fn send_status(
    on_status: &impl Fn(Status),
    device: &str,
    simulated: bool,
    error: &Option<String>,
    rate: u32,
    center: f64,
    tuned: f64,
    mode: Mode,
    playing: bool,
    gains: Vec<i32>,
    floor: f32,
    snr: f32,
    squelch_db: Option<f32>,
    squelch_open: bool,
    scanner: &Option<Scanner>,
    rds: Option<crate::rds::Info>,
    signals: &[Detected],
    hd_sidebands: bool,
    hd: Option<HdInfo>,
    hd_images: Option<Vec<HdImage>>,
    p25: Option<crate::p25::Info>,
    ppm: i32,
    crystal: Option<Crystal>,
) {
    on_status(Status {
        device: device.to_string(),
        simulated,
        error: error.clone(),
        sample_rate: rate,
        center_hz: center,
        tuned_hz: tuned,
        mode,
        playing,
        gains,
        floor_db: floor,
        channel_snr_db: snr,
        squelch_db,
        squelch_open,
        rds: rds.filter(|i| *i != crate::rds::Info::default()),
        sondes: scanner.as_ref().map(|s| s.sondes()).unwrap_or_default(),
        decoding_hz: scanner.as_ref().map(|s| s.channels()).unwrap_or_default(),
        signals: signals.to_vec(),
        hd_sidebands,
        hd,
        hd_images,
        p25,
        ppm,
        crystal,
    });
}

/// What the simulated receiver transmits when no dongle is plugged in.
pub fn demo_signals() -> Vec<SimSignal> {
    let fm = |mhz: f64, amp: f32| SimSignal { hz: mhz * 1e6, amp, fm_dev: 60_000.0, am_depth: 0.0, tone_hz: 440.0, symbols: None };
    let nfm = |mhz: f64, amp: f32, tone: f64| SimSignal { hz: mhz * 1e6, amp, fm_dev: 3_000.0, am_depth: 0.0, tone_hz: tone, symbols: None };
    vec![
        fm(100.3, 0.25),
        fm(100.7, 0.08),
        fm(101.1, 0.35),
        fm(101.9, 0.15),
        fm(102.3, 0.1),
        nfm(146.52, 0.2, 600.0),
        nfm(146.94, 0.3, 800.0),
        nfm(162.55, 0.3, 1_000.0),
        SimSignal { hz: 119.1e6, amp: 0.2, fm_dev: 0.0, am_depth: 0.7, tone_hz: 700.0, symbols: None },
        // a weather balloon, so the decoders have something to find
        SimSignal { hz: 403.3e6, amp: 0.3, fm_dev: 3_000.0, am_depth: 0.0, tone_hz: crate::sonde::dfm::BAUD as f64, symbols: Some(crate::sonde::dfm::simulate(23_038_743, 39.7392, -104.9903, 10_000.0).into()) },
    ]
}
