//! The resync-k fork rule: where unrecovered divergence begins.
//!
//! Locked by Amendment 2026-07-08.A after the naive rule ("fork = first non-sync move")
//! measured 0.00 exact localization — benign retries and rewordings are almost always the
//! first non-sync move. The validated rule: walk the alignment's non-sync *blocks*; a block
//! immediately followed by at least `resync_k` in-sync moves is a benign blip the alignment
//! recovered from; **the fork is the first block it does not recover from**. A fully in-sync
//! alignment has no fork — [`None`] is the designed converged state, not an error.

use amberfork_model::{Fork, Move, MoveKind};

/// Parameters of the fork rule. Defaults are dev-calibrated (notebook 001–003): `resync_k = 2`
/// per Amendment A; `tau = 0.3`, the middle of the plateau (0.2–0.4 scored identically for the
/// token-level lexical cost model on the dev pairs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForkParams {
    /// Sync threshold on move cost: a sync move with `cost <= tau` counts as in-sync.
    pub tau: f64,
    /// How many consecutive in-sync moves after a non-sync block mean "recovered".
    pub resync_k: usize,
}

impl Default for ForkParams {
    fn default() -> Self {
        Self {
            tau: 0.3,
            resync_k: 2,
        }
    }
}

/// Locate the fork in an alignment, or `None` if the runs converged (all blocks benign).
///
/// The returned [`Fork`] points at the first move of the unrecovered block: `index` into the
/// alignment, the move's own step indices, and a confidence in `[0, 1]` measuring how far the
/// move's divergence evidence clears `tau` — `(evidence - tau) / (1 - tau)`, where evidence is
/// a sync move's cost or a gap move's distance-to-closest-counterpart ([`crate::align`]'s gap
/// confidence). A marginal call scores near 0, a total mismatch scores 1.
pub fn find_fork(alignment: &[Move], params: &ForkParams) -> Option<Fork> {
    let in_sync = |m: &Move| m.kind == MoveKind::Sync && m.cost <= params.tau;

    let mut idx = 0;
    while idx < alignment.len() {
        if in_sync(&alignment[idx]) {
            idx += 1;
            continue;
        }
        // A non-sync block starts here; find where it ends...
        let block_start = idx;
        while idx < alignment.len() && !in_sync(&alignment[idx]) {
            idx += 1;
        }
        // ...and how many in-sync moves immediately follow it.
        let resynced = alignment[idx..].iter().take_while(|m| in_sync(m)).count();
        if resynced >= params.resync_k {
            continue; // benign blip — the alignment recovered
        }
        let first = &alignment[block_start];
        return Some(Fork {
            index: block_start,
            a_step: first.a_idx,
            b_step: first.b_idx,
            confidence: divergence_confidence(first, params.tau),
        });
    }
    None
}

