//! Minimal-cause labeling: turn a ddmin verdict into refined attribution.
//!
//! Between the pure reducer ([`crate::ddmin`]) and the I/O orchestrator ([`crate::verify`]) sits two
//! pure mappings: from the alignment to the candidate steps the reducer works over, and from its
//! verdict back to the attribution contract. Static attribution paints the whole divergent tail one
//! colour — origin at the fork, everything after it "propagation" — because structure alone cannot
//! tell a carried error from an independent one (DR4's uniform divergent path). Counterfactual
//! re-execution can: [`fork_candidates`] derives the *patchable* steps, and once ddmin has found the
//! minimal recovering subset, [`relabel`] splits the region into **origination** (the minimal cause)
//! and **propagation** (what recovers for free once the cause is patched).

use amberfork_model::{DiffResult, Recovery};

use crate::ddmin::Reduction;

/// A patchable step in the divergent region: a bad-run exchange whose response can be swapped for an
/// aligned good-run one. Only two-sided (sync) alignment moves qualify — a one-sided move has no
/// good response to graft, so it is observed but not re-executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// Exchange index in the bad cassette — the step that gets patched.
    pub(crate) bad_step: usize,
    /// Aligned exchange index in the good cassette — the response grafted in.
    pub(crate) good_step: usize,
}

/// The attribution labels counterfactual re-execution refines out of a ddmin verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Labeling {
    pub(crate) origin_step: Option<usize>,
    pub(crate) propagation: Vec<usize>,
    pub(crate) recovered: Recovery,
}

/// The contiguous patchable candidate steps from the fork onward, in causal order.
///
/// The candidate set ddmin minimizes: the fork step and the run of downstream sync moves that follow
/// it unbroken — each has both an observed step to patch (`b_idx`) and an aligned good response to
/// graft (`a_idx`). The run **stops at the first one-sided move**: a structural gap (a step present
/// in only one run) has no response to graft, and re-executing *past* an unpatchable step would be
/// serving a branch we cannot actually set. Empty when the diff has no fork, or the fork itself is
/// one-sided — matching the single-patch counterfactual, the caller then leaves attribution exactly
/// as static analysis left it, honestly unverified.
pub(crate) fn fork_candidates(diff: &DiffResult) -> Vec<Candidate> {
    let Some(fork) = diff.fork else {
        return Vec::new();
    };
    diff.alignment
        .get(fork.index..)
        .unwrap_or(&[])
        .iter()
        .map_while(|mv| {
            Some(Candidate {
                bad_step: mv.b_idx?,
                good_step: mv.a_idx?,
            })
        })
        .collect()
}

/// Split the divergent region into origination and propagation from ddmin's verdict.
///
/// `region` is the observed divergent region in causal order — the static origin step followed by
/// its propagation tail. `candidates` maps ddmin's indices back to the bad-run steps they patch.
///
/// - [`Reduction::Minimized`]: the minimal cause's steps become **origination**; `origin_step` is
///   its earliest step, and everything else in the region — the tail that recovered once the cause
///   was patched, plus any observed-only steps — is **propagation**. So an independent downstream
///   fault, which static analysis would have mislabeled propagation, is pulled out into the cause.
///   `recovered = Recovered`.
/// - [`Reduction::Persisted`] / [`Reduction::Inconclusive`]: re-execution added no trustworthy
///   evidence, so the static labels stand unchanged; only `recovered` records whether the region
///   failed to recover (`NotRecovered`) or the oracle could not decide (`Unverified`).
pub(crate) fn relabel(
    reduction: &Reduction,
    candidates: &[Candidate],
    region: &[usize],
) -> Labeling {
    match reduction {
        Reduction::Minimized(indices) => {
            // `candidates.get` keeps this total against a malformed index; for a reduction ddmin
            // built over `candidates` it always resolves.
            let cause: Vec<usize> = indices
                .iter()
                .filter_map(|&i| candidates.get(i).map(|c| c.bad_step))
                .collect();
            let origin_step = cause.iter().copied().min();
            let propagation = region
                .iter()
                .copied()
                .filter(|step| !cause.contains(step))
                .collect();
            Labeling {
                origin_step,
                propagation,
                recovered: Recovery::Recovered,
            }
        }
        Reduction::Persisted => static_labels(region, Recovery::NotRecovered),
        Reduction::Inconclusive => static_labels(region, Recovery::Unverified),
    }
}

