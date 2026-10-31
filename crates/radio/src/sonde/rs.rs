//! Reed-Solomon over GF(256) with field polynomial 0x11D and roots α^0..α^(nroots-1).
//! Codeword index i is the coefficient of x^i, parity in the low `nroots` positions.

pub struct Gf {
    exp: [u8; 512],
    log: [u8; 256],
}

impl Gf {
    pub fn new() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x = 1u16;
        for i in 0..255 {
            exp[i] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11D;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        Gf { exp, log }
    }

    pub fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
    }

    pub fn div(&self, a: u8, b: u8) -> u8 {
        if a == 0 {
            return 0;
        }
        self.exp[self.log[a as usize] as usize + 255 - self.log[b as usize] as usize]
    }

    /// α^n for any n ≥ 0.
    pub fn pow(&self, n: usize) -> u8 {
        self.exp[n % 255]
    }

    fn eval(&self, poly: &[u8], x: u8) -> u8 {
        poly.iter().rev().fold(0, |acc, &c| self.mul(acc, x) ^ c)
    }
}

impl Default for Gf {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Rs {
    gf: Gf,
    nroots: usize,
}

impl Rs {
    pub fn new(nroots: usize) -> Self {
        Rs { gf: Gf::new(), nroots }
    }

    /// Fill `cw[..nroots]` with parity for the message in `cw[nroots..]`.
    pub fn encode(&self, cw: &mut [u8]) {
        let gf = &self.gf;
        // generator g(x) = Π (x - α^i), lowest coefficient first
        let mut g = vec![1u8];
        for i in 0..self.nroots {
            let r = gf.pow(i);
            let mut next = vec![0u8; g.len() + 1];
            for (j, &c) in g.iter().enumerate() {
                next[j + 1] ^= c;
                next[j] ^= gf.mul(c, r);
            }
            g = next;
        }
        // remainder of m(x)·x^nroots divided by g(x)
        let mut rem = cw.to_vec();
        rem[..self.nroots].fill(0);
        for i in (self.nroots..rem.len()).rev() {
            let c = rem[i];
            if c != 0 {
                for (j, &gj) in g.iter().enumerate() {
                    rem[i - self.nroots + j] ^= gf.mul(c, gj);
                }
            }
        }
        cw[..self.nroots].copy_from_slice(&rem[..self.nroots]);
    }

    /// Correct `cw` in place. Errors may only fall below `used` (the rest is zero padding).
    /// Returns the number of corrected bytes, or `None` if the codeword is beyond repair.
    pub fn decode(&self, cw: &mut [u8], used: usize) -> Option<usize> {
        let gf = &self.gf;
        let n = self.nroots;
        let s: Vec<u8> = (0..n).map(|j| gf.eval(cw, gf.pow(j))).collect();
        if s.iter().all(|&v| v == 0) {
            return Some(0);
        }
        // Berlekamp-Massey: error locator Λ(x)
        let mut lambda = vec![0u8; n + 1];
        lambda[0] = 1;
        let mut b = lambda.clone();
        let mut l = 0usize;
        let mut m = 1usize;
        let mut bb = 1u8;
        for k in 0..n {
            let mut d = s[k];
            for i in 1..=l {
                d ^= gf.mul(lambda[i], s[k - i]);
            }
            if d == 0 {
                m += 1;
                continue;
            }
            let coef = gf.div(d, bb);
            let prev = lambda.clone();
            for i in m..=n {
                lambda[i] ^= gf.mul(coef, b[i - m]);
            }
            if 2 * l <= k {
                l = k + 1 - l;
                b = prev;
                bb = d;
                m = 1;
            } else {
                m += 1;
            }
        }
        lambda.truncate(l + 1);
        // Ω(x) = S(x)Λ(x) mod x^n
        let mut omega = vec![0u8; n];
        for (i, &li) in lambda.iter().enumerate() {
            for (j, &sj) in s.iter().enumerate() {
                if i + j < n {
                    omega[i + j] ^= gf.mul(li, sj);
                }
            }
        }
        // Λ'(x): only odd terms survive in characteristic 2
        let deriv: Vec<u8> = (1..lambda.len()).map(|i| if i % 2 == 1 { lambda[i] } else { 0 }).collect();
        let mut fixed = 0;
        for pos in 0..cw.len() {
            let xinv = gf.pow(255 - pos % 255);
            if gf.eval(&lambda, xinv) != 0 {
                continue;
            }
            if pos >= used {
                return None;
            }
            let den = gf.eval(&deriv, xinv);
            if den == 0 {
                return None;
            }
            // Forney with first root α^0: e = X · Ω(X⁻¹) / Λ'(X⁻¹)
            let e = gf.mul(gf.pow(pos), gf.div(gf.eval(&omega, xinv), den));
            cw[pos] ^= e;
            fixed += 1;
        }
        if fixed != l {
            return None;
        }
        Some(fixed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codeword(used: usize) -> (Rs, Vec<u8>) {
        let rs = Rs::new(24);
        let mut cw = vec![0u8; 255];
        for i in 24..used {
            cw[i] = (i * 37 + 11) as u8;
        }
        rs.encode(&mut cw);
        (rs, cw)
    }

    #[test]
    fn clean_codeword_has_no_errors() {
        let (rs, mut cw) = codeword(156);
        assert_eq!(rs.decode(&mut cw, 156), Some(0));
    }

    #[test]
    fn corrects_up_to_twelve_errors() {
        let (rs, good) = codeword(156);
        let mut cw = good.clone();
        for k in 0..12 {
            cw[k * 13 + 1] ^= 0x5A ^ k as u8;
        }
        assert_eq!(rs.decode(&mut cw, 156), Some(12));
        assert_eq!(cw, good);
    }

    #[test]
    fn refuses_too_many_errors() {
        let (rs, good) = codeword(156);
        let mut cw = good.clone();
        for k in 0..20 {
            cw[k * 7 + 3] ^= 0xFF;
        }
        assert!(rs.decode(&mut cw, 156).is_none_or(|_| cw != good));
    }
}
