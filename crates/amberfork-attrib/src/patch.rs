//! Building the counterfactual patch: graft the good run's response onto the bad run's fork step.
//!
//! The first, pure stage of `verify` (issue #37). Given the diff and the two cassettes it was
//! built from, produce the cassette to re-execute: the bad run, unchanged, except that the fork
//! step now answers with what the *good* run got there. Re-driving the agent against this asks the
//! counterfactual question directly — "had this one step gone the good way, would the run have
//! recovered?" — which is what lifts attribution from `Static` to `Counterfactual`.
//!
//! Why swap the whole response rather than one field: the record path normalizes a step's
//! `outputs` to the exchange's *entire* response body, so the smallest honest unit to substitute
//! is that body. Field-level minimization (a tighter cause) is [ddmin's job in issue #38], not a
//! prerequisite for the payoff.

use amberfork_model::DiffResult;
use amberfork_record::Cassette;

/// Produce the patched bad cassette to re-execute, or `None` when the diff has no two-sided fork.
///
/// The single-patch counterfactual needs both sides of the fork: the good step whose response is
/// grafted in (`fork.a_step`) and the bad step it is grafted onto (`fork.b_step`). A one-sided
/// fork — a step present in only one run (a log-only or model-only move) — has no aligned pair to
/// swap, so there is nothing to patch and the honest answer is `None`, not a guess. A converged
/// diff (no fork) is `None` for the same reason.
///
/// Indices come straight from the fork because `amberfork_record::normalize` maps `step.idx ==
/// exchange.idx` 1:1: the fork's step indices *are* cassette exchange indices. The bounds checks
/// keep the function total against a diff paired with a mismatched cassette (a hand-forged input);
/// for a diff built from these very cassettes they never fire.
#[must_use]
pub fn patch_cassette(diff: &DiffResult, bad: &Cassette, good: &Cassette) -> Option<Cassette> {
    let fork = diff.fork?;
    patch_many(bad, good, &[(fork.b_step?, fork.a_step?)])
}

