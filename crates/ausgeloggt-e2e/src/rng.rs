//! Tiny deterministic PRNG (SplitMix64). No external dependencies, stable across platforms.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64) < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u64() as u8).collect()
    }

    /// Random string of `n` chars drawn from a mixed pool (ASCII, umlauts, emoji, CJK, RTL,
    /// control characters, separators that are special in headers / CSV / JSON).
    pub fn nasty_string(&mut self, n: usize) -> String {
        const POOL: &[&str] = &[
            "a",
            "B",
            "z",
            "0",
            "9",
            " ",
            "  ",
            "\t",
            "\n",
            "\r\n",
            "\r",
            "ä",
            "ö",
            "ü",
            "ß",
            "é",
            "ñ",
            "😀",
            "🔥",
            "日本語",
            "漢字",
            "مرحبا",
            "שלום",
            "\u{202e}",
            "\u{200b}",
            "\u{feff}",
            "\u{0}",
            "\u{7f}",
            "\"",
            "'",
            ",",
            ";",
            "\\",
            "<",
            ">",
            "@",
            ":",
            "=",
            "?",
            "=?utf-8?q?",
            "=?UTF-8?B?",
            "?=",
            "=C3=A4",
            "%",
            "%41",
            "{",
            "}",
            "[",
            "]",
            "/",
            "..",
            "http://",
            "https://",
            "mailto:",
            "android://",
            "-",
            "–",
            "—",
            "€",
            "$",
            "£",
            "1.234,56",
        ];
        (0..n).map(|_| *self.pick(POOL)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn range_bounds() {
        let mut r = Rng::new(1);
        for _ in 0..1000 {
            let v = r.range(-3, 5);
            assert!((-3..=5).contains(&v));
        }
    }
}
