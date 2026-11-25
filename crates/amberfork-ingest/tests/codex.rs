//! Codex CLI adapter tests: a rollout (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`) becomes one
//! canonical [`amberfork_model::Run`].
//!
//! The fixture's *structure* is taken from real rollouts written by codex-cli 0.171 (record types,
//! payload types and key sets surveyed across several hundred local sessions); its content is
//! invented. A rollout interleaves two views of one session: `response_item` records are the
//! model-facing transcript, and `event_msg` records are a UI stream that repeats much of it. The
//! contract under test:
//! - only `response_item` records become steps; `event_msg` and every bookkeeping type
//!   (`token_usage_record`, `world_state`, `compacted`, …) contribute nothing, and an unknown
//!   record or payload type is skipped, not fatal;
//! - an assistant `message` → an `llm` step named `assistant` — the same naming the Claude Code
//!   and ATIF adapters use — with the model taken from the latest `turn_context`;
//! - a `user` / `developer` message → an `other` step named for its role;
//! - `function_call` and `custom_tool_call` → a `tool` step named by `name`, with the output
//!   record that carries the same `call_id` paired back as its output, whatever order they
//!   arrive in; an output with no matching call is dropped;
//! - `reasoning` contributes nothing (its text is encrypted on disk);
//! - an inter-agent `agent_message` → an `other` step named `agent_message`;
//! - the run is identified by the first `session_meta`, and claims no outcome.

use amberfork_ingest::{IngestError, codex};
use amberfork_model::{Payload, StepKind};

