//! Canonical trajectory model for amberfork — the frozen contract every crate reads.
//!
//! A [`Run`] is one agent trajectory: an ordered list of [`Step`]s plus optional DAG
//! [`Edge`]s. This is the *input* seam — `amberfork-ingest` produces it from OTel/plain JSON,
//! and `amberfork-align` consumes it. It mirrors the public wire format documented in
//! `docs/trace-format.md`; once this crate exists, these types are the source of truth and
//! that document tracks them.
//!
//! [`DiffResult`] is the matching *output* seam: what `amberfork-align` fills in and the CLI's
//! `--json` and the Leptos UI render.
//!
//! Design rules baked into the types (see `docs/design/design-run-diff-debugger.md`):
//! - `outcome` is a run-level verdict supplied by the user, **never inferred from span
//!   status**.
//! - Timing (`t_start`/`t_end`) is display-only and is **never** an alignment signal, so it
//!   stays a raw string here rather than a parsed timestamp.
//! - `inputs`/`outputs` distinguish text from structured payloads in the type itself, because
//!   the diff engine text-diffs strings but field-diffs objects.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod diff;
pub use diff::{
    Attribution, AttributionMode, Counterfactual, DiffResult, FieldDiff, FieldDiffKind, Fork, Meta,
    Move, MoveKind, Recovery, RunPair, RunRef, Source, Warning, WarningCode,
};

/// Version of the trace-format / model contract. Breaking changes (renames, removals,
/// semantic shifts) bump it; additive optional fields do not. Kept as a newtype so the
/// version is a first-class, self-documenting value across the workspace rather than a
/// bare string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub String);

impl SchemaVersion {
    /// The version this build emits and treats as native.
    pub const CURRENT: &'static str = "0.1";

    /// The current contract version.
    #[must_use]
    pub fn current() -> Self {
        Self(Self::CURRENT.to_string())
    }

    /// Whether this run declares the version this build emits natively.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.0 == Self::CURRENT
    }
}

impl Default for SchemaVersion {
    fn default() -> Self {
        Self::current()
    }
}

/// Run-level verdict, if known. Deliberately supplied by the user (an assertion, a label, a
/// gold answer) — amberfork never derives it from trace/span status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    Unknown,
}

/// The structural class of a step. Part of the identity the aligner keys on. This is the
/// canonical, post-normalization vocabulary: `amberfork-ingest` is responsible for mapping the
/// many framework-specific span kinds down onto these four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    Llm,
    Tool,
    Agent,
    Other,
}

/// A step's `inputs` or `outputs`. The variant is the semantic seam the diff engine reads:
/// [`Payload::Text`] gets text diffing, [`Payload::Object`] gets field-level diffing. The
/// untagged catch-all [`Payload::Other`] keeps the "any log massaged into this shape" promise
/// alive — an array/number/bool payload is preserved verbatim instead of failing the parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Payload {
    Text(String),
    Object(Map<String, Value>),
    Other(Value),
}

/// A directed DAG edge between step indices, `[from, to]` on the wire. A tuple struct so it
/// serializes as a two-element array, matching `docs/trace-format.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge(pub usize, pub usize);

/// One node in a trajectory.
///
/// Minimal valid step: `idx`, `kind`, `name`, and at least one of `inputs`/`outputs`. All
/// other fields are optional and omitted from serialization when empty, so re-emitted traces
/// stay compact without losing information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// 0-based position in the trajectory.
    pub idx: usize,
    /// Structural class of the step.
    pub kind: StepKind,
    /// Agent or tool name — part of the structural identity the aligner keys on.
    pub name: String,
    /// Step input, if captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Payload>,
    /// Step output, if captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Payload>,
    /// Anything else worth keeping (model, tokens, cost, …).
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attrs: Map<String, Value>,
    /// RFC3339 start time. Display-only — never an alignment signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_start: Option<String>,
    /// RFC3339 end time. Display-only — never an alignment signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_end: Option<String>,
    /// Caller step index; absent/null on every step means a linear chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_idx: Option<usize>,
}

/// One agent trajectory: the unit `amberfork diff` aligns against another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Version of the contract this run was written against.
    pub schema_version: SchemaVersion,
    /// Unique run id (any string).
    pub id: String,
    /// Human label of what the run attempted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// Run-level verdict, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    /// The trajectory, in order.
    pub steps: Vec<Step>,
    /// Explicit DAG edges. If absent, the graph is derived from `parent_idx`, else linear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<Edge>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_current_is_native() {
        let v = SchemaVersion::current();
        assert!(v.is_current());
        assert_eq!(v.0, SchemaVersion::CURRENT);
        assert_eq!(SchemaVersion::default(), v);
    }

    #[test]
    fn schema_version_serializes_transparently() {
        // The newtype must be indistinguishable from a bare string on the wire.
        let json = serde_json::to_string(&SchemaVersion::current()).unwrap();
        assert_eq!(json, "\"0.1\"");
        let back: SchemaVersion = serde_json::from_str("\"0.1\"").unwrap();
        assert_eq!(back, SchemaVersion::current());
    }
}
