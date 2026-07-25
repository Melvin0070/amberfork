//! Adapter from a HAL `hal_traces` config dump into canonical [`amberfork_model::Run`]s — the
//! reference side of the natural-pair benchmark (issue #41 S4b).
//!
//! HAL (`hal.cs.princeton.edu`, Princeton) publishes agent-eval traces as one encrypted zip per
//! *agent config* on Hugging Face (`agent-evals/hal_traces`); decrypted, each is a single JSON blob
//! holding **every** GAIA task that config ran (notebook 047). Its `raw_logging_results` is a flat,
//! W&B-Weave-instrumented turn stream — not a span tree — with two structural facts this adapter
//! turns on:
//!
//! - **The turn stream is double-logged 1:1.** Every model call appears twice: a `litellm.completion`
//!   wrapper and the `openai.chat.completions.create` leaf it delegates to (verified: exactly paired
//!   across all tasks, notebook 047). The leaf is content-complete (`inputs.messages` in,
//!   `output.choices` out); the wrapper is redundant. So the adapter keeps only the openai leaves as
//!   turns and drops the wrappers — a task's step count is its turn count, the granularity a fork
//!   between two same-agent runs actually lives at.
//! - **Grouping is by GAIA task, ordering is by wall-clock.** Each record carries its GAIA
//!   `task_id` under `attributes.weave_task_id` (the same UUID namespace TRAIL's S4a join key lifts),
//!   so one blob fans out into one [`Run`] per task. A Weave record's `started_at` is RFC3339, so
//!   the turns of a task sort into execution order by string comparison — no dependence on the
//!   (arbitrary) array position. Turns form a linear chain, so `parent_idx` stays `None`.
//!
//! Three boundaries the adapter makes explicit, each mirroring an existing rule:
//! - **Only semantic content crosses into a step.** `inputs` keeps `{model, messages}` and `outputs`
//!   `{choices, usage}`; the request's client/transport internals (`self`, `extra_headers`,
//!   `extra_body`, timeouts) and the response's transport ids are dropped. This is fidelity — a
//!   trajectory is the conversation, not the SDK plumbing — and a safety guard: an `extra_headers`
//!   auth token can never ride into a committed `Run` (issue #43). Both land as field-diffable
//!   [`Payload::Object`]s.
//! - **`outcome` is never asserted on the trajectory.** HAL grades each task pass/fail in
//!   `results.successful_tasks`, but a run's verdict is a user assertion, not a trace fact
//!   (`docs/trace-format.md`), so [`Run::outcome`] stays `None` and the pass/fail rides beside the
//!   run in [`HalMeta::passed`] — exactly as the `trail`/`tape` adapters keep their bookkeeping
//!   beside the run, never inside it.
//! - **Missing pieces are data, not failures.** A record with no `weave_task_id` cannot be placed
//!   and is skipped; a turn with neither input nor output content degrades to a metadata-only step
//!   plus a [`WarningCode::ContentAbsent`] advisory. Past a successful parse nothing errors.

use crate::IngestError;
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind, Warning, WarningCode};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::Path;

/// The Weave `trace_name` of the content-bearing model call — the leaf the adapter keeps as a turn.
/// Its `litellm.completion` wrapper (the 1:1 outer log) is dropped.
const OPENAI_LEAF: &str = "openai.chat.completions.create";

/// The request fields that are trajectory content; everything else on `inputs` (the serialized
/// client `self`, `extra_headers`/`extra_body`, timeouts) is SDK plumbing and never crosses over.
const INPUT_KEYS: &[&str] = &["model", "messages"];

/// The response fields that are trajectory content: the completion (`choices`) and its cost
/// (`usage`). Transport ids (`id`, `system_fingerprint`, `service_tier`, …) are dropped.
const OUTPUT_KEYS: &[&str] = &["choices", "usage"];

