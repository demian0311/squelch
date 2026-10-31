//! RDS (Radio Data System): station name, programme type and text sent alongside FM broadcasts.
//!
//! `phy` turns the 57 kHz subcarrier into bits, `blocks` finds and checks the 26-bit blocks,
//! and this module reads the groups: 0A/0B for the station name (PS), 2A/2B for RadioText,
//! and RadioText Plus, when announced, for the song title and artist.

pub mod blocks;
pub mod phy;
pub mod text;

use blocks::{Blocks, Group};
use phy::Phy;
use serde::Serialize;

/// RadioText Plus application ID, announced in group 3A.
const RT_PLUS_AID: u16 = 0x4BD7;
const RT_PLUS_TITLE: u8 = 1;
const RT_PLUS_ARTIST: u8 = 4;
const TERMINATOR: u8 = 0x0D;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Info {
    pub pi: Option<u16>,
    /// US call letters worked out from the PI code.
    pub call: Option<String>,
    /// Programme service name: 8 characters, often the station's brand.
    pub ps: Option<String>,
    pub radiotext: Option<String>,
    pub pty: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
}

pub struct Rds {
    phy: Phy,
    blocks: Blocks,
    bits: Vec<bool>,
    groups: Vec<Group>,
    info: Info,
    last_pi: Option<u16>,
    ps: [u8; 8],
    ps_got: u8,
    rt: [u8; 64],
    rt_got: u16,
    rt_flag: Option<bool>,
    rt_version_b: bool,
    /// Group type code (number << 1 | version) carrying RadioText Plus tags.
    rt_plus: Option<u8>,
}

impl Rds {
    pub fn new(mpx_rate: u32) -> Self {
        Rds {
            phy: Phy::new(mpx_rate),
            blocks: Blocks::new(),
            bits: Vec::new(),
            groups: Vec::new(),
            info: Info::default(),
            last_pi: None,
            ps: [b' '; 8],
            ps_got: 0,
            rt: [b' '; 64],
            rt_got: 0,
            rt_flag: None,
            rt_version_b: false,
            rt_plus: None,
        }
    }

    /// Forget the station; the receive chain keeps its lock.
    pub fn clear(&mut self) {
        self.info = Info::default();
        self.last_pi = None;
        self.ps = [b' '; 8];
        self.ps_got = 0;
        self.rt = [b' '; 64];
        self.rt_got = 0;
        self.rt_flag = None;
        self.rt_plus = None;
    }

    pub fn info(&self) -> &Info {
        &self.info
    }

    pub fn stats(&self) -> blocks::Stats {
        self.blocks.stats
    }

    pub fn lock(&self) -> (f32, f32, f64) {
        let (f, p) = self.phy.lock();
        (f, p, self.phy.pilot_hz())
    }

    pub fn synced(&self) -> bool {
        self.blocks.synced()
    }

    /// Feed FM multiplex samples (before de-emphasis).
    pub fn process(&mut self, mpx: &[f32]) {
        self.bits.clear();
        self.phy.process(mpx, &mut self.bits);
        self.groups.clear();
        for i in 0..self.bits.len() {
            self.blocks.push(self.bits[i], &mut self.groups);
        }
        for g in std::mem::take(&mut self.groups) {
            self.group(&g);
        }
    }

    pub fn group(&mut self, g: &Group) {
        if let Some(pi) = g.a {
            // two in a row before believing a new PI
            if self.last_pi == Some(pi) && self.info.pi != Some(pi) {
                self.info.pi = Some(pi);
                self.info.call = text::call_letters(pi);
            }
            self.last_pi = Some(pi);
        }
        let Some(b) = g.b else { return };
        let kind = (b >> 12) as u8;
        let version_b = b & 0x800 != 0;
        let name = text::PTY[((b >> 5) & 0x1F) as usize];
        self.info.pty = (!name.is_empty()).then(|| name.to_string());
        if self.rt_plus == Some(kind << 1 | version_b as u8) {
            self.rt_plus_tags(b, g.c, g.d);
        }
        match (kind, version_b) {
            (0, _) => {
                if let Some(d) = g.d {
                    let at = (b & 3) as usize * 2;
                    if at == 0 {
                        self.ps_got = 0;
                    }
                    self.ps[at] = (d >> 8) as u8;
                    self.ps[at + 1] = d as u8;
                    self.ps_got |= 1 << (b & 3);
                    if self.ps_got == 0xF {
                        self.info.ps = Some(decode(&self.ps)).filter(|s| !s.is_empty());
                    }
                }
            }
            (2, vb) => self.radiotext(b, vb, g.c, g.d),
            (3, false) => {
                if g.d == Some(RT_PLUS_AID) {
                    self.rt_plus = Some((b & 0x1F) as u8);
                }
            }
            _ => {}
        }
    }

