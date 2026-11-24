//! Adapter from the Agent Trajectory Interchange Format (ATIF — Harbor's `trajectory.json`, the
//! format Terminal-Bench trials are published in) into the canonical [`amberfork_model::Run`].
//!
//! Written against the ATIF RFC, versions 1.0–1.8. Unlike the Claude Code transcript this is a
//! documented, versioned schema, so the envelope is typed; every field the RFC marks optional is
//! optional here, and fields this adapter has no use for are ignored rather than rejected.
//!
//! An ATIF *step* is a whole agent turn: message, tool calls and their observation together. The
//! aligner's unit is finer, so one agent turn fans out — its message becomes an `llm` step and
//! each tool call its own `tool` step. That keeps a turn that made three calls comparable to one
//! that made two, and matches what [`crate::claudecode`] produces for the same behaviour.
//!
//! Mapping decisions:
//! - **An LLM step is named `assistant`, never the model.** Terminal-Bench pairs are the same task
//!   under different models as often as the same model twice; a model-named step would leave
//!   those pairs with no shared LLM vocabulary (notebook 070). The model goes in `attrs`.
//! - **A result is paired to its call by `source_call_id`,** never by position in `results`. A
//!   result with no call id (a non-tool action, a system event) becomes an `observation` step.
//! - **`reasoning_content` contributes nothing,** as with Claude Code `thinking` blocks.
//! - **`is_copied_context` steps are skipped.** The RFC defines them as prior turns re-inserted
//!   after a context compaction — not new behaviour, and counting them would duplicate steps.
//! - **Embedded `subagent_trajectories` are not inlined.** The run is the parent's linear
//!   trajectory; subagent trees are the DAG-alignment frontier (#59).
//! - **`outcome` is never set.** The verdict lives in the harness's result file, not here.
//! - **Every step carries the ATIF `step_id` it came from** in `attrs`, so a label expressed in
//!   ATIF step ids maps onto the fanned-out run.

use crate::{IngestError, Ingested};
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind, Warning, WarningCode};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

/// The `schema_version` prefix every ATIF document carries (`ATIF-v1.8`).
const VERSION_PREFIX: &str = "ATIF-";
/// The version line this adapter was written against.
const SUPPORTED_LINE: &str = "ATIF-v1.";

/// Whether `text` is an ATIF document: a JSON object whose `schema_version` starts with `ATIF-`.
/// A canonical trace carries `schema_version` too, so the discriminator is the value, not the key.
#[must_use]
pub fn looks_like_trajectory(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|doc| {
        doc["schema_version"]
            .as_str()
            .is_some_and(|version| version.starts_with(VERSION_PREFIX))
    })
}

