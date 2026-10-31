//! Prints the station's ground point now and its next passes over a place.
//! Usage: iss <tle file> <zip | lat lon> [hours]

use radio::iss::{zip, Orbit, Site};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let orbit = Orbit::parse(&std::fs::read_to_string(&args[0])?)?;
    let (site, rest) = match zip::lookup(&args[1]) {
        Some(s) => (s, &args[2..]),
        None => (Site { lat: args[1].parse()?, lon: args[2].parse()? }, &args[3..]),
    };
    let hours: f64 = rest.first().map_or(Ok(24.0), |h| h.parse())?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let p = orbit.ecef(now).unwrap();
    let (lat, lon, h) = radio::sonde::geo::ecef_to_geodetic(p[0] * 1e3, p[1] * 1e3, p[2] * 1e3);
    println!("now {now:.0}  TLE age {:.2} d  ground point {lat:.2}, {lon:.2}  altitude {:.0} km", orbit.age_days(now), h / 1e3);
    println!("site {:.2}, {:.2}", site.lat, site.lon);
    for p in orbit.passes(site, now, hours) {
        println!("rise {:.0}  top {:.0}  set {:.0}  {:.0}°  {:.1} min", p.rise, p.top, p.set, p.max_elevation, (p.set - p.rise) / 60.0);
    }
    Ok(())
}
