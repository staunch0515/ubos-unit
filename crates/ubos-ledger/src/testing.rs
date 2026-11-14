//! A small deterministic generator for the property tests of this crate.

/// SplitMix64.
pub(crate) struct Rng(pub u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`.
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A date `2026-01-01 + k` days with `k < span`.
    pub(crate) fn date(&mut self, span: u64) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date")
            + chrono::Duration::days(self.below(span) as i64)
    }
}
