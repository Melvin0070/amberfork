//! The one operation a judge implements.

use crate::context::ExplainContext;
use std::fmt;
use std::future::Future;

/// A judge's raw claim about a diff — not yet trusted. [`crate::ground`] validates it against
/// the [`amberfork_model::DiffResult`] it was asked about before a caller may render it.
#[derive(Debug, Clone, PartialEq)]
pub struct Explanation {
    /// The alignment index (`DiffResult::alignment` position, i.e. `Fork::index`) the judge
    /// claims to be describing. `None` is the judge's answer for a converged
    /// [`ExplainContext`] — "no divergence to explain".
    pub fork_index: Option<usize>,
    /// 2-4 sentence plain-English description of what forked, grounded in the window it was
    /// given.
    pub narrative: String,
    /// An optional suggested fix. Always guesswork — `DiffResult` carries no remediation data —
    /// so a caller must render this under a separate speculative label, never alongside
    /// `narrative`'s grounding guarantee.
    pub speculative_fix: Option<String>,
}

/// One operation: narrate the fork an [`ExplainContext`] already points at. The aligner has
/// already decided *where*; a judge only ever describes and suggests (design guardrail #1) —
/// nothing in this trait lets an implementation report a different fork, and [`crate::ground`]
/// is the enforcement point that catches an implementation that tries anyway.
///
/// Native async trait (return-position `impl Future`, not the `async-trait` crate), mirroring
/// `amberfork-replay::Upstream`: a call site picks its implementation at compile time —
/// [`crate::ScriptedJudge`] in tests, a live local-model client in production — so no `dyn`, and
/// no per-call allocation, is needed.
pub trait Judge: Send + Sync {
    /// Explain the fork `context` points at.
    ///
    /// # Errors
    ///
    /// Returns [`JudgeError`] when no explanation can be produced. The documented caller
    /// behavior on error is to degrade to the plain deterministic fork plus a warning — never a
    /// block (issue #10 edge cases).
    fn explain(
        &self,
        context: &ExplainContext<'_>,
    ) -> impl Future<Output = Result<Explanation, JudgeError>> + Send;
}

/// Why a judge could not produce an explanation.
#[derive(Debug)]
#[non_exhaustive]
pub enum JudgeError {
    /// The scripted stub ran out of queued responses — a test drove more calls than it
    /// scripted.
    Exhausted,
    /// A live provider was unreachable or returned a transport-level failure.
    Unreachable(String),
}

impl fmt::Display for JudgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => f.write_str("scripted judge has no more responses"),
            Self::Unreachable(msg) => write!(f, "judge provider unreachable: {msg}"),
        }
    }
}

impl std::error::Error for JudgeError {}
