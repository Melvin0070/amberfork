//! Claude Code session adapter tests: a session transcript (`~/.claude/projects/<slug>/
//! <session-id>.jsonl`) becomes one canonical [`amberfork_model::Run`] with no conversion step.
//!
//! The fixture is synthetic but record-shaped: every line carries the `type` + `sessionId` pair
//! real transcripts do, plus the bookkeeping a real session interleaves (a `mode` record, a
//! subagent sidechain, a meta user record, a `thinking` block). The contract under test:
//! - an assistant `text` block → an `llm` step named `assistant` — NOT the model id, so a
//!   model-bump pair (the commonest regression pair there is) still shares step names;
//! - an assistant `tool_use` block → a `tool` step named by the tool, its `input` field-diffable,
//!   its `tool_result` paired back by id as the step's output;
//! - a user prompt → an `other` step named `user`;
//! - sidechain, meta, `thinking` and non-message records contribute no step;
//! - `outcome` is never inferred — a transcript does not know whether the session succeeded.

use amberfork_ingest::{IngestError, claudecode};
use amberfork_model::{Payload, StepKind};

const SESSION: &str = r#"{"type":"mode","sessionId":"sess-1","mode":"default"}
{"type":"user","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:00Z","message":{"role":"user","content":"Why does the build fail?"}}
{"type":"assistant","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:01Z","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"thinking","thinking":"private scratch","signature":"sig"},{"type":"text","text":"I'll read the config first."}]}}
{"type":"assistant","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:02Z","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"config.toml"}}]}}
{"type":"user","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:03Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":"edition = \"2024\""}]}}
{"type":"assistant","sessionId":"sess-1","isSidechain":true,"timestamp":"2026-11-01T10:00:04Z","message":{"role":"assistant","model":"claude-haiku-5-5","content":[{"type":"text","text":"subagent chatter"}]}}
{"type":"user","sessionId":"sess-1","isSidechain":false,"isMeta":true,"timestamp":"2026-11-01T10:00:05Z","message":{"role":"user","content":"<local-command-caveat>harness note</local-command-caveat>"}}
{"type":"assistant","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:06Z","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"toolu_02","name":"Bash","input":{"command":"cargo build"}}]}}
{"type":"user","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:09Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","is_error":true,"content":[{"type":"text","text":"error[E0432]"},{"type":"text","text":"unresolved import"}]}]}}
{"type":"assistant","sessionId":"sess-1","isSidechain":false,"timestamp":"2026-11-01T10:00:10Z","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"The import path is wrong."}]}}
"#;

#[test]
fn a_session_becomes_one_run_of_prompt_llm_and_tool_steps() {
    let ingested = claudecode::from_jsonl_str(SESSION).expect("a session transcript loads");
    let run = &ingested.run;

    let shape: Vec<(StepKind, &str)> = run
        .steps
        .iter()
        .map(|s| (s.kind, s.name.as_str()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (StepKind::Other, "user"),
            (StepKind::Llm, "assistant"),
            (StepKind::Tool, "Read"),
            (StepKind::Tool, "Bash"),
            (StepKind::Llm, "assistant"),
        ],
        "sidechain, meta, thinking and bookkeeping records contribute no step"
    );
    let idxs: Vec<usize> = run.steps.iter().map(|s| s.idx).collect();
    assert_eq!(idxs, vec![0, 1, 2, 3, 4], "idx is the dense position");
}

#[test]
fn the_run_is_identified_by_its_session_and_claims_no_outcome() {
    let run = claudecode::from_jsonl_str(SESSION).unwrap().run;
    assert_eq!(run.id, "claude-code_sess-1");
    assert_eq!(
        run.outcome, None,
        "a transcript does not know if it succeeded"
    );
}

#[test]
fn a_tool_step_carries_its_input_fields_and_its_paired_result() {
    let run = claudecode::from_jsonl_str(SESSION).unwrap().run;

    let read = &run.steps[2];
    let Some(Payload::Object(input)) = &read.inputs else {
        panic!(
            "tool input stays a field-diffable object: {:?}",
            read.inputs
        );
    };
    assert_eq!(input["file_path"], "config.toml");
    assert_eq!(
        read.outputs,
        Some(Payload::Text("edition = \"2024\"".to_owned())),
        "the tool_result is paired back by tool_use_id"
    );
    assert_eq!(read.t_start.as_deref(), Some("2026-11-01T10:00:02Z"));
    assert!(!read.attrs.contains_key("is_error"));
}

#[test]
fn a_block_list_result_is_joined_and_an_error_result_is_flagged() {
    let run = claudecode::from_jsonl_str(SESSION).unwrap().run;
    let bash = &run.steps[3];
    assert_eq!(
        bash.outputs,
        Some(Payload::Text("error[E0432]\nunresolved import".to_owned()))
    );
    assert_eq!(bash.attrs["is_error"], true);
}

#[test]
fn the_model_is_kept_as_an_attribute_not_as_the_step_name() {
    let run = claudecode::from_jsonl_str(SESSION).unwrap().run;
    let llm = &run.steps[1];
    assert_eq!(llm.name, "assistant");
    assert_eq!(llm.attrs["model"], "claude-opus-5-5");
    assert_eq!(
        llm.outputs,
        Some(Payload::Text("I'll read the config first.".to_owned()))
    );
}

#[test]
fn a_session_is_recognized_and_other_jsonl_is_not() {
    assert!(claudecode::looks_like_session(SESSION));
    // Generic JSONL (an exporter dump, an OTLP line stream) must keep its own error path.
    assert!(!claudecode::looks_like_session("{\"a\":1}\n{\"b\":2}\n"));
    // A canonical trace is one pretty-printed or single-line object without `sessionId`.
    assert!(!claudecode::looks_like_session(
        r#"{"schema_version":"0.1","id":"x","steps":[]}"#
    ));
    assert!(!claudecode::looks_like_session(""));
}

#[test]
fn a_corrupt_line_is_a_parse_error_not_a_silently_shorter_run() {
    let broken = format!("{SESSION}{{\"type\":\"assistant\",\"sessionId\":\n");
    let err = claudecode::from_jsonl_str(&broken).expect_err("a corrupt line must not be dropped");
    assert!(matches!(err, IngestError::Parse { .. }));
}
