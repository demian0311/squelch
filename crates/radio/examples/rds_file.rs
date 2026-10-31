//! Decode RDS from a recorded multiplex signal (240 kHz f32 LE, from the mpx_dump example).

use radio::rds::Rds;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: rds_file <mpx.f32>");
    let data = std::fs::read(&path)?;
    let x: Vec<f32> = data.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    let fs = 240_000.0f64;
    // tone levels from one second
    for f in [1_000.0, 19_000.0, 38_000.0, 54_000.0, 57_000.0, 60_000.0, 67_000.0, 92_000.0] {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in x[..240_000].iter().enumerate() {
            let p = std::f64::consts::TAU * f * i as f64 / fs;
            re += v as f64 * p.cos();
            im -= v as f64 * p.sin();
        }
        println!("{f:>7} Hz  {:>6.1} dB", 20.0 * ((re * re + im * im).sqrt() / 240_000.0).log10());
    }
    let tone = |f: f64| {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in x[..480_000].iter().enumerate() {
            let p = std::f64::consts::TAU * f * i as f64 / fs;
            re += v as f64 * p.cos();
            im -= v as f64 * p.sin();
        }
        (re * re + im * im).sqrt() / 480_000.0
    };
    let best = (0..81).map(|k| 18_980.0 + k as f64 * 0.5).max_by(|a, b| tone(*a).total_cmp(&tone(*b))).unwrap();
    println!("pilot peak at {best:.1} Hz ({:.1} dB)", 20.0 * tone(best).log10());
    // averaged power spectrum, 4096 points (58.6 Hz bins)
    let n = 4096;
    let fft = rustfft::FftPlanner::<f32>::new().plan_fft_forward(n);
    let mut psd = vec![0f32; n / 2];
    for blk in x.chunks_exact(n).take(400) {
        let mut b: Vec<num_complex::Complex32> = blk.iter().enumerate().map(|(i, &v)| num_complex::Complex32::new(v * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()), 0.0)).collect();
        fft.process(&mut b);
        for (p, c) in psd.iter_mut().zip(&b) {
            *p += c.norm_sqr();
        }
    }
    let band = |lo: f64, hi: f64| {
        let (a, b) = ((lo / fs * n as f64) as usize, (hi / fs * n as f64) as usize);
        10.0 * (psd[a..b].iter().sum::<f32>() / (b - a) as f32).log10()
    };
    for (lo, hi) in [(15_000.0, 17_000.0), (18_900.0, 19_100.0), (23_000.0, 53_000.0), (53_500.0, 55_000.0), (55_500.0, 56_500.0), (56_500.0, 57_500.0), (57_500.0, 58_500.0), (59_000.0, 60_500.0), (62_000.0, 70_000.0)] {
        println!("band {lo:>7.0}-{hi:<7.0} {:>6.1} dB/bin", band(lo, hi));
    }
    // pilot phase in consecutive 50 ms blocks: steady drift means a clean recording
    let blk = 12_000;
    let mut prev: Option<f64> = None;
    let mut jumps = Vec::new();
    for b in 0..(x.len() / blk).min(200) {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for i in 0..blk {
            let n = b * blk + i;
            let p = std::f64::consts::TAU * best * n as f64 / fs;
            re += x[n] as f64 * p.cos();
            im -= x[n] as f64 * p.sin();
        }
        let ph = im.atan2(re);
        if let Some(q) = prev {
            let mut d = ph - q;
            d -= std::f64::consts::TAU * (d / std::f64::consts::TAU).round();
            jumps.push(d);
        }
        prev = Some(ph);
    }
    println!("pilot phase step per 50 ms block (rad): {:?}", jumps.iter().take(40).map(|d| (d * 100.0).round() / 100.0).collect::<Vec<_>>());
    let mut r = Rds::new(240_000);
    for (k, chunk) in x.chunks(24_000).enumerate() {
        r.process(chunk);
        if k % 10 == 9 {
            println!("{:>4.1} s  synced {}  {:?}  lock {:?}", (k + 1) as f32 / 10.0, r.synced(), r.stats(), r.lock());
        }
    }
    println!("{:?}", r.info());
    Ok(())
}
