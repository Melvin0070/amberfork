//! Adapter from ServiceNow TapeAgents tapes into the canonical [`amberfork_model::Run`] plus the
//! metadata needed to pair one. Ported from `spike/make_realpairs.py`'s `convert_tape`
//! (TapeAgents = ServiceNow/TapeAgents, Apache-2.0).
//!
//! A tape is a successful *reference* trajectory: an ordered `steps` array of typed nodes plus a
//! top-level `metadata` block naming the GAIA task and the answer the tape produced. Like
//! [`crate::whowhen`], this is a *source adapter* kept apart from the canonical loader
//! ([`crate::from_json_str`]) — a tape's node shape is nothing like the canonical trace, so it
//! gets its own namespace rather than bending the forgiving loader to fit.
//!
//! Three boundaries the port makes explicit:
//! - **Gold lives beside the run, never inside it.** The GAIA `task_id` (the key a Mode A′ pair is
//!   matched on) and the `Final answer` are returned in [`TapeMeta`], not smuggled into the trace.
//! - **The node body survives as structured data.** Each node's remaining fields (everything past
//!   its `kind` and `metadata`) become a [`Payload::Object`], which the diff engine field-diffs —
//!   not a stringified blob. The spike serialized the body with `json.dumps` only because Python
//!   had no typed payload; the canonical model does, so we use it.
//! - **`outcome` is decided honestly, not asserted.** A tape earns [`Outcome::Pass`] only when the
//!   `result` it produced matches the gold `Final answer` (trimmed, case-folded — GAIA's grading);
//!   otherwise it is [`Outcome::Fail`]. The spike hardcoded `pass` on every tape and filtered
//!   later; here the run itself never claims a success it did not achieve, and the pairing filter
//!   reads [`TapeMeta::is_success`].

use crate::IngestError;
use amberfork_model::{Outcome, Payload, Run, SchemaVersion, Step, StepKind};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

/// The pairing metadata shipped alongside a converted tape: which GAIA task it answers, the gold
/// answer, and the answer the tape actually produced. Kept beside the [`Run`], never merged into
/// it — a reference run's identity is its trajectory, not the benchmark bookkeeping around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeMeta {
    /// The GAIA task id (`metadata.task.task_id`) — the key a Mode A′ pair is matched on. `None`
    /// when the tape carries no structured task block.
    pub task_id: Option<String>,
    /// The gold answer (`metadata.task["Final answer"]`); empty when the tape recorded none.
    pub final_answer: String,
    /// The answer the tape produced (`metadata.result`); empty when the tape recorded none.
    pub result: String,
}

impl TapeMeta {
    /// Whether the tape actually solved its task: its produced `result` matches the gold
    /// `Final answer` after trimming and case-folding (GAIA's grading, the spike's
    /// `.strip().lower()`). Only a successful tape may serve as a Mode A′ reference.
    #[must_use]
    pub fn is_success(&self) -> bool {
        normalize(&self.result) == normalize(&self.final_answer)
    }
}

/// A converted tape: the canonical reference trajectory and its pairing metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertedTape {
    pub run: Run,
    pub meta: TapeMeta,
}

/// Convert a single TapeAgents tape from a JSON string. `stem` is the source identifier (typically
/// the file name without extension) folded into the run id as `tape_<stem>`.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid JSON for a tape. Everything past a
/// successful parse is forgiving: a missing task block, result, or node field degrades to an empty
/// value, never an error.
pub fn convert_str(raw_json: &str, stem: &str) -> Result<ConvertedTape, IngestError> {
    let raw: RawTape = serde_json::from_str(raw_json).map_err(IngestError::Parse)?;

    let steps: Vec<Step> = raw
        .steps
        .into_iter()
        .enumerate()
        .map(|(idx, node)| node.into_step(idx))
        .collect();

    let task = raw.metadata.task.unwrap_or_default();
    let meta = TapeMeta {
        task_id: nonempty(task.task_id),
        final_answer: task.final_answer,
        result: raw.metadata.result.map(stringify).unwrap_or_default(),
    };

    let outcome = if meta.is_success() {
        Outcome::Pass
    } else {
        Outcome::Fail
    };
    let run = Run {
        schema_version: SchemaVersion::current(),
        id: format!("tape_{stem}"),
        task: nonempty(task.question),
        outcome: Some(outcome),
        steps,
        edges: None,
    };

    Ok(ConvertedTape { run, meta })
}

