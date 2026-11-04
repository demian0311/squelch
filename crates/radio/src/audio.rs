//! Plays 48 kHz mono audio on the default output device.
//!
//! The radio thread and the device's callback share a lock-free ring, so the callback, which has a hard deadline,
//! never waits on the radio thread.

use crate::demod::AUDIO_RATE;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Keep at most this much audio queued; older samples are dropped so delay stays low.
const MAX_QUEUE: usize = AUDIO_RATE as usize * 3 / 10;
/// Frames per device callback, about 43 ms at 48 kHz. Fewer wake-ups than the device default of 512 and more room for a late one.
const DEVICE_FRAMES: u32 = 2048;
/// Device errors are reported at most this often, with the count since the last report.
const REPORT_EVERY: Duration = Duration::from_secs(10);

pub struct AudioOut {
    _stream: cpal::Stream,
    /// Only `push` locks this, so the callback is never waiting on it.
    tx: Mutex<Producer<f32>>,
    clear: Arc<AtomicBool>,
}

/// Counts device errors and writes one dated line per `REPORT_EVERY`, so a burst can be matched to what else happened.
struct ErrorLog {
    last: Option<Instant>,
    unreported: u64,
}

impl ErrorLog {
    fn note(&mut self, e: &dyn std::fmt::Display) {
        self.unreported += 1;
        if self.last.is_some_and(|t| t.elapsed() < REPORT_EVERY) {
            return;
        }
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        eprintln!("audio: {} errors, unix time {at}: {e}", self.unreported);
        self.last = Some(Instant::now());
        self.unreported = 0;
    }
}

/// Fills one device buffer from the ring. `prev` holds the last sample and `frac` the resampler's position.
fn fill(data: &mut [f32], channels: usize, step: f64, frac: &mut f64, prev: &mut f32, rx: &mut Consumer<f32>, clear: &AtomicBool) {
    let drop_all = clear.swap(false, Ordering::Acquire);
    let over = if drop_all { rx.slots() } else { rx.slots().saturating_sub(MAX_QUEUE) };
    if over > 0 {
        if let Ok(chunk) = rx.read_chunk(over) {
            chunk.commit_all();
        }
    }
    for frame in data.chunks_mut(channels) {
        *frac += step;
        while *frac >= 1.0 {
            *prev = rx.pop().unwrap_or(0.0);
            *frac -= 1.0;
        }
        frame.iter_mut().for_each(|s| *s = *prev);
    }
}

impl AudioOut {
    pub fn open() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no audio output device"))?;
        let supported = device.default_output_config()?;
        let clear = Arc::new(AtomicBool::new(false));
        let build = |size: cpal::BufferSize| {
            let mut config = supported.config();
            config.buffer_size = size;
            let channels = config.channels as usize;
            // read the 48 kHz ring at the device's own rate
            let step = AUDIO_RATE as f64 / config.sample_rate as f64;
            let (tx, mut rx) = RingBuffer::<f32>::new(MAX_QUEUE * 2);
            let flag = clear.clone();
            let (mut frac, mut prev) = (0.0f64, 0.0f32);
            let mut errors = ErrorLog { last: None, unreported: 0 };
            device
                .build_output_stream::<f32, _, _>(
                    config,
                    move |data: &mut [f32], _| fill(data, channels, step, &mut frac, &mut prev, &mut rx, &flag),
                    move |e| errors.note(&e),
                    None,
                )
                .map(|stream| (stream, tx))
        };
        // not every device takes a fixed size
        let (stream, tx) = build(cpal::BufferSize::Fixed(DEVICE_FRAMES)).or_else(|_| build(cpal::BufferSize::Default))?;
        stream.play()?;
        Ok(AudioOut { _stream: stream, tx: Mutex::new(tx), clear })
    }

    pub fn push(&self, samples: &[f32], volume: f32) {
        let mut tx = self.tx.lock().unwrap();
        // the ring holds twice `MAX_QUEUE` and the callback trims it, so this drops only if the device has stopped
        let n = samples.len().min(tx.slots());
        if let Ok(chunk) = tx.write_chunk_uninit(n) {
            chunk.fill_from_iter(samples[..n].iter().map(|s| s * volume));
        }
    }

    pub fn clear(&self) {
        self.clear.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rx: &mut Consumer<f32>, clear: &AtomicBool, frames: usize) -> Vec<f32> {
        let mut out = vec![9.0; frames];
        let (mut frac, mut prev) = (0.0, 0.0);
        fill(&mut out, 1, 1.0, &mut frac, &mut prev, rx, clear);
        out
    }

    #[test]
    fn an_empty_ring_plays_silence() {
        let (_tx, mut rx) = RingBuffer::<f32>::new(8);
        assert_eq!(run(&mut rx, &AtomicBool::new(false), 3), [0.0; 3]);
    }

    #[test]
    fn samples_play_in_order() {
        let (mut tx, mut rx) = RingBuffer::<f32>::new(8);
        for s in [1.0, 2.0, 3.0] {
            tx.push(s).unwrap();
        }
        assert_eq!(run(&mut rx, &AtomicBool::new(false), 3), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn clear_drops_what_is_queued() {
        let (mut tx, mut rx) = RingBuffer::<f32>::new(8);
        tx.push(1.0).unwrap();
        let clear = AtomicBool::new(true);
        assert_eq!(run(&mut rx, &clear, 1), [0.0]);
        assert!(!clear.load(Ordering::Relaxed));
    }

    #[test]
    fn too_much_queued_drops_the_oldest() {
        let (mut tx, mut rx) = RingBuffer::<f32>::new(MAX_QUEUE + 4);
        for i in 0..MAX_QUEUE + 2 {
            tx.push(i as f32).unwrap();
        }
        assert_eq!(run(&mut rx, &AtomicBool::new(false), 1), [2.0]);
    }
}
