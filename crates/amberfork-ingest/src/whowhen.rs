//! Adapter from Who&When failure logs into the canonical [`amberfork_model::Run`] plus its gold
//! label. Ported from `spike/convert_whowhen.py` (Who&When = `ag2ai/Agents_Failure_Attribution`,
//! MIT).
//!
//! This is a *source adapter*, deliberately kept apart from the canonical loader
//! ([`crate::from_json_str`]): Who&When's raw shape (a `history` array of chat turns plus dataset
//! annotations) is nothing like the canonical trace, so it gets its own namespace rather than
//! bending the forgiving loader to fit.
//!
//! Two boundaries the port makes explicit:
//! - **Gold lives beside the run, never inside it.** The dataset's blame annotation
//!   (`mistake_step`/`mistake_agent`) and reference answer (`ground_truth`) are what a benchmark
//!   scores localization *against* — they are returned in [`WhoWhenGold`], never smuggled into
//!   the trace, because a run's `outcome` is a user verdict, not a dataset label. (`outcome` is
//!   nonetheless [`Outcome::Fail`] here: every Who&When log is a failure by construction.)
//! - **The split is a label, not a parse switch.** Hand-Crafted logs key the speaker on `role`,
//!   Algorithm-Generated logs on `name`; reading `name`-then-`role` handles both uniformly, so
//!   [`Split`] only tags provenance and the generated id — it does not fork the conversion.
//!
//! Empirical quirks handled (spike, verified 2026-07-07): `mistake_step` is a **string-encoded
//! 0-indexed** int; the reference answer key drifts between `ground_truth` and `groundtruth`; the
//! agent name `Websurfer` drifts from the canonical `WebSurfer`.

use crate::IngestError;
use amberfork_model::{Outcome, Payload, Run, SchemaVersion, Step, StepKind};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

/// Which Who&When annotation split a log came from. Purely a provenance label: the converter
/// reads `name`-then-`role` regardless, so the split never changes how a log is parsed — only its
/// generated id and the [`WhoWhenGold::split`] tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    HandCrafted,
    AlgorithmGenerated,
}

impl Split {
    /// Short slug used in generated run ids (`whowhen_hand_4`, `whowhen_algo_57`).
    const fn slug(self) -> &'static str {
        match self {
            Self::HandCrafted => "hand",
            Self::AlgorithmGenerated => "algo",
        }
    }
}

/// The dataset's blame index, resolved against the trajectory it points into. A three-state type
/// rather than a bare `Option<usize>` so "unusable annotation" is a first-class, honest value the
/// benchmark's gold-sanity report can act on — and so an out-of-range or negative index can never
/// masquerade as a valid target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoldStep {
    /// A usable 0-based target index, in range for the run.
    Valid(usize),
    /// No `mistake_step` was annotated on the log.
    Absent,
    /// A `mistake_step` was present but is not a usable target — non-numeric, negative, or past
    /// the end of the trajectory. The raw annotation is preserved for the sanity report.
    Unusable(String),
}

/// The gold annotation shipped alongside a Who&When failure log: which step (and agent) the
/// dataset blames, plus the reference answer. Kept beside the [`Run`], never merged into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoWhenGold {
    /// Which annotation split this log came from.
    pub split: Split,
    /// The dataset's question hash (`question_ID` on the wire).
    pub question_id: String,
    /// The blamed step, resolved against the trajectory length.
    pub gold_step: GoldStep,
    /// The blamed agent, casing-normalized; `None` when unannotated.
    pub mistake_agent: Option<String>,
    /// The reference answer; empty when the log recorded none.
    pub ground_truth: String,
}

/// A converted Who&When log: the canonical trajectory and its gold label.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub run: Run,
    pub gold: WhoWhenGold,
}