/// Convert a single TapeAgents tape from a file on disk. The run id's `stem` is taken from the file
/// name without its extension.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] if its
/// contents are not valid tape JSON.
pub fn convert_file(path: impl AsRef<Path>) -> Result<ConvertedTape, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    convert_str(&text, stem)
}

/// A raw tape. Only the two fields the adapter reads are named; anything else is ignored.
#[derive(Deserialize)]
struct RawTape {
    #[serde(default)]
    metadata: RawTapeMetadata,
    #[serde(default)]
    steps: Vec<RawNode>,
}

/// The tape's top-level `metadata`. `task` is optional and only honored when it is a JSON object —
/// a tape carrying a non-object `task` (seen in the wild) degrades to no task rather than failing
/// the parse, mirroring the spike's `isinstance(..., dict)` guard.
#[derive(Deserialize, Default)]
struct RawTapeMetadata {
    #[serde(default, deserialize_with = "object_or_none")]
    task: Option<RawTask>,
    /// The answer the tape produced. Kept as a raw [`Value`] so a stray number or bool is
    /// stringified rather than failing the parse.
    #[serde(default)]
    result: Option<Value>,
}

/// The GAIA task block inside a tape's metadata.
#[derive(Deserialize, Default)]
struct RawTask {
    #[serde(default, rename = "Question")]
    question: String,
    #[serde(default)]
    task_id: String,
    #[serde(default, rename = "Final answer")]
    final_answer: String,
}

/// One node in a tape's `steps`. Its `kind` and `metadata` are lifted into the step's identity;
/// every other field is captured by `#[serde(flatten)]` as the node body.
#[derive(Deserialize)]
struct RawNode {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    metadata: Option<RawNodeMetadata>,
    /// Everything past `kind`/`metadata` — the node's semantic payload.
    #[serde(flatten)]
    body: Map<String, Value>,
}

/// A tape node's `metadata`. Only the acting `agent` is read.
#[derive(Deserialize)]
struct RawNodeMetadata {
    #[serde(default)]
    agent: Option<String>,
}

impl RawNode {
    fn into_step(self, idx: usize) -> Step {
        let kind = nonempty(self.kind.unwrap_or_default()).unwrap_or_else(|| "step".to_string());
        let agent = self.metadata.and_then(|m| m.agent).and_then(nonempty);
        // Name carries the node's own kind, prefixed by the acting agent when one is annotated —
        // this is the structural identity the aligner keys on, so the tape's node vocabulary is
        // preserved here rather than flattened away.
        let name = match agent {
            Some(agent) => format!("{agent}:{kind}"),
            None => kind,
        };
        // Empty body → no outputs at all, so a bookkeeping-only node (kind + metadata, nothing
        // else) does not masquerade as content.
        let outputs = (!self.body.is_empty()).then_some(Payload::Object(self.body));
        Step {
            idx,
            kind: StepKind::Agent,
            name,
            inputs: None,
            outputs,
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        }
    }
}

/// GAIA-style answer normalization for the success check: trim surrounding whitespace and
/// case-fold, matching the spike's `.strip().lower()`.
fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Render a raw JSON value as a plain string for the produced-result comparison: a JSON string
/// keeps its inner text; anything else is rendered as JSON.
fn stringify(value: Value) -> String {
    match value {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

/// `Some` only for a non-empty string — an empty task_id/question/agent is treated as absent,
/// mirroring the Python reference's truthiness.
fn nonempty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

/// Deserialize a field as `Some` only when it is a JSON object, else `None`. Lets a non-object
/// `task` (a bare string, say) degrade gracefully instead of failing the whole parse.
fn object_or_none<'de, D>(deserializer: D) -> Result<Option<RawTask>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Object(_) => RawTask::deserialize(value)
            .map(Some)
            .map_err(serde::de::Error::custom),
        _ => Ok(None),
    }
}
