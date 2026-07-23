//! Adapter from TRAIL / Patronus-SDK trace trees into canonical [`amberfork_model::Run`]s.
//!
//! TRAIL (Patronus, arXiv 2505.08638) publishes real agent traces as the Patronus SDK's own
//! **nested** JSON: `{"trace_id", "spans": [span, …]}`, where each span carries its children inline
//! under `child_spans`. This is a *different envelope* from the flat OTLP/JSON export
//! [`crate::openinference`] reads — but the semantic attributes on each span are the *same*
//! OpenInference vocabulary (`openinference.span.kind`, `input.value`/`output.value`, `llm.*`,
//! `tool.*`). So this adapter owns only the envelope — walking the tree — and delegates every
//! attribute decision to the shared [`crate::oivocab`] mapper, exactly as `openinference` does.
//!
//! Four boundaries the adapter makes explicit:
//! - **One run per trace file.** A TRAIL file is a single trace; it becomes one [`Run`] whose id is
//!   the `trace_id`.
//! - **Order is the tree, in pre-order.** Unlike a flat OTLP export (whose span order is not
//!   execution order), a Patronus trace tree is emitted by one SDK with children nested under their
//!   parent, so a pre-order depth-first walk *is* the execution order. Steps are indexed in that
//!   walk order and `parent_idx` comes from the nesting — no timestamp re-sort, no time dependency.
//! - **Kind is the OpenInference attribute, not the wire `span_kind`.** Every TRAIL span reports a
//!   transport `span_kind` of `"Internal"`; the semantic kind lives in
//!   `span_attributes["openinference.span.kind"]`, which is what [`crate::oivocab`] reads.
//! - **`outcome` is never inferred from `status_code`.** A TRAIL span's `status_code` (`Error`, …)
//!   is not a run verdict (architecture rule, `docs/trace-format.md`): a run's outcome is a user
//!   assertion, so this adapter always leaves it `None` and never reads the field.
//!
//! Provenance for downstream scoring: the source `span_id` is retained in `attrs["otel.span_id"]`
//! so a TRAIL error annotation — which locates a decisive error by span id — can be resolved to a
//! step index by a later slice. `timestamp` is RFC3339, so it lands natively in [`Step::t_start`];
//! `duration` (an ISO-8601 span, display-only) is preserved in `attrs`.

use crate::{IngestError, Ingested, oivocab};
use amberfork_model::{Run, SchemaVersion, Step, Warning};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

/// Parse a TRAIL/Patronus trace tree into one canonical [`Run`].
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid TRAIL JSON. Everything past a
/// successful parse is forgiving: a missing span kind, absent content, or a foreign attribute
/// yields a canonical fallback plus a warning, never an error. A trace with no spans yields a run
/// with no steps.
pub fn from_trace_json_str(s: &str) -> Result<Ingested, IngestError> {
    let trace: RawTrace =
        serde_json::from_str(s).map_err(|source| IngestError::Parse { path: None, source })?;
    Ok(normalize(trace))
}

/// Load a TRAIL/Patronus trace tree from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] (with the path
/// attached) if its contents are not valid TRAIL JSON.
pub fn load_file(path: impl AsRef<Path>) -> Result<Ingested, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_trace_json_str(&text).map_err(|err| err.with_path(path))
}

// --- Gold annotations -----------------------------------------------------------------------
//
// TRAIL ships error annotations *beside* the traces, one `processed_annotations_gaia/<id>.json`
// per trace: a list of decisive errors, each located at a span id. This is the gold a benchmark
// scores a fork prediction against. Like the `tape`/`whowhen` gold, it is parsed and returned
// beside the run — never merged into the trajectory, which is the run's own identity — and the
// span id → step index join is done here (via the `otel.span_id` the adapter retained) so the
// `otel.span_id` provenance key stays an internal detail of this adapter.

/// The impact an annotator assigned an error. A closed vocabulary in the dataset (`LOW`/`MEDIUM`/
/// `HIGH`), kept typed — with [`Impact::Other`] preserving any out-of-vocabulary value losslessly
/// rather than failing the parse, matching the crate's forgiving-input contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impact {
    Low,
    Medium,
    High,
    /// An impact value outside the known vocabulary, preserved verbatim.
    Other(String),
}