/// Convert a single Who&When log from a JSON string. `stem` is the source identifier (typically
/// the file name without extension) folded into the run id as `whowhen_<split>_<stem>`.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid JSON for a Who&When log. Everything
/// past a successful parse is forgiving: a missing or malformed annotation yields [`GoldStep`]
/// variants, never an error.
pub fn convert_str(raw_json: &str, split: Split, stem: &str) -> Result<Converted, IngestError> {
    let raw: RawLog = serde_json::from_str(raw_json).map_err(IngestError::Parse)?;

    let steps: Vec<Step> = raw
        .history
        .iter()
        .enumerate()
        .map(|(idx, entry)| entry.to_step(idx))
        .collect();

    let gold = WhoWhenGold {
        split,
        question_id: raw.question_id,
        gold_step: resolve_gold_step(raw.mistake_step.as_ref(), steps.len()),
        mistake_agent: nonempty(raw.mistake_agent.as_deref()).map(normalize_agent_name),
        ground_truth: raw.ground_truth.or(raw.groundtruth).unwrap_or_default(),
    };

    let run = Run {
        schema_version: SchemaVersion::current(),
        id: format!("whowhen_{}_{stem}", split.slug()),
        task: (!raw.question.is_empty()).then_some(raw.question),
        outcome: Some(Outcome::Fail),
        steps,
        edges: None,
    };

    Ok(Converted { run, gold })
}

/// Convert a single Who&When log from a file on disk. The run id's `stem` is taken from the file
/// name without its extension.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] if its
/// contents are not valid Who&When JSON.
pub fn convert_file(path: impl AsRef<Path>, split: Split) -> Result<Converted, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    convert_str(&text, split, stem)
}

/// Raw Who&When log. Dataset-specific annotation fields are captured here and lifted into
/// [`WhoWhenGold`]; unknown fields are ignored (serde's default) since none map onto a step.
#[derive(Deserialize)]
struct RawLog {
    #[serde(default)]
    question: String,
    #[serde(default, rename = "question_ID")]
    question_id: String,
    #[serde(default)]
    history: Vec<RawEntry>,
    #[serde(default)]
    mistake_agent: Option<String>,
    /// String-encoded 0-indexed int on the wire, but kept as a raw [`Value`] so a stray number
    /// or null is tolerated rather than failing the parse.
    #[serde(default)]
    mistake_step: Option<Value>,
    #[serde(default)]
    ground_truth: Option<String>,
    /// Alternate spelling of `ground_truth` seen on the Hand-Crafted split.
    #[serde(default)]
    groundtruth: Option<String>,
}

/// One chat turn in a Who&When `history`. `name` (Algorithm-Generated) takes priority over `role`
/// (Hand-Crafted) as the speaker.
#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

impl RawEntry {
    fn to_step(&self, idx: usize) -> Step {
        let speaker = nonempty(self.name.as_deref())
            .or_else(|| nonempty(self.role.as_deref()))
            .unwrap_or("unknown");
        Step {
            idx,
            kind: StepKind::Agent,
            name: normalize_agent_name(speaker),
            inputs: None,
            outputs: nonempty(self.content.as_deref()).map(|c| Payload::Text(c.to_string())),
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        }
    }
}

/// Resolve the string-encoded `mistake_step` against the trajectory length. Accepts a JSON string
/// (the dataset's form) or a bare number; anything non-numeric, negative, or `>= n_steps` is
/// [`GoldStep::Unusable`] with the raw value preserved.
fn resolve_gold_step(raw: Option<&Value>, n_steps: usize) -> GoldStep {
    let value = match raw {
        None | Some(Value::Null) => return GoldStep::Absent,
        Some(value) => value,
    };
    let parsed = match value {
        // `usize::parse` rejects negatives, so a negative index cannot pass as valid.
        Value::String(s) => s.trim().parse::<usize>().ok(),
        Value::Number(n) => n.as_u64().and_then(|u| usize::try_from(u).ok()),
        _ => None,
    };
    match parsed {
        Some(idx) if idx < n_steps => GoldStep::Valid(idx),
        _ => GoldStep::Unusable(display_raw(value)),
    }
}

/// The raw annotation as it appeared, for the unusable-gold report (a JSON string keeps its inner
/// text; anything else is rendered as JSON).
fn display_raw(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Canonicalize a known agent-name casing drift. One documented drift for now (`Websurfer` →
/// `WebSurfer`, spike 2026-07-07); extend here if more surface.
fn normalize_agent_name(name: &str) -> String {
    name.replace("Websurfer", "WebSurfer")
}

/// `Some` only for a present, non-empty string — mirrors the Python reference's truthiness, where
/// an empty `name`/`content` is treated as absent.
fn nonempty(s: Option<&str>) -> Option<&str> {
    s.filter(|t| !t.is_empty())
}