/// The static labels unchanged (origin = the region's head, propagation = its tail), tagged with the
/// re-execution verdict that added no refinement.
fn static_labels(region: &[usize], recovered: Recovery) -> Labeling {
    Labeling {
        origin_step: region.first().copied(),
        propagation: region.get(1..).unwrap_or(&[]).to_vec(),
        recovered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Fork, Meta, Move, RunPair, RunRef, Source};

    fn run_ref(id: &str, n_steps: usize) -> RunRef {
        RunRef {
            id: id.to_string(),
            task: None,
            outcome: None,
            n_steps,
        }
    }

    fn diff_with(alignment: Vec<Move>, fork: Option<Fork>) -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: run_ref("good", alignment.len()),
                b: run_ref("bad", alignment.len()),
            },
            alignment,
            fork,
            field_diffs: Vec::new(),
            attribution: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Record),
        }
    }

    fn fork_at(index: usize) -> Fork {
        Fork {
            index,
            a_step: Some(index),
            b_step: Some(index),
            confidence: 0.9,
        }
    }

    fn candidate(bad_step: usize, good_step: usize) -> Candidate {
        Candidate {
            bad_step,
            good_step,
        }
    }

    #[test]
    fn candidates_are_the_two_sided_moves_from_the_fork_onward() {
        // Sync moves at 1 and 2 are patchable (a good response to graft); the log-only tail at 3 is
        // observed-only and ends the run. The sync at 0 is before the fork, so it is not a candidate.
        let alignment = vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::sync(1, 1, 0.9, 0.1), // fork
            Move::sync(2, 2, 0.8, 0.2),
            Move::log(3, 0.6, 0.9),
        ];
        let got = fork_candidates(&diff_with(alignment, Some(fork_at(1))));
        assert_eq!(got, vec![candidate(1, 1), candidate(2, 2)]);
    }

    #[test]
    fn the_candidate_run_stops_at_the_first_one_sided_move() {
        // A structural gap (the model-only move at 2) breaks the patchable run: the sync at 3, though
        // two-sided, sits past an unpatchable step and is not a candidate.
        let alignment = vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::sync(1, 1, 0.9, 0.1), // fork
            Move::model(2, 0.7, 0.8),   // one-sided: no response to graft
            Move::sync(3, 2, 0.8, 0.2),
        ];
        let got = fork_candidates(&diff_with(alignment, Some(fork_at(1))));
        assert_eq!(got, vec![candidate(1, 1)]);
    }

    #[test]
    fn a_one_sided_fork_has_no_candidates() {
        // The fork itself is a log-only move — nothing to graft at the branch — so there is no
        // counterfactual to run, exactly as the single-patch path bails.
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::log(1, 0.7, 0.9)];
        let fork = Fork {
            index: 1,
            a_step: None,
            b_step: Some(1),
            confidence: 0.5,
        };
        assert!(fork_candidates(&diff_with(alignment, Some(fork))).is_empty());
    }

    #[test]
    fn no_fork_has_no_candidates() {
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0)];
        assert!(fork_candidates(&diff_with(alignment, None)).is_empty());
    }

    #[test]
    fn a_single_cause_labels_the_whole_tail_propagation() {
        // Acceptance criterion 1: step k is the sole cause, the tail merely propagates it. ddmin
        // minimized to just the fork candidate, so origin stays k and everything after is propagation.
        let candidates = [candidate(1, 1), candidate(2, 2), candidate(3, 3)];
        let region = [1, 2, 3];
        let labeling = relabel(&Reduction::Minimized(vec![0]), &candidates, &region);
        assert_eq!(labeling.origin_step, Some(1));
        assert_eq!(labeling.propagation, vec![2, 3]);
        assert_eq!(labeling.recovered, Recovery::Recovered);
    }

    #[test]
    fn an_independent_downstream_fault_is_pulled_out_of_propagation() {
        // Two independent causes (bad steps 1 and 3): ddmin returned both. Origin is the earliest,
        // and step 3 — which static analysis would call propagation — is correctly NOT propagation.
        let candidates = [candidate(1, 1), candidate(2, 2), candidate(3, 3)];
        let region = [1, 2, 3];
        let labeling = relabel(&Reduction::Minimized(vec![0, 2]), &candidates, &region);
        assert_eq!(labeling.origin_step, Some(1));
        assert_eq!(
            labeling.propagation,
            vec![2],
            "step 3 is origination, not propagation"
        );
        assert_eq!(labeling.recovered, Recovery::Recovered);
    }

    #[test]
    fn a_region_that_never_recovers_keeps_static_labels_as_not_recovered() {
        let candidates = [candidate(1, 1), candidate(2, 2)];
        let region = [1, 2, 3]; // includes an observed-only step 3 with no candidate
        let labeling = relabel(&Reduction::Persisted, &candidates, &region);
        assert_eq!(labeling.origin_step, Some(1));
        assert_eq!(
            labeling.propagation,
            vec![2, 3],
            "static tail preserved verbatim"
        );
        assert_eq!(labeling.recovered, Recovery::NotRecovered);
    }

    #[test]
    fn an_inconclusive_oracle_keeps_static_labels_as_unverified() {
        let candidates = [candidate(1, 1), candidate(2, 2)];
        let region = [1, 2];
        let labeling = relabel(&Reduction::Inconclusive, &candidates, &region);
        assert_eq!(labeling.origin_step, Some(1));
        assert_eq!(labeling.propagation, vec![2]);
        assert_eq!(labeling.recovered, Recovery::Unverified);
    }
}
