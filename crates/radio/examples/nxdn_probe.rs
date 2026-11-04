//! Read NXDN fields from a recording or the dongle.
//! cargo run -p radio --release --example nxdn_probe -- <file.wav>          (mono 48 kHz FM audio)
//! cargo run -p radio --release --example nxdn_probe -- <MHz> <secs>        (close the app first)
//!
//! Prints how many frames were found, the link values, and every message whose check passed.

use radio::demod::{Demodulator, Mode};
use radio::device::{Gain, IqSource, RtlSdr};
use radio::dsp::{u8_to_iq, Mixer};
use radio::nxdn::{self, Sacch};
use std::collections::BTreeMap;

const RATE: u32 = 2_400_000;
/// Off the dongle's DC spike.
const OFFSET_HZ: f64 = 300_000.0;

fn wav(path: &str) -> anyhow::Result<Vec<f32>> {
    let data = std::fs::read(path)?;
    let at = data.windows(4).position(|w| w == b"data").ok_or_else(|| anyhow::anyhow!("no data chunk"))? + 8;
    Ok(data[at..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect())
}

fn live(mhz: f64, secs: f64) -> anyhow::Result<Vec<f32>> {
    let mut src = RtlSdr::open(0, RATE)?;
    src.set_gain(Gain::Tenths(*src.gains().last().unwrap_or(&400)))?;
    src.set_center((mhz * 1e6 + OFFSET_HZ) as u32)?;
    let mut mixer = Mixer::new(-OFFSET_HZ, RATE as f64);
    let mut demod = Demodulator::new(Mode::Nfm, RATE);
    let (mut raw, mut iq, mut audio) = (vec![0u8; 262_144], Vec::new(), Vec::new());
    let mut got = 0.0;
    while got < secs {
        let n = src.read(&mut raw)?;
        iq.clear();
        u8_to_iq(&raw[..n], &mut iq);
        mixer.process(&mut iq);
        demod.process(&iq, &mut audio);
        got += n as f64 / 2.0 / RATE as f64;
    }
    Ok(audio)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let audio = match a.len() {
        2 => wav(&a[1])?,
        3 => live(a[1].parse()?, a[2].parse()?)?,
        _ => anyhow::bail!("usage: nxdn_probe <file.wav> | <MHz> <secs>"),
    };
    let secs = audio.len() as f64 / nxdn::AUDIO_RATE as f64;
    let raws = nxdn::find_frames(&audio);
    println!("{secs:.1} s of audio, {} frames, {:.0} expected if on the air throughout", raws.len(), secs * 12.5);
    let (mut no_link, mut voice, mut sacch_ok, mut sacch_bad, mut bad) = (0, 0, 0, 0, 0);
    let mut control_ran: BTreeMap<u8, u32> = BTreeMap::new();
    let mut links: BTreeMap<u8, u32> = BTreeMap::new();
    let mut seen: BTreeMap<String, (u32, f64)> = BTreeMap::new();
    let mut rans: BTreeMap<u32, u32> = BTreeMap::new();
    let mut join = Sacch::default();
    let mut note = |text: String, at: usize| {
        let e = seen.entry(text).or_insert((0, at as f64 / nxdn::AUDIO_RATE as f64));
        e.0 += 1;
    };
    for raw in &raws {
        let f = nxdn::read_frame(raw);
        let Some(lich) = f.lich else {
            no_link += 1;
            continue;
        };
        *links.entry(lich).or_default() += 1;
        // A link value with one wrong bit can read as voice; good slow control says it is.
        voice += (f.voice != 0 && f.sacch.is_some()) as u32;
        if let Some(r) = f.ran {
            *control_ran.entry(r).or_default() += 1;
        }
        bad += f.bad;
        sacch_bad += f.sacch_bad as u32;
        if let Some(seg) = &f.sacch {
            sacch_ok += 1;
            if let Some((ran, m)) = join.push(seg) {
                *rans.entry(ran).or_default() += 1;
                note(format!("{} {}", hex(&m), nxdn::message_name(m[0])), raw.at);
                if let Some((src, dst, cipher, key)) = nxdn::voice_call(&m) {
                    note(format!("  call from {src} to {dst}, cipher type {cipher}, key {key}"), raw.at);
                }
            }
        }
        for m in &f.messages {
            let words = nxdn::describe(m).map(|w| format!(": {w}")).unwrap_or_default();
            note(format!("{} {}{words}", hex(m), nxdn::message_name(m[0])), raw.at);
            if let Some((src, dst, cipher, key)) = nxdn::voice_call(m) {
                note(format!("  call from {src} to {dst}, cipher type {cipher}, key {key}"), raw.at);
            }
        }
    }
    let corr: f32 = raws.iter().map(|r| r.corr).sum::<f32>() / raws.len().max(1) as f32;
    println!("mean sync match {corr:.2}; link parity failed {no_link}; frames with voice {voice}");
    println!("slow control good {sacch_ok}, failed {sacch_bad}; other control blocks failed {bad}");
    println!("radio access numbers: traffic {rans:?}, control {control_ran:?}");
    let frames = raws.len().max(1) as u32;
    for (l, n) in links.iter().filter(|(_, n)| **n * 100 >= frames) {
        println!("link 0x{l:02x} x{n}: {}", nxdn::lich_name(*l));
    }
    for (t, (n, first)) in &seen {
        println!("x{n} from {first:.1} s  {t}");
    }
    Ok(())
}
