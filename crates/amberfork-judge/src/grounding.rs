//! The grounding guard: the enforcement point for guardrail #1 (the aligner stays the headline,
//! the AI layer never localizes).

use crate::judge::Explanation;
use amberfork_model::DiffResult;
use std::fmt;

/// An [`Explanation`] that has been checked against the [`DiffResult`] it claims to describe —
/// safe for a caller to render. The only way to get one is [`ground`].
#[derive(Debug, Clone, PartialEq)]
pub struct Grounded {
    pub narrative: String,
    pub speculative_fix: Option<String>,
}

/// Validate a judge's claim against the result it was asked about.
///
/// A converged `result` (`fork: None`) is grounded only by an explanation that also claims no
/// fork; a forked `result` is grounded only by an explanation naming that exact `Fork::index`.
/// Any other combination means the judge described a different divergence than the one the
/// aligner found — rejected outright rather than rendered, so prose can never second-guess the
/// deterministic fork.
///
/// # Errors
///
/// Returns [`GroundingError::Ungrounded`] when the claim does not match. The documented caller
/// behavior is to drop the explanation and fall back to the plain deterministic fork.
pub fn ground(result: &DiffResult, explanation: Explanation) -> Result<Grounded, GroundingError> {
    let actual = result.fork.map(|fork| fork.index);
    if explanation.fork_index != actual {
        return Err(GroundingError {
            claimed: explanation.fork_index,
            actual,
        });
    }
    Ok(Grounded {
        narrative: explanation.narrative,
        speculative_fix: explanation.speculative_fix,
    })
}

/// A judge's claim did not match the `DiffResult` it was asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundingError {
    /// The fork index (or lack of one) the explanation claimed.
    pub claimed: Option<usize>,
    /// The fork index (or lack of one) the `DiffResult` actually carries.
    pub actual: Option<usize>,
}

impl fmt::Display for GroundingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "judge claimed fork index {:?} but the result's fork is {:?}",
            self.claimed, self.actual
        )
    }
}

impl std::error::Error for GroundingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Fork, Meta, Outcome, RunPair, RunRef, Source};

    fn diff_result(fork: Option<Fork>) -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: RunRef {
                    id: "a".into(),
                    task: None,
                    outcome: Some(Outcome::Pass),
                    n_steps: 5,
                },
                b: RunRef {
                    id: "b".into(),
                    task: None,
                    outcome: Some(Outcome::Fail),
                    n_steps: 5,
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

    fn fork_at(index: usize) -> Fork {
        Fork {
            index,
            a_step: Some(index),
            b_step: Some(index),
            confidence: 0.8,
        }
    }

    #[test]
    fn a_matching_claim_on_a_forked_result_is_grounded() {
        let result = diff_result(Some(fork_at(3)));
        let explanation = Explanation {
            fork_index: Some(3),
            narrative: "the tool call diverges here".into(),
            speculative_fix: None,
        };

        let grounded = ground(&result, explanation).expect("matching claim grounds");

        assert_eq!(grounded.narrative, "the tool call diverges here");
    }

    #[test]
    fn a_mismatched_claim_on_a_forked_result_is_rejected() {
        let result = diff_result(Some(fork_at(3)));
        let explanation = Explanation {
            fork_index: Some(7),
            narrative: "a different step diverges".into(),
            speculative_fix: None,
        };

        let err = ground(&result, explanation).expect_err("mismatched claim is ungrounded");

        assert_eq!(err.claimed, Some(7));
        assert_eq!(err.actual, Some(3));
    }

    #[test]
    fn no_claim_on_a_forked_result_is_rejected() {
        let result = diff_result(Some(fork_at(3)));
        let explanation = Explanation {
            fork_index: None,
            narrative: "no divergence to explain".into(),
            speculative_fix: None,
        };

        let err = ground(&result, explanation).expect_err("silence on a real fork is ungrounded");

        assert_eq!(err.claimed, None);
        assert_eq!(err.actual, Some(3));
    }

    #[test]
    fn no_claim_on_a_converged_result_is_grounded() {
        let result = diff_result(None);
        let explanation = Explanation {
            fork_index: None,
            narrative: "no divergence to explain".into(),
            speculative_fix: None,
        };

        let grounded = ground(&result, explanation).expect("agreeing on convergence grounds");

        assert_eq!(grounded.narrative, "no divergence to explain");
    }

    #[test]
    fn a_fabricated_fork_on_a_converged_result_is_rejected() {
        let result = diff_result(None);
        let explanation = Explanation {
            fork_index: Some(2),
            narrative: "step 2 diverges".into(),
            speculative_fix: None,
        };

        let err =
            ground(&result, explanation).expect_err("a fabricated fork on convergence is caught");

        assert_eq!(err.claimed, Some(2));
        assert_eq!(err.actual, None);
    }
}
