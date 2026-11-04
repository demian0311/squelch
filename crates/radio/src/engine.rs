//! The receive loop: reads the dongle, produces spectrum rows, finds signals and plays audio.

use anyhow::{bail, Result};
use crate::acars::Acars;
use crate::adsb::Adsb;
use crate::audio::AudioOut;
use crate::record::Recorder;
use crate::demod::{Demodulator, Mode};
use crate::detect::{detect, Detected};
use crate::device::{Gain, IqSource, RtlSdr, SimSignal, SimSource};
use crate::dsp::{u8_to_iq, Mixer, C32};
use crate::hd::{Hd, HdImage, HdInfo};
use crate::look::{BandLook, Hop};
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
    /// Start or stop recording what is heard to a file (`record::info` says which).
    Record(bool),
    /// Mute below this many dB above the floor; `None` turns squelch off.
    SetSquelch(Option<f32>),
    /// Run the radiosonde decoders on every narrow signal in the capture.
    SetSondes(bool),
    /// Decode ACARS on every US channel in the capture.
    SetAcars(bool),
    /// Decode aircraft broadcasts (ADS-B) across the whole capture; the capture should sit on 1090 MHz.
    SetAdsb(bool),
    /// Share the dongle between aircraft broadcasts and aircraft messages, this many seconds on each, or stop.
    AirTurns(Option<(f64, f64)>),
    /// Hop across these ranges (Hz) now and then and report them; `None` stops. Repeating the same ranges changes nothing.
    BandLook(Option<Vec<(f64, f64)>>),
    /// Where the listener is; surface positions need it.
    SetSite(Option<(f64, f64)>),
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
    /// Aircraft heard recently, while ADS-B decoding is on.
    pub adsb: Option<crate::adsb::Info>,
    pub signals: Vec<Detected>,
    /// The tuned FM station shows HD Radio sidebands.
    pub hd_sidebands: bool,
    /// HD Radio decoding of the tuned station, while it is on.
    pub hd: Option<HdInfo>,
    /// The HD pictures, sent only when they change.
    pub hd_images: Option<Vec<HdImage>>,
    /// Trunked radio state, in P25 mode.
    pub p25: Option<crate::p25::Info>,
    /// The whole band, while `Command::BandLook` is on.
    pub look: Option<crate::look::Info>,
    /// Aircraft messages, while ACARS decoding is on.
    pub acars: Option<crate::acars::Info>,
    /// Correction applied now.
    pub ppm: i32,
    pub crystal: Option<Crystal>,
}

pub struct Engine {
    tx: Sender<Command>,
    handle: Option<JoinHandle<()>>,
}

/// Stands in between a dongle that died and the next one that opens.
struct Closed;

impl IqSource for Closed {
    fn name(&self) -> String {
        "No dongle".into()
    }
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }
    fn set_center(&mut self, _hz: u32) -> Result<()> {
        Ok(())
    }
    fn set_gain(&mut self, _gain: Gain) -> Result<()> {
        Ok(())
    }
    fn set_ppm(&mut self, _ppm: i32) -> Result<()> {
        Ok(())
    }
    fn gains(&self) -> Vec<i32> {
        vec![]
    }
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        bail!("the dongle is gone; looking for it again")
    }
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

/// Hardware centre for aircraft broadcasts, off the replies by the same DC offset the page uses.
pub const ADSB_CENTER_HZ: f64 = 1_090_060_000.0;
/// Samples just after a retune are the tuner settling.
const SETTLE_SECS: f64 = 0.05;

#[derive(Clone, Copy, PartialEq, Debug)]
enum AirJob {
    Both,
    Planes,
    Messages,
    Settling,
}

/// One dongle, two aircraft jobs far apart: planes on 1090 MHz, messages around 130.5 MHz.
struct Turns {
    planes_s: f64,
    messages_s: f64,
    on_planes: bool,
    left: i64,
    settle: i64,
}

impl Turns {
    fn new(planes_s: f64, messages_s: f64, rate: u32) -> Self {
        Turns { planes_s, messages_s, on_planes: true, left: (planes_s * rate as f64) as i64, settle: (SETTLE_SECS * rate as f64) as i64 }
    }