impl Impact {
    fn from_raw(raw: &str) -> Self {
        match raw.to_ascii_uppercase().as_str() {
            "LOW" => Self::Low,
            "MEDIUM" => Self::Medium,
            "HIGH" => Self::High,
            _ => Self::Other(raw.to_string()),
        }
    }
}

/// One TRAIL error annotation: a decisive error the annotators located at a span, with its
/// taxonomy category and impact. `location` is a source span id — resolve it to a step index with
/// [`TrailGold::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailError {
    /// The error taxonomy category (free text — TRAIL's taxonomy, e.g. `Instruction
    /// Non-compliance`), carried through for per-category coverage reporting.
    pub category: String,
    /// The source span id the error is attributed to.
    pub location: String,
    /// The annotator's impact rating.
    pub impact: Impact,
}

/// The TRAIL gold for one trace: the annotated errors, in annotation-file order. Kept beside the
/// run, never merged — a run's trajectory is its identity; the benchmark bookkeeping rides
/// alongside, exactly as the `tape`/`whowhen` gold does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailGold {
    pub trace_id: String,
    pub errors: Vec<TrailError>,
}

/// One error resolved against a run: the step index its span id maps to (`None` when that span is
/// absent from the run), with the category and impact carried for later per-category scoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldStep {
    /// The step index this error's span id resolved to, or `None` if the run has no such span.
    pub step: Option<usize>,
    pub category: String,
    pub location: String,
    pub impact: Impact,
}

/// Parse a TRAIL error-annotation file into the gold for one trace.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid annotation JSON. An empty `errors`
/// array is valid — a trace the annotators found no fault in has no gold fork.
pub fn annotations_from_json_str(s: &str) -> Result<TrailGold, IngestError> {
    let raw: RawAnnotations =
        serde_json::from_str(s).map_err(|source| IngestError::Parse { path: None, source })?;
    Ok(raw.into_gold())
}

/// Load a TRAIL error-annotation file from disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] (with the path
/// attached) if its contents are not valid annotation JSON.
pub fn load_annotations(path: impl AsRef<Path>) -> Result<TrailGold, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    annotations_from_json_str(&text).map_err(|err| err.with_path(path))
}

impl TrailGold {
    /// Resolve each error's span id to a step index in `run`, via the `otel.span_id` provenance the
    /// adapter retained on every step. Returns one [`GoldStep`] per error in file order; a span id
    /// absent from the run resolves to `None` (data, per protocol rule 4 — never a silent drop).
    #[must_use]
    pub fn resolve(&self, run: &Run) -> Vec<GoldStep> {
        let idx_of: HashMap<&str, usize> = run
            .steps
            .iter()
            .filter_map(|step| {
                step.attrs
                    .get("otel.span_id")
                    .and_then(Value::as_str)
                    .map(|span_id| (span_id, step.idx))
            })
            .collect();
        self.errors
            .iter()
            .map(|error| GoldStep {
                step: idx_of.get(error.location.as_str()).copied(),
                category: error.category.clone(),
                location: error.location.clone(),
                impact: error.impact.clone(),
            })
            .collect()
    }
}

/// One span flattened out of the tree: its assigned index, its parent's index (from the nesting),
/// and the envelope-specific fields the shared mapper needs.
struct FlatSpan {
    idx: usize,
    parent_idx: Option<usize>,
    span_id: String,
    span_name: String,
    timestamp: Option<String>,
    duration: Option<String>,
    attrs: Map<String, Value>,
}

