//! What a [`crate::Judge`] is allowed to see.

use amberfork_model::{DiffResult, Payload, Run, StepKind};

/// Which run a windowed [`StepSnapshot`] came from — the same neutral `a`/`b` convention
/// [`DiffResult`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// A minimal, judge-safe view of one step: enough content to narrate, deliberately shaped like
/// [`amberfork_model::Step`] but never the step itself — a judge reads a copy, not a borrow of
/// the trace, so widening what it can reach later is a conscious type change here, not a
/// visibility slip.
#[derive(Debug, Clone, PartialEq)]
pub struct StepSnapshot {
    pub side: Side,
    pub idx: usize,
    pub kind: StepKind,
    pub name: String,
    pub inputs: Option<Payload>,
    pub outputs: Option<Payload>,
}

/// What a [`crate::Judge`] is handed: the full [`DiffResult`] (so it can see the fork,
/// attribution, and warnings) plus a narrow content window — the fork step and its `k`
/// neighbours on each side, never the two full trajectories. This is guardrail #3 from the
/// issue: a judge cannot hunt for a different fork because it never sees content outside the
/// window.
#[derive(Debug, Clone)]
pub struct ExplainContext<'a> {
    pub result: &'a DiffResult,
    pub window: Vec<StepSnapshot>,
}

impl<'a> ExplainContext<'a> {
    /// Build the fork-region window: `k` neighbours before and after the fork step on each side
    /// that has one. A converged `result` (no fork) yields an empty window — there is nothing to
    /// narrate, and [`crate::ground`] treats a converged result as its own case.
    #[must_use]
    pub fn windowed(result: &'a DiffResult, a: &Run, b: &Run, k: usize) -> Self {
        let mut window = Vec::new();
        if let Some(fork) = result.fork {
            if let Some(a_step) = fork.a_step {
                window.extend(neighborhood(Side::A, a, a_step, k));
            }
            if let Some(b_step) = fork.b_step {
                window.extend(neighborhood(Side::B, b, b_step, k));
            }
        }
        Self { result, window }
    }
}

/// The steps within `k` positions of `center` in `run`, clamped to the run's bounds. Indexes by
/// position (the contract a well-formed [`Run`]'s `Step::idx` already agrees with), and never
/// panics on an out-of-range `center` — an untrusted/deserialized `DiffResult` just yields an
/// empty or partial window rather than a crash.
fn neighborhood(side: Side, run: &Run, center: usize, k: usize) -> Vec<StepSnapshot> {
    if run.steps.is_empty() {
        return Vec::new();
    }
    let start = center.saturating_sub(k);
    let end = (center.saturating_add(k)).min(run.steps.len() - 1);
    (start..=end)
        .filter_map(|i| run.steps.get(i))
        .map(|step| StepSnapshot {
            side,
            idx: step.idx,
            kind: step.kind,
            name: step.name.clone(),
            inputs: step.inputs.clone(),
            outputs: step.outputs.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support::{run, step};
    use amberfork_model::{Fork, Meta, Outcome, RunPair, RunRef, Source};

    fn make_run(id: &str, n: usize) -> Run {
        let steps = (0..n)
            .map(|i| step(i, format!("s{i}")).kind(StepKind::Llm).build())
            .collect();
        run(id, steps).build()
    }

    fn diff_result_with_fork(fork: Option<Fork>, a_steps: usize, b_steps: usize) -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: RunRef {
                    id: "a".into(),
                    task: None,
                    outcome: Some(Outcome::Pass),
                    n_steps: a_steps,
                },
                b: RunRef {
                    id: "b".into(),
                    task: None,
                    outcome: Some(Outcome::Fail),
                    n_steps: b_steps,
                },
            },
            alignment: Vec::new(),
            fork,
            field_diffs: Vec::new(),
            attribution: None,
            deltas: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Passive),
        }
    }

    #[test]
    fn converged_result_yields_an_empty_window() {
        let a = make_run("a", 5);
        let b = make_run("b", 5);
        let result = diff_result_with_fork(None, 5, 5);

        let ctx = ExplainContext::windowed(&result, &a, &b, 2);

        assert!(ctx.window.is_empty());
    }

    #[test]
    fn window_covers_k_neighbours_on_each_side_that_has_a_fork_step() {
        let a = make_run("a", 10);
        let b = make_run("b", 10);
        let fork = Fork {
            index: 4,
            a_step: Some(5),
            b_step: Some(5),
            confidence: 0.9,
        };
        let result = diff_result_with_fork(Some(fork), 10, 10);

        let ctx = ExplainContext::windowed(&result, &a, &b, 1);

        let a_idxs: Vec<usize> = ctx
            .window
            .iter()
            .filter(|s| s.side == Side::A)
            .map(|s| s.idx)
            .collect();
        let b_idxs: Vec<usize> = ctx
            .window
            .iter()
            .filter(|s| s.side == Side::B)
            .map(|s| s.idx)
            .collect();
        assert_eq!(a_idxs, vec![4, 5, 6]);
        assert_eq!(b_idxs, vec![4, 5, 6]);
    }

    #[test]
    fn window_clamps_to_run_bounds_without_panicking() {
        let a = make_run("a", 3);
        let b = make_run("b", 3);
        let fork = Fork {
            index: 0,
            a_step: Some(0),
            b_step: Some(2),
            confidence: 0.5,
        };
        let result = diff_result_with_fork(Some(fork), 3, 3);

        let ctx = ExplainContext::windowed(&result, &a, &b, 5);

        let a_idxs: Vec<usize> = ctx
            .window
            .iter()
            .filter(|s| s.side == Side::A)
            .map(|s| s.idx)
            .collect();
        let b_idxs: Vec<usize> = ctx
            .window
            .iter()
            .filter(|s| s.side == Side::B)
            .map(|s| s.idx)
            .collect();
        assert_eq!(a_idxs, vec![0, 1, 2]);
        assert_eq!(b_idxs, vec![0, 1, 2]);
    }

    #[test]
    fn a_model_only_fork_windows_only_the_a_side() {
        let a = make_run("a", 5);
        let b = make_run("b", 5);
        let fork = Fork {
            index: 2,
            a_step: Some(2),
            b_step: None,
            confidence: 0.7,
        };
        let result = diff_result_with_fork(Some(fork), 5, 5);

        let ctx = ExplainContext::windowed(&result, &a, &b, 1);

        assert!(ctx.window.iter().all(|s| s.side == Side::A));
        assert!(!ctx.window.is_empty());
    }
}