/// Parse an ATIF trajectory into one canonical run.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not JSON shaped like an ATIF trajectory (a
/// `steps` array of objects each with a `step_id` and `source`). A version outside the v1 line
/// is a warning, not an error.
pub fn from_json_str(s: &str) -> Result<Ingested, IngestError> {
    let raw: RawTrajectory =
        serde_json::from_str(s).map_err(|source| IngestError::Parse { path: None, source })?;

    let mut warnings = Vec::new();
    if !raw.schema_version.starts_with(SUPPORTED_LINE) {
        warnings.push(Warning {
            code: WarningCode::SchemaVersionMismatch,
            msg: format!(
                "trajectory declares schema_version {:?}; this build reads {SUPPORTED_LINE}x — \
                 fields may be read under the wrong contract",
                raw.schema_version
            ),
        });
    }

    let default_model = raw.agent.model_name;
    let mut steps: Vec<Step> = Vec::new();
    for turn in raw.steps {
        if turn.is_copied_context == Some(true) {
            continue;
        }
        let push = |steps: &mut Vec<Step>, kind: StepKind, name: &str| -> usize {
            steps.push(new_step(
                steps.len(),
                kind,
                name,
                turn.step_id,
                turn.timestamp.as_deref(),
            ));
            steps.len() - 1
        };

        let message = content_text(&turn.message);
        let mut results = turn.observation.map(|o| o.results).unwrap_or_default();

        if turn.source == "agent" {
            if !message.trim().is_empty() {
                let idx = push(&mut steps, StepKind::Llm, "assistant");
                steps[idx].outputs = Some(Payload::Text(message));
                if let Some(model) = turn.model_name.as_ref().or(default_model.as_ref()) {
                    steps[idx]
                        .attrs
                        .insert("model".to_owned(), model.as_str().into());
                }
            }
            for call in turn.tool_calls {
                let idx = push(&mut steps, StepKind::Tool, &call.function_name);
                steps[idx].inputs = Some(match call.arguments {
                    Value::Object(fields) => Payload::Object(fields),
                    Value::Null => Payload::Object(Map::new()),
                    other => Payload::Other(other),
                });
                let answered = results
                    .iter()
                    .position(|r| r.source_call_id.as_deref() == Some(call.tool_call_id.as_str()));
                if let Some(at) = answered {
                    let result = results.remove(at);
                    steps[idx].outputs = Some(Payload::Text(content_text(&result.content)));
                }
            }
        } else if !message.trim().is_empty() {
            let name = if turn.source == "system" {
                "system"
            } else {
                "user"
            };
            let idx = push(&mut steps, StepKind::Other, name);
            steps[idx].outputs = Some(Payload::Text(message));
        }

        // Whatever no tool call claimed: non-tool actions and system events.
        for result in results {
            let text = content_text(&result.content);
            if !text.trim().is_empty() {
                let idx = push(&mut steps, StepKind::Other, "observation");
                steps[idx].outputs = Some(Payload::Text(text));
            }
        }
    }

    let id = raw
        .session_id
        .or(raw.trajectory_id)
        .unwrap_or_else(|| "trajectory".to_owned());
    let run = Run {
        schema_version: SchemaVersion::current(),
        id: format!("atif_{id}"),
        task: None,
        outcome: None,
        steps,
        edges: None,
    };
    Ok(Ingested { run, warnings })
}

/// Load an ATIF trajectory from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] (with the
/// path attached) if its contents are not an ATIF trajectory.
pub fn load_file(path: impl AsRef<Path>) -> Result<Ingested, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_json_str(&text).map_err(|err| err.with_path(path))
}

fn new_step(idx: usize, kind: StepKind, name: &str, step_id: u64, timestamp: Option<&str>) -> Step {
    let mut attrs = Map::new();
    attrs.insert("step_id".to_owned(), step_id.into());
    Step {
        idx,
        kind,
        name: name.to_owned(),
        inputs: None,
        outputs: None,
        attrs,
        t_start: timestamp.map(str::to_owned),
        t_end: None,
        parent_idx: None,
    }
}

/// A `message` or result `content` as text: a string as-is, a `ContentPart` array as its `text`
/// parts joined by newlines (image and audio parts carry nothing the lexical model can read).
fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[derive(Deserialize)]
struct RawTrajectory {
    schema_version: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    trajectory_id: Option<String>,
    #[serde(default)]
    agent: RawAgent,
    steps: Vec<RawStep>,
}

#[derive(Deserialize, Default)]
struct RawAgent {
    #[serde(default)]
    model_name: Option<String>,
}

#[derive(Deserialize)]
struct RawStep {
    step_id: u64,
    source: String,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    message: Value,
    #[serde(default)]
    tool_calls: Vec<RawToolCall>,
    #[serde(default)]
    observation: Option<RawObservation>,
    #[serde(default)]
    is_copied_context: Option<bool>,
}

#[derive(Deserialize)]
struct RawToolCall {
    #[serde(default)]
    tool_call_id: String,
    function_name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Deserialize)]
struct RawObservation {
    #[serde(default)]
    results: Vec<RawResult>,
}

#[derive(Deserialize)]
struct RawResult {
    #[serde(default)]
    source_call_id: Option<String>,
    #[serde(default)]
    content: Value,
}
