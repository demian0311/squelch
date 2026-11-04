//! Finds transmitting signals in a spectrum row.

use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Detected {
    /// Absolute centre frequency, Hz.
    pub hz: f64,
    /// Occupied width, Hz.
    pub width_hz: f64,
    /// Peak height above the noise floor, dB.
    pub snr_db: f32,
    /// FM only: shows HD Radio sidebands. `None`: not looked at, or too near the capture's edge to see both.
    pub hd: Option<bool>,
}

/// Groups bins more than `threshold_db` above `floor` into signals. Gaps of up
/// to `max_gap` bins inside one signal are bridged.
pub fn detect(row: &[f32], floor: f32, threshold_db: f32, center_hz: f64, rate: f64, max_gap: usize) -> Vec<Detected> {
    let n = row.len();
    let bin_hz = rate / n as f64;
    // Bin i is centred on its own frequency, not half a bin above it.
    let hz_of = |i: f64| center_hz - rate / 2.0 + i * bin_hz;
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if row[i] - floor < threshold_db {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i;
        let mut gap = 0;
        let mut j = i + 1;
        while j < n {
            if row[j] - floor >= threshold_db {
                end = j;
                gap = 0;
            } else {
                gap += 1;
                if gap > max_gap {
                    break;
                }
            }
            j += 1;
        }
        let slice = &row[start..=end];
        let peak = slice.iter().cloned().fold(f32::MIN, f32::max);
        // power-weighted centroid gives a steadier centre than the peak bin
        let (mut wsum, mut isum) = (0.0f64, 0.0f64);
        for (k, &v) in slice.iter().enumerate() {
            let w = 10f64.powf(((v - floor) / 10.0) as f64);
            wsum += w;
            isum += w * (start + k) as f64;
        }
        out.push(Detected { hz: hz_of(isum / wsum), width_hz: (end - start + 1) as f64 * bin_hz, snr_db: peak - floor, hd: None });
        i = end + 1;
    }
    // the DC bin of the tuner is not a signal
    out.retain(|d| (d.hz - center_hz).abs() > bin_hz * 1.5 || d.width_hz > bin_hz * 4.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A carrier exactly on a known frequency, through the real transform, is reported on it.
    #[test]
    fn a_tone_is_reported_on_its_own_frequency() {
        use crate::spectrum::Spectrum;
        use num_complex::Complex32;
        let (rate, n) = (2_400_000.0f64, 2048usize);
        let centre = 27_000_000.0;
        let offset = 65_000.0;
        let mut sp = Spectrum::new(n, rate as u32, 10);
        let iq: Vec<Complex32> = (0..rate as usize)
            .map(|i| Complex32::from_polar(0.5, (std::f64::consts::TAU * offset * i as f64 / rate) as f32))
            .collect();
        let mut rows = Vec::new();
        sp.process(&iq, &mut rows);
        let d = detect(&rows[0], -90.0, 20.0, centre, rate, 2);
        let got = d.iter().max_by(|a, b| a.snr_db.total_cmp(&b.snr_db)).unwrap().hz;
        assert!((got - (centre + offset)).abs() < 100.0, "off by {} Hz", got - (centre + offset));
    }

    #[test]
    fn finds_two_signals_and_ignores_noise() {
        let mut row = vec![-80.0f32; 1000];
        for v in row.iter_mut().step_by(7) {
            *v = -76.0;
        }
        for v in &mut row[100..104] {
            *v = -50.0;
        }
        for v in &mut row[600..700] {
            *v = -60.0;
        }
        row[650] = -79.0; // a dip inside the wide one is bridged
        let d = detect(&row, -80.0, 10.0, 100e6, 1e6, 3);
        assert_eq!(d.len(), 2, "{d:?}");
        assert!((d[0].hz - (100e6 - 0.5e6 + 102.0 * 1000.0)).abs() < 1000.0);
        assert!((d[1].width_hz - 100_000.0).abs() < 2_000.0);
        assert!((d[0].snr_db - 30.0).abs() < 0.01);
    }
}
