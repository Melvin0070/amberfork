//! The in-process [`Judge`] that keeps `cargo test --workspace` offline.

use crate::context::ExplainContext;
use crate::judge::{Explanation, Judge, JudgeError};
use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Mutex, PoisonError};

/// An in-process [`Judge`] that serves a fixed script of results in order.
///
/// This is the seam that keeps judge tests offline: instead of a network provider, a call is
/// answered from a queue the test set up. It hands out results in FIFO order and reports
/// [`JudgeError::Exhausted`] once the script runs dry, so a test that under-scripts fails
/// loudly rather than serving a stale answer — the same discipline
/// `amberfork_replay::ScriptedUpstream` uses for the replay path.
#[derive(Debug)]
pub struct ScriptedJudge {
    results: Mutex<VecDeque<Result<Explanation, JudgeError>>>,
}

impl ScriptedJudge {
    /// A stub that will serve `results` in order on successive calls.
    #[must_use]
    pub fn new(results: impl IntoIterator<Item = Result<Explanation, JudgeError>>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
        }
    }
}

impl Judge for ScriptedJudge {
    fn explain(
        &self,
        _context: &ExplainContext<'_>,
    ) -> impl Future<Output = Result<Explanation, JudgeError>> + Send {
        // The next scripted result is resolved synchronously and moved into the returned
        // future; the async wrapper exists only to satisfy the trait's I/O-shaped signature, so
        // it borrows neither `self` nor `context`.
        let next = self
            .results
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Err(JudgeError::Exhausted));
        async move { next }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support::run;
    use amberfork_model::{DiffResult, Meta, Outcome, RunPair, RunRef, Source};

    fn empty_runs() -> (amberfork_model::Run, amberfork_model::Run) {
        (run("a", Vec::new()).build(), run("b", Vec::new()).build())
    }

    fn converged_result() -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: RunRef {
                    id: "a".into(),
                    task: None,
                    outcome: Some(Outcome::Pass),
                    n_steps: 0,
                },
                b: RunRef {
                    id: "b".into(),
                    task: None,
                    outcome: Some(Outcome::Pass),
                    n_steps: 0,
                },
            },
            alignment: Vec::new(),
            fork: None,
            field_diffs: Vec::new(),
            attribution: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Passive),
        }
    }

    #[tokio::test]
    async fn scripted_results_are_served_in_order() {
        let judge = ScriptedJudge::new([
            Ok(Explanation {
                fork_index: None,
                narrative: "first".into(),
                speculative_fix: None,
            }),
            Ok(Explanation {
                fork_index: None,
                narrative: "second".into(),
                speculative_fix: None,
            }),
        ]);
        let result = converged_result();
        let (a, b) = empty_runs();
        let ctx = ExplainContext::windowed(&result, &a, &b, 2);

        let first = judge.explain(&ctx).await.expect("first is queued");
        let second = judge.explain(&ctx).await.expect("second is queued");

        assert_eq!(first.narrative, "first");
        assert_eq!(second.narrative, "second");
    }

    #[tokio::test]
    async fn an_exhausted_script_reports_exhausted() {
        let judge = ScriptedJudge::new(Vec::new());
        let result = converged_result();
        let (a, b) = empty_runs();
        let ctx = ExplainContext::windowed(&result, &a, &b, 2);

        let err = judge.explain(&ctx).await.expect_err("nothing was queued");

        assert!(matches!(err, JudgeError::Exhausted));
    }
}
