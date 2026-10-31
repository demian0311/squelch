//! Plays 48 kHz mono audio on the default output device.

use crate::demod::AUDIO_RATE;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Keep at most this much audio queued; older samples are dropped so delay stays low.
const MAX_QUEUE: usize = AUDIO_RATE as usize * 3 / 10;

pub struct AudioOut {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
}

impl AudioOut {
    pub fn open() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("no audio output device"))?;
        let supported = device.default_output_config()?;
        let mut config = supported.config();
        config.buffer_size = cpal::BufferSize::Default;
        let channels = config.channels as usize;
        // read the 48 kHz queue at the device's own rate
        let step = AUDIO_RATE as f64 / config.sample_rate as f64;
        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_QUEUE * 2)));
        let q = queue.clone();
        let mut frac = 0.0f64;
        let mut prev = 0.0f32;
        let stream = device.build_output_stream::<f32, _, _>(
            config,
            move |data: &mut [f32], _| {
                let mut q = q.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    frac += step;
                    while frac >= 1.0 {
                        prev = q.pop_front().unwrap_or(0.0);
                        frac -= 1.0;
                    }
                    frame.iter_mut().for_each(|s| *s = prev);
                }
            },
            |e| eprintln!("audio: {e}"),
            None,
        )?;
        stream.play()?;
        Ok(AudioOut { _stream: stream, queue })
    }

    pub fn push(&self, samples: &[f32], volume: f32) {
        let mut q = self.queue.lock().unwrap();
        q.extend(samples.iter().map(|s| s * volume));
        let over = q.len().saturating_sub(MAX_QUEUE);
        q.drain(..over);
    }

    pub fn clear(&self) {
        self.queue.lock().unwrap().clear();
    }
}
