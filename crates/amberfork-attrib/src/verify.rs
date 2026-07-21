//! The public entry: verify a fork by re-executing it N times and resolving a consensus.
//!
//! This is the crate's headline — what `amberfork diff --verify` ultimately calls. It composes the
//! three prior stages into one honest verdict: build the patch, re-execute it `runs` times through
//! the [driver](crate::driver), and resolve the per-run verdicts into the model's tri-state
//! [`Recovery`]. Nondeterminism is expected and absorbed here: a live provider will not answer
//! bit-identically twice, so a single run's crisp call is only evidence, and the consensus degrades
//! to [`Recovery::Unverified`] rather than asserting a result the runs did not agree on.

use std::sync::atomic::{AtomicUsize, Ordering};

use amberfork_align::DiffParams;
use amberfork_model::{Attribution, AttributionMode, Counterfactual, DiffResult, Recovery, Run};
use amberfork_record::{Cassette, normalize};
use amberfork_replay::Upstream;

use crate::AgentDriver;
use crate::cause::{Labeling, fork_candidates, relabel};
use crate::ddmin::minimize;
use crate::driver::{ReexecError, reexecute_once};
use crate::oracle::RunVerdict;
use crate::patch::patch_many;

/// Resolve N per-run verdicts into the model's [`Recovery`] tri-state.
///
/// Only *conclusive* runs vote: the side with a strict majority among them wins. A tie — including
/// no conclusive runs at all (every run inconclusive, or none run) — is no majority, so the honest
/// answer is [`Recovery::Unverified`]. `Inconclusive` runs never count as failures; they simply do
/// not vote.
fn consensus(verdicts: &[RunVerdict]) -> Recovery {
    let recovered = verdicts
        .iter()
        .filter(|v| **v == RunVerdict::Recovered)
        .count();
    let not_recovered = verdicts
        .iter()
        .filter(|v| **v == RunVerdict::NotRecovered)
        .count();
    if recovered > not_recovered {
        Recovery::Recovered
    } else if not_recovered > recovered {
        Recovery::NotRecovered
    } else {
        Recovery::Unverified
    }
}

/// The re-execution context shared by every ddmin experiment — everything the recovery oracle needs
/// beyond the particular patched cassette under test. Bundled so [`Experiment::recovery_of`] stays a
/// one-argument call: the candidate subset (hence the patched cassette) is the only thing that varies
/// from experiment to experiment.
struct Experiment<'a, D, F> {
    good_run: &'a Run,
    /// The fork step — the branch point every re-run must reach for its outcome to be evidence.
    branch: usize,
    driver: &'a D,
    make_upstream: &'a F,
    runs: u32,
    params: &'a DiffParams,
    id_prefix: &'a str,
}

impl<D: AgentDriver, F> Experiment<'_, D, F> {
    /// Re-execute one patched cassette `runs` times and fold the per-run verdicts into a
    /// [`Recovery`].
    ///
    /// One ddmin experiment: stand up `runs` fresh re-executions of the patched cassette — each
    /// served its own `upstream` for post-branch relays — classify each against the good run, and
    /// resolve the tri-state by [`consensus`]. A re-run that never reached the branch is
    /// inconclusive (see [`classify_recovery`](crate::oracle)).
    async fn recovery_of<U>(&self, patched: Cassette) -> Result<Recovery, ReexecError>
    where
        U: Upstream + 'static,
        F: Fn() -> U,
    {
        let mut verdicts = Vec::with_capacity(self.runs as usize);
        for run in 0..self.runs {
            let reexecuted_id = format!("{}-{run}", self.id_prefix);
            let verdict = reexecute_once(
                patched.clone(),
                self.good_run,
                self.branch,
                self.driver,
                (self.make_upstream)(),
                &reexecuted_id,
                self.params,
            )
            .await?;
            verdicts.push(verdict);
        }
        Ok(consensus(&verdicts))
    }
}

