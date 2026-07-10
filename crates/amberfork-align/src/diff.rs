//! The crate's public seam: two [`Run`]s in, one [`DiffResult`] out.
//!
//! This is the function `amberfork diff <bad> --against <good>` ultimately calls: `<good>` is
//! `reference` (side `a`, the "model"), `<bad>` is `observed` (side `b`, the "log"). It fills
//! exactly what the alignment engine computes — run refs, the move-typed alignment, the fork,
//! static attribution derived from both, passive-source meta — and leaves the rest honestly
//! empty rather than approximated: `field_diffs` is a later pipeline stage (issue #13), and
//! `warnings` belong to whoever loaded the runs (`amberfork-ingest` returns them; the CLI
//! merges them into the result).

use crate::attribution::static_attribution;
use crate::cost::CostModel;
use crate::fork::{ForkParams, find_fork};
use crate::nw::{AlignParams, align};
use amberfork_model::{DiffResult, Meta, Run, RunPair, RunRef, Source};

/// Everything tunable about a diff, one level up: the aligner's gap penalties and the fork
/// rule's threshold/recovery window. `Default` is the dev-calibrated configuration.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DiffParams {
    pub align: AlignParams,
    pub fork: ForkParams,
}

/// Diff `observed` against `reference`: align, locate the fork, assemble the result.
#[must_use]
pub fn diff(
    reference: &Run,
    observed: &Run,
    cost_model: &impl CostModel,
    params: &DiffParams,
) -> DiffResult {
    let alignment = align(&reference.steps, &observed.steps, cost_model, &params.align);
    let fork = find_fork(&alignment, &params.fork);
    let mut result = DiffResult {
        runs: RunPair {
            a: run_ref(reference),
            b: run_ref(observed),
        },
        alignment,
        fork,
        field_diffs: Vec::new(),
        attribution: None,
        warnings: Vec::new(),
        meta: Meta::current(Source::Passive),
    };
    // Attribution reads the assembled result (it reuses `fork_step_observed`), so it is the
    // one field filled in a second pass.
    result.attribution = static_attribution(&result);
    result
}

fn run_ref(run: &Run) -> RunRef {
    RunRef {
        id: run.id.clone(),
        task: run.task.clone(),
        outcome: run.outcome,
        n_steps: run.steps.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::LexicalCost;
    use amberfork_model::{
        AttributionMode, MoveKind, Outcome, Payload, SchemaVersion, Step, StepKind,
    };
    use serde_json::Map;

    fn step(idx: usize, name: &str, out: &str) -> Step {
        Step {
            idx,
            kind: StepKind::Tool,
            name: name.to_string(),
            inputs: None,
            outputs: Some(Payload::Text(out.to_string())),
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        }
    }

    fn run(id: &str, outcome: Outcome, names_outs: &[(&str, &str)]) -> Run {
        Run {
            schema_version: SchemaVersion::current(),
            id: id.to_string(),
            task: Some("find the census figure".to_string()),
            outcome: Some(outcome),
            steps: names_outs
                .iter()
                .enumerate()
                .map(|(i, (n, o))| step(i, n, o))
                .collect(),
            edges: None,
        }
    }

    #[test]
    fn converged_diff_has_no_fork_and_honest_empties() {
        let good = run(
            "good",
            Outcome::Pass,
            &[("plan", "search then verify"), ("search", "9 results")],
        );
        let result = diff(&good, &good, &LexicalCost, &DiffParams::default());

        assert!(result.fork.is_none(), "self-diff is the converged state");
        assert!(result.alignment.iter().all(|m| m.kind == MoveKind::Sync));
        assert!(result.field_diffs.is_empty());
        assert!(result.attribution.is_none());
        assert!(result.warnings.is_empty());
        assert_eq!(result.meta.source, Source::Passive);
        assert!(result.meta.schema_version.is_current());
    }

    #[test]
    fn run_refs_carry_identity_without_the_trajectory() {
        let good = run("good", Outcome::Pass, &[("plan", "x"), ("act", "y")]);
        let bad = run("bad", Outcome::Fail, &[("plan", "x")]);
        let result = diff(&good, &bad, &LexicalCost, &DiffParams::default());

        assert_eq!(result.runs.a.id, "good");
        assert_eq!(result.runs.a.outcome, Some(Outcome::Pass));
        assert_eq!(result.runs.a.n_steps, 2);
        assert_eq!(result.runs.b.id, "bad");
        assert_eq!(result.runs.b.outcome, Some(Outcome::Fail));
        assert_eq!(result.runs.b.n_steps, 1);
    }

    #[test]
    fn diverging_tail_forks_on_the_observed_side() {
        let good = run(
            "good",
            Outcome::Pass,
            &[
                ("plan", "search for census data"),
                ("search", "census.gov top result"),
                ("fetch", "census.gov page: population 8,443,000"),
                ("answer", "population is 8,443,000"),
            ],
        );
        let bad = run(
            "bad",
            Outcome::Fail,
            &[
                ("plan", "search for census data"),
                ("search", "census.gov top result"),
                ("fetch", "blogspot page: the city has grown to 9,100,000"),
                ("answer", "population is 9,100,000"),
            ],
        );
        let result = diff(&good, &bad, &LexicalCost, &DiffParams::default());
        let fork = result.fork.expect("diverging tail must fork");
        assert_eq!(fork.b_step, Some(2), "fork at the bad fetch");
        assert!(fork.confidence > 0.0);
    }

    #[test]
    fn forked_diff_carries_static_attribution() {
        let good = run(
            "good",
            Outcome::Pass,
            &[
                ("plan", "search for census data"),
                ("search", "census.gov top result"),
                ("fetch", "census.gov page: population 8,443,000"),
                ("answer", "population is 8,443,000"),
            ],
        );
        let bad = run(
            "bad",
            Outcome::Fail,
            &[
                ("plan", "search for census data"),
                ("search", "census.gov top result"),
                ("fetch", "blogspot page: the city has grown to 9,100,000"),
                ("answer", "population is 9,100,000"),
            ],
        );
        let result = diff(&good, &bad, &LexicalCost, &DiffParams::default());
        let fork = result.fork.expect("diverging tail must fork");
        let attribution = result
            .attribution
            .as_ref()
            .expect("a forked diff must attribute the regression");

        assert_eq!(attribution.mode, AttributionMode::Static);
        assert_eq!(
            attribution.origin_step,
            result.fork_step_observed(),
            "origin is the canonical observed fork step, one rule for every consumer"
        );
        assert_eq!(attribution.propagation, vec![3], "the divergent tail");
        assert_eq!(attribution.confidence, fork.confidence);
        assert!(attribution.counterfactual.is_none());
        assert!(attribution.cause_label.is_none());
    }
}
