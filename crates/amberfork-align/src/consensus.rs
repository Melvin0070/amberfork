//! Multi-reference consensus: one suspect run against *many* reference runs (issue #45).
//!
//! `diff` answers "where did my run diverge from **this** good run". Agents are
//! non-deterministic, so a single reference may be a lucky sample and that answer inherits the
//! sample's luck. Consensus converts the assumption "the reference is representative" into a
//! measurement: align the suspect against each reference independently, then report the
//! **modal fork step and how many references voted for it**. "7 of 10 good runs fork here" is a
//! claim about the suspect; "1 of 1" was a claim about a coin flip.
//!
//! Deliberately the *cheap* version: pairwise [`diff`] plus a tally. No partial-order
//! alignment, no consensus DAG, no clustering — the aggregation lives here and the moat
//! ([`crate::align`], [`crate::find_fork`]) is untouched. Whether the expensive version is ever
//! worth building is gated on this one beating single-reference on the dev fixtures.

use crate::cost::CostModel;
use crate::diff::{DiffParams, diff};
use crate::params::ParamError;
use amberfork_model::Run;
use std::collections::BTreeMap;

/// What N references agree on about one suspect run.
///
/// Two absences are distinct and both are honest: `references == 0` (nothing was asked) and
/// every reference converging (asked, and the suspect looks normal to all of them) both leave
/// [`Consensus::modal_step`] as `None`. [`Consensus::references`] tells them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consensus {
    /// One entry per reference, in input order: the *suspect*-side step that reference
    /// localized the fork to ([`amberfork_model::DiffResult::fork_step_observed`]), or `None`
    /// where that reference converged with the suspect.
    ///
    /// Kept per-reference rather than pre-summed so a caller can see the shape of the
    /// disagreement — a 6/4 split at adjacent steps and a 6/4 split at opposite ends of the
    /// run are very different situations that one modal number hides.
    pub votes: Vec<Option<usize>>,
    /// The step the plurality of *forking* references named, or `None` when none forked.
    pub modal_step: Option<usize>,
    /// How many references voted for [`Consensus::modal_step`]. `0` exactly when it is `None`.
    pub support: usize,
}

impl Consensus {
    /// How many references were consulted.
    #[must_use]
    pub fn references(&self) -> usize {
        self.votes.len()
    }

    /// How many references forked at all. The denominator that makes `support` a rate:
    /// references that converged never voted, so counting them against the modal step would
    /// understate agreement among the ones that actually saw a divergence.
    #[must_use]
    pub fn forked(&self) -> usize {
        self.votes.iter().filter(|v| v.is_some()).count()
    }

    /// Share of *forking* references backing the modal step, in `[0, 1]`; `None` when no
    /// reference forked (a rate with a zero denominator is not zero, it is undefined).
    #[must_use]
    pub fn agreement(&self) -> Option<f64> {
        let forked = self.forked();
        (forked > 0).then(|| self.support as f64 / forked as f64)
    }
}

