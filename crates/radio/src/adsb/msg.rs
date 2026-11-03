//! Extended squitter contents (DF17 from transponders, DF18 from other ADS-B transmitters).
//! Field offsets count from the first bit of the 56-bit ME field.

use super::phy::LONG_BYTES;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Ident { callsign: String, category: Option<&'static str> },
    /// Airborne position. Latitude and longitude are 17-bit CPR fractions of a zone.
    Position { alt_ft: Option<i32>, gnss_ft: Option<i32>, odd: bool, lat: u32, lon: u32 },
    /// On the ground. Same CPR fields, in zones a quarter the size.
    Surface { speed_kt: Option<f32>, track_deg: Option<f32>, odd: bool, lat: u32, lon: u32 },
    /// `airspeed`: speed and heading are through the air, not over the ground.
    Velocity { speed_kt: Option<f32>, heading_deg: Option<f32>, vrate_fpm: Option<i32>, airspeed: bool, gnss_minus_baro_ft: Option<i32> },
    /// Emergency state and squawk (TC 28 subtype 1).
    Status { emergency: Option<&'static str>, squawk: String },
    /// A collision-avoidance resolution advisory is active (TC 28 subtype 2).
    Advisory,
    /// What the crew has selected and which modes are on (TC 29 subtype 1).
    Target(Target),
    /// ADS-B version (TC 31).
    Operational { version: u8, on_ground: bool },
    /// Replies to radar and other aircraft, not a broadcast (DF0, 4, 5, 11, 20, 21).
    Reply { alt_ft: Option<i32>, squawk: Option<String>, on_ground: Option<bool>, alert: bool, spi: bool },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Target {
    pub selected_alt_ft: Option<i32>,
    /// The selected altitude comes from the flight management system, not the autopilot panel.
    pub selected_alt_fms: bool,
    pub baro_hpa: Option<f32>,
    pub selected_heading_deg: Option<f32>,
    /// Names of the modes reported on, when the mode bits are valid.
    pub modes: Option<Vec<&'static str>>,
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
            Msg::Ident { callsign: s.trim_end_matches([' ', '#']).to_string(), category: category(field(0, 5), field(5, 3)) }
        }
        5..=8 => {
            let mov = field(5, 7);
            Msg::Surface {
                speed_kt: movement(mov),
                track_deg: (field(12, 1) == 1).then(|| field(13, 7) as f32 * 360.0 / 128.0),
                odd: field(21, 1) == 1,
                lat: field(22, 17),
                lon: field(39, 17),
            }
        }
        9..=18 => Msg::Position { alt_ft: altitude(field(8, 12)), gnss_ft: None, odd: field(21, 1) == 1, lat: field(22, 17), lon: field(39, 17) },
        // GNSS height in metres instead of pressure altitude
        20..=22 => {
            let m = field(8, 12);
            Msg::Position { alt_ft: None, gnss_ft: (m != 0).then(|| (m as f64 * FT_PER_M) as i32), odd: field(21, 1) == 1, lat: field(22, 17), lon: field(39, 17) }
        }
        19 => {
            let st = field(5, 3);
            let mult = if st == 2 || st == 4 { 4.0 } else { 1.0 };
            let vr = field(37, 9);
            let vrate_fpm = (vr != 0).then(|| (vr as i32 - 1) * 64 * if field(36, 1) == 1 { -1 } else { 1 });
            let diff = field(49, 7);
            let gnss_minus_baro_ft = (diff != 0 && diff != 127).then(|| (diff as i32 - 1) * 25 * if field(48, 1) == 1 { -1 } else { 1 });
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
                    Msg::Velocity { speed_kt, heading_deg, vrate_fpm, airspeed: false, gnss_minus_baro_ft }
                }
                3 | 4 => {
                    let heading_deg = (field(13, 1) == 1).then(|| field(14, 10) as f32 * 360.0 / 1024.0);
                    let a = field(25, 10);
                    let speed_kt = (a != 0).then(|| (a as f32 - 1.0) * mult);
                    Msg::Velocity { speed_kt, heading_deg, vrate_fpm, airspeed: true, gnss_minus_baro_ft }
                }
                _ => return None,
            }
        }
        28 => match field(5, 3) {
            1 => Msg::Status { emergency: emergency(field(8, 3)), squawk: squawk(field(11, 13)) },
            2 => Msg::Advisory,
            _ => return None,
        },
        29 if field(5, 2) == 1 => {
            let (alt, baro) = (field(9, 11), field(20, 9));
            let bit = |k: u32| field(k, 1) == 1;
            Msg::Target(Target {
                selected_alt_ft: (alt != 0).then(|| (alt as i32 - 1) * 32),
                selected_alt_fms: bit(8),
                baro_hpa: (baro != 0).then(|| 800.0 + (baro as f32 - 1.0) * 0.8),
                selected_heading_deg: bit(29).then(|| field(30, 9) as f32 * 180.0 / 256.0),
                modes: bit(46).then(|| {
                    [(47, "Autopilot"), (48, "VNAV"), (49, "Altitude hold"), (51, "Approach"), (52, "TCAS"), (53, "LNAV")].iter().filter(|m| bit(m.0)).map(|m| m.1).collect()
                }),
            })
        }
        31 if field(5, 3) <= 1 => Msg::Operational { version: field(40, 3) as u8, on_ground: field(5, 3) == 1 },
        _ => return None,
    };
    Some((icao, msg))
}

