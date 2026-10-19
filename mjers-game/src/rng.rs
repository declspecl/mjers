//! Deterministic, portable randomness.
//!
//! xoshiro256** seeded through SplitMix64. Implemented here rather than taken
//! from a crate so a seed reproduces the same games on every platform
//! (including WASM) and across dependency upgrades.

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(GOLDEN);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// An independent seed for sub-stream `stream` of `seed`, such as hand `n`
/// of a match.
pub fn derive_seed(
    seed: u64,
    stream: u64,
) -> u64 {
    let mut state = seed ^ stream.wrapping_mul(GOLDEN).rotate_left(17);
    splitmix64(&mut state)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: [u64; 4],
}

impl Rng {
    pub fn seed(seed: u64) -> Rng {
        let mut sm = seed;
        Rng {
            state: std::array::from_fn(|_| splitmix64(&mut sm)),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Uniform in `0..bound` (Lemire's method, unbiased).
    pub fn below(
        &mut self,
        bound: u64,
    ) -> u64 {
        assert!(bound > 0, "empty range");
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let product = self.next_u64() as u128 * bound as u128;
            if (product as u64) >= threshold {
                return (product >> 64) as u64;
            }
        }
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn shuffle<T>(
        &mut self,
        items: &mut [T],
    ) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::seed(42);
        let mut b = Rng::seed(42);
        let mut c = Rng::seed(43);
        let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        assert_eq!(xs, (0..8).map(|_| b.next_u64()).collect::<Vec<_>>());
        assert_ne!(xs, (0..8).map(|_| c.next_u64()).collect::<Vec<_>>());
    }

    #[test]
    fn stream_is_pinned() {
        // Changing the generator silently changes every recorded experiment.
        let mut rng = Rng::seed(0);
        assert_eq!(rng.next_u64(), 0x99EC_5F36_CB75_F2B4);
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut rng = Rng::seed(7);
        let mut seen = [0u32; 6];
        for _ in 0..6000 {
            seen[rng.below(6) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| (800..1200).contains(&n)), "{seen:?}");
    }

    #[test]
    fn unit_in_half_open_interval() {
        let mut rng = Rng::seed(1);
        assert!((0..1000).map(|_| rng.unit()).all(|x| (0.0..1.0).contains(&x)));
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = Rng::seed(3);
        let mut items: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut items);
        assert_ne!(items, (0..50).collect::<Vec<_>>());
        items.sort();
        assert_eq!(items, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn derived_seeds_differ() {
        let seeds: std::collections::HashSet<u64> = (0..1000).map(|i| derive_seed(9, i)).collect();
        assert_eq!(seeds.len(), 1000);
    }
}