/// Verify a diff's fork by counterfactual re-execution, upgrading its attribution from `Static` to
/// `Counterfactual` and refining its origin/propagation split.
///
/// Builds the candidate set — the fork step plus its patchable downstream tail — and asks
/// [`minimize`](crate::ddmin) for the smallest subset whose patch still recovers the run, each
/// experiment re-executing the agent `runs` times through `driver` (a fresh upstream from
/// `make_upstream` per run, for post-branch relays). The minimal cause becomes **origination**, the
/// rest of the region **propagation**, and `confidence` reflects how stable the oracle was across
/// the ddmin re-runs. `cause_label` stays `None` — semantic naming is the judge's job, never
/// localization's.
///
/// Returns `None` only when the diff converged (no attribution to upgrade). A fork with no patchable
/// pair is returned unchanged — still `Static`, honestly unverified.
///
/// # Errors
///
/// Propagates [`ReexecError`] when a re-execution cannot be carried out (the listener will not bind,
/// or the agent cannot be launched) — such a failure would repeat every run, so it aborts the whole
/// minimization rather than being folded into a verdict.
pub async fn verify<U, D, F>(
    diff: &DiffResult,
    good: &Cassette,
    bad: &Cassette,
    driver: &D,
    make_upstream: F,
    runs: u32,
    params: &DiffParams,
) -> Result<Option<Attribution>, ReexecError>
where
    U: Upstream + 'static,
    D: AgentDriver,
    F: Fn() -> U,
{
    // The static attribution is the base we upgrade. Without one the diff converged — there is no
    // regression to verify — so there is nothing to return.
    let Some(base) = diff.attribution.clone() else {
        return Ok(None);
    };
    // The candidate set is the fork step plus its patchable downstream tail. With none — a one-sided
    // fork, nothing to graft — attribution stays exactly as static analysis left it.
    let candidates = fork_candidates(diff);
    let Some(first) = candidates.first().copied() else {
        return Ok(Some(base));
    };
    // The earliest candidate is the fork step: the branch point every re-run must reach to be evidence.
    let good_run = normalize(good);
    let id_prefix = format!("{}-cf", diff.runs.b.id);
    let experiment = Experiment {
        good_run: &good_run,
        branch: first.bad_step,
        driver,
        make_upstream: &make_upstream,
        runs,
        params,
        id_prefix: &id_prefix,
    };
    // The observed divergent region, in causal order: the static origin followed by its tail. ddmin
    // partitions it into origination and propagation.
    let region: Vec<usize> = base
        .origin_step
        .into_iter()
        .chain(base.propagation.iter().copied())
        .collect();

    // Minimize the candidate set to the smallest patch that still recovers. The oracle re-executes a
    // subset and returns its consensus verdict; we tally how many of those verdicts were conclusive,
    // which becomes the attribution's confidence — a fork verified across stable re-runs is worth
    // more than one whose oracle kept wavering.
    let total = AtomicUsize::new(0);
    let conclusive = AtomicUsize::new(0);
    let reduction = minimize(candidates.len(), |subset: &[usize]| {
        let grafts: Vec<(usize, usize)> = subset
            .iter()
            .map(|&i| (candidates[i].bad_step, candidates[i].good_step))
            .collect();
        let patched = patch_many(bad, good, &grafts);
        let experiment = &experiment;
        let total = &total;
        let conclusive = &conclusive;
        async move {
            total.fetch_add(1, Ordering::Relaxed);
            let recovery = match patched {
                Some(patched) => experiment.recovery_of(patched).await?,
                // A subset we cannot even build (a diff paired with a mismatched cassette) is no
                // evidence, never a false recovery.
                None => Recovery::Unverified,
            };
            if recovery != Recovery::Unverified {
                conclusive.fetch_add(1, Ordering::Relaxed);
            }
            Ok(recovery)
        }
    })
    .await?;

    let Labeling {
        origin_step,
        propagation,
        recovered,
    } = relabel(&reduction, &candidates, &region);

    let total = total.load(Ordering::Relaxed);
    let conclusive = conclusive.load(Ordering::Relaxed);
    // The precondition always runs, so `total >= 1` and this never divides by zero; the static
    // fallback covers only the theoretical no-call case.
    let confidence = if total == 0 {
        base.confidence
    } else {
        conclusive as f64 / total as f64
    };

    Ok(Some(Attribution {
        mode: AttributionMode::Counterfactual,
        origin_step,
        propagation,
        counterfactual: Some(Counterfactual { recovered, runs }),
        cause_label: None,
        confidence,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{G2, ScriptedAgent, bad_cassette, body_of, good_cassette, response};
    use amberfork_align::LexicalCost;
    use amberfork_replay::ScriptedUpstream;

    use RunVerdict::{Inconclusive, NotRecovered, Recovered};

    #[test]
    fn a_strict_majority_of_conclusive_runs_wins_and_inconclusive_ones_do_not_vote() {
        // Two recovered, one not, one inconclusive: the inconclusive run is dropped, and 2 > 1 is a
        // strict majority of the conclusive runs.
        assert_eq!(
            consensus(&[Recovered, Recovered, NotRecovered, Inconclusive]),
            Recovery::Recovered
        );
        assert_eq!(
            consensus(&[NotRecovered, NotRecovered, Recovered]),
            Recovery::NotRecovered
        );
    }

    #[test]
    fn a_tie_between_conclusive_runs_is_unverified() {
        // One each way, however many inconclusive runs surround them: no strict majority.
        assert_eq!(
            consensus(&[Recovered, NotRecovered, Inconclusive, Inconclusive]),
            Recovery::Unverified
        );
    }

    #[test]
    fn all_inconclusive_or_no_runs_is_unverified() {
        assert_eq!(
            consensus(&[Inconclusive, Inconclusive]),
            Recovery::Unverified
        );
        assert_eq!(consensus(&[]), Recovery::Unverified);
    }

    #[tokio::test]
    async fn verify_upgrades_a_recovering_fork_to_counterfactual() {
        // The full pipeline: align the good/bad pair, then verify the fork. The scripted agent
        // follows the good path, so all three re-runs recover and the consensus is Recovered.
        let good = good_cassette();
        let bad = bad_cassette();
        let params = DiffParams::default();
        let diff =
            amberfork_align::diff(&normalize(&good), &normalize(&bad), &LexicalCost, &params)
                .expect("the good/bad pair aligns");
        assert_eq!(
            diff.attribution
                .as_ref()
                .expect("a fork is attributed")
                .origin_step,
            Some(1),
            "static analysis localizes the fork at the bad run's turn 1"
        );

        let agent = ScriptedAgent {
            requests: vec![body_of("q0"), body_of("q1"), body_of("q2_good")],
        };
        let attribution = verify(
            &diff,
            &good,
            &bad,
            &agent,
            || ScriptedUpstream::new([response(G2)]),
            3,
            &params,
        )
        .await
        .expect("the experiment runs")
        .expect("a forked diff yields an attribution");

        assert_eq!(attribution.mode, AttributionMode::Counterfactual);
        assert_eq!(
            attribution.origin_step,
            Some(1),
            "ddmin confirms the fork step as the minimal cause"
        );
        assert_eq!(
            attribution.propagation,
            vec![2],
            "the tail recovered once the fork was patched, so it is propagation"
        );
        let counterfactual = attribution
            .counterfactual
            .expect("counterfactual evidence is present");
        assert_eq!(counterfactual.recovered, Recovery::Recovered);
        assert_eq!(counterfactual.runs, 3);
        assert!(
            (attribution.confidence - 1.0).abs() < f64::EPSILON,
            "every ddmin oracle call was conclusive, so confidence is 1.0, was {}",
            attribution.confidence
        );
        assert!(
            attribution.cause_label.is_none(),
            "counterfactual localizes; naming stays the judge's job"
        );
    }
}
