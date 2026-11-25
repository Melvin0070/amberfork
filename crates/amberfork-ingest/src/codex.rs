//! Adapter from Codex CLI rollouts (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`) into the
//! canonical [`amberfork_model::Run`].
//!
//! Like a Claude Code transcript, a rollout needs no export step: it is already on the
//! developer's disk. It is JSON-Lines, each record `{timestamp, type, payload}`, and it
//! interleaves two views of one session. `response_item` records are the model-facing
//! transcript — what was sent to and returned by the model. `event_msg` records are a UI event
//! stream that repeats much of the same content. Only the first is trajectory; reading both
//! would double every assistant message.
//!
//! Mapping decisions, the same ones [`crate::claudecode`] and [`crate::atif`] make:
//! - **An LLM step is named `assistant`, never the model id** (the model, from the latest
//!   `turn_context`, rides in `attrs`), so a before/after-model-bump pair still aligns.
//! - **A tool call and its output are one step.** `function_call` and `custom_tool_call` open
//!   it; the `*_output` record with the same `call_id` is paired back, in whatever order the
//!   two arrive. An output whose call is not in the file is dropped.
//! - **`reasoning` contributes nothing.** Its text is encrypted on disk.
//! - **`outcome` is never set.** A rollout does not know whether the task succeeded.
//!
//! Limits, stated because the format is undocumented and this was built against rollouts written
//! by codex-cli 0.171:
//! - Records are read as loose [`Value`]s; an unknown record or payload type is skipped, never an
//!   error. A line that fails to parse *is* an error — dropping it would silently shorten the run
//!   and move the fork.
//! - A subagent runs in its own rollout file. The messages it exchanges with its parent appear
//!   here as `agent_message` steps; its own trajectory does not.
//! - After a context compaction, the replacement history lives inside the `compacted` record and
//!   is not expanded.
//! - Compressed rollouts (`.zst`) are not read; decompress first.

use crate::{IngestError, Ingested};
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

/// Whether `text` is a Codex rollout: its first non-empty line is a `session_meta` record with an
/// object payload, which is how every rollout opens. Checked before the generic JSON-Lines
/// classification so a rollout routes here and any other JSONL keeps its own error.
#[must_use]
pub fn looks_like_rollout(text: &str) -> bool {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .is_some_and(|record| record["type"] == "session_meta" && record["payload"].is_object())
}