    /// Count a block. Returns the new hardware centre when the turn ends.
    fn advance(&mut self, samples: i64, rate: u32) -> Option<f64> {
        self.left -= samples;
        self.settle -= samples;
        if self.left > 0 {
            return None;
        }
        self.on_planes = !self.on_planes;
        let secs = if self.on_planes { self.planes_s } else { self.messages_s };
        self.left = (secs * rate as f64) as i64;
        self.settle = (SETTLE_SECS * rate as f64) as i64;
        Some(if self.on_planes { ADSB_CENTER_HZ } else { crate::acars::CENTER_HZ })
    }

    fn job(&self) -> AirJob {
        match (self.settle > 0, self.on_planes) {
            (true, _) => AirJob::Settling,
            (false, true) => AirJob::Planes,
            (false, false) => AirJob::Messages,
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
    let mut gain: Option<Gain> = None;
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
    let mut pcm: Vec<f32> = Vec::new();
    let mut rows: Vec<Vec<f32>> = Vec::new();
    let mut floor = -60.0f32;
    let mut recent: Vec<Vec<f32>> = Vec::new();
    let mut signals: Vec<Detected> = Vec::new();
    let mut snr = 0.0f32;
    let mut squelch = Squelch::new();
    let mut squelch_db: Option<f32> = None;
    let mut scanner: Option<Scanner> = None;
    let mut acars: Option<Acars> = None;
    let mut adsb: Option<Adsb> = None;
    let mut turns: Option<Turns> = None;
    let mut look: Option<BandLook> = None;
    // kept across a decoder switching off; nothing from the simulator goes in it
    let mut airlog: Option<crate::airlog::Writer> = None;
    let mut air_lines: Vec<crate::airlog::Line> = Vec::new();
    let mut site: Option<(f64, f64)> = None;
    let mut hd_program: Option<u32> = None;
    let mut hd: Option<Hd> = None;
    let mut hd_sidebands = false;
    let hd_for = |program: Option<u32>, mode: Mode, center: f64, tuned: f64| -> Option<Hd> {
        program.filter(|_| mode == Mode::Wfm).map(|p| Hd::new(tuned - center, rate, p, true))
    };
    let p25_for = |mode: Mode, tuned: f64| (mode == Mode::P25).then(|| P25::new(rate, tuned));
    let mut p25: Option<P25> = None;
    let mut last_status = Instant::now() - Duration::from_secs(1);
    let mut recorder: Option<Recorder> = None;
    let mut prof = Prof::new();

    loop {
        loop {
            match rx.try_recv() {
                Ok(Command::Stop) | Err(TryRecvError::Disconnected) => return,
                Ok(Command::Tune { center_hz, tuned_hz }) => {
                    // while the two aircraft jobs take turns, the turns say where the dongle sits
                    let center_hz = if turns.is_some() { center } else { center_hz };
                    // the view moves the dongle but not what is playing: only a new frequency restarts the audio
                    let center_moved = (center_hz - center).abs() > 1.0;
                    let tuned_moved = (tuned_hz - tuned).abs() > 1.0;
                    // the listener comes first: a look in progress gives the dongle straight back
                    if center_moved || tuned_moved {
                        if look.as_mut().is_some_and(BandLook::abort) && !center_moved {
                            if let Err(e) = src.set_center(center as u32) {
                                error = Some(e.to_string());
                            }
                        }
                    }
                    if center_moved {
                        center = center_hz;
                        if let Err(e) = src.set_center(center as u32) {
                            error = Some(e.to_string());
                        }
                        recent.clear();
                    }
                    if center_moved || tuned_moved {
                        tuned = tuned_hz;
                        mixer.set(tuned - center, rate as f64);
                    }
                    if !tuned_moved {
                        if let (true, Some(h)) = (center_moved, &mut hd) {
                            h.set_offset(tuned - center);
                        }
                    } else {
                        if let Some(a) = &audio {
                            a.clear();
                        }
                        snr = 0.0;
                        squelch.reset();
                        if let Some(r) = &mut demod.rds {
                            r.clear();
                        }
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
                    gain = Some(g);
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
                Ok(Command::SetAcars(on)) => {
                    if on != acars.is_some() {
                        acars = on.then(|| Acars::new(rate));
                    }
                }
                Ok(Command::SetAdsb(on)) => {
                    if on != adsb.is_some() {
                        adsb = on.then(|| {
                            let mut a = Adsb::new(rate);
                            a.set_site(site);
                            a
                        });
                    }
                }
                Ok(Command::AirTurns(t)) => {
                    match (t, &mut turns) {
                        (None, _) => turns = None,
                        (Some((planes_s, messages_s)), Some(old)) => (old.planes_s, old.messages_s) = (planes_s, messages_s),
                        (Some((planes_s, messages_s)), None) => {
                            turns = Some(Turns::new(planes_s, messages_s, rate));
                            center = ADSB_CENTER_HZ;
                            if let Err(e) = src.set_center(center as u32) {
                                error = Some(e.to_string());
                            }
                            mixer.set(tuned - center, rate as f64);
                            recent.clear();
                        }
                    }
                }
                Ok(Command::BandLook(ranges)) => {
                    if look.as_ref().map(BandLook::ranges) != ranges.as_deref() {
                        if look.as_mut().is_some_and(BandLook::abort) {
                            if let Err(e) = src.set_center(center as u32) {
                                error = Some(e.to_string());
                            }
                        }
                        look = ranges.map(|r| BandLook::new(r, rate, BELOW_FLOOR_DB, RANGE_DB));
                    }
                }
                Ok(Command::SetSite(s)) => {
                    site = s;
                    if let Some(a) = &mut adsb {
                        a.set_site(s);
                    }
                }
                Ok(Command::SetHd(p)) => {
                    match (&hd, p) {
                        (Some(h), Some(p)) => {
                            h.set_program(p);
                            h.set_play(true);
                        }
                        // the decoder costs about a fifth of a core, so it runs only while digital audio is asked for
                        _ => hd = hd_for(p, mode, center, tuned),
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
                Ok(Command::Record(on)) => {
                    recorder = None;
                    if on {
                        match Recorder::start(crate::record::dir(), tuned, mode) {
                            Ok(r) => recorder = Some(r),
                            Err(e) => crate::record::fail(&e),
                        }
                    }
                }
                Ok(Command::SetPlaying(p)) => {
                    playing = p;
                    if let (false, Some(a)) = (p, &audio) {
                        a.clear();
                    }
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        prof.lap("commands");
        let n = match src.read(&mut raw) {
            Ok(n) => n,
            Err(e) => {
                error = Some(e.to_string());
                if let Some(l) = &mut look {
                    l.abort();
                }
                send_status(&on_status, &device, simulated, &error, rate, center, tuned, mode, playing, src.gains(), floor, 0.0, squelch_db, squelch.is_open(), &scanner, None, &adsb, &[], hd_sidebands, None, None, None, None, None, ppm, crystal);
                std::thread::sleep(Duration::from_millis(500));
                // a sleep or a replug kills the USB handle: close it and open the dongle again
                if !simulated {
                    src = Box::new(Closed);
                    if let Ok(d) = RtlSdr::open(0, SAMPLE_RATE) {
                        src = Box::new(d);
                        let _ = src.set_ppm(ppm);
                        let _ = src.set_center(center as u32);
                        if let Some(g) = gain {
                            let _ = src.set_gain(g);
                        }
                        error = None;
                    }
                }
                continue;
            }
        };
        prof.lap("read");
        u8_to_iq(&raw[..n], &mut iq);
        prof.lap("convert");
        if let Some(h) = &hd {
            h.feed(&iq);
        }
        prof.lap("hd feed");

        rows.clear();
        spectrum.process(&iq, &mut rows);
        prof.lap("spectrum");
        // hopping: this block is another frequency, so it goes nowhere else
        if let Some(l) = look.as_mut().filter(|l| l.sweeping()) {
            let hop = l.sweep(&rows, iq.len() as i64);
            rows.clear();
            match hop {
                Hop::Stay => {}
                Hop::To(hz) => {
                    if let Err(e) = src.set_center(hz as u32) {
                        error = Some(e.to_string());
                    }
                }
                Hop::Home => {
                    if let Err(e) = src.set_center(center as u32) {
                        error = Some(e.to_string());
                    }
                    recent.clear();
                }
            }
            if last_status.elapsed() >= Duration::from_millis(200) {
                last_status = Instant::now();
                send_status(&on_status, &device, simulated, &error, rate, center, tuned, mode, playing, src.gains(), floor, snr, squelch_db, squelch.is_open(), &scanner, None, &adsb, &signals, hd_sidebands, None, None, p25.as_ref().map(P25::info), look.as_ref().map(BandLook::info), acars.as_ref().map(Acars::info), ppm, crystal);
            }
            prof.report();
            continue;
        }
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
        prof.lap("rows");
        if recent.len() >= 6 {
            let n = recent[0].len();
            let mut avg = vec![0.0f32; n];
            for r in &recent {
                for (a, v) in avg.iter_mut().zip(r) {
                    *a += v / recent.len() as f32;
                }
            }
            signals = detect(&avg, floor, DETECT_DB, center, rate as f64, 3);
            if mode == Mode::Wfm {
                crate::hd::drop_sidebands(&mut signals);
                crate::hd::mark_sidebands(&mut signals, &avg, floor, center, rate as f64);
            }
            hd_sidebands = mode == Mode::Wfm && crate::hd::sidebands(&avg, floor, center, tuned, rate as f64);
            if let Some(sc) = &mut scanner {
                sc.retarget(center, &signals);
            }
            recent.clear();
        }
        prof.lap("detect");

        if let Some(sc) = &mut scanner {
            sc.process(&iq);
        }
        prof.lap("sondes");

        let job = match &mut turns {
            None => AirJob::Both,
            Some(t) => {
                if let Some(c) = t.advance(iq.len() as i64, rate) {
                    center = c;
                    if let Err(e) = src.set_center(center as u32) {
                        error = Some(e.to_string());
                    }
                    mixer.set(tuned - center, rate as f64);
                    recent.clear();
                }
                t.job()
            }
        };
        if let (Some(a), AirJob::Both | AirJob::Messages) = (&mut acars, job) {
            a.process(&iq, center);
        }
        prof.lap("acars");
        if let Some(a) = &mut adsb {
            if matches!(job, AirJob::Both | AirJob::Planes) {
                a.process(&iq);
            } else {
                a.skip(iq.len());
            }
        }
        prof.lap("adsb");
        if !simulated && (acars.is_some() || adsb.is_some()) {
            let now = crate::airlog::now_ms();
            if let Some(a) = &mut adsb {
                a.drain_log(now, &mut air_lines);
            }
            if let Some(a) = &mut acars {
                air_lines.extend(a.take_fresh().into_iter().map(crate::airlog::Line::Message));
            }
            if !air_lines.is_empty() {
                let log = airlog.get_or_insert_with(|| crate::airlog::Writer::start(crate::airlog::dir().unwrap_or_else(std::env::temp_dir)));
                for l in air_lines.drain(..) {
                    log.send(l);
                }
            }
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
            prof.lap("p25");
        } else {
            demod.process_mixed(&iq, &mut mixer, &mut pcm);
            prof.lap("demod");
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
            // the view took the dongle out of reach; what the mixer finds there is another frequency's alias
            if (tuned - center).abs() > rate as f64 / 2.0 {
                pcm.clear();
            }
            prof.lap("hd fill");
        }
        if let (true, Some(a)) = (playing, &audio) {
            a.push(&pcm, volume);
        }
        if let Some(r) = &recorder {
            r.push(&pcm);
        }
        prof.lap("audio");
        if let Some(l) = &mut look {
            // a call in progress, or a plain channel someone is listening to, holds a look off
            let busy = match &p25 {
                Some(p) => p.in_call(),
                None => squelch.is_open(),
            };
            if let Some(hz) = l.tick(iq.len() as i64, busy) {
                if let Err(e) = src.set_center(hz as u32) {
                    error = Some(e.to_string());
                }
            }
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
            send_status(&on_status, &device, simulated, &error, rate, center, tuned, mode, playing, src.gains(), floor, snr, squelch_db, squelch.is_open(), &scanner, demod.rds.as_ref().map(|r| r.info().clone()), &adsb, &signals, hd_sidebands, hd.as_ref().map(Hd::info), hd.as_mut().and_then(Hd::new_images), p25_info, look.as_ref().map(BandLook::info), acars.as_ref().map(Acars::info), ppm, crystal);
        }
        prof.lap("status");
        prof.report();
    }
}

/// Where the radio thread's time goes, printed to stderr every `PROF_EVERY` when `SQUELCH_PROFILE` is set.
/// Each lap charges the time since the previous one to a stage; the rest of a period is time the thread waited.
struct Prof {
    on: bool,
    last: Instant,
    since: Instant,
    stages: Vec<(&'static str, Duration)>,
}

const PROF_EVERY: Duration = Duration::from_secs(5);

impl Prof {
    fn new() -> Self {
        let now = Instant::now();
        Prof { on: std::env::var_os("SQUELCH_PROFILE").is_some(), last: now, since: now, stages: Vec::new() }
    }

    fn lap(&mut self, stage: &'static str) {
        if !self.on {
            return;
        }
        let now = Instant::now();
        match self.stages.iter_mut().find(|(s, _)| *s == stage) {
            Some((_, d)) => *d += now - self.last,
            None => self.stages.push((stage, now - self.last)),
        }
        self.last = now;
    }

    fn report(&mut self) {
        if !self.on || self.since.elapsed() < PROF_EVERY {
            return;
        }
        let wall = self.since.elapsed().as_secs_f64();
        let busy: f64 = self.stages.iter().map(|(_, d)| d.as_secs_f64()).sum();
        let mut line = format!("radio thread {:>5.1}% busy:", busy / wall * 100.0);
        self.stages.sort_by(|a, b| b.1.cmp(&a.1));
        for (s, d) in &self.stages {
            let pct = d.as_secs_f64() / wall * 100.0;
            if pct >= 0.05 {
                line += &format!("  {s} {pct:.1}%");
            }
        }
        eprintln!("{line}");
        self.stages.clear();
        self.since = Instant::now();
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
    adsb: &Option<Adsb>,
    signals: &[Detected],
    hd_sidebands: bool,
    hd: Option<HdInfo>,
    hd_images: Option<Vec<HdImage>>,
    p25: Option<crate::p25::Info>,
    look: Option<crate::look::Info>,
    acars: Option<crate::acars::Info>,
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
        adsb: adsb.as_ref().map(Adsb::info),
        signals: signals.to_vec(),
        hd_sidebands,
        hd,
        hd_images,
        p25,
        look,
        acars,
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
        // EMS: both bands, for the whole-band strip
        nfm(770.26875, 0.2, 600.0),
        nfm(773.05625, 0.1, 500.0),
        nfm(853.9375, 0.2, 800.0),
        nfm(860.2125, 0.15, 900.0),
        nfm(866.5875, 0.1, 700.0),
        // a weather balloon, so the decoders have something to find
        SimSignal { hz: 403.3e6, amp: 0.3, fm_dev: 3_000.0, am_depth: 0.0, tone_hz: crate::sonde::dfm::BAUD as f64, symbols: Some(crate::sonde::dfm::simulate(23_038_743, 39.7392, -104.9903, 10_000.0).into()) },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_band_look_reports_both_ranges_and_returns_to_the_channel() {
        let signals = vec![SimSignal { hz: 853.9375e6, amp: 0.2, fm_dev: 3_000.0, am_depth: 0.0, tone_hz: 440.0, symbols: None }];
        let source = Box::new(SimSource::new(SAMPLE_RATE, signals, true));
        let seen = Arc::new(Mutex::new(Vec::<(f64, Option<crate::look::Info>)>::new()));
        let log = seen.clone();
        let engine = Engine::start(source, None, |_| {}, move |s| log.lock().unwrap().push((s.center_hz, s.look)));
        engine.send(Command::Tune { center_hz: 771e6, tuned_hz: 771e6 });
        engine.send(Command::SetSquelch(Some(100.0)));
        engine.send(Command::BandLook(Some(vec![(769e6, 775e6), (851e6, 869e6)])));
        let until = Instant::now() + Duration::from_secs(20);
        let line = loop {
            std::thread::sleep(Duration::from_millis(50));
            let got = seen.lock().unwrap().iter().find_map(|(_, l)| l.as_ref().and_then(|l| l.line.clone()));
            if let Some(l) = got {
                break l;
            }
            assert!(Instant::now() < until, "no line");
        };
        std::thread::sleep(Duration::from_millis(300));
        let p = &line.parts[1];
        let strongest = (0..p.cells.len()).max_by_key(|&i| p.cells[i]).unwrap();
        assert!(strongest.abs_diff(((853.9375e6 - 851e6) / crate::look::CELL_HZ) as usize) <= 2, "cell {strongest}");
        assert!(seen.lock().unwrap().iter().any(|(_, l)| l.as_ref().is_some_and(|l| l.looking)));
        // the page is never told the dongle moved
        assert!(seen.lock().unwrap().iter().skip(2).all(|(c, _)| *c == 771e6));
        engine.send(Command::BandLook(None));
    }

    #[test]
    fn air_turns_alternate_and_own_the_dongle() {
        let source = Box::new(SimSource::new(SAMPLE_RATE, demo_signals(), true));
        let centers = Arc::new(Mutex::new(Vec::<f64>::new()));
        let log = centers.clone();
        let engine = Engine::start(source, None, |_| {}, move |s| log.lock().unwrap().push(s.center_hz));
        engine.send(Command::SetAdsb(true));
        engine.send(Command::SetAcars(true));
        engine.send(Command::AirTurns(Some((0.4, 0.4))));
        std::thread::sleep(Duration::from_millis(300));
        engine.send(Command::Tune { center_hz: 100e6, tuned_hz: 100e6 });
        std::thread::sleep(Duration::from_millis(2500));
        {
            let seen = centers.lock().unwrap();
            assert!(!seen.contains(&100e6), "a tune moved the dongle during turns");
            assert!(seen.contains(&ADSB_CENTER_HZ), "never on planes: {seen:?}");
            assert!(seen.contains(&crate::acars::CENTER_HZ), "never on messages: {seen:?}");
        }
        engine.send(Command::AirTurns(None));
        engine.send(Command::Tune { center_hz: 100e6, tuned_hz: 100e6 });
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(centers.lock().unwrap().last(), Some(&100e6));
    }

    #[test]
    fn turns_rest_after_each_retune() {
        let rate = 1000;
        let mut t = Turns::new(1.0, 2.0, rate);
        assert_eq!(t.job(), AirJob::Settling);
        assert_eq!(t.advance(100, rate), None);
        assert_eq!(t.job(), AirJob::Planes);
        assert_eq!(t.advance(900, rate), Some(crate::acars::CENTER_HZ));
        assert_eq!(t.job(), AirJob::Settling);
        assert_eq!(t.advance(1999, rate), None);
        assert_eq!(t.job(), AirJob::Messages);
        assert_eq!(t.advance(1, rate), Some(ADSB_CENTER_HZ));
    }

    /// Mean channel level over `secs` of status, while `drag` runs between statuses.
    fn level(engine: &Engine, seen: &Arc<Mutex<Vec<f32>>>, secs: f64, drag: Option<Command>) -> f32 {
        seen.lock().unwrap().clear();
        let end = Instant::now() + Duration::from_secs_f64(secs);
        while Instant::now() < end {
            if let Some(Command::Tune { center_hz, tuned_hz }) = drag {
                engine.send(Command::Tune { center_hz, tuned_hz });
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        let got = seen.lock().unwrap();
        got.iter().sum::<f32>() / got.len().max(1) as f32
    }

    /// Dragging the waterfall sends the same tune every frame; that must not touch what is playing.
    #[test]
    fn repeating_a_tune_leaves_the_channel_alone() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let source = Box::new(SimSource::new(SAMPLE_RATE, demo_signals(), true));
        let engine = Engine::start(source, None, |_| {}, move |s| log.lock().unwrap().push(s.channel_snr_db));
        engine.send(Command::SetVolume(0.0));
        engine.send(Command::SetPlaying(false));
        std::thread::sleep(Duration::from_secs(1));
        let still = level(&engine, &seen, 1.5, None);
        assert!(still > 10.0, "the simulated station should be strong, got {still} dB");
        let dragging = level(&engine, &seen, 1.5, Some(Command::Tune { center_hz: 101_400_000.0, tuned_hz: 101_100_000.0 }));
        assert!(dragging > still * 0.8, "{dragging} dB while dragging, {still} dB still");
    }
}