/// The pairing metadata shipped alongside a converted HAL reference run: which GAIA task it answers,
/// which model produced it, and whether HAL graded it a pass. Kept beside the [`Run`], never merged
/// into it — a run's identity is its trajectory, not the benchmark bookkeeping — exactly as the
/// `trail` adapter's [`crate::trail::TrailMeta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HalMeta {
    /// The canonical GAIA `task_id` (a UUID) the run answers — the key a TRAIL failing trace is
    /// paired against (issue #41). Taken from the records' `attributes.weave_task_id`.
    pub gaia_task_id: String,
    /// The backing model of the agent config, from `config.agent_args.model_name`. `None` when the
    /// dump names none. The reference is a *same-agent, different-model* run of the task, so this is
    /// the cross-model caveat the scoring layer reports against (notebook 047).
    pub model: Option<String>,
    /// Whether HAL graded this task a pass (`results.successful_tasks`). The pairing layer keeps only
    /// passing runs as references; a failing one is retained here as data, its verdict beside the run
    /// rather than asserted on it.
    pub passed: bool,
}

/// A converted HAL reference run: the canonical trajectory for one GAIA task, its pairing metadata,
/// and the warnings normalization raised. The `trail`-style companion for the pairing layer that
/// needs the join key and verdict beside the run.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertedHal {
    pub run: Run,
    pub meta: HalMeta,
    pub warnings: Vec<Warning>,
}

/// Convert one decrypted HAL config dump into a canonical [`Run`] per GAIA task it ran.
///
/// Returns one [`ConvertedHal`] per task that has at least one content turn, sorted by GAIA task id
/// for deterministic output. A dump with no `openai.chat.completions.create` records yields an empty
/// vector — data, not an error.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid HAL dump JSON. Everything past a
/// successful parse is forgiving: an absent field, a placeless record, or a content-free turn yields
/// a canonical fallback (plus a warning where a step is affected), never an error.
pub fn convert_str(s: &str) -> Result<Vec<ConvertedHal>, IngestError> {
    let dump: RawHalDump =
        serde_json::from_str(s).map_err(|source| IngestError::Parse { path: None, source })?;
    Ok(convert_dump(dump))
}

/// Convert a decrypted HAL config dump from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] (with the path
/// attached) if its contents are not valid HAL dump JSON.
pub fn convert_file(path: impl AsRef<Path>) -> Result<Vec<ConvertedHal>, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    convert_str(&text).map_err(|err| err.with_path(path))
}

/// Group the openai-leaf turns by GAIA task, order each task's turns by wall-clock, and normalize
/// each into a [`Run`]. Pure and infallible — the parse already succeeded.
fn convert_dump(dump: RawHalDump) -> Vec<ConvertedHal> {
    let passed: HashSet<&str> = dump
        .results
        .successful_tasks
        .iter()
        .map(String::as_str)
        .collect();
    let model = dump.config.agent_args.model_name;
    let run_prefix = dump.config.run_id;

    // Keep only the content-bearing openai leaves, grouped by task. A BTreeMap gives deterministic
    // (task-id-sorted) output; a placeless record (no task id) cannot be paired and is skipped.
    let mut by_task: BTreeMap<String, Vec<RawHalRecord>> = BTreeMap::new();
    for record in dump.raw_logging_results {
        if record.summary.weave.trace_name.as_deref() != Some(OPENAI_LEAF) {
            continue;
        }
        let Some(task_id) = record.attributes.weave_task_id.clone() else {
            continue;
        };
        by_task.entry(task_id).or_default().push(record);
    }

    by_task
        .into_iter()
        .map(|(task_id, mut records)| {
            // RFC3339 timestamps sort lexically into execution order; array position is arbitrary.
            records.sort_by(|a, b| a.started_at.cmp(&b.started_at));
            let mut warnings = Vec::new();
            let steps: Vec<Step> = records
                .into_iter()
                .enumerate()
                .map(|(idx, record)| record.into_step(idx, &mut warnings))
                .collect();
            let id = if run_prefix.is_empty() {
                task_id.clone()
            } else {
                format!("{run_prefix}/{task_id}")
            };
            let passed = passed.contains(task_id.as_str());
            ConvertedHal {
                run: Run {
                    schema_version: SchemaVersion::current(),
                    id,
                    task: None,
                    outcome: None,
                    steps,
                    edges: None,
                },
                meta: HalMeta {
                    gaia_task_id: task_id,
                    model: model.clone(),
                    passed,
                },
                warnings,
            }
        })
        .collect()
}