/// Parse a rollout into one canonical run.
///
/// # Errors
/// Returns [`IngestError::Parse`] if any non-empty line is not valid JSON. Everything past a
/// successful parse is forgiving — see the module docs.
pub fn from_jsonl_str(s: &str) -> Result<Ingested, IngestError> {
    let mut session_id: Option<String> = None;
    let mut model: Option<String> = None;
    let mut steps: Vec<Step> = Vec::new();
    let mut awaiting_output: HashMap<String, usize> = HashMap::new();

    for line in s.lines().filter(|line| !line.trim().is_empty()) {
        let record: Value = serde_json::from_str(line)
            .map_err(|source| IngestError::Parse { path: None, source })?;
        let payload = &record["payload"];
        let timestamp = record["timestamp"].as_str();
        match record["type"].as_str() {
            // A resumed or forked session writes a second `session_meta`; the first names the run.
            Some("session_meta") if session_id.is_none() => {
                session_id = payload["id"].as_str().map(str::to_owned);
            }
            Some("turn_context") => {
                if let Some(name) = payload["model"].as_str() {
                    model = Some(name.to_owned());
                }
            }
            Some("response_item") => match payload["type"].as_str() {
                Some("message") => {
                    let Some(text) = text_parts(&payload["content"]) else {
                        continue;
                    };
                    let mut step = match payload["role"].as_str() {
                        Some("assistant") => {
                            let mut step =
                                new_step(steps.len(), StepKind::Llm, "assistant", timestamp);
                            if let Some(model) = &model {
                                step.attrs.insert("model".to_owned(), model.as_str().into());
                            }
                            if let Some(phase) = payload["phase"].as_str() {
                                step.attrs.insert("phase".to_owned(), phase.into());
                            }
                            step
                        }
                        Some(role) => new_step(steps.len(), StepKind::Other, role, timestamp),
                        None => continue,
                    };
                    step.outputs = Some(Payload::Text(text));
                    steps.push(step);
                }
                Some(kind @ ("function_call" | "custom_tool_call")) => {
                    let name = payload["name"].as_str().unwrap_or("tool");
                    let mut step = new_step(steps.len(), StepKind::Tool, name, timestamp);
                    step.inputs = Some(if kind == "function_call" {
                        arguments(&payload["arguments"])
                    } else {
                        loose(&payload["input"])
                    });
                    if let Some(namespace) = payload["namespace"].as_str() {
                        step.attrs.insert("namespace".to_owned(), namespace.into());
                    }
                    if let Some(id) = payload["call_id"].as_str() {
                        awaiting_output.insert(id.to_owned(), steps.len());
                    }
                    steps.push(step);
                }
                Some("function_call_output" | "custom_tool_call_output") => {
                    let pending = payload["call_id"]
                        .as_str()
                        .and_then(|id| awaiting_output.remove(id));
                    if let Some(idx) = pending {
                        steps[idx].outputs = Some(Payload::Text(output_text(&payload["output"])));
                    }
                }
                Some("agent_message") => {
                    let Some(text) = text_parts(&payload["content"]) else {
                        continue;
                    };
                    let mut step =
                        new_step(steps.len(), StepKind::Other, "agent_message", timestamp);
                    step.outputs = Some(Payload::Text(text));
                    for key in ["author", "recipient"] {
                        if let Some(value) = payload[key].as_str() {
                            step.attrs.insert(key.to_owned(), value.into());
                        }
                    }
                    steps.push(step);
                }
                _ => {}
            },
            _ => {}
        }
    }

    let run = Run {
        schema_version: SchemaVersion::current(),
        id: format!("codex_{}", session_id.as_deref().unwrap_or("session")),
        task: None,
        outcome: None,
        steps,
        edges: None,
    };
    Ok(Ingested {
        run,
        warnings: Vec::new(),
    })
}

/// Load a rollout from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] (with the
/// path attached) if a line is not valid JSON.
pub fn load_file(path: impl AsRef<Path>) -> Result<Ingested, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_jsonl_str(&text).map_err(|err| err.with_path(path))
}

fn new_step(idx: usize, kind: StepKind, name: &str, timestamp: Option<&str>) -> Step {
    Step {
        idx,
        kind,
        name: name.to_owned(),
        inputs: None,
        outputs: None,
        attrs: Map::new(),
        t_start: timestamp.map(str::to_owned),
        t_end: None,
        parent_idx: None,
    }
}

/// The `text` of a content list's parts, joined by newlines; `None` when there is none to keep.
/// Image and encrypted parts carry nothing the lexical model can read.
fn text_parts(content: &Value) -> Option<String> {
    let text = content
        .as_array()?
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

/// A `function_call`'s arguments: a JSON-encoded object on the wire, decoded so it is
/// field-diffable; anything that does not decode to an object is kept as it is.
fn arguments(raw: &Value) -> Payload {
    match raw.as_str().map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Object(fields))) => Payload::Object(fields),
        _ => loose(raw),
    }
}

fn loose(value: &Value) -> Payload {
    match value {
        Value::String(text) => Payload::Text(text.clone()),
        Value::Object(fields) => Payload::Object(fields.clone()),
        Value::Null => Payload::Text(String::new()),
        other => Payload::Other(other.clone()),
    }
}

/// A tool output as text: a bare string as-is, a part list as its text parts.
fn output_text(output: &Value) -> String {
    match output {
        Value::String(text) => text.clone(),
        Value::Array(_) => text_parts(output).unwrap_or_default(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
