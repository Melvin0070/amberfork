//! Stable, dependency-free hashing for protocol machinery. The values this module produces
//! are part of the reproducibility promise (pre-registered protocol rule 5): committed
//! constants, no external hash crate whose output could shift under a version bump.

/// FNV-1a over `data`. Fixed for all time — the dev/test split (rule 1) and the random arm's
/// per-pair stream seed both key on it.
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// One splitmix64 draw, advancing `state`. The project's only RNG: an in-crate stream so no
/// external crate's version bump can shift a published number (rule 5).
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Map a full-width draw onto `[0, len)` by widening multiply (Lemire). Bias is O(len/2⁶⁴) —
/// immaterial at run lengths — and it needs no rejection loop.
pub fn bounded(draw: u64, len: usize) -> usize {
    let wide = u128::from(draw) * (len as u128);
    (wide >> 64) as usize
}

/// A draw in `[0, 1)` with 53 bits of mantissa — the same top-bits construction CPython's
/// `random.random` uses, so a probability threshold here means what it means in
/// `spike/make_pairs.py`.
pub fn unit(state: &mut u64) -> f64 {
    (splitmix64(state) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_independent_reference_values() {
        // Empty input = the FNV offset basis; "pair_00" computed independently (Python).
        assert_eq!(fnv1a64(b""), 0xCBF2_9CE4_8422_2325);
        assert_eq!(fnv1a64(b"pair_00"), 0x45D7_A4AA_EE8B_FDBA);
    }

    #[test]
    fn splitmix64_matches_the_published_vectors() {
        // First outputs for seeds 0 and 1234567, from Vigna's reference implementation.
        let mut state = 0u64;
        assert_eq!(splitmix64(&mut state), 0xE220_A839_7B1D_CDAF);
        let mut state = 1_234_567u64;
        assert_eq!(splitmix64(&mut state), 0x599E_D017_FB08_FC85);
    }

    #[test]
    fn bounded_draws_stay_in_range_and_split_evenly_at_the_edges() {
        assert_eq!(bounded(0, 10), 0);
        assert_eq!(bounded(u64::MAX, 10), 9);
        // Widening multiply: the draw maps proportionally, so mid-range lands mid-interval.
        assert_eq!(bounded(u64::MAX / 2 + 1, 2), 1);
        for len in [1, 7, 100] {
            assert!(bounded(0x9E37_79B9_7F4A_7C15, len) < len);
        }
    }

    #[test]
    fn unit_draws_stay_in_the_half_open_interval() {
        let mut state = 0u64;
        for _ in 0..10_000 {
            let u = unit(&mut state);
            assert!((0.0..1.0).contains(&u), "unit() escaped [0, 1): {u}");
        }
    }
}
