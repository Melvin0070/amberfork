//! Adapter from Claude Code session transcripts (`~/.claude/projects/<slug>/<session-id>.jsonl`)
//! into the canonical [`amberfork_model::Run`].
//!
//! The one foreign shape that needs no export step: the transcripts are already on the
//! developer's disk. A transcript is JSON-Lines, one record per line, each carrying a `type` and
//! the `sessionId`. Only `assistant` and `user` records hold trajectory; the rest (`mode`,
//! `attachment`, `queue-operation`, `system`, …) is harness bookkeeping.
//!
//! Mapping decisions, each made for alignment rather than fidelity to the wire:
//! - **An LLM step is named `assistant`, never the model id.** The aligner keys on `name`, and
//!   the commonest regression pair there is — the same task before and after a model bump —
//!   would otherwise share no LLM step name at all (the zero-shared-vocabulary collapse notebook
//!   070 diagnosed on TRAIL↔HAL). The model is kept in `attrs`.
//! - **A tool call and its result are one step.** The `tool_use` block opens the step; the
//!   `tool_result` that answers it (a later `user` record) is paired back by id as its output.
//! - **`thinking` blocks contribute nothing.** They are frequently redacted to a signature, and
//!   where present they are the least reproducible text in the session.
//! - **Sidechain (subagent) and meta records are skipped**, so the run is the main thread's
//!   linear trajectory. Subagent trees are the DAG-alignment frontier (#59), not this adapter's.
//! - **`outcome` is never set.** A transcript does not know whether the session succeeded.
//!
//! The format is undocumented and drifts between releases, so records are read as loose
//! [`Value`]s: an unknown record type or block type is skipped, never an error. What is NOT
//! forgiven is a line that fails to parse — dropping it would silently shorten the run and move
//! the fork.

use crate::{IngestError, Ingested};
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

/// Whether `text` is a Claude Code session transcript: its first non-empty line is a JSON object
/// carrying string `type` and `sessionId` fields, which every transcript record does. Checked
/// before the generic JSON-Lines classification so a transcript routes here and any other JSONL
/// keeps its own error.
#[must_use]
pub fn looks_like_session(text: &str) -> bool {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .is_some_and(|record| record["type"].is_string() && record["sessionId"].is_string())
}

/// Parse a session transcript into one canonical run.
///
/// # Errors
/// Returns [`IngestError::Parse`] if any non-empty line is not valid JSON. Everything past a
/// successful parse is forgiving — see the module docs.
pub fn from_jsonl_str(s: &str) -> Result<Ingested, IngestError> {
    let mut session_id: Option<String> = None;
    let mut steps: Vec<Step> = Vec::new();
    let mut awaiting_result: HashMap<String, usize> = HashMap::new();

    for line in s.lines().filter(|line| !line.trim().is_empty()) {
        let record: Value = serde_json::from_str(line)
            .map_err(|source| IngestError::Parse { path: None, source })?;
        if session_id.is_none() {
            session_id = record["sessionId"].as_str().map(str::to_owned);
        }
        if record["isSidechain"] == true || record["isMeta"] == true {
            continue;
        }
        let timestamp = record["timestamp"].as_str();
        let message = &record["message"];
        match record["type"].as_str() {
            Some("assistant") => {
                let model = message["model"].as_str();
                for block in blocks(&message["content"]) {
                    match block["type"].as_str() {
                        Some("text") => {
                            if let Some(text) = nonblank(&block["text"]) {
                                let mut step =
                                    new_step(steps.len(), StepKind::Llm, "assistant", timestamp);
                                step.outputs = Some(Payload::Text(text.to_owned()));
                                if let Some(model) = model {
                                    step.attrs.insert("model".to_owned(), model.into());
                                }
                                steps.push(step);
                            }
                        }
                        Some("tool_use") => {
                            let name = block["name"].as_str().unwrap_or("tool");
                            let mut step = new_step(steps.len(), StepKind::Tool, name, timestamp);
                            step.inputs = Some(match &block["input"] {
                                Value::Object(fields) => Payload::Object(fields.clone()),
                                Value::Null => Payload::Object(Map::new()),
                                other => Payload::Other(other.clone()),
                            });
                            if let Some(id) = block["id"].as_str() {
                                awaiting_result.insert(id.to_owned(), steps.len());
                            }
                            steps.push(step);
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => match &message["content"] {
                Value::String(prompt) => push_prompt(&mut steps, prompt, timestamp),
                content => {
                    for block in blocks(content) {
                        match block["type"].as_str() {
                            Some("tool_result") => {
                                let pending = block["tool_use_id"]
                                    .as_str()
                                    .and_then(|id| awaiting_result.remove(id));
                                if let Some(idx) = pending {
                                    let step = &mut steps[idx];
                                    step.outputs =
                                        Some(Payload::Text(result_text(&block["content"])));
                                    if block["is_error"] == true {
                                        step.attrs.insert("is_error".to_owned(), true.into());
                                    }
                                }
                            }
                            Some("text") => {
                                if let Some(text) = block["text"].as_str() {
                                    push_prompt(&mut steps, text, timestamp);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            },
            _ => {}
        }
    }

    let run = Run {
        schema_version: SchemaVersion::current(),
        id: format!("claude-code_{}", session_id.as_deref().unwrap_or("session")),
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

/// Load a session transcript from a file on disk.
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

fn push_prompt(steps: &mut Vec<Step>, text: &str, timestamp: Option<&str>) {
    if text.trim().is_empty() {
        return;
    }
    let mut step = new_step(steps.len(), StepKind::Other, "user", timestamp);
    step.outputs = Some(Payload::Text(text.to_owned()));
    steps.push(step);
}

/// A message's content blocks; a non-array (absent, or a bare string) has none.
fn blocks(content: &Value) -> &[Value] {
    content.as_array().map_or(&[], Vec::as_slice)
}

fn nonblank(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

/// A `tool_result`'s content as text: a bare string as-is, a block list as its `text` blocks
/// joined by newlines (image and other non-text blocks carry nothing the lexical model can read).
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
