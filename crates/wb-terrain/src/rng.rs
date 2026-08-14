//! A small deterministic generator.
//!
//! Hand-rolled rather than pulled from `rand` for one reason: terrain is a *cache key*.
//! The same world file must produce the same coastline on every machine and every
//! release, and `rand`'s algorithms are explicitly allowed to change between versions.
//! SplitMix64 is eight lines and frozen forever.

/// SplitMix64. Seeded, deterministic, and independent of any crate's release policy.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Any seed works, including zero — SplitMix64 has no bad states.
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn f64(&mut self) -> f64 {
        // Top 53 bits: exactly the mantissa an f64 can hold without rounding.
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.f64() * (hi - lo)
    }

    /// Uniform in `[0, n)`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }
}

/// FNV-1a over raw bytes. Used for cache keys, so it must stay stable forever —
/// `DefaultHasher` explicitly does not promise that.
pub struct Digest(u64);

impl Default for Digest {
    fn default() -> Self {
        Self::new()
    }
}

impl Digest {
    pub fn new() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }

    pub fn bytes(&mut self, data: &[u8]) -> &mut Self {
        for b in data {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x1000_0000_01B3);
        }
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn f64(&mut self, v: f64) -> &mut Self {
        // Normalize the two zeroes and every NaN payload so equal params digest equally.
        let v = if v == 0.0 {
            0.0
        } else if v.is_nan() {
            f64::NAN
        } else {
            v
        };
        self.bytes(&v.to_bits().to_le_bytes())
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.f64(f64::from(v))
    }

    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_stream() {
        let a: Vec<u64> = (0..8).scan(Rng::new(812), |r, _| Some(r.next_u64())).collect();
        let b: Vec<u64> = (0..8).scan(Rng::new(812), |r, _| Some(r.next_u64())).collect();
        assert_eq!(a, b);
        assert_ne!(a, (0..8).scan(Rng::new(813), |r, _| Some(r.next_u64())).collect::<Vec<_>>());
    }

    #[test]
    fn floats_stay_in_the_unit_interval() {
        let mut rng = Rng::new(7);
        for _ in 0..10_000 {
            let v = rng.f64();
            assert!((0.0..1.0).contains(&v), "{v} escaped [0, 1)");
        }
    }

    #[test]
    fn the_mean_of_many_draws_is_near_a_half() {
        let mut rng = Rng::new(99);
        let n = 100_000;
        let mean: f64 = (0..n).map(|_| rng.f64()).sum::<f64>() / f64::from(n);
        assert!((mean - 0.5).abs() < 0.01, "mean {mean} is not uniform");
    }

    #[test]
    fn the_digest_separates_the_two_zeroes_from_nothing() {
        // -0.0 and 0.0 are the same number and must not invalidate a cache.
        let mut a = Digest::new();
        let mut b = Digest::new();
        assert_eq!(a.f64(0.0).finish(), b.f64(-0.0).finish());
    }

    #[test]
    fn the_digest_reacts_to_every_field() {
        let mut a = Digest::new();
        let mut b = Digest::new();
        assert_ne!(a.f64(0.30).str("x").finish(), b.f64(0.31).str("x").finish());
    }
}