/// How far a move's divergence evidence clears `tau`, rescaled to `[0, 1]`. Evidence is a sync
/// move's cost, or a gap move's distance-to-closest-counterpart (its `confidence`, as stamped
/// by [`crate::align`]). A gapped step with a sync-grade twin on the other side clamps to 0 —
/// an honest "this fork is a weak call".
fn divergence_confidence(m: &Move, tau: f64) -> f64 {
    let evidence = match m.kind {
        MoveKind::Sync => m.cost,
        MoveKind::Log | MoveKind::Model => m.confidence,
    };
    ((evidence - tau) / (1.0 - tau).max(f64::EPSILON)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: f64 = 0.6;

    fn sync(i: usize, cost: f64) -> Move {
        Move::sync(i, i, cost, 1.0 - cost)
    }

    /// A log move whose distance-to-closest-counterpart is `conf`.
    fn log(i: usize, conf: f64) -> Move {
        Move::log(i, OPEN, conf)
    }

    fn kinds_at(fork: &Fork, moves: &[Move]) -> MoveKind {
        moves[fork.index].kind
    }

    #[test]
    fn all_sync_alignment_converges() {
        let moves = vec![sync(0, 0.0), sync(1, 0.1), sync(2, 0.2)];
        assert_eq!(find_fork(&moves, &ForkParams::default()), None);
    }

    #[test]
    fn empty_alignment_converges() {
        assert_eq!(find_fork(&[], &ForkParams::default()), None);
    }

    #[test]
    fn cost_exactly_tau_is_in_sync() {
        let params = ForkParams::default();
        let moves = vec![sync(0, params.tau)];
        assert_eq!(find_fork(&moves, &params), None);
    }

    #[test]
    fn benign_blip_is_recovered_from() {
        // One gap move, then >= resync_k in-sync moves: a retry, not a fork.
        let moves = vec![sync(0, 0.0), log(1, 1.0), sync(1, 0.1), sync(2, 0.0)];
        assert_eq!(find_fork(&moves, &ForkParams::default()), None);
    }

    #[test]
    fn insufficient_resync_is_a_fork() {
        // Only one in-sync move follows the block; k = 2 says that is not recovery.
        let moves = vec![sync(0, 0.0), log(1, 1.0), sync(1, 0.1)];
        let fork = find_fork(&moves, &ForkParams::default()).expect("fork");
        assert_eq!(fork.index, 1);
        assert_eq!(fork.b_step, Some(1));
        assert_eq!(fork.a_step, None);
    }

    #[test]
    fn costly_sync_is_divergence() {
        // A sync move above tau is a content divergence even though steps paired up.
        let moves = vec![sync(0, 0.0), sync(1, 0.9), sync(2, 0.95)];
        let fork = find_fork(&moves, &ForkParams::default()).expect("fork");
        assert_eq!(fork.index, 1);
        assert_eq!((fork.a_step, fork.b_step), (Some(1), Some(1)));
    }

    #[test]
    fn divergence_running_to_the_end_is_a_fork() {
        let moves = vec![sync(0, 0.0), sync(1, 0.0), log(2, 0.8), log(3, 0.9)];
        let fork = find_fork(&moves, &ForkParams::default()).expect("fork");
        assert_eq!(fork.index, 2);
    }

    #[test]
    fn fork_is_first_unrecovered_block_not_first_blip() {
        // blip (recovers, k=2 syncs) ... then the real fork.
        let moves = vec![
            sync(0, 0.0),
            log(1, 1.0),  // blip
            sync(1, 0.0), // recovery 1
            sync(2, 0.1), // recovery 2
            sync(3, 0.9), // the real fork: costly sync block ...
            log(4, 1.0),  // ... never recovers
        ];
        let fork = find_fork(&moves, &ForkParams::default()).expect("fork");
        assert_eq!(fork.index, 4, "must skip the recovered blip");
        assert!(kinds_at(&fork, &moves) == MoveKind::Sync);
    }

    #[test]
    fn confidence_scales_with_evidence_over_tau() {
        let params = ForkParams::default();
        // Marginal: cost barely above tau -> confidence near 0.
        let marginal = find_fork(&[sync(0, 0.31)], &params).expect("fork");
        assert!(marginal.confidence < 0.05, "marginal call must read low");
        // Total mismatch: cost 1.0 -> confidence 1.0.
        let certain = find_fork(&[sync(0, 1.0)], &params).expect("fork");
        assert_eq!(certain.confidence, 1.0);
        // Gap move: evidence is its distance-to-closest-counterpart, here 0.65.
        let gappy = find_fork(&[log(0, 0.65)], &params).expect("fork");
        let expected = (0.65 - params.tau) / (1.0 - params.tau);
        assert!((gappy.confidence - expected).abs() < 1e-12);
        // A gap whose step has a sync-grade twin elsewhere: honest zero, not negative.
        let twinned = find_fork(&[log(0, 0.1)], &params).expect("fork");
        assert_eq!(twinned.confidence, 0.0);
    }
}