/// A short reply or Comm-B, with the address it is from: for most formats the parity syndrome, for DF11 its own field.
pub fn decode_reply(f: &[u8], overlaid: u32) -> Option<(u32, Msg)> {
    let df = f[0] >> 3;
    let low3 = (f[0] & 7) as u32;
    let thirteen = ((f[2] as u32) << 8 | f[3] as u32) & 0x1FFF;
    let fs = |m: &mut Msg| {
        if let Msg::Reply { on_ground, alert, spi, .. } = m {
            *on_ground = match low3 {
                0 | 2 => Some(false),
                1 | 3 => Some(true),
                _ => None,
            };
            *alert = matches!(low3, 2..=4);
            *spi = matches!(low3, 4 | 5);
        }
    };
    let mut m = Msg::Reply { alt_ft: None, squawk: None, on_ground: None, alert: false, spi: false };
    Some(match df {
        0 => {
            let on_ground = Some(f[0] & 4 != 0);
            (overlaid, Msg::Reply { alt_ft: altitude13(thirteen), squawk: None, on_ground, alert: false, spi: false })
        }
        4 | 20 => {
            fs(&mut m);
            if let Msg::Reply { alt_ft, .. } = &mut m {
                *alt_ft = altitude13(thirteen);
            }
            (overlaid, m)
        }
        5 | 21 => {
            fs(&mut m);
            if let Msg::Reply { squawk: sq, .. } = &mut m {
                *sq = Some(squawk(thirteen));
            }
            (overlaid, m)
        }
        11 => {
            let icao = (f[1] as u32) << 16 | (f[2] as u32) << 8 | f[3] as u32;
            let on_ground = match low3 {
                4 => Some(true),
                5 => Some(false),
                _ => None,
            };
            (icao, Msg::Reply { alt_ft: None, squawk: None, on_ground, alert: false, spi: false })
        }
        _ => return None,
    })
}

const FT_PER_M: f64 = 3.28084;

/// Wake and type category: TC and the 3-bit CA field.
fn category(tc: u32, ca: u32) -> Option<&'static str> {
    Some(match (tc, ca) {
        (2, 1) => "Surface emergency vehicle",
        (2, 2) => "Surface service vehicle",
        (2, 3) => "Ground obstruction",
        (3, 1) => "Glider",
        (3, 2) => "Lighter than air",
        (3, 3) => "Parachutist",
        (3, 4) => "Ultralight",
        (3, 6) => "Drone",
        (3, 7) => "Space vehicle",
        (4, 1) => "Light",
        (4, 2) => "Medium",
        (4, 3) => "Large",
        (4, 4) => "High vortex",
        (4, 5) => "Heavy",
        (4, 6) => "High performance",
        (4, 7) => "Helicopter",
        _ => return None,
    })
}

fn emergency(e: u32) -> Option<&'static str> {
    Some(match e {
        1 => "General emergency",
        2 => "Medical emergency",
        3 => "Minimum fuel",
        4 => "No communications",
        5 => "Unlawful interference",
        6 => "Downed aircraft",
        _ => return None,
    })
}

/// Surface speed, kt, from the 7-bit movement code: the low end of each code's step.
fn movement(m: u32) -> Option<f32> {
    let m = m as f32;
    Some(match m as u32 {
        1 => 0.0,
        2..=8 => 0.125 + (m - 2.0) * 0.125,
        9..=12 => 1.0 + (m - 9.0) * 0.25,
        13..=38 => 2.0 + (m - 13.0) * 0.5,
        39..=93 => 15.0 + (m - 39.0),
        94..=108 => 70.0 + (m - 94.0) * 2.0,
        109..=123 => 100.0 + (m - 109.0) * 5.0,
        124 => 175.0,
        _ => return None,
    })
}

