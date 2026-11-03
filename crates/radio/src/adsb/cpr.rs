//! Compact Position Reporting, airborne format: 17-bit fractions of a latitude zone and a longitude zone.
//! An even and an odd report together give the position anywhere (`global`); one report plus a
//! position within half a zone of it gives the position (`local`).

const SCALE: f64 = 131_072.0;

/// Longitude zones at latitude `lat`.
pub fn nl(lat: f64) -> u32 {
    let lat = lat.abs();
    if lat < 1e-9 {
        return 59;
    }
    if (lat - 87.0).abs() < 1e-9 {
        return 2;
    }
    if lat > 87.0 {
        return 1;
    }
    let a = 1.0 - (std::f64::consts::PI / 30.0).cos();
    let b = lat.to_radians().cos().powi(2);
    (std::f64::consts::TAU / (1.0 - a / b).acos()).floor() as u32
}

fn dlat(odd: bool) -> f64 {
    360.0 / if odd { 59.0 } else { 60.0 }
}

/// Position from an even and an odd report, at the time of whichever came last. `None` when the two
/// straddle a longitude-zone boundary and cannot be paired.
pub fn global(even: (u32, u32), odd: (u32, u32), odd_last: bool) -> Option<(f64, f64)> {
    let (lat_e, lon_e) = (even.0 as f64 / SCALE, even.1 as f64 / SCALE);
    let (lat_o, lon_o) = (odd.0 as f64 / SCALE, odd.1 as f64 / SCALE);
    let j = (59.0 * lat_e - 60.0 * lat_o + 0.5).floor();
    let wrap = |l: f64| if l >= 270.0 { l - 360.0 } else { l };
    let la_e = wrap(dlat(false) * (j.rem_euclid(60.0) + lat_e));
    let la_o = wrap(dlat(true) * (j.rem_euclid(59.0) + lat_o));
    if nl(la_e) != nl(la_o) {
        return None;
    }
    let (lat, n, lon_frac) = if odd_last { (la_o, nl(la_o) as f64 - 1.0, lon_o) } else { (la_e, nl(la_e) as f64, lon_e) };
    let nlat = nl(lat) as f64;
    let m = (lon_e * (nlat - 1.0) - lon_o * nlat + 0.5).floor();
    let ni = n.max(1.0);
    let mut lon = 360.0 / ni * (m.rem_euclid(ni) + lon_frac);
    if lon >= 180.0 {
        lon -= 360.0;
    }
    Some((lat, lon))
}

/// Position from one report near a known one.
pub fn local(report: (u32, u32), odd: bool, near: (f64, f64)) -> (f64, f64) {
    let (lat_c, lon_c) = (report.0 as f64 / SCALE, report.1 as f64 / SCALE);
    let d = dlat(odd);
    let j = (near.0 / d).floor() + (near.0.rem_euclid(d) / d - lat_c + 0.5).floor();
    let lat = d * (j + lat_c);
    let dlon = 360.0 / (nl(lat) as f64 - if odd { 1.0 } else { 0.0 }).max(1.0);
    let m = (near.1 / dlon).floor() + (near.1.rem_euclid(dlon) / dlon - lon_c + 0.5).floor();
    (lat, dlon * (m + lon_c))
}

/// Surface position from one report near a known one, which must be within 45 NM: zones are a quarter the size.
pub fn surface_local(report: (u32, u32), odd: bool, near: (f64, f64)) -> (f64, f64) {
    let (lat_c, lon_c) = (report.0 as f64 / SCALE, report.1 as f64 / SCALE);
    let d = dlat(odd) / 4.0;
    let j = (near.0 / d).floor() + (near.0.rem_euclid(d) / d - lat_c + 0.5).floor();
    let lat = d * (j + lat_c);
    let dlon = 90.0 / (nl(lat) as f64 - if odd { 1.0 } else { 0.0 }).max(1.0);
    let m = (near.1 / dlon).floor() + (near.1.rem_euclid(dlon) / dlon - lon_c + 0.5).floor();
    (lat, dlon * (m + lon_c))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVEN: (u32, u32) = (93_000, 51_372);
    const ODD: (u32, u32) = (74_158, 50_194);

    #[test]
    fn zones() {
        assert_eq!(nl(0.0), 59);
        assert_eq!(nl(52.2572), 36);
        assert_eq!(nl(-52.2572), 36);
        assert_eq!(nl(87.0), 2);
        assert_eq!(nl(89.0), 1);
        // the first boundary is at 10.47047130°
        assert_eq!(nl(10.47), 59);
        assert_eq!(nl(10.48), 58);
    }

    #[test]
    fn even_odd_pair() {
        let (lat, lon) = global(EVEN, ODD, false).unwrap();
        assert!((lat - 52.25720).abs() < 1e-5, "{lat}");
        assert!((lon - 3.91937).abs() < 1e-5, "{lon}");
        // at the odd report's time, two seconds earlier: the Riddle's odd latitude, and close by
        let (lat, lon) = global(EVEN, ODD, true).unwrap();
        assert!((lat - 52.26578).abs() < 1e-5, "{lat}");
        assert!((lon - 3.91937).abs() < 0.05, "{lon}");
    }

    #[test]
    fn near_a_known_position() {
        let (lat, lon) = local(EVEN, false, (52.258, 3.918));
        assert!((lat - 52.25720).abs() < 1e-5, "{lat}");
        assert!((lon - 3.91937).abs() < 1e-5, "{lon}");
        // agrees with the pair decode for the odd report too
        let (g_lat, g_lon) = global(EVEN, ODD, true).unwrap();
        let (lat, lon) = local(ODD, true, (52.258, 3.918));
        assert!((lat - g_lat).abs() < 1e-9 && (lon - g_lon).abs() < 1e-9);
    }

    #[test]
    fn western_and_southern_hemispheres() {
        // encode a point and decode it back, both parities
        let encode = |lat: f64, lon: f64, odd: bool| {
            let d = dlat(odd);
            let yz = (SCALE * lat.rem_euclid(d) / d + 0.5).floor();
            let rlat = d * (yz / SCALE + (lat / d).floor());
            let dlon = 360.0 / (nl(rlat) as f64 - if odd { 1.0 } else { 0.0 }).max(1.0);
            let xz = (SCALE * lon.rem_euclid(dlon) / dlon + 0.5).floor();
            ((yz as u32) & 0x1FFFF, (xz as u32) & 0x1FFFF)
        };
        for (lat, lon) in [(39.7392, -104.9903), (-33.9461, 151.1772), (-22.8, -43.25), (64.13, -21.94)] {
            let (e, o) = (encode(lat, lon, false), encode(lat, lon, true));
            for odd_last in [false, true] {
                let (la, lo) = global(e, o, odd_last).unwrap();
                assert!((la - lat).abs() < 1e-3 && (lo - lon).abs() < 1e-3, "{lat} {lon} -> {la} {lo}");
                let (la, lo) = local(if odd_last { o } else { e }, odd_last, (lat + 0.3, lon - 0.4));
                assert!((la - lat).abs() < 1e-3 && (lo - lon).abs() < 1e-3, "local {lat} {lon} -> {la} {lo}");
            }
        }
    }
}
