//! Extended squitter contents (DF17 from transponders, DF18 from other ADS-B transmitters).
//! Field offsets count from the first bit of the 56-bit ME field.

use super::phy::LONG_BYTES;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Ident { callsign: String },
    /// Airborne position. Latitude and longitude are 17-bit CPR fractions of a zone.
    Position { alt_ft: Option<i32>, odd: bool, lat: u32, lon: u32 },
    /// `airspeed`: speed and heading are through the air, not over the ground.
    Velocity { speed_kt: Option<f32>, heading_deg: Option<f32>, vrate_fpm: Option<i32>, airspeed: bool },
}

const CHARSET: &[u8; 64] = b"#ABCDEFGHIJKLMNOPQRSTUVWXYZ##### ###############0123456789######";

/// The sender's 24-bit address and what it says, for the message types this decoder reads.
pub fn decode(f: &[u8; LONG_BYTES]) -> Option<(u32, Msg)> {
    let df = f[0] >> 3;
    // DF18 control field 0 and 1 are ADS-B from a non-transponder device, same layout as DF17
    if df != 17 && !(df == 18 && f[0] & 7 <= 1) {
        return None;
    }
    let icao = (f[1] as u32) << 16 | (f[2] as u32) << 8 | f[3] as u32;
    let me = f[4..11].iter().fold(0u64, |a, &b| a << 8 | b as u64);
    let field = |off: u32, len: u32| ((me >> (56 - off - len)) & ((1 << len) - 1)) as u32;
    let msg = match field(0, 5) {
        1..=4 => {
            let s: String = (0..8).map(|k| CHARSET[field(8 + 6 * k, 6) as usize] as char).collect();
            Msg::Ident { callsign: s.trim_end_matches([' ', '#']).to_string() }
        }
        9..=18 => Msg::Position { alt_ft: altitude(field(8, 12)), odd: field(21, 1) == 1, lat: field(22, 17), lon: field(39, 17) },
        // GNSS height instead of pressure altitude
        20..=22 => Msg::Position { alt_ft: None, odd: field(21, 1) == 1, lat: field(22, 17), lon: field(39, 17) },
        19 => {
            let st = field(5, 3);
            let mult = if st == 2 || st == 4 { 4.0 } else { 1.0 };
            let vr = field(37, 9);
            let vrate_fpm = (vr != 0).then(|| (vr as i32 - 1) * 64 * if field(36, 1) == 1 { -1 } else { 1 });
            match st {
                1 | 2 => {
                    let (ew, ns) = (field(14, 10), field(25, 10));
                    let (speed_kt, heading_deg) = if ew == 0 || ns == 0 {
                        (None, None)
                    } else {
                        // east and north positive
                        let vx = (ew as f32 - 1.0) * mult * if field(13, 1) == 1 { -1.0 } else { 1.0 };
                        let vy = (ns as f32 - 1.0) * mult * if field(24, 1) == 1 { -1.0 } else { 1.0 };
                        (Some(vx.hypot(vy)), Some(vx.atan2(vy).to_degrees().rem_euclid(360.0)))
                    };
                    Msg::Velocity { speed_kt, heading_deg, vrate_fpm, airspeed: false }
                }
                3 | 4 => {
                    let heading_deg = (field(13, 1) == 1).then(|| field(14, 10) as f32 * 360.0 / 1024.0);
                    let a = field(25, 10);
                    let speed_kt = (a != 0).then(|| (a as f32 - 1.0) * mult);
                    Msg::Velocity { speed_kt, heading_deg, vrate_fpm, airspeed: true }
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    Some((icao, msg))
}

/// The 12-bit altitude field: 25 ft steps when the Q bit is set, else Gillham code in 100 ft steps.
pub fn altitude(a: u32) -> Option<i32> {
    if a == 0 {
        return None;
    }
    if a & 0x10 != 0 {
        let n = (a >> 5) << 4 | (a & 0xF);
        return Some(n as i32 * 25 - 1000);
    }
    // bits from the top: C1 A1 C2 A2 C4 A4 B1 Q B2 D2 B4 D4
    let bit = |k: u32| (a >> (11 - k)) & 1;
    let (c1, a1, c2, a2, c4, a4, b1, b2, d2, b4, d4) = (bit(0), bit(1), bit(2), bit(3), bit(4), bit(5), bit(6), bit(8), bit(9), bit(10), bit(11));
    let n500 = gray([d2, d4, a1, a2, a4, b1, b2, b4]) as i32;
    let mut n100 = gray([c1, c2, c4]) as i32;
    if matches!(n100, 0 | 5 | 6) {
        return None;
    }
    if n100 == 7 {
        n100 = 5;
    }
    // the 100 ft code runs backwards in odd 500 ft steps
    if n500 % 2 == 1 {
        n100 = 6 - n100;
    }
    Some(n500 * 500 + n100 * 100 - 1300)
}

/// Gray code, most significant bit first, to binary.
fn gray<const N: usize>(bits: [u32; N]) -> u32 {
    let mut out = 0;
    let mut prev = 0;
    for b in bits {
        prev ^= b;
        out = out << 1 | prev;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::phy::testutil::hex;
    use super::*;

    // frames from "The 1090 MHz Riddle" (Junzi Sun)
    #[test]
    fn identification() {
        assert_eq!(decode(&hex("8D4840D6202CC371C32CE0576098")), Some((0x4840D6, Msg::Ident { callsign: "KLM1023".into() })));
    }

    #[test]
    fn airborne_position() {
        let (icao, m) = decode(&hex("8D40621D58C382D690C8AC2863A7")).unwrap();
        assert_eq!(icao, 0x40621D);
        assert_eq!(m, Msg::Position { alt_ft: Some(38_000), odd: false, lat: 93_000, lon: 51_372 });
        let (_, m) = decode(&hex("8D40621D58C386435CC412692AD6")).unwrap();
        assert_eq!(m, Msg::Position { alt_ft: Some(38_000), odd: true, lat: 74_158, lon: 50_194 });
    }

    #[test]
    fn ground_velocity() {
        let Some((0x485020, Msg::Velocity { speed_kt: Some(s), heading_deg: Some(h), vrate_fpm, airspeed: false })) = decode(&hex("8D485020994409940838175B284F")) else { panic!() };
        assert!((s - 159.20).abs() < 0.01, "{s}");
        assert!((h - 182.88).abs() < 0.01, "{h}");
        assert_eq!(vrate_fpm, Some(-832));
    }

    #[test]
    fn air_velocity() {
        let Some((0xA05F21, Msg::Velocity { speed_kt: Some(s), heading_deg: Some(h), vrate_fpm, airspeed: true })) = decode(&hex("8DA05F219B06B6AF189400CBC33F")) else { panic!() };
        assert_eq!(s, 375.0);
        assert!((h - 243.98).abs() < 0.01, "{h}");
        assert_eq!(vrate_fpm, Some(-2304));
    }

    #[test]
    fn gillham_altitude() {
        // −1000 ft is C2 alone; 0 ft adds B2 and B4 (500 ft code 2) with C1 C2 C4 = 010 again
        assert_eq!(altitude(0b0010_0000_0000), Some(-1000));
        assert_eq!(altitude(0b0010_0000_1010), Some(0));
        // the 100 ft code is not allowed to read 0
        assert_eq!(altitude(0b0000_0000_1010), None);
    }

    #[test]
    fn other_formats_are_skipped() {
        // DF11 all-call reply, a short frame padded out
        assert_eq!(decode(&hex("5D4840D6E1F4C30000000000000000")), None);
    }
}