/// Diff `suspect` against every run in `references` and tally where they say it forked.
///
/// Tie-break is the **lowest step index**, which is not arbitrary: it matches the fork rule's
/// own bias (first non-sync block that never re-syncs) and points at the earliest plausible
/// origin, and — unlike "first one seen" — it makes the answer independent of the order the
/// references were passed in.
///
/// # Errors
/// The first [`ParamError`] any reference produces, references taken in order. A run over the
/// size guard is a configuration fact about that run, not a vote to be silently dropped.
pub fn consensus(
    references: &[Run],
    suspect: &Run,
    cost_model: &impl CostModel,
    params: &DiffParams,
) -> Result<Consensus, ParamError> {
    let mut votes = Vec::with_capacity(references.len());
    for reference in references {
        votes.push(diff(reference, suspect, cost_model, params)?.fork_step_observed());
    }

    // BTreeMap, not HashMap: iterating tallies in step order makes `>` alone break ties toward
    // the lowest step, and keeps the walk deterministic across builds.
    let mut tally: BTreeMap<usize, usize> = BTreeMap::new();
    for step in votes.iter().flatten() {
        *tally.entry(*step).or_default() += 1;
    }
    let (modal_step, support) = tally
        .into_iter()
        .fold(
            None,
            |best: Option<(usize, usize)>, (step, count)| match best {
                Some((_, best_count)) if best_count >= count => best,
                _ => Some((step, count)),
            },
        )
        .map_or((None, 0), |(step, count)| (Some(step), count));

    Ok(Consensus {
        votes,
        modal_step,
        support,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::LexicalCost;
    use crate::nw::AlignParams;
    use amberfork_model::{Outcome, test_support};

    fn run(id: &str, outcome: Outcome, outputs: &[&str]) -> Run {
        let steps = outputs
            .iter()
            .enumerate()
            .map(|(i, o)| test_support::step(i, "act").text_output(*o).build())
            .collect();
        test_support::run(id, steps)
            .task("find the census figure")
            .outcome(outcome)
            .build()
    }

    /// The suspect: a plausible run whose third step picks up a bad source.
    fn suspect() -> Run {
        run(
            "bad",
            Outcome::Fail,
            &[
                "plan search for census data",
                "search census gov top result",
                "fetch blogspot page the city has grown to 9,100,000",
                "answer population is 9,100,000",
            ],
        )
    }

    /// A reference that shares the suspect's first two steps then does the right thing —
    /// forks the suspect at step 2.
    fn reference_forking_at_2(id: &str) -> Run {
        run(
            id,
            Outcome::Pass,
            &[
                "plan search for census data",
                "search census gov top result",
                "fetch census gov page population 8,443,000",
                "answer population is 8,443,000",
            ],
        )
    }

    /// A reference that diverges a step earlier — forks the suspect at step 1.
    fn reference_forking_at_1(id: &str) -> Run {
        run(
            id,
            Outcome::Pass,
            &[
                "plan search for census data",
                "consult the almanac index directly",
                "read almanac entry population 8,443,000",
                "answer population is 8,443,000",
            ],
        )
    }

    fn tally(references: &[Run], suspect: &Run) -> Consensus {
        consensus(references, suspect, &LexicalCost, &DiffParams::default())
            .expect("fixtures are under the size guard")
    }

    #[test]
    fn unanimous_references_all_back_one_step() {
        let refs = [
            reference_forking_at_2("g0"),
            reference_forking_at_2("g1"),
            reference_forking_at_2("g2"),
        ];
        let c = tally(&refs, &suspect());

        assert_eq!(c.votes, vec![Some(2), Some(2), Some(2)]);
        assert_eq!(c.modal_step, Some(2));
        assert_eq!(c.support, 3);
        assert_eq!(c.references(), 3);
        assert_eq!(c.agreement(), Some(1.0));
    }

    #[test]
    fn the_plurality_wins_and_the_minority_stays_visible() {
        let refs = [
            reference_forking_at_2("g0"),
            reference_forking_at_1("g1"),
            reference_forking_at_2("g2"),
        ];
        let c = tally(&refs, &suspect());

        assert_eq!(c.modal_step, Some(2));
        assert_eq!(
            c.support, 2,
            "2 of 3 — not unanimity, and it must not read as one"
        );
        assert_eq!(
            c.votes,
            vec![Some(2), Some(1), Some(2)],
            "the dissenting reference is still on the record"
        );
        assert_eq!(c.agreement(), Some(2.0 / 3.0));
    }

    #[test]
    fn a_tie_breaks_to_the_earliest_step_whatever_the_input_order() {
        let early = reference_forking_at_1("g_early");
        let late = reference_forking_at_2("g_late");

        let one_way = tally(&[early.clone(), late.clone()], &suspect());
        let other_way = tally(&[late, early], &suspect());

        assert_eq!(one_way.modal_step, Some(1), "earliest wins a 1–1 tie");
        assert_eq!(one_way.support, 1);
        assert_eq!(
            one_way.modal_step, other_way.modal_step,
            "input order must not decide the answer"
        );
        assert_eq!(one_way.support, other_way.support);
    }

    #[test]
    fn one_reference_matches_the_pairwise_diff_exactly() {
        let suspect = suspect();
        let reference = reference_forking_at_2("g0");
        let pairwise = diff(&reference, &suspect, &LexicalCost, &DiffParams::default())
            .expect("fixtures are under the size guard");

        let c = tally(std::slice::from_ref(&reference), &suspect);

        assert_eq!(
            c.modal_step,
            pairwise.fork_step_observed(),
            "N=1 consensus is the single-reference answer, unchanged"
        );
        assert_eq!(c.support, 1);
        assert_eq!(c.agreement(), Some(1.0));
    }

    #[test]
    fn references_that_converge_leave_no_modal_fork() {
        let suspect = suspect();
        let refs = [suspect.clone(), suspect.clone()];
        let c = tally(&refs, &suspect);

        assert_eq!(c.votes, vec![None, None], "a self-diff converges");
        assert_eq!(c.modal_step, None);
        assert_eq!(c.support, 0);
        assert_eq!(c.forked(), 0);
        assert_eq!(
            c.agreement(),
            None,
            "no reference saw a divergence — that is undefined agreement, not 0%"
        );
    }

    #[test]
    fn converged_references_do_not_dilute_the_forking_ones() {
        let suspect = suspect();
        let refs = [
            suspect.clone(),
            reference_forking_at_2("g1"),
            reference_forking_at_2("g2"),
        ];
        let c = tally(&refs, &suspect);

        assert_eq!(c.votes, vec![None, Some(2), Some(2)]);
        assert_eq!(c.modal_step, Some(2));
        assert_eq!(c.references(), 3);
        assert_eq!(c.forked(), 2);
        assert_eq!(
            c.agreement(),
            Some(1.0),
            "both references that forked agreed; the abstainer is not a dissent"
        );
    }

    #[test]
    fn no_references_is_an_empty_consensus_not_a_converged_one() {
        let c = tally(&[], &suspect());

        assert!(c.votes.is_empty());
        assert_eq!(c.modal_step, None);
        assert_eq!(c.support, 0);
        assert_eq!(
            c.references(),
            0,
            "the caller can tell 'nobody was asked' from 'everybody converged'"
        );
        assert_eq!(c.agreement(), None);
    }

    #[test]
    fn the_size_guard_propagates_from_any_reference() {
        let params = DiffParams {
            align: AlignParams {
                max_steps: 1,
                ..AlignParams::default()
            },
            ..DiffParams::default()
        };
        let refs = [reference_forking_at_2("g0")];

        assert_eq!(
            consensus(&refs, &suspect(), &LexicalCost, &params).unwrap_err(),
            ParamError::StepsExceedMax { steps: 4, max: 1 },
            "an unalignable reference is an error, never a silently dropped vote"
        );
    }
}
