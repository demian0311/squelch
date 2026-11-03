//! What a 24-bit address alone says: the country it was allocated to and, where the allocation is
//! algorithmic, the registration. Both from wiedehopf's tar1090 (`flags.js`, `registrations.js`),
//! fetched 2026-11-02. An address with no rule here has no registration, not a wrong one.

use std::sync::OnceLock;

const LIMITED: &[u8; 24] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

fn countries() -> &'static Vec<(u32, u32, &'static str)> {
    static TABLE: OnceLock<Vec<(u32, u32, &'static str)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("countries.txt")
            .lines()
            .filter_map(|l| {
                let (range, name) = l.split_once(' ')?;
                let (a, b) = range.split_once('-')?;
                Some((u32::from_str_radix(a, 16).ok()?, u32::from_str_radix(b, 16).ok()?, name))
            })
            .collect()
    })
}

/// First match wins: the file lists territories before the larger block around them.
pub fn country(address: u32) -> Option<&'static str> {
    countries().iter().find(|c| address >= c.0 && address <= c.1).map(|c| c.2)
}

fn n_letter(r: u32) -> String {
    if r == 0 { String::new() } else { (LIMITED[r as usize - 1] as char).to_string() }
}

fn n_letters(r: u32) -> String {
    if r == 0 {
        return String::new();
    }
    let r = r - 1;
    format!("{}{}", LIMITED[(r / 25) as usize] as char, n_letter(r % 25))
}

fn n_number(address: u32) -> Option<String> {
    let mut off = address.checked_sub(0xA00001)?;
    if off >= 915_399 {
        return None;
    }
    let mut s = format!("N{}", off / 101_711 + 1);
    off %= 101_711;
    for (div, _) in [(10_111, 2), (951, 3)] {
        if off <= 600 {
            return Some(s + &n_letters(off));
        }
        off -= 601;
        s += &(off / div).to_string();
        off %= div;
    }
    if off <= 600 {
        return Some(s + &n_letters(off));
    }
    off -= 601;
    s += &(off / 35).to_string();
    off %= 35;
    Some(if off <= 24 { s + &n_letter(off) } else { s + &(off - 25).to_string() })
}

fn japan(address: u32) -> Option<String> {
    let off = address.checked_sub(0x840000)?;
    if off >= 229_840 {
        return None;
    }
    let mut s = format!("JA{}{}", off / 22_984, (off % 22_984) / 916);
    let r = off % 22_984 % 916;
    if r < 340 {
        s += &(r / 34).to_string();
        let r = r % 34;
        if r < 10 {
            s += &r.to_string();
        } else {
            s.push(LIMITED[r as usize - 10] as char);
        }
    } else {
        let r = r - 340;
        s.push(LIMITED[(r / 24) as usize] as char);
        s.push(LIMITED[(r % 24) as usize] as char);
    }
    Some(s)
}

fn korea(a: u32) -> Option<String> {
    let v = match a {
        0x71BA00..=0x71BF99 => a - 0x71BA00 + 0x7200,
        0x71C000..=0x71C099 => a - 0x71C000 + 0x8000,
        0x71C200..=0x71C299 => a - 0x71C200 + 0x8200,
        _ => return None,
    };
    Some(format!("HL{v:X}"))
}

