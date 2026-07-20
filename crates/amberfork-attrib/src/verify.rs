//! The public entry: verify a fork by re-executing it N times and resolving a consensus.
//!
//! This is the crate's headline — what `amberfork diff --verify` ultimately calls. It composes the
//! three prior stages into one honest verdict: build the patch, re-execute it `runs` times through
//! the [driver](crate::driver), and resolve the per-run verdicts into the model's tri-state
//! [`Recovery`]. Nondeterminism is expected and absorbed here: a live provider will not answer
//! bit-identically twice, so a single run's crisp call is only evidence, and the consensus degrades
//! to [`Recovery::Unverified`] rather than asserting a result the runs did not agree on.

use amberfork_align::DiffParams;
use amberfork_model::{Attribution, AttributionMode, Counterfactual, DiffResult, Recovery};
use amberfork_record::{Cassette, normalize};
use amberfork_replay::Upstream;

use crate::AgentDriver;
use crate::driver::{ReexecError, reexecute_once};
use crate::oracle::RunVerdict;
use crate::patch_cassette;

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

/// Verify a diff's fork by counterfactual re-execution, upgrading its attribution from `Static` to
/// `Counterfactual`.
///
/// Patches the fork step with the good run's response, re-executes the agent `runs` times through
/// `driver` (each run served a fresh upstream from `make_upstream` for its post-branch relays), and
/// folds the per-run verdicts into a [`Recovery`]. The returned [`Attribution`] keeps static
/// analysis's `origin_step`, `propagation`, and `confidence` and adds the counterfactual evidence;
/// `cause_label` stays `None` (semantic naming is the judge's job, never localization's).
///
/// Returns `None` only when the diff converged (no attribution to upgrade). A fork with no
/// two-sided pair to patch is returned unchanged — still `Static`, honestly unverified.
///
/// # Errors
///
/// Propagates [`ReexecError`] when a re-execution cannot be carried out (the listener will not bind,
/// or the agent cannot be launched) — such a failure would repeat every run, so it aborts rather
/// than being folded into a verdict.
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
    // The single-patch counterfactual needs a two-sided fork. With none to build, the attribution
    // stays exactly as static analysis left it — honestly unverified, not upgraded.
    let Some(patched) = patch_cassette(diff, bad, good) else {
        return Ok(Some(base));
    };
    // `patch_cassette` builds only for a two-sided fork, so the b-side — the patched step's index in
    // the re-run — exists; treating it as a guard keeps `verify` total rather than asserting it.
    let Some(origin) = diff.fork.and_then(|fork| fork.b_step) else {
        return Ok(Some(base));
    };

    let good_run = normalize(good);
    let mut verdicts = Vec::with_capacity(runs as usize);
    for run in 0..runs {
        let reexecuted_id = format!("{}-cf-{run}", diff.runs.b.id);
        let verdict = reexecute_once(
            patched.clone(),
            &good_run,
            origin,
            driver,
            make_upstream(),
            &reexecuted_id,
            params,
        )
        .await?;
        verdicts.push(verdict);
    }

    Ok(Some(Attribution {
        mode: AttributionMode::Counterfactual,
        counterfactual: Some(Counterfactual {
            recovered: consensus(&verdicts),
            runs,
        }),
        ..base
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
            "origin carried from static"
        );
        let counterfactual = attribution
            .counterfactual
            .expect("counterfactual evidence is present");
        assert_eq!(counterfactual.recovered, Recovery::Recovered);
        assert_eq!(counterfactual.runs, 3);
        assert!(
            attribution.cause_label.is_none(),
            "counterfactual localizes; naming stays the judge's job"
        );
    }
}
