//! Space station passes, offline: SGP4 on a cached TLE (two-line element set).
//! Accuracy decays with the TLE's age — drag and reboosts — so callers show `age_days`.

pub mod place;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

/// CelesTrak's current ISS elements. They ask for at most one fetch per 2 hours.
pub const TLE_URL: &str = "https://celestrak.org/NORAD/elements/gp.php?CATNR=25544&FORMAT=TLE";

/// Last good TLE: `~/.cache/squelch/iss.tle`. Its mtime is when it was fetched.
pub fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("squelch").join("iss.tle"))
}

pub fn load_cached() -> Option<Orbit> {
    Orbit::parse(&std::fs::read_to_string(cache_path()?).ok()?).ok()
}

/// Downloads the current elements with curl and replaces the cache only if they parse.
pub fn fetch() -> Result<()> {
    let path = cache_path().context("no cache directory")?;
    let out = Command::new("curl").args(["-sfL", "--max-time", "20", TLE_URL]).output().context("running curl")?;
    if !out.status.success() {
        bail!("curl exited with {}", out.status);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Orbit::parse(&text).with_context(|| format!("CelesTrak sent: {}", text.trim()))?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// A ZIP code, a US state, or "lat, lon" in degrees.
pub fn parse_site(text: &str) -> Option<Site> {
    let t = text.trim();
    if let Some(s) = place::zip(t).or_else(|| place::state(t)) {
        return Some(s);
    }
    let mut f = t.split(|c: char| c == ',' || c.is_whitespace()).filter(|v| !v.is_empty()).map(|v| v.parse::<f64>().ok());
    let (lat, lon) = (f.next()??, f.next()??);
    (f.next().is_none() && lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some(Site { lat, lon })
}

/// Unix time of J2000 (2000-01-01 12:00 UTC), the epoch sgp4 counts years from.
const J2000_UNIX: f64 = 946_728_000.0;
const YEAR_SECS: f64 = 365.25 * 86_400.0;
const EARTH_A_KM: f64 = 6378.137;
const EARTH_E2: f64 = 6.694_379_990_14e-3;
/// Search step; a pass above the horizon lasts several minutes, so none is skipped.
const STEP_SECS: f64 = 30.0;

pub struct Orbit {
    constants: sgp4::Constants,
    /// Unix seconds of the TLE epoch.
    pub epoch: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Site {
    pub lat: f64,
    pub lon: f64,
}

/// A point on the station's ground track. `t` is Unix seconds.
#[derive(Clone, Debug, Serialize)]
pub struct GroundPoint {
    pub t: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt_km: f64,
}

/// One pass above the horizon. Times are Unix seconds.
#[derive(Clone, Debug, Serialize)]
pub struct Pass {
    pub rise: f64,
    pub top: f64,
    pub set: f64,
    pub max_elevation: f64,
}

impl Orbit {
    /// Reads the first `1 …` / `2 …` line pair; a name line is optional.
    pub fn parse(text: &str) -> Result<Self> {
        let line = |n: &str| text.lines().map(str::trim_end).find(|l| l.starts_with(n)).ok_or_else(|| anyhow!("no TLE line {}", n.trim()));
        let elements = sgp4::Elements::from_tle(None, line("1 ")?.as_bytes(), line("2 ")?.as_bytes()).map_err(|e| anyhow!("bad TLE: {e}"))?;
        let constants = sgp4::Constants::from_elements(&elements).map_err(|e| anyhow!("bad TLE: {e}"))?;
        Ok(Self { constants, epoch: J2000_UNIX + elements.epoch() * YEAR_SECS })
    }

    pub fn age_days(&self, now: f64) -> f64 {
        (now - self.epoch) / 86_400.0
    }

    /// Earth-fixed position in km at Unix time `t`.
    pub fn ecef(&self, t: f64) -> Option<[f64; 3]> {
        let p = self.constants.propagate(sgp4::MinutesSinceEpoch((t - self.epoch) / 60.0)).ok()?.position;
        let g = sgp4::iau_epoch_to_sidereal_time((t - J2000_UNIX) / YEAR_SECS);
        let (s, c) = g.sin_cos();
        Some([c * p[0] + s * p[1], -s * p[0] + c * p[1], p[2]])
    }

    /// The point under the station at `t`.
    pub fn ground(&self, t: f64) -> Option<GroundPoint> {
        let p = self.ecef(t)?;
        let (lat, lon, h) = crate::sonde::geo::ecef_to_geodetic(p[0] * 1e3, p[1] * 1e3, p[2] * 1e3);
        Some(GroundPoint { t, lat, lon, alt_km: h / 1e3 })
    }

    /// Ground points every `step` seconds over `[from, to]`.
    pub fn track(&self, from: f64, to: f64, step: f64) -> Vec<GroundPoint> {
        (0..=((to - from) / step) as usize).filter_map(|i| self.ground(from + i as f64 * step)).collect()
    }

    /// Degrees above the site's horizon; negative below it.
    pub fn elevation(&self, site: Site, t: f64) -> Option<f64> {
        let sat = self.ecef(t)?;
        let (lat, lon) = (site.lat.to_radians(), site.lon.to_radians());
        let n = EARTH_A_KM / (1.0 - EARTH_E2 * lat.sin().powi(2)).sqrt();
        let obs = [n * lat.cos() * lon.cos(), n * lat.cos() * lon.sin(), n * (1.0 - EARTH_E2) * lat.sin()];
        let up = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];
        let d = [sat[0] - obs[0], sat[1] - obs[1], sat[2] - obs[2]];
        let range = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        Some(((d[0] * up[0] + d[1] * up[1] + d[2] * up[2]) / range).asin().to_degrees())
    }

    /// Passes that end after `from` and rise before `from + hours`. A pass under way at `from` is included.
    pub fn passes(&self, site: Site, from: f64, hours: f64) -> Vec<Pass> {
        let el = |t: f64| self.elevation(site, t).unwrap_or(-90.0);
        let end = from + hours * 3600.0;
        // Start below the horizon, stepping back if a pass is under way.
        let mut t = from;
        while el(t) > 0.0 {
            t -= STEP_SECS;
        }
        let mut out = Vec::new();
        let mut rise = None;
        while t < end || rise.is_some() {
            let next = t + STEP_SECS;
            let up = el(next) > 0.0;
            match rise {
                None if up => rise = Some(self.bisect(site, t, next, true)),
                Some(r) if !up => {
                    let set = self.bisect(site, t, next, false);
                    let top = peak(r, set, el);
                    if set > from {
                        out.push(Pass { rise: r, top, set, max_elevation: el(top) });
                    }
                    rise = None;
                }
                _ => {}
            }
            t = next;
        }
        out
    }

    /// Bisects `[a, b]` to 1 s for the horizon crossing; `rising` says which way it goes.
    fn bisect(&self, site: Site, mut a: f64, mut b: f64, rising: bool) -> f64 {
        while b - a > 1.0 {
            let m = (a + b) / 2.0;
            if (self.elevation(site, m).unwrap_or(-90.0) > 0.0) == rising {
                b = m;
            } else {
                a = m;
            }
        }
        (a + b) / 2.0
    }
}

/// Time of greatest elevation, by golden-section search; a pass has one peak.
fn peak(mut a: f64, mut b: f64, el: impl Fn(f64) -> f64) -> f64 {
    let r = (5f64.sqrt() - 1.0) / 2.0;
    while b - a > 1.0 {
        let (c, d) = (b - r * (b - a), a + r * (b - a));
        if el(c) > el(d) {
            b = d;
        } else {
            a = c;
        }
    }
    (a + b) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Epoch 2026-10-31 04:14:04 UTC (day 304.17644455).
    const TLE: &str = "ISS (ZARYA)\n1 25544U 98067A   26304.17644455  .00006183  00000+0  11976-3 0  9995\n2 25544  51.6332 347.1486 0007016 330.4977  29.5614 15.49124187592838\n";

    #[test]
    fn sites_from_zip_or_coordinates() {
        assert!(parse_site(" 80302 ").is_some());
        assert!(parse_site("Colorado").is_some());
        let s = parse_site("39.74, -104.99").unwrap();
        assert_eq!((s.lat, s.lon), (39.74, -104.99));
        assert!(parse_site("39.74 -104.99").is_some());
        assert!(parse_site("95, 10").is_none());
        assert!(parse_site("Denver").is_none());
        assert!(parse_site("1, 2, 3").is_none());
    }

    #[test]
    fn track_steps_evenly_and_crosses_the_date_line() {
        let o = Orbit::parse(TLE).unwrap();
        let tr = o.track(o.epoch, o.epoch + 5580.0, 30.0);
        assert_eq!(tr.len(), 187);
        assert!(tr.windows(2).all(|w| (w[1].t - w[0].t - 30.0).abs() < 1e-6));
        // One orbit (~93 min) covers every longitude.
        assert!(tr.windows(2).any(|w| (w[1].lon - w[0].lon).abs() > 180.0));
        assert!(tr.iter().all(|p| (380.0..450.0).contains(&p.alt_km)));
    }

    #[test]
    fn epoch_is_the_tle_day() {
        let o = Orbit::parse(TLE).unwrap();
        // 2026-10-31 00:00 UTC = 1793404800; .17644455 day = 15244.8 s.
        assert!((o.epoch - (1_793_404_800.0 + 15_244.8)).abs() < 1.0, "{}", o.epoch);
    }

    #[test]
    fn altitude_and_inclination_are_the_stations() {
        let o = Orbit::parse(TLE).unwrap();
        let mut max_lat: f64 = 0.0;
        for i in 0..200 {
            let p = o.ecef(o.epoch + i as f64 * 60.0).unwrap();
            let (lat, _, h) = crate::sonde::geo::ecef_to_geodetic(p[0] * 1e3, p[1] * 1e3, p[2] * 1e3);
            assert!((380e3..450e3).contains(&h), "altitude {h}");
            max_lat = max_lat.max(lat.abs());
        }
        // Geodetic latitude peaks a little above the 51.63° inclination.
        assert!((51.0..52.5).contains(&max_lat), "{max_lat}");
    }

    #[test]
    fn passes_are_ordered_and_above_the_horizon() {
        let o = Orbit::parse(TLE).unwrap();
        let site = Site { lat: 40.0, lon: -105.3 };
        let ps = o.passes(site, o.epoch, 48.0);
        // At 40° N the station passes 4–6 times a day.
        assert!((6..=14).contains(&ps.len()), "{}", ps.len());
        for (i, p) in ps.iter().enumerate() {
            assert!(p.rise < p.top && p.top < p.set, "{p:?}");
            assert!((120.0..900.0).contains(&(p.set - p.rise)), "{p:?}");
            assert!(p.max_elevation > 0.0 && p.max_elevation <= 90.0);
            assert!(o.elevation(site, p.rise - 5.0).unwrap() < 0.0);
            assert!(o.elevation(site, p.top).unwrap() > 0.0);
            if i > 0 {
                assert!(ps[i - 1].set < p.rise);
            }
        }
    }

    #[test]
    fn a_pass_under_way_is_included_with_its_rise() {
        let o = Orbit::parse(TLE).unwrap();
        let site = Site { lat: 40.0, lon: -105.3 };
        let p = o.passes(site, o.epoch, 24.0)[0].clone();
        let mid = (p.rise + p.set) / 2.0;
        let q = &o.passes(site, mid, 1.0)[0];
        assert!((q.rise - p.rise).abs() < 2.0 && (q.set - p.set).abs() < 2.0, "{p:?} {q:?}");
    }
}