const ROLLOUT: &str = r#"{"timestamp":"2026-11-01T10:00:00.000Z","type":"session_meta","ordinal":0,"payload":{"id":"sess-1","cwd":"/work","cli_version":"0.171.0","source":"cli"}}
{"timestamp":"2026-11-01T10:00:00.100Z","type":"turn_context","ordinal":1,"payload":{"model":"gpt-6-astra","cwd":"/work","effort":"medium"}}
{"timestamp":"2026-11-01T10:00:00.200Z","type":"response_item","ordinal":2,"payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"You are Codex."}]}}
{"timestamp":"2026-11-01T10:00:01.000Z","type":"response_item","ordinal":3,"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Create hello.txt."},{"type":"input_image","image_url":"data:image/png;base64,AAAA"}]}}
{"timestamp":"2026-11-01T10:00:01.001Z","type":"event_msg","ordinal":4,"payload":{"type":"user_message","message":"Create hello.txt."}}
{"timestamp":"2026-11-01T10:00:02.000Z","type":"response_item","ordinal":5,"payload":{"type":"reasoning","summary":[],"content":null,"encrypted_content":"gAAAA"}}
{"timestamp":"2026-11-01T10:00:03.000Z","type":"response_item","ordinal":6,"payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"I'll create it, then check."}]}}
{"timestamp":"2026-11-01T10:00:04.000Z","type":"response_item","ordinal":7,"payload":{"type":"custom_tool_call","status":"completed","call_id":"c1","name":"exec","input":"printf Hello > hello.txt"}}
{"timestamp":"2026-11-01T10:00:04.500Z","type":"response_item","ordinal":8,"payload":{"type":"function_call","call_id":"c2","name":"js","namespace":"mcp__repl","arguments":"{\"code\":\"1+1\"}"}}
{"timestamp":"2026-11-01T10:00:05.000Z","type":"response_item","ordinal":9,"payload":{"type":"function_call_output","call_id":"c2","output":"2"}}
{"timestamp":"2026-11-01T10:00:05.500Z","type":"response_item","ordinal":10,"payload":{"type":"custom_tool_call_output","call_id":"c1","name":"exec","output":[{"type":"input_text","text":"File created"},{"type":"input_image","image_url":"data:image/png;base64,BBBB","detail":"auto"}]}}
{"timestamp":"2026-11-01T10:00:05.600Z","type":"event_msg","ordinal":11,"payload":{"type":"agent_message","message":"I'll create it, then check.","phase":"commentary"}}
{"timestamp":"2026-11-01T10:00:06.000Z","type":"turn_context","ordinal":12,"payload":{"model":"gpt-6-luna"}}
{"timestamp":"2026-11-01T10:00:07.000Z","type":"response_item","ordinal":13,"payload":{"type":"agent_message","author":"/root/helper","recipient":"/root","content":[{"type":"input_text","text":"subagent finished"},{"type":"encrypted_content","encrypted_content":"gAAAA"}]}}
{"timestamp":"2026-11-01T10:00:08.000Z","type":"response_item","ordinal":14,"payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"Done."}]}}
{"timestamp":"2026-11-01T10:00:08.100Z","type":"future_record","ordinal":15,"payload":{"anything":1}}
{"timestamp":"2026-11-01T10:00:08.200Z","type":"response_item","ordinal":16,"payload":{"type":"future_item","text":"x"}}
{"timestamp":"2026-11-01T10:00:08.300Z","type":"response_item","ordinal":17,"payload":{"type":"function_call_output","call_id":"c9","output":"orphan"}}
{"timestamp":"2026-11-01T10:00:08.400Z","type":"token_usage_record","ordinal":18,"payload":{"usage":{"total_tokens":10}}}
{"timestamp":"2026-11-01T10:00:09.000Z","type":"session_meta","ordinal":19,"payload":{"id":"sess-resumed"}}
"#;

#[test]
fn a_rollout_becomes_the_model_facing_transcript_and_nothing_else() {
    let run = codex::from_jsonl_str(ROLLOUT).expect("a rollout loads").run;
    let shape: Vec<(StepKind, &str)> = run
        .steps
        .iter()
        .map(|s| (s.kind, s.name.as_str()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (StepKind::Other, "developer"),
            (StepKind::Other, "user"),
            (StepKind::Llm, "assistant"),
            (StepKind::Tool, "exec"),
            (StepKind::Tool, "js"),
            (StepKind::Other, "agent_message"),
            (StepKind::Llm, "assistant"),
        ],
        "event_msg, reasoning, bookkeeping, unknown types and an orphan output contribute no step"
    );
    let idxs: Vec<usize> = run.steps.iter().map(|s| s.idx).collect();
    assert_eq!(
        idxs,
        (0..7).collect::<Vec<_>>(),
        "idx is the dense position"
    );
}

#[test]
fn the_run_is_identified_by_its_first_session_meta_and_claims_no_outcome() {
    let run = codex::from_jsonl_str(ROLLOUT).unwrap().run;
    assert_eq!(run.id, "codex_sess-1");
    assert_eq!(
        run.outcome, None,
        "a rollout does not know whether the task succeeded"
    );
}

#[test]
fn an_llm_step_carries_its_text_and_the_model_of_the_turn_it_ran_in() {
    let run = codex::from_jsonl_str(ROLLOUT).unwrap().run;
    let first = &run.steps[2];
    assert_eq!(
        first.outputs,
        Some(Payload::Text("I'll create it, then check.".to_owned()))
    );
    assert_eq!(first.attrs["model"], "gpt-6-astra");
    assert_eq!(first.attrs["phase"], "commentary");
    assert_eq!(first.t_start.as_deref(), Some("2026-11-01T10:00:03.000Z"));
    // A later turn_context switched the model.
    assert_eq!(run.steps[6].attrs["model"], "gpt-6-luna");
}

#[test]
fn a_prompt_keeps_its_text_parts() {
    let run = codex::from_jsonl_str(ROLLOUT).unwrap().run;
    assert_eq!(
        run.steps[1].outputs,
        Some(Payload::Text("Create hello.txt.".to_owned()))
    );
}

#[test]
fn a_tool_call_is_paired_with_its_output_by_call_id_not_by_position() {
    let run = codex::from_jsonl_str(ROLLOUT).unwrap().run;

    // `custom_tool_call` carries free-form text input; its output arrived second.
    let exec = &run.steps[3];
    assert_eq!(
        exec.inputs,
        Some(Payload::Text("printf Hello > hello.txt".to_owned()))
    );
    assert_eq!(
        exec.outputs,
        Some(Payload::Text("File created".to_owned())),
        "a block-list output keeps its text parts"
    );

    // `function_call` carries JSON-encoded arguments, decoded so they are field-diffable.
    let js = &run.steps[4];
    let Some(Payload::Object(args)) = &js.inputs else {
        panic!("arguments decode to an object: {:?}", js.inputs);
    };
    assert_eq!(args["code"], "1+1");
    assert_eq!(js.outputs, Some(Payload::Text("2".to_owned())));
    assert_eq!(js.attrs["namespace"], "mcp__repl");
}

#[test]
fn an_inter_agent_message_keeps_its_text_and_who_it_was_between() {
    let run = codex::from_jsonl_str(ROLLOUT).unwrap().run;
    let message = &run.steps[5];
    assert_eq!(
        message.outputs,
        Some(Payload::Text("subagent finished".to_owned()))
    );
    assert_eq!(message.attrs["author"], "/root/helper");
    assert_eq!(message.attrs["recipient"], "/root");
}

#[test]
fn a_rollout_is_recognized_by_its_leading_session_meta_and_nothing_else_is() {
    assert!(codex::looks_like_rollout(ROLLOUT));
    // A Claude Code transcript is JSON-Lines too, with a different first record.
    assert!(!codex::looks_like_rollout(
        r#"{"type":"user","sessionId":"abc","message":{"role":"user","content":"hi"}}"#
    ));
    assert!(!codex::looks_like_rollout(
        r#"{"schema_version":"0.1","id":"x","steps":[]}"#
    ));
    assert!(!codex::looks_like_rollout("{not json"));
}

#[test]
fn a_corrupt_line_is_a_parse_error_not_a_silently_shorter_run() {
    let broken = ROLLOUT.replacen(r#"{"timestamp":"2026-11-01T10:00:04.000Z""#, "{oops", 1);
    let err = codex::from_jsonl_str(&broken).expect_err("a corrupt line is fatal");
    assert!(matches!(err, IngestError::Parse { .. }));
}