/// Normalize a parsed trace: flatten the tree to steps in pre-order, then map each span's
/// attributes through the shared OpenInference vocabulary. Pure and infallible — the parse already
/// succeeded, so every remaining decision degrades to a canonical fallback plus a warning.
fn normalize(trace: RawTrace) -> Ingested {
    let mut flat = Vec::new();
    flatten(trace.spans, None, &mut flat);

    let mut warnings = Vec::new();
    let steps: Vec<Step> = flat
        .into_iter()
        .map(|span| span.into_step(&mut warnings))
        .collect();

    let id = if trace.trace_id.is_empty() {
        "trace-unknown".to_string()
    } else {
        trace.trace_id
    };
    let run = Run {
        schema_version: SchemaVersion::current(),
        id,
        task: None,
        outcome: None,
        steps,
        edges: None,
    };
    Ingested { run, warnings }
}

/// Depth-first pre-order walk: assign each span the next index, record its parent, then recurse
/// into its children with this span as their parent. The nesting is the only structure that
/// matters — `parent_span_id` on the wire is redundant with it and is not read.
fn flatten(spans: Vec<RawTrailSpan>, parent_idx: Option<usize>, out: &mut Vec<FlatSpan>) {
    for span in spans {
        let idx = out.len();
        let RawTrailSpan {
            span_id,
            span_name,
            timestamp,
            duration,
            span_attributes,
            child_spans,
        } = span;
        out.push(FlatSpan {
            idx,
            parent_idx,
            span_id,
            span_name,
            timestamp,
            duration,
            attrs: span_attributes,
        });
        flatten(child_spans, Some(idx), out);
    }
}

impl FlatSpan {
    fn into_step(self, warnings: &mut Vec<Warning>) -> Step {
        // The source span id is retained so a TRAIL error annotation (which locates a decisive
        // error by span id) can be resolved to this step later. Duration is display-only
        // provenance. Both are stamped after the foreign scan (in the shared mapper) so they are
        // never flagged as unmapped.
        let mut provenance = Vec::new();
        if !self.span_id.is_empty() {
            provenance.push(("otel.span_id", Value::from(self.span_id)));
        }
        if let Some(duration) = self.duration {
            provenance.push(("otel.duration", Value::from(duration)));
        }
        oivocab::map_span_to_step(
            self.idx,
            self.parent_idx,
            &self.span_name,
            self.attrs,
            provenance,
            self.timestamp,
            None,
            warnings,
        )
    }
}

// --- TRAIL/Patronus wire types --------------------------------------------------------------

/// A TRAIL trace file: an id and a forest of root spans. Every field defaults so a partial or
/// unfamiliar trace degrades to fewer spans rather than a parse failure.
#[derive(Deserialize)]
struct RawTrace {
    #[serde(default)]
    trace_id: String,
    #[serde(default)]
    spans: Vec<RawTrailSpan>,
}

/// One Patronus span. Only the fields the adapter reads are named; `parent_span_id` (redundant
/// with the nesting), `span_kind` (always `"Internal"` — the semantic kind is an attribute),
/// `status_code`, `logs`, `events`, and the rest are ignored by serde, never a parse failure.
#[derive(Deserialize)]
struct RawTrailSpan {
    #[serde(default)]
    span_id: String,
    #[serde(default)]
    span_name: String,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    span_attributes: Map<String, Value>,
    #[serde(default)]
    child_spans: Vec<RawTrailSpan>,
}

/// A TRAIL annotation file: the trace's errors plus a `scores` block. Only the errors are read;
/// `scores` and any other field are ignored by serde, never a parse failure.
#[derive(Deserialize)]
struct RawAnnotations {
    #[serde(default)]
    trace_id: String,
    #[serde(default)]
    errors: Vec<RawError>,
}

impl RawAnnotations {
    fn into_gold(self) -> TrailGold {
        TrailGold {
            trace_id: self.trace_id,
            errors: self
                .errors
                .into_iter()
                .map(|error| TrailError {
                    impact: Impact::from_raw(&error.impact),
                    category: error.category,
                    location: error.location,
                })
                .collect(),
        }
    }
}

/// One raw error annotation. `evidence`/`description` are human context the benchmark does not
/// read, so they are ignored; only the category, its span-located `location`, and impact are kept.
#[derive(Deserialize)]
struct RawError {
    #[serde(default)]
    category: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    impact: String,
}