/// Copy just the trajectory-content fields (`keys`) out of a request/response object into a
/// field-diffable [`Payload::Object`]. `None` when the object is absent or carries none of them —
/// the signal a turn has no content of that side.
fn project(value: &Value, keys: &[&str]) -> Option<Payload> {
    let object = value.as_object()?;
    let mut kept = Map::new();
    for key in keys {
        if let Some(found) = object.get(*key) {
            kept.insert((*key).to_string(), found.clone());
        }
    }
    (!kept.is_empty()).then_some(Payload::Object(kept))
}

// --- HAL/Weave wire types -------------------------------------------------------------------

/// A HAL config dump. Every field defaults so a partial or unfamiliar dump degrades to fewer runs
/// rather than a parse failure. Only the fields the adapter reads are named; `raw_eval_results`,
/// `total_usage`, `git_info`, and the rest are ignored by serde.
#[derive(Deserialize, Default)]
struct RawHalDump {
    #[serde(default)]
    config: RawHalConfig,
    #[serde(default)]
    results: RawHalResults,
    #[serde(default)]
    raw_logging_results: Vec<RawHalRecord>,
}

#[derive(Deserialize, Default)]
struct RawHalConfig {
    /// Identifies the agent config; prefixes each run id so references from different configs never
    /// collide once combined by the pairing layer.
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    agent_args: RawHalAgentArgs,
}

#[derive(Deserialize, Default)]
struct RawHalAgentArgs {
    #[serde(default)]
    model_name: Option<String>,
}

/// HAL's per-task grading. Only the pass list is read (a task is a pass iff it appears here);
/// `failed_tasks` is parsed for symmetry but the verdict is a simple membership test.
#[derive(Deserialize, Default)]
struct RawHalResults {
    #[serde(default)]
    successful_tasks: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    failed_tasks: Vec<String>,
}

/// One Weave call record. `inputs`/`output` are kept as raw [`Value`]s (they may be absent or `{}`)
/// and projected to content in [`RawHalRecord::into_step`]; the wrapper `litellm.completion` records
/// are filtered out before this type is used to build a step.
#[derive(Deserialize)]
struct RawHalRecord {
    #[serde(default)]
    id: String,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    ended_at: Option<String>,
    #[serde(default)]
    attributes: RawHalAttrs,
    #[serde(default)]
    summary: RawHalSummary,
    #[serde(default)]
    inputs: Value,
    #[serde(default)]
    output: Value,
}

impl RawHalRecord {
    /// Normalize one openai-leaf turn into a canonical [`Step`]: an `LLM` step named by its Weave
    /// `trace_name`, with content projected from `inputs`/`output` and the source Weave ids retained
    /// as provenance. A turn with no content on either side raises a content-absent advisory.
    fn into_step(self, idx: usize, warnings: &mut Vec<Warning>) -> Step {
        let name = self
            .summary
            .weave
            .trace_name
            .unwrap_or_else(|| OPENAI_LEAF.to_string());
        let inputs = project(&self.inputs, INPUT_KEYS);
        let outputs = project(&self.output, OUTPUT_KEYS);

        // The source Weave call id (and its trace id) are retained so a later slice can trace a step
        // back to the record it came from — the `otel.span_id` provenance discipline of `trail`.
        let mut attrs = Map::new();
        if !self.id.is_empty() {
            attrs.insert("weave.call_id".to_string(), Value::from(self.id));
        }
        if let Some(trace_id) = self.trace_id.filter(|t| !t.is_empty()) {
            attrs.insert("weave.trace_id".to_string(), Value::from(trace_id));
        }

        if inputs.is_none() && outputs.is_none() {
            warnings.push(Warning {
                code: WarningCode::ContentAbsent,
                msg: format!("step {idx} ({name}): no input or output content captured"),
            });
        }

        Step {
            idx,
            kind: StepKind::Llm,
            name,
            inputs,
            outputs,
            attrs,
            t_start: self.started_at,
            t_end: self.ended_at,
            parent_idx: None,
        }
    }
}

#[derive(Deserialize, Default)]
struct RawHalAttrs {
    #[serde(default)]
    weave_task_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawHalSummary {
    #[serde(default)]
    weave: RawHalWeave,
}

#[derive(Deserialize, Default)]
struct RawHalWeave {
    #[serde(default)]
    trace_name: Option<String>,
}
