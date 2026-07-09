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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_independent_reference_values() {
        // Empty input = the FNV offset basis; "pair_00" computed independently (Python).
        assert_eq!(fnv1a64(b""), 0xCBF2_9CE4_8422_2325);
        assert_eq!(fnv1a64(b"pair_00"), 0x45D7_A4AA_EE8B_FDBA);
    }
}