    fn radiotext(&mut self, b: u16, version_b: bool, c: Option<u16>, d: Option<u16>) {
        let flag = b & 0x10 != 0;
        if self.rt_flag != Some(flag) || self.rt_version_b != version_b {
            self.rt_flag = Some(flag);
            self.rt_version_b = version_b;
            self.rt = [b' '; 64];
            self.rt_got = 0;
        }
        let seg = (b & 0xF) as usize;
        let chars: Vec<u8> = if version_b {
            let Some(d) = d else { return };
            vec![(d >> 8) as u8, d as u8]
        } else {
            let (Some(c), Some(d)) = (c, d) else { return };
            vec![(c >> 8) as u8, c as u8, (d >> 8) as u8, d as u8]
        };
        let at = seg * chars.len();
        self.rt[at..at + chars.len()].copy_from_slice(&chars);
        self.rt_got |= 1 << seg;
        let len = if version_b { 32 } else { 64 };
        let end = self.rt[..len].iter().position(|&ch| ch == TERMINATOR).map_or(len, |p| p + 1);
        let need = end.div_ceil(chars.len());
        if (0..need).all(|s| self.rt_got & (1 << s) != 0) {
            let text = decode(&self.rt[..end]);
            if !text.is_empty() {
                self.info.radiotext = Some(text);
            }
        }
    }

    /// RadioText Plus: content type, start and length of up to two tagged spans of RadioText.
    fn rt_plus_tags(&mut self, b: u16, c: Option<u16>, d: Option<u16>) {
        let (Some(c), Some(d)) = (c, d) else { return };
        let Some(rt) = self.info.radiotext.clone() else { return };
        let tags = [
            (((b & 7) << 3 | c >> 13) as u8, ((c >> 7) & 0x3F) as usize, ((c >> 1) & 0x3F) as usize + 1),
            (((c & 1) << 5 | d >> 11) as u8, ((d >> 5) & 0x3F) as usize, (d & 0x1F) as usize + 1),
        ];
        let chars: Vec<char> = rt.chars().collect();
        for (kind, start, len) in tags {
            if kind == 0 || start >= chars.len() {
                continue;
            }
            let span: String = chars[start..(start + len).min(chars.len())].iter().collect::<String>().trim().to_string();
            match kind {
                RT_PLUS_TITLE => self.info.title = Some(span),
                RT_PLUS_ARTIST => self.info.artist = Some(span),
                _ => {}
            }
        }
    }
}

/// RDS bytes to text, stopping at the terminator and trimming padding.
fn decode(bytes: &[u8]) -> String {
    bytes.iter().take_while(|&&b| b != TERMINATOR).map(|&b| text::char_of(b)).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(a: u16, b: u16, c: u16, d: u16) -> Group {
        Group { a: Some(a), b: Some(b), c: Some(c), d: Some(d), c_prime: false }
    }

    fn pair(s: &str, i: usize) -> u16 {
        let b = s.as_bytes();
        (b[i] as u16) << 8 | b[i + 1] as u16
    }

    /// Groups a station would send: name, 64-char RadioText, RT+ announcement and tags.
    pub fn station_groups() -> Vec<Group> {
        let pi = 0x3AAB;
        // PTY 5 (rock) sits in bits 9..5 of block B
        let pty = 5 << 5;
        let mut out = Vec::new();
        for _ in 0..2 {
            for seg in 0..4u16 {
                out.push(g(pi, pty | seg, 0, pair("KQED FM ", seg as usize * 2)));
            }
        }
        let rt = "Now playing Bowie - Heroes\r";
        let padded = format!("{rt:<64}");
        for seg in 0..7u16 {
            let i = seg as usize * 4;
            out.push(g(pi, 2 << 12 | pty | seg, pair(&padded, i), pair(&padded, i + 2)));
        }
        // announce RT+ in group 12A: number 12, version A
        out.push(g(pi, 3 << 12 | pty | 12 << 1, 0, RT_PLUS_AID));
        // title "Heroes" at 20 (6 chars), artist "Bowie" at 12 (5 chars)
        let (t1, s1, l1) = (RT_PLUS_TITLE as u16, 20u16, 6u16 - 1);
        let (t2, s2, l2) = (RT_PLUS_ARTIST as u16, 12u16, 5u16 - 1);
        let b = 12 << 12 | pty | (t1 >> 3);
        let c = (t1 & 7) << 13 | s1 << 7 | l1 << 1 | (t2 >> 5);
        let d = (t2 & 0x1F) << 11 | s2 << 5 | l2;
        out.push(g(pi, b, c, d));
        out
    }

    #[test]
    fn reads_name_text_and_song() {
        let mut r = Rds::new(240_000);
        for grp in station_groups() {
            r.group(&grp);
        }
        let i = r.info();
        assert_eq!(i.call.as_deref(), Some("KQED"));
        assert_eq!(i.ps.as_deref(), Some("KQED FM"));
        assert_eq!(i.radiotext.as_deref(), Some("Now playing Bowie - Heroes"));
        assert_eq!(i.pty.as_deref(), Some("Rock"));
        assert_eq!((i.title.as_deref(), i.artist.as_deref()), (Some("Heroes"), Some("Bowie")));
    }
}
