//! RDS text fields: the character set, RBDS call letters and programme types.

/// EN 50067 Annex E, rows 0x80–0xFF.
const HIGH: [&str; 8] = [
    "áàéèíìóòúùÑÇŞβ¡Ĳ",
    "âäêëîïôöûüñçşǧıĳ",
    "ªα©‰Ǧěňőπ€£$←↑→↓",
    "º¹²³±İńűµ¿÷°¼½¾§",
    "ÁÀÉÈÍÌÓÒÚÙŘČŠŽÐĿ",
    "ÂÄÊËÎÏÔÖÛÜřčšžđŀ",
    "ÃÅÆŒŷÝÕØÞŊŔĆŚŹŦð",
    "ãåæœŵýõøþŋŕćśźŧ ",
];

/// One RDS character byte as Unicode.
pub fn char_of(b: u8) -> char {
    match b {
        0x24 => '¤',
        0x5E => '―',
        0x60 => '‖',
        0x7E => '¯',
        0x0A => '\n',
        0x20..=0x7D => b as char,
        0x80..=0xFF => HIGH[(b as usize - 0x80) / 16].chars().nth(b as usize % 16).unwrap_or(' '),
        _ => ' ',
    }
}

/// North American programme types (RBDS).
pub const PTY: [&str; 32] = [
    "", "News", "Information", "Sports", "Talk", "Rock", "Classic rock", "Adult hits", "Soft rock", "Top 40", "Country", "Oldies", "Soft", "Nostalgia",
    "Jazz", "Classical", "Rhythm and blues", "Soft rhythm and blues", "Language", "Religious music", "Religious talk", "Personality", "Public", "College",
    "Spanish talk", "Spanish music", "Hip hop", "", "", "Weather", "Emergency test", "Emergency",
];

/// Three-letter calls have their own codes; only the first ones are listed in the standard's table.
const THREE: [(u16, &str); 8] = [(0x9950, "KEX"), (0x9965, "KSL"), (0x9978, "WHO"), (0x99B9, "WRC"), (0x99AB, "KOB"), (0x9988, "WWJ"), (0x9989, "WWL"), (0x9992, "KOY")];

/// US call letters from a PI code (NRSC-4-B), when the code encodes them.
pub fn call_letters(pi: u16) -> Option<String> {
    let mut pi = pi;
    if pi & 0xFFF0 == 0xAFA0 && pi & 0xF < 0xA {
        pi <<= 12;
    } else if pi & 0xFF00 == 0xAF00 {
        pi <<= 8;
    } else if pi & 0xF000 == 0xA000 {
        pi = ((pi & 0x0F00) << 4) | (pi & 0x00FF);
    }
    if (0x9950..=0x9EFF).contains(&pi) {
        return THREE.iter().find(|(p, _)| *p == pi).map(|(_, c)| c.to_string());
    }
    if !(0x1000..=0x994F).contains(&pi) {
        return None;
    }
    let (first, n) = if pi <= 0x54A7 { ('K', pi - 0x1000) } else { ('W', pi - 0x54A8) };
    let l = |v: u16| (b'A' + (v % 26) as u8) as char;
    Some(format!("{first}{}{}{}", l(n / 676), l(n / 26), l(n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_letters_follow_nrsc() {
        assert_eq!(call_letters(0x101C).as_deref(), Some("KABC"));
        assert_eq!(call_letters(0x796E).as_deref(), Some("WNYC"));
        assert_eq!(call_letters(0x3AAB).as_deref(), Some("KQED"));
        assert_eq!(call_letters(0x54A8).as_deref(), Some("WAAA"));
        assert_eq!(call_letters(0xAF12).as_deref(), Some("KATS"));
        assert_eq!(call_letters(0xB001), None);
    }

    #[test]
    fn characters() {
        assert_eq!(char_of(b'A'), 'A');
        assert_eq!(char_of(0x24), '¤');
        assert_eq!(char_of(0x82), 'é');
    }
}