/// Graft several good-run responses onto the bad cassette at once — the multi-step generalization
/// of [`patch_cassette`], and the primitive the ddmin oracle re-executes (issue #38).
///
/// `grafts` is `(bad_step, good_step)` pairs: at bad-cassette exchange `bad_step`, serve good-cassette
/// exchange `good_step`'s response instead. This is exactly how [`patch_cassette`] patches the single
/// fork step, applied to a *subset* of the divergent region so ddmin can ask "does patching just
/// these steps recover the run?". Requests are never touched — only the answers change.
///
/// Returns `None` if any index is out of bounds (a diff paired with a mismatched cassette); for the
/// aligned pairs [`crate::cause::fork_candidates`] derives from these very cassettes, it never fires.
/// An empty `grafts` yields an unchanged clone — the untouched bad run.
pub(crate) fn patch_many(
    bad: &Cassette,
    good: &Cassette,
    grafts: &[(usize, usize)],
) -> Option<Cassette> {
    let mut patched = bad.clone();
    for &(bad_step, good_step) in grafts {
        let good_response = good.exchanges.get(good_step)?.response.clone();
        patched.exchanges.get_mut(bad_step)?.response = good_response;
    }
    Some(patched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Fork, Meta, RunPair, RunRef, Source};
    use amberfork_record::{Body, CapturedRequest, CapturedResponse, Exchange};
    use serde_json::json;

    /// A minimal `DiffResult` carrying only what `patch_cassette` reads: the fork. Everything else
    /// is left empty — the patch builder never looks at the alignment, field diffs, or attribution.
    fn diff_with_fork(fork: Option<Fork>) -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: run_ref("good"),
                b: run_ref("bad"),
            },
            alignment: Vec::new(),
            fork,
            field_diffs: Vec::new(),
            attribution: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Record),
        }
    }

    fn run_ref(id: &str) -> RunRef {
        RunRef {
            id: id.to_string(),
            task: None,
            outcome: None,
            n_steps: 0,
        }
    }

    /// An exchange whose request and response bodies are tagged so a swap is visible: the request
    /// carries the turn (`q{idx}`), the response carries a side-stamped answer (`bad-a{idx}` /
    /// `good-a{idx}`).
    fn exchange(idx: usize, question: &str, answer: &str) -> Exchange {
        Exchange {
            idx,
            request: CapturedRequest {
                method: "POST".to_string(),
                path: "/v1/messages".to_string(),
                headers: Vec::new(),
                body: Body::Json(json!({ "content": question })),
            },
            response: CapturedResponse {
                status: 200,
                headers: Vec::new(),
                body: Body::Json(json!({ "content": answer })),
            },
        }
    }

    fn cassette(id: &str, exchanges: Vec<Exchange>) -> Cassette {
        let mut cassette = Cassette::new(id);
        cassette.exchanges = exchanges;
        cassette
    }

    fn fork_at(index: usize, a_step: Option<usize>, b_step: Option<usize>) -> Fork {
        Fork {
            index,
            a_step,
            b_step,
            confidence: 0.9,
        }
    }

    #[test]
    fn the_good_response_lands_at_the_fork_step_and_no_other_exchange_moves() {
        let bad = cassette(
            "bad",
            vec![
                exchange(0, "q0", "bad-a0"),
                exchange(1, "q1", "bad-a1"), // the fork step (b_step = 1)
                exchange(2, "q2", "bad-a2"),
            ],
        );
        let good = cassette(
            "good",
            vec![
                exchange(0, "q0", "good-a0"),
                exchange(1, "q1", "good-a1"), // the aligned good step (a_step = 1)
                exchange(2, "q2", "good-a2"),
            ],
        );

        let patched = patch_cassette(
            &diff_with_fork(Some(fork_at(1, Some(1), Some(1)))),
            &bad,
            &good,
        )
        .expect("a two-sided fork produces a patch");

        // The fork step now answers with the good run's response ...
        assert_eq!(patched.exchanges[1].response, good.exchanges[1].response);
        // ... its request is untouched (we swap the answer, never the question) ...
        assert_eq!(patched.exchanges[1].request, bad.exchanges[1].request);
        // ... and every other exchange, plus the cassette's identity, is byte-identical to `bad`.
        assert_eq!(patched.exchanges[0], bad.exchanges[0]);
        assert_eq!(patched.exchanges[2], bad.exchanges[2]);
        assert_eq!(patched.id, bad.id);
        assert_eq!(patched.exchanges.len(), bad.exchanges.len());
    }

    #[test]
    fn a_one_sided_fork_has_no_pair_to_patch() {
        let bad = cassette("bad", vec![exchange(0, "q0", "bad")]);
        let good = cassette("good", vec![exchange(0, "q0", "good")]);

        // Log-only fork: a step present only in the bad run — no good response to graft in.
        let log_only = fork_at(0, None, Some(0));
        assert!(patch_cassette(&diff_with_fork(Some(log_only)), &bad, &good).is_none());

        // Model-only fork: a step the bad run is missing — nothing on the bad side to patch onto.
        let model_only = fork_at(0, Some(0), None);
        assert!(patch_cassette(&diff_with_fork(Some(model_only)), &bad, &good).is_none());
    }

    #[test]
    fn a_converged_diff_has_nothing_to_patch() {
        let bad = cassette("bad", vec![exchange(0, "q0", "same")]);
        let good = cassette("good", vec![exchange(0, "q0", "same")]);
        assert!(patch_cassette(&diff_with_fork(None), &bad, &good).is_none());
    }

    #[test]
    fn patch_many_grafts_every_listed_step_and_leaves_the_rest_alone() {
        let bad = cassette(
            "bad",
            vec![
                exchange(0, "q0", "bad-a0"),
                exchange(1, "q1", "bad-a1"),
                exchange(2, "q2", "bad-a2"),
                exchange(3, "q3", "bad-a3"),
            ],
        );
        let good = cassette(
            "good",
            vec![
                exchange(0, "q0", "good-a0"),
                exchange(1, "q1", "good-a1"),
                exchange(2, "q2", "good-a2"),
                exchange(3, "q3", "good-a3"),
            ],
        );

        // Graft the good responses at steps 1 and 3; steps 0 and 2 keep the bad answers.
        let patched = patch_many(&bad, &good, &[(1, 1), (3, 3)]).expect("in-bounds grafts patch");

        assert_eq!(patched.exchanges[0], bad.exchanges[0]);
        assert_eq!(patched.exchanges[1].response, good.exchanges[1].response);
        assert_eq!(patched.exchanges[1].request, bad.exchanges[1].request);
        assert_eq!(patched.exchanges[2], bad.exchanges[2]);
        assert_eq!(patched.exchanges[3].response, good.exchanges[3].response);
        assert_eq!(patched.id, bad.id);
    }

    #[test]
    fn patch_many_with_an_out_of_bounds_graft_yields_no_patch() {
        let bad = cassette("bad", vec![exchange(0, "q0", "bad")]);
        let good = cassette("good", vec![exchange(0, "q0", "good")]);
        assert!(patch_many(&bad, &good, &[(0, 0), (5, 0)]).is_none());
    }

    #[test]
    fn a_fork_index_past_the_cassette_yields_no_patch_rather_than_panicking() {
        // A diff paired with the wrong (shorter) cassette must not panic. Here the good side is in
        // bounds but the bad target step is not, so the `get_mut` guard is what keeps this total.
        let bad = cassette("bad", vec![exchange(0, "q0", "bad")]);
        let good = cassette("good", vec![exchange(0, "q0", "good")]);
        let out_of_range = fork_at(9, Some(0), Some(3));
        assert!(patch_cassette(&diff_with_fork(Some(out_of_range)), &bad, &good).is_none());
    }
}