/// Start, stride 1, stride 2, prefix: three letters counted off the address.
const STRIDES: [(u32, u32, u32, &str); 29] = [
    (0x380000, 1024, 32, "F-B"),
    (0x388000, 1024, 32, "F-I"),
    (0x390000, 1024, 32, "F-G"),
    (0x398000, 1024, 32, "F-H"),
    (0x3A0000, 1024, 32, "F-O"),
    (0x3C4421, 1024, 32, "D-A"),
    (0x3C0001, 676, 26, "D-A"),
    (0x3C8421, 1024, 32, "D-B"),
    (0x3C2001, 676, 26, "D-B"),
    (0x3CC000, 676, 26, "D-C"),
    (0x3D04A8, 676, 26, "D-E"),
    (0x3D4950, 676, 26, "D-F"),
    (0x3D8DF8, 676, 26, "D-G"),
    (0x3DD2A0, 676, 26, "D-H"),
    (0x3E1748, 676, 26, "D-I"),
    (0x448421, 1024, 32, "OO-"),
    (0x458421, 1024, 32, "OY-"),
    (0x460000, 676, 26, "OH-"),
    (0x468421, 1024, 32, "SX-"),
    (0x490421, 1024, 32, "CS-"),
    (0x4A0421, 1024, 32, "YR-"),
    (0x4B8421, 1024, 32, "TC-"),
    (0x740421, 1024, 32, "JY-"),
    (0x760421, 1024, 32, "AP-"),
    (0x768421, 1024, 32, "9V-"),
    (0x778421, 1024, 32, "YK-"),
    (0xC00001, 676, 26, "C-F"),
    (0xC044A9, 676, 26, "C-G"),
    (0xE01041, 4096, 64, "LV-"),
];

/// Letters AAA to ZZZ per stride block; D-A and D-B split into two ranges at PAA.
fn stride(address: u32) -> Option<String> {
    for &(start, s1, s2, prefix) in &STRIDES {
        let (first, last) = match (start, prefix) {
            (0x3C4421 | 0x3C8421, _) => (0, 14 * 676 + 25 * 26 + 25),
            (0x3C0001 | 0x3C2001, _) => (15 * 676, 25 * 676 + 25 * 26 + 25),
            _ => (0, 25 * 676 + 25 * 26 + 25),
        };
        let Some(off) = address.checked_sub(start) else { continue };
        // the offset of `first`, in the same strides as the block
        let base = first / 676 * s1 + first / 26 % 26 * s2 + first % 26;
        let o = off + base;
        let (i1, i2, i3) = (o / s1, o % s1 / s2, o % s2);
        if i1 >= 26 || i2 >= 26 || i3 >= 26 {
            continue;
        }
        if i1 * 676 + i2 * 26 + i3 > last {
            continue;
        }
        let ch = |i: u32| (b'A' + i as u8) as char;
        return Some(format!("{prefix}{}{}{}", ch(i1), ch(i2), ch(i3)));
    }
    None
}

fn numeric(a: u32) -> Option<String> {
    if (0x140000..0x140000 + 100_000).contains(&a) {
        return Some(format!("RA-{:05}", a - 0x140000));
    }
    if (0x0B03E8..0x0B03E8 + 1000).contains(&a) {
        return Some(format!("CU-T{:04}", a - 0x0B03E8 + 1000));
    }
    None
}

pub fn registration(address: u32) -> Option<String> {
    n_number(address).or_else(|| japan(address)).or_else(|| korea(address)).or_else(|| numeric(address)).or_else(|| stride(address))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countries_first_match_wins() {
        assert_eq!(country(0xA835AF), Some("United States"));
        assert_eq!(country(0x4840D6), Some("Netherlands"));
        assert_eq!(country(0x424200), Some("Bermuda"));
        assert_eq!(country(0x400400), Some("United Kingdom"));
        assert_eq!(country(0x789100), Some("Hong Kong"));
        assert_eq!(country(0xFFFFFF), None);
    }

    #[test]
    fn us_n_numbers() {
        assert_eq!(registration(0xA00001).as_deref(), Some("N1"));
        assert_eq!(registration(0xA00002).as_deref(), Some("N1A"));
        assert_eq!(registration(0xA835AF).as_deref(), Some("N628TS"));
        assert_eq!(registration(0xADF7C7).as_deref(), Some("N99999"));
    }

    #[test]
    fn stride_blocks() {
        assert_eq!(registration(0x380000).as_deref(), Some("F-BAAA"));
        assert_eq!(registration(0xC00001).as_deref(), Some("C-FAAA"));
        assert_eq!(registration(0x3C0001).as_deref(), Some("D-APAA"));
        assert_eq!(registration(0x3C4421).as_deref(), Some("D-AAAA"));
    }

    #[test]
    fn no_rule_means_no_registration() {
        assert_eq!(registration(0x4840D6), None);
    }
}
