//! The `DiffResult` output schema — the public `--json` contract and the shape the UI reads.
//!
//! Where [`crate`]'s [`Run`](crate::Run) is the *input* seam, `DiffResult` is the *output*
//! seam: what `adiff-align` fills in and `adiff-cli` / the Leptos UI render. It is transcribed
//! from the design doc's result schema (`docs/design/design-run-diff-debugger.md`, "Result
//! schema").
//!
//! Two invariants are encoded in the types rather than left to convention:
//! - A converged diff (the self-align case: a run against itself) has **no fork** — hence
//!   [`DiffResult::fork`] is an `Option`, and its `None` state is the designed converged state.
//! - A [`Move`] is well-formed only in three shapes (synchronous / log-only / model-only); the
//!   [`Move::sync`], [`Move::log`], and [`Move::model`] constructors are the way to build one so
//!   an aligner can't accidentally emit an illegal index combination.

use crate::{Outcome, SchemaVersion};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The two runs a diff is over. `a` and `b` are neutral sides; which one failed is carried by
/// each ref's [`Outcome`], and step indices in [`Move`]/[`Fork`] are relative to these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPair {
    pub a: RunRef,
    pub b: RunRef,
}

/// A lightweight handle to a run — enough to label a side in the UI without re-embedding the
/// whole trajectory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    /// Number of steps in the run.
    pub n_steps: usize,
}

/// The class of an alignment move, in the process-mining sense the aligner uses: a synchronous
/// move pairs a step from each run; a log/model move is a gap present on only one side. By
/// convention run `b` is the "log" (observed/failing) and run `a` is the "model" (reference),
/// so a [`MoveKind::Log`] step is extra in `b` and a [`MoveKind::Model`] step is missing from
/// `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveKind {
    Sync,
    Log,
    Model,
}

/// One move in the alignment. The invariant tying `kind` to the indices — sync has both, log
/// has only `b_idx`, model has only `a_idx` — is guaranteed by the [`Move::sync`]/[`Move::log`]
/// /[`Move::model`] constructors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub kind: MoveKind,
    /// Step index in run `a`, present unless this is a log-only move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_idx: Option<usize>,
    /// Step index in run `b`, present unless this is a model-only move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_idx: Option<usize>,
    /// Alignment cost of this move (lower = more similar). Domain of the cost model.
    pub cost: f64,
    /// Confidence in this move, in `[0, 1]`.
    pub confidence: f64,
}

impl Move {
    /// A synchronous move: step `a_idx` in run `a` aligned to step `b_idx` in run `b`.
    #[must_use]
    pub fn sync(a_idx: usize, b_idx: usize, cost: f64, confidence: f64) -> Self {
        Self {
            kind: MoveKind::Sync,
            a_idx: Some(a_idx),
            b_idx: Some(b_idx),
            cost,
            confidence,
        }
    }

    /// A log-only move: a step present in run `b` with no counterpart in run `a`.
    #[must_use]
    pub fn log(b_idx: usize, cost: f64, confidence: f64) -> Self {
        Self {
            kind: MoveKind::Log,
            a_idx: None,
            b_idx: Some(b_idx),
            cost,
            confidence,
        }
    }

    /// A model-only move: a step present in run `a` with no counterpart in run `b`.
    #[must_use]
    pub fn model(a_idx: usize, cost: f64, confidence: f64) -> Self {
        Self {
            kind: MoveKind::Model,
            a_idx: Some(a_idx),
            b_idx: None,
            cost,
            confidence,
        }
    }
}

/// The divergence point: the first non-sync block the alignment does not recover from
/// (resync-k rule; see the 2026-07-08 amendment). Absent on a converged diff.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fork {
    /// Index into [`DiffResult::alignment`] where the unrecovered divergence begins.
    pub index: usize,
    /// Diverging step in run `a`, if the fork has an `a` side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_step: Option<usize>,
    /// Diverging step in run `b`, if the fork has a `b` side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_step: Option<usize>,
    /// Confidence in the fork localization, in `[0, 1]`.
    pub confidence: f64,
}

/// How a single field changed between two aligned steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldDiffKind {
    /// Present in run `b` but not run `a`.
    Added,
    /// Present in run `a` but not run `b`.
    Removed,
    /// Present in both, with a different value.
    Changed,
}

/// A field-level difference within an aligned step pair (the object/text diff inside a sync
/// move). Emitted only where the payloads actually differ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDiff {
    /// Index into [`DiffResult::alignment`] identifying the aligned pair this diff refines.
    pub step: usize,
    /// Path into the payload, e.g. `outputs.status` (or the empty string for a whole text body).
    pub path: String,
    /// Value on run `a`'s side; `None` when the field was [`FieldDiffKind::Added`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    /// Value on run `b`'s side; `None` when the field was [`FieldDiffKind::Removed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
    pub kind: FieldDiffKind,
}

/// Whether attribution was computed structurally or by counterfactual re-execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttributionMode {
    Static,
    Counterfactual,
}

/// Outcome of counterfactual re-execution: did patching the origin step recover the run? The
/// tri-state (rather than a bare `bool`) makes "we did not verify" a first-class, honest value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recovery {
    Recovered,
    NotRecovered,
    Unverified,
}

/// The counterfactual evidence behind an attribution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Counterfactual {
    pub recovered: Recovery,
    /// Number of counterfactual re-execution runs performed.
    pub runs: u32,
}

/// Where the regression is attributed and how far it propagated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribution {
    pub mode: AttributionMode,
    /// The step the regression originated at, if localized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_step: Option<usize>,
    /// Steps the error propagated through, in order.
    #[serde(default)]
    pub propagation: Vec<usize>,
    /// Counterfactual evidence, present only in [`AttributionMode::Counterfactual`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterfactual: Option<Counterfactual>,
    /// Optional human-readable cause name from the judge. Semantic naming only — never
    /// localization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause_label: Option<String>,
    /// Confidence in the attribution, in `[0, 1]`.
    pub confidence: f64,
}

/// A non-fatal diagnostic emitted while building the diff (e.g. unmapped attributes, content
/// absent from a metadata-only trace).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub code: WarningCode,
    pub msg: String,
}

/// Known warning codes. A closed set so the vocabulary stays single-sourced across crates;
/// adding a code is a deliberate, versioned change to this contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningCode {
    /// Attributes present on the source span that did not map onto the canonical model.
    UnmappedAttributes,
    /// A step carried no input/output content (metadata-only trace).
    ContentAbsent,
}

/// Which execution path produced the inputs: passively aligning existing traces, or an
/// `adiff record` capture session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Passive,
    Record,
}

/// Envelope metadata for a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub schema_version: SchemaVersion,
    pub source: Source,
}

impl Meta {
    /// Metadata stamped with the current schema version.
    #[must_use]
    pub fn current(source: Source) -> Self {
        Self {
            schema_version: SchemaVersion::current(),
            source,
        }
    }
}

/// The complete result of diffing two runs — the frozen output contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffResult {
    pub runs: RunPair,
    /// The full move-typed alignment, in order.
    #[serde(default)]
    pub alignment: Vec<Move>,
    /// The divergence point, or `None` on a converged diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<Fork>,
    /// Field-level differences within aligned pairs.
    #[serde(default)]
    pub field_diffs: Vec<FieldDiff>,
    /// Regression attribution, if computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,
    /// Non-fatal diagnostics.
    #[serde(default)]
    pub warnings: Vec<Warning>,
    pub meta: Meta,
}
