//! The recovery oracle: did patching the fork step recover one re-run?
//!
//! The pure, sync heart of counterfactual verification. Slice 3's driver re-executes the patched
//! cassette into a [`Run`]; this stage judges one such re-run against the good run. It reuses the
//! ordinary diff engine — align the re-run against good, apply the same resync-k fork rule — so
//! "recovered" means exactly what "converged" already means everywhere else in amberfork: no
//! sustained fork survives. There is deliberately no second, forkable definition of recovery.
//!
//! It never touches the network and never re-executes anything: it is `(good, reexecuted) ->
//! verdict` and nothing more. Nondeterminism is not smoothed here — one run gets a crisp call; the
//! consensus layer (slice 4) turns N crisp calls into the honest [`Recovery`](amberfork_model::Recovery)
//! tri-state, degrading to `Unverified` when they disagree.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_model::Run;

/// One re-execution's verdict on whether the patch recovered the run.
///
/// Distinct from the model's [`Recovery`](amberfork_model::Recovery): that is the *consensus* over
/// N runs; this is one run's contribution to it. `Inconclusive` is a run that yields no evidence
/// about the patch — it never reached the patched step, or produced no trajectory to judge — and
/// is dropped from the vote rather than counted as a failure.
// Consumed by the multi-run consensus in slice 4 of #37; until that lands it is exercised only by
// this module's tests. The allow goes the moment `verify` calls the oracle.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunVerdict {
    Recovered,
    NotRecovered,
    Inconclusive,
}

/// Judge one re-executed run against the good run.
///
/// `origin` is the patched step's index — shared between the bad run and the re-run, whose prefixes
/// are the same tape. The re-run must at least *reach* that step for its outcome to be evidence: a
/// run that stopped before the patch ran (the live provider errored on the first post-prefix call,
/// or the agent died) says nothing about the patch and is `Inconclusive`. Otherwise the re-run is
/// aligned against good and the standard fork rule decides: no surviving fork → the run converged
/// with good → `Recovered`; a fork that persists → `NotRecovered`.
// See the note on `RunVerdict`: wired into `verify` in slice 4.
#[allow(dead_code)]
pub(crate) fn classify_recovery(
    good: &Run,
    reexecuted: &Run,
    origin: usize,
    params: &DiffParams,
) -> RunVerdict {
    // A re-run that never reached the patched step exercised nothing downstream of it — its
    // outcome is a truncated-trajectory artifact, not a signal about the patch.
    if reexecuted.steps.len() <= origin {
        return RunVerdict::Inconclusive;
    }
    // Reuse the ordinary engine: good is the reference (side a), the re-run is the observed (side
    // b) — exactly the roles the good and bad runs held in the original diff. A diff that cannot be
    // computed (the size guard trips on a runaway re-run) is no verdict, never a false one.
    let Ok(result) = diff(good, reexecuted, &LexicalCost, params) else {
        return RunVerdict::Inconclusive;
    };
    if result.fork.is_some() {
        RunVerdict::NotRecovered
    } else {
        RunVerdict::Recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Run, test_support};

    /// A run from `(name, text-output)` pairs — enough for the aligner to sync a matching prefix
    /// and fork on a divergent tail.
    fn run(id: &str, steps: &[(&str, &str)]) -> Run {
        let steps = steps
            .iter()
            .enumerate()
            .map(|(idx, (name, out))| test_support::step(idx, *name).text_output(*out).build())
            .collect();
        test_support::run(id, steps).build()
    }

    #[test]
    fn a_rerun_that_converges_with_good_is_recovered() {
        // The patch worked: the re-run follows good's trajectory step for step, so aligning them
        // yields no fork — the same "converged" the self-align invariant guarantees.
        let good = run("good", &[("plan", "p"), ("search", "s"), ("answer", "a")]);
        let reexecuted = run("reexec", &[("plan", "p"), ("search", "s"), ("answer", "a")]);
        assert_eq!(
            classify_recovery(&good, &reexecuted, 1, &DiffParams::default()),
            RunVerdict::Recovered
        );
    }

    #[test]
    fn a_rerun_that_still_diverges_after_the_patch_is_not_recovered() {
        // The patch did not help: past the shared prefix the re-run goes its own way and never
        // re-syncs, so the fork rule still finds a sustained divergence at/after the patch.
        let good = run(
            "good",
            &[
                ("plan", "p"),
                ("search", "s"),
                ("read", "r"),
                ("answer", "a"),
            ],
        );
        let reexecuted = run(
            "reexec",
            &[
                ("plan", "p"),
                ("search", "s"),
                ("flail", "x"),
                ("flail-again", "y"),
            ],
        );
        assert_eq!(
            classify_recovery(&good, &reexecuted, 1, &DiffParams::default()),
            RunVerdict::NotRecovered
        );
    }

    #[test]
    fn a_rerun_too_short_to_reach_the_patch_is_inconclusive() {
        // origin = 2, but the re-run produced only two steps (indices 0,1): it stopped before the
        // patched step ever ran, so its outcome is no evidence about the patch.
        let good = run("good", &[("plan", "p"), ("search", "s"), ("read", "r")]);
        let reexecuted = run("reexec", &[("plan", "p"), ("search", "s")]);
        assert_eq!(
            classify_recovery(&good, &reexecuted, 2, &DiffParams::default()),
            RunVerdict::Inconclusive
        );
    }

    #[test]
    fn an_empty_rerun_is_inconclusive() {
        // The live provider errored on the very first post-prefix call, or the agent produced no
        // turn at all: there is no trajectory to judge.
        let good = run("good", &[("plan", "p"), ("answer", "a")]);
        let reexecuted = run("reexec", &[]);
        assert_eq!(
            classify_recovery(&good, &reexecuted, 0, &DiffParams::default()),
            RunVerdict::Inconclusive
        );
    }
}
