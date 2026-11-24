//! ATIF adapter tests: an Agent Trajectory Interchange Format document (Harbor's
//! `trajectory.json`, the format Terminal-Bench trials are published in) becomes one canonical
//! [`amberfork_model::Run`].
//!
//! The fixture follows the ATIF RFC (v1.0–v1.8). An ATIF *step* is a whole agent turn — message,
//! tool calls and their observation together — so one ATIF step fans out into several canonical
//! steps. The contract under test:
//! - a `system` / `user` step → an `other` step named for its source;
//! - an agent turn's `message` → an `llm` step named `assistant` (model in `attrs`, the step's own
//!   `model_name` overriding the agent default) — the same naming the Claude Code adapter uses, so
//!   the two formats describe a turn the same way;
//! - each `tool_calls` entry → a `tool` step named by `function_name`, with the observation result
//!   whose `source_call_id` matches paired back as its output;
//! - an observation result with no `source_call_id` → an `other` step named `observation`;
//! - `reasoning_content` and `is_copied_context` steps contribute nothing; embedded
//!   `subagent_trajectories` are not inlined;
//! - every canonical step records the ATIF `step_id` it came from, so a label expressed in ATIF
//!   step ids can be mapped onto the run.

use amberfork_ingest::{IngestError, atif};
use amberfork_model::{Payload, StepKind, WarningCode};

const TRAJECTORY: &str = r#"{
  "schema_version": "ATIF-v1.8",
  "session_id": "trial-42",
  "agent": { "name": "terminus", "version": "2.0.0", "model_name": "openai/gpt-5.6-sol" },
  "steps": [
    { "step_id": 1, "source": "system", "message": "You are a terminal agent." },
    { "step_id": 2, "source": "user", "timestamp": "2026-11-01T10:00:00Z",
      "message": [ { "type": "text", "text": "Create hello.txt." },
                   { "type": "image", "source": { "media_type": "image/png", "path": "s.png" } } ] },
    { "step_id": 3, "source": "user", "message": "stale prior turn", "is_copied_context": true },
    { "step_id": 4, "source": "agent", "timestamp": "2026-11-01T10:00:01Z",
      "message": "I'll create it, then check.",
      "reasoning_content": "private scratch",
      "tool_calls": [
        { "tool_call_id": "call-1", "function_name": "write_file",
          "arguments": { "path": "hello.txt", "content": "Hello" } },
        { "tool_call_id": "call-2", "function_name": "bash", "arguments": { "cmd": "cat hello.txt" } }
      ],
      "observation": { "results": [
        { "source_call_id": "call-2", "content": [ { "type": "text", "text": "Hello" } ] },
        { "source_call_id": "call-1", "content": "File created" }
      ] } },
    { "step_id": 5, "source": "agent", "model_name": "openai/gpt-5.6-luna", "message": "",
      "observation": { "results": [ { "content": "context was compacted" } ] } },
    { "step_id": 6, "source": "agent", "message": "Done." }
  ],
  "subagent_trajectories": [
    { "schema_version": "ATIF-v1.8", "trajectory_id": "sub-1",
      "agent": { "name": "helper", "version": "1" },
      "steps": [ { "step_id": 1, "source": "agent", "message": "subagent chatter" } ] }
  ]
}"#;

#[test]
fn a_trajectory_fans_each_agent_turn_out_into_llm_and_tool_steps() {
    let run = atif::from_json_str(TRAJECTORY)
        .expect("an ATIF trajectory loads")
        .run;

    let shape: Vec<(StepKind, &str)> = run
        .steps
        .iter()
        .map(|s| (s.kind, s.name.as_str()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (StepKind::Other, "system"),
            (StepKind::Other, "user"),
            (StepKind::Llm, "assistant"),
            (StepKind::Tool, "write_file"),
            (StepKind::Tool, "bash"),
            (StepKind::Other, "observation"),
            (StepKind::Llm, "assistant"),
        ],
        "copied-context steps, reasoning and embedded subagents contribute no step"
    );
    let idxs: Vec<usize> = run.steps.iter().map(|s| s.idx).collect();
    assert_eq!(
        idxs,
        (0..7).collect::<Vec<_>>(),
        "idx is the dense position"
    );
}

