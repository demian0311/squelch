//! WGS84 Earth-centred coordinates to latitude, longitude and height.

const A: f64 = 6_378_137.0;
const B: f64 = 6_356_752.314_245_18;

/// (lat°, lon°, height m above the ellipsoid) from ECEF metres, by Bowring's method.
pub fn ecef_to_geodetic(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let e2 = (A * A - B * B) / (A * A);
    let ep2 = (A * A - B * B) / (B * B);
    let p = (x * x + y * y).sqrt();
    let th = (z * A).atan2(p * B);
    let lat = (z + ep2 * B * th.sin().powi(3)).atan2(p - e2 * A * th.cos().powi(3));
    let lon = y.atan2(x);
    let n = A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    let h = p / lat.cos() - n;
    (lat.to_degrees(), lon.to_degrees(), h)
}

/// ECEF velocity (m/s) at a position to (horizontal speed, heading°, climb rate).
pub fn ecef_velocity(lat_deg: f64, lon_deg: f64, vx: f64, vy: f64, vz: f64) -> (f64, f64, f64) {
    let (phi, lam) = (lat_deg.to_radians(), lon_deg.to_radians());
    let vn = -vx * phi.sin() * lam.cos() - vy * phi.sin() * lam.sin() + vz * phi.cos();
    let ve = -vx * lam.sin() + vy * lam.cos();
    let vu = vx * phi.cos() * lam.cos() + vy * phi.cos() * lam.sin() + vz * phi.sin();
    let mut heading = ve.atan2(vn).to_degrees();
    if heading < 0.0 {
        heading += 360.0;
    }
    ((vn * vn + ve * ve).sqrt(), heading, vu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denver_round_trip() {
        // Denver, 1609 m: ECEF computed from the forward WGS84 formula
        let (lat, lon, h) = (39.7392f64, -104.9903f64, 1609.0);
        let e2 = (A * A - B * B) / (A * A);
        let (p, l) = (lat.to_radians(), lon.to_radians());
        let n = A / (1.0 - e2 * p.sin().powi(2)).sqrt();
        let x = (n + h) * p.cos() * l.cos();
        let y = (n + h) * p.cos() * l.sin();
        let z = (n * (1.0 - e2) + h) * p.sin();
        let (a, b, c) = ecef_to_geodetic(x, y, z);
        assert!((a - lat).abs() < 1e-6 && (b - lon).abs() < 1e-6 && (c - h).abs() < 0.01);
    }
}
