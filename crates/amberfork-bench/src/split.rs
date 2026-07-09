//! The dev/test split — pre-registered protocol rule 1. Every pair is assigned by a stable
//! hash of its task key, so the assignment is fixed the moment a task exists: nothing about
//! a result can move a case across the line, and pairs generated later slot in without
//! re-splitting anything. ALL tuning (costs, gaps, τ, resync-k) happens on dev; the test
//! split runs with frozen params.
//!
//! Keying on the *task* (not the pair) is the leakage guard: every pair built from the same
//! underlying question lands on the same side, so dev tuning never sees test material.
//! Chimera caveat, for honesty: a pair's divergent tail comes from a second source log Y,
//! which is not split-keyed — the prefix task X is what the gold step lives in and what the
//! split protects.
//!
//! The constants below are protocol, not parameters: they are deliberately NOT in
//! `bench/params.toml` (rule 2's tunables), because a split that could be re-tuned is no
//! split at all.

use crate::hash::fnv1a64;

/// Share of task keys assigned to dev, in percent. "~30/70" per rule 1; the realized
/// fraction wobbles with the task-id population, which is expected and fine.
const DEV_PERCENT: u64 = 30;

/// Which side of the protocol split a task key falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// Tuning side: every parameter decision is made here.
    Dev,
    /// Held-out side: scored with frozen params, once per release tag.
    Test,
}

impl Split {
    /// The assignment for `task_key` — pure, stable, committed.
    #[must_use]
    pub fn of(task_key: &str) -> Self {
        if fnv1a64(task_key.as_bytes()) % 100 < DEV_PERCENT {
            Self::Dev
        } else {
            Self::Test
        }
    }

    /// The split's name in tables, flags, and results JSON.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Test => "test",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignments_match_independent_reference_values() {
        // Buckets computed independently (Python, same FNV-1a): restock-good → 48,
        // whowhen_hand_14 → 79 (test); whowhen_hand_32 → 15, whowhen_hand_49 → 21,
        // greenhouse-good → 26 (dev). Real Who&When ids and both committed fixture sets are
        // pinned so a hashing regression cannot slide the whole world into one split.
        assert_eq!(Split::of("restock-good"), Split::Test);
        assert_eq!(Split::of("whowhen_hand_14"), Split::Test);
        assert_eq!(Split::of("whowhen_hand_32"), Split::Dev);
        assert_eq!(Split::of("whowhen_hand_49"), Split::Dev);
        assert_eq!(Split::of("greenhouse-good"), Split::Dev);
    }

    #[test]
    fn the_dev_share_lands_near_thirty_percent() {
        // Exact count over a fixed key family, computed independently (Python): 311 of
        // 1000 — the "~30/70" of rule 1 realized. Exact, not a range, per rule 5.
        let dev = (0..1000)
            .filter(|i| Split::of(&format!("task-{i:04}")) == Split::Dev)
            .count();
        assert_eq!(dev, 311);
    }
}