/// The 13-bit identity field, C1 A1 C2 A2 C4 A4 X B1 D1 B2 D2 B4 D4, as four octal digits.
pub fn squawk(id: u32) -> String {
    let b = |k: u32| (id >> (12 - k)) & 1;
    let a = b(5) << 2 | b(3) << 1 | b(1);
    let bb = b(11) << 2 | b(9) << 1 | b(7);
    let c = b(4) << 2 | b(2) << 1 | b(0);
    let d = b(12) << 2 | b(10) << 1 | b(8);
    format!("{a}{bb}{c}{d}")
}

/// The 13-bit altitude of replies: the 12-bit code with an M bit (metres) in the 7th place.
pub fn altitude13(ac: u32) -> Option<i32> {
    if ac == 0 {
        return None;
    }
    let twelve = (ac & 0x1F80) >> 1 | (ac & 0x3F);
    if ac & 0x40 != 0 {
        return Some((twelve as f64 * FT_PER_M) as i32);
    }
    altitude(twelve)
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
        assert_eq!(decode(&hex("8D4840D6202CC371C32CE0576098")), Some((0x4840D6, Msg::Ident { callsign: "KLM1023".into(), category: None })));
    }

    #[test]
    fn airborne_position() {
        let (icao, m) = decode(&hex("8D40621D58C382D690C8AC2863A7")).unwrap();
        assert_eq!(icao, 0x40621D);
        assert_eq!(m, Msg::Position { alt_ft: Some(38_000), gnss_ft: None, odd: false, lat: 93_000, lon: 51_372 });
        let (_, m) = decode(&hex("8D40621D58C386435CC412692AD6")).unwrap();
        assert_eq!(m, Msg::Position { alt_ft: Some(38_000), gnss_ft: None, odd: true, lat: 74_158, lon: 50_194 });
    }

    #[test]
    fn ground_velocity() {
        let Some((0x485020, Msg::Velocity { speed_kt: Some(s), heading_deg: Some(h), vrate_fpm, airspeed: false, gnss_minus_baro_ft })) = decode(&hex("8D485020994409940838175B284F")) else { panic!() };
        assert!((s - 159.20).abs() < 0.01, "{s}");
        assert!((h - 182.88).abs() < 0.01, "{h}");
        assert_eq!(vrate_fpm, Some(-832));
        assert_eq!(gnss_minus_baro_ft, Some(550));
    }

    #[test]
    fn air_velocity() {
        let Some((0xA05F21, Msg::Velocity { speed_kt: Some(s), heading_deg: Some(h), vrate_fpm, airspeed: true, .. })) = decode(&hex("8DA05F219B06B6AF189400CBC33F")) else { panic!() };
        assert_eq!(s, 375.0);
        assert!((h - 243.98).abs() < 0.01, "{h}");
        assert_eq!(vrate_fpm, Some(-2304));
    }

    #[test]
    fn surface_movement_and_track() {
        let Some((0x484175, Msg::Surface { speed_kt: Some(s), track_deg: Some(t), odd, .. })) = decode(&hex("8C4841753A9A153237AEF0F275BE")) else { panic!() };
        assert_eq!(s, 17.0);
        assert!((t - 92.8125).abs() < 1e-3, "{t}");
        assert!(odd);
    }

    #[test]
    fn status_squawk() {
        assert_eq!(decode(&hex("8DA2C1B6E112B600000000760759")), Some((0xA2C1B6, Msg::Status { emergency: None, squawk: "6513".into() })));
    }

    #[test]
    fn target_state() {
        let Some((0xA05629, Msg::Target(t))) = decode(&hex("8DA05629EA21485CBF3F8CADAEEB")) else { panic!() };
        assert_eq!(t.selected_alt_ft, Some(16_992));
        assert!(!t.selected_alt_fms);
        assert!((t.baro_hpa.unwrap() - 1012.8).abs() < 0.01);
        assert!((t.selected_heading_deg.unwrap() - 66.8).abs() < 0.1);
        let modes = t.modes.unwrap();
        for m in ["Autopilot", "VNAV", "TCAS"] {
            assert!(modes.contains(&m), "{modes:?}");
        }
        assert!(!modes.contains(&"Altitude hold") && !modes.contains(&"Approach"));
    }

    #[test]
    fn short_replies() {
        let df4 = hex("2000171806A983");
        let Some((1, Msg::Reply { alt_ft, .. })) = decode_reply(&df4, 1) else { panic!() };
        assert_eq!(alt_ft, Some(36_000));
        let df5 = hex("2A00516D492B80");
        let Some((1, Msg::Reply { squawk, alt_ft: None, .. })) = decode_reply(&df5, 1) else { panic!() };
        assert_eq!(squawk.as_deref(), Some("0356"));
        let df11 = hex("5D484FDEA248F5");
        let Some((0x484FDE, Msg::Reply { on_ground: Some(false), .. })) = decode_reply(&df11, 0) else { panic!() };
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
