//! Static attribution — the first real producer for `DiffResult::attribution` (issue #12).
//!
//! The cheap, structural mode: no counterfactual re-execution, no judge. It restates the
//! alignment's own evidence in the attribution contract's terms — the regression originates
//! at the fork's observed step and propagates through every observed step downstream of it
//! (DR4's uniform divergent path: static analysis cannot tell a benign downstream resync from
//! carried error; distinguishing them is exactly what counterfactual re-execution is for).
//! Confidence is the fork's own localization confidence: this mode adds no new evidence, so
//! it must not claim any. `counterfactual` stays `None` — that field is evidence of
//! re-execution, which never happened here — and `cause_label` stays `None` (semantic naming
//! is the judge's job, never localization's).

use amberfork_model::{Attribution, AttributionMode, DiffResult};

/// Attribute the regression structurally from an assembled diff, or `None` on convergence —
/// a converged pair has no regression to attribute.
pub(crate) fn static_attribution(result: &DiffResult) -> Option<Attribution> {
    let fork = result.fork?;
    // The one canonical "where did MY run go wrong" rule — never re-derived here. On a
    // model-only fork it names the nearest observed step, which may itself sit downstream;
    // the propagation filter below keeps that step from being counted twice.
    let origin_step = result.fork_step_observed();
    let downstream_start = (fork.index + 1).min(result.alignment.len());
    let propagation = result.alignment[downstream_start..]
        .iter()
        .filter_map(|m| m.b_idx)
        .filter(|&step| Some(step) != origin_step)
        .collect();
    Some(Attribution {
        mode: AttributionMode::Static,
        origin_step,
        propagation,
        counterfactual: None,
        cause_label: None,
        confidence: fork.confidence,
    })
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

    fn result(alignment: Vec<Move>, fork: Option<Fork>, n_observed: usize) -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: run_ref("good", alignment.len()),
                b: run_ref("bad", n_observed),
            },
            alignment,
            fork,
            field_diffs: Vec::new(),
            attribution: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Passive),
        }
    }

    /// A fork pointing at the first move of the unrecovered block, the way `find_fork` builds
    /// one from that move's own indices.
    fn fork_at(alignment: &[Move], index: usize, confidence: f64) -> Fork {
        let mv = &alignment[index];
        Fork {
            index,
            a_step: mv.a_idx,
            b_step: mv.b_idx,
            confidence,
        }
    }

    #[test]
    fn no_fork_yields_no_attribution() {
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::sync(1, 1, 0.1, 0.9)];
        assert_eq!(static_attribution(&result(alignment, None, 2)), None);
    }

    #[test]
    fn sync_fork_originates_at_its_observed_step_and_propagates_downstream() {
        let alignment = vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::sync(1, 1, 0.9, 0.1), // the fork
            Move::sync(2, 2, 0.8, 0.2),
            Move::log(3, 0.6, 0.9),
        ];
        let fork = fork_at(&alignment, 1, 0.85);
        let attribution =
            static_attribution(&result(alignment, Some(fork), 4)).expect("fork must attribute");

        assert_eq!(attribution.mode, AttributionMode::Static);
        assert_eq!(attribution.origin_step, Some(1));
        assert_eq!(attribution.propagation, vec![2, 3]);
        assert_eq!(attribution.confidence, 0.85);
        assert!(attribution.counterfactual.is_none(), "nothing was re-run");
        assert!(attribution.cause_label.is_none(), "naming is the judge's");
    }

    #[test]
    fn model_only_fork_does_not_double_count_the_nearest_observed_step() {
        // The fork is a gap on the observed side: origin falls to the nearest observed step
        // (fork_step_observed's rule), which is also the next move's b step — propagation
        // must not repeat it.
        let alignment = vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::model(1, 0.7, 0.8), // the fork: observed run is missing this step
            Move::sync(2, 1, 0.5, 0.5),
            Move::sync(3, 2, 0.6, 0.4),
        ];
        let fork = fork_at(&alignment, 1, 0.5);
        let attribution =
            static_attribution(&result(alignment, Some(fork), 3)).expect("fork must attribute");

        assert_eq!(attribution.origin_step, Some(1), "nearest observed step");
        assert_eq!(attribution.propagation, vec![2], "origin not repeated");
    }

    #[test]
    fn fork_on_the_last_move_propagates_nothing() {
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::sync(1, 1, 0.9, 0.1)];
        let fork = fork_at(&alignment, 1, 0.9);
        let attribution =
            static_attribution(&result(alignment, Some(fork), 2)).expect("fork must attribute");

        assert_eq!(attribution.origin_step, Some(1));
        assert!(attribution.propagation.is_empty());
    }
}