#[test]
fn the_run_is_identified_by_its_session_and_claims_no_outcome() {
    let run = atif::from_json_str(TRAJECTORY).unwrap().run;
    assert_eq!(run.id, "atif_trial-42");
    assert_eq!(
        run.outcome, None,
        "a trajectory does not carry its verifier's verdict"
    );
}

#[test]
fn a_tool_call_is_paired_with_its_result_by_call_id_not_by_position() {
    let run = atif::from_json_str(TRAJECTORY).unwrap().run;

    let write = &run.steps[3];
    let Some(Payload::Object(args)) = &write.inputs else {
        panic!("arguments stay a field-diffable object: {:?}", write.inputs);
    };
    assert_eq!(args["path"], "hello.txt");
    assert_eq!(
        write.outputs,
        Some(Payload::Text("File created".to_owned()))
    );

    // The results array lists call-2 first; pairing must follow the id.
    let bash = &run.steps[4];
    assert_eq!(bash.outputs, Some(Payload::Text("Hello".to_owned())));
}

#[test]
fn multimodal_content_keeps_its_text_parts() {
    let run = atif::from_json_str(TRAJECTORY).unwrap().run;
    assert_eq!(
        run.steps[1].outputs,
        Some(Payload::Text("Create hello.txt.".to_owned()))
    );
    assert_eq!(
        run.steps[1].t_start.as_deref(),
        Some("2026-11-01T10:00:00Z")
    );
}

#[test]
fn every_step_records_its_atif_step_id_and_llm_steps_their_model() {
    let run = atif::from_json_str(TRAJECTORY).unwrap().run;
    let step_ids: Vec<u64> = run
        .steps
        .iter()
        .map(|s| s.attrs["step_id"].as_u64().expect("step_id attr"))
        .collect();
    assert_eq!(step_ids, vec![1, 2, 4, 4, 4, 5, 6]);

    assert_eq!(run.steps[2].attrs["model"], "openai/gpt-5.6-sol");
    assert_eq!(
        run.steps[2].outputs,
        Some(Payload::Text("I'll create it, then check.".to_owned()))
    );
    // Step 6 has no model of its own and inherits the agent default.
    assert_eq!(run.steps[6].attrs["model"], "openai/gpt-5.6-sol");
}

#[test]
fn a_trajectory_is_recognized_by_its_schema_version_and_nothing_else_is() {
    assert!(atif::looks_like_trajectory(TRAJECTORY));
    // A canonical trace also carries `schema_version`; only the `ATIF-` value routes here.
    assert!(!atif::looks_like_trajectory(
        r#"{"schema_version":"0.1","id":"x","steps":[]}"#
    ));
    assert!(!atif::looks_like_trajectory(
        r#"{"cassette_version":"0.1"}"#
    ));
    assert!(!atif::looks_like_trajectory("{not json"));
}

#[test]
fn a_version_past_the_v1_line_loads_with_a_warning() {
    let v2 = TRAJECTORY.replace(
        "\"schema_version\": \"ATIF-v1.8\",\n  \"session_id\"",
        "\"schema_version\": \"ATIF-v2.0\",\n  \"session_id\"",
    );
    let ingested = atif::from_json_str(&v2).expect("an unknown version is not fatal");
    assert_eq!(ingested.run.steps.len(), 7);
    assert!(
        ingested
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::SchemaVersionMismatch && w.msg.contains("ATIF-v2.0")),
        "{:?}",
        ingested.warnings
    );
    assert!(atif::from_json_str(TRAJECTORY).unwrap().warnings.is_empty());
}

#[test]
fn a_document_without_steps_is_a_parse_error() {
    let err =
        atif::from_json_str(r#"{"schema_version":"ATIF-v1.8","agent":{"name":"a","version":"1"}}"#)
            .expect_err("steps is required");
    assert!(matches!(err, IngestError::Parse { .. }));
}
