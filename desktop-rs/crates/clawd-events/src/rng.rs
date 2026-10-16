//! The two random draws `events.py` makes (`rng.choice`, `rng.uniform`), behind a trait so the
//! golden tests can pin them (the Python side passes a matching fake).

use std::time::{SystemTime, UNIX_EPOCH};

/// `random.Random` as the Tracker and the Pet use it.
pub trait Rng: Send {
    /// `rng.choice(seq)`: an index below `len` (`len > 0`).
    fn below(&mut self, len: usize) -> usize;
    /// `rng.uniform(a, b)`.
    fn uniform(&mut self, a: f64, b: f64) -> f64;
}

/// xorshift64*, seeded from the system clock: nothing here needs more than "not predictable by
/// the user's eye".
pub struct SystemRng {
    state: u64,
}

impl SystemRng {
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::seeded(nanos)
    }

    pub fn seeded(seed: u64) -> Self {
        Self {
            state: seed | 1, // xorshift must not start at 0
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl Default for SystemRng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng for SystemRng {
    fn below(&mut self, len: usize) -> usize {
        (self.next() % len.max(1) as u64) as usize
    }

    fn uniform(&mut self, a: f64, b: f64) -> f64 {
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        a + (b - a) * unit
    }
}

/// Deterministic draws for goldens: always the first choice, `uniform` is the lower bound.
/// `python-0.1.x:tools/golden/export_events.py` passes the same behaviour to the Python classes.
#[derive(Debug, Clone, Copy, Default)]
pub struct FirstRng;

impl Rng for FirstRng {
    fn below(&mut self, _len: usize) -> usize {
        0
    }

    fn uniform(&mut self, a: f64, _b: f64) -> f64 {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_rng_stays_in_range_and_varies() {
        let mut rng = SystemRng::seeded(7);
        let mut seen = [false; 5];
        for _ in 0..200 {
            seen[rng.below(5)] = true;
            let u = rng.uniform(180.0, 360.0);
            assert!((180.0..360.0).contains(&u));
        }
        assert!(seen.iter().all(|s| *s));
    }
}
