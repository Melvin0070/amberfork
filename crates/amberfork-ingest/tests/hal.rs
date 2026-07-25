//! HAL Open Deep Research adapter tests: a real-shaped HAL `hal_traces` config dump (the decrypted
//! payload of one `agent-evals/hal_traces` zip) becomes canonical [`amberfork_model::Run`]s, one per
//! GAIA task, for the natural-pair benchmark's reference side (issue #41 S4b).
//!
//! The fixture exercises every mapping decision the adapter makes (notebook 047):
//! - `raw_logging_results` is a flat, double-logged turn stream — a `litellm.completion` wrapper 1:1
//!   over an `openai.chat.completions.create` leaf. The adapter keeps the content-bearing openai
//!   leaves as turns and drops the redundant litellm wrappers, so a task's step count is its turn
//!   count, not its record count;
//! - turns are grouped by `attributes.weave_task_id` (a GAIA UUID — the same join key TRAIL's S4a
//!   lifts) and ordered by `started_at` (RFC3339), NOT by array position;
//! - each turn is an `LLM` step whose `inputs` keep only `{model, messages}` and `outputs` only
//!   `{choices, usage}` — client/transport internals (`self`, `extra_headers`, …) are dropped, both
//!   as fidelity and so a leaked auth header can never ride into a canonical `Run` (issue #43);
//! - the run's verdict is never asserted on the trajectory: `Run.outcome` stays `None` and the
//!   HAL pass/fail rides beside it in `HalMeta.passed`, keyed off `results.successful_tasks`;
//! - a turn with neither input nor output content degrades to a metadata-only step plus a
//!   content-absent advisory, never a parse failure.
//!
//! No GAIA question/answer text appears — real HAL dumps embed gated GAIA content and are never
//! committed; the fixture's task ids and message bodies are synthetic.

use amberfork_ingest::hal;
use amberfork_model::{Outcome, Payload, StepKind, WarningCode};

/// A spec-faithful HAL config dump: two GAIA tasks. `task-aaaa` passed and has two turns whose
/// openai leaves are placed OUT of chronological order in the array (to prove `started_at` sorting);
/// `task-bbbb` failed and has one turn. Every turn is a `litellm.completion` wrapper over an
/// `openai.chat.completions.create` leaf, and one leaf carries client-internal `self`/`extra_headers`
/// that must not survive into the canonical run. A third turn on `task-bbbb` is content-free.
const HAL_DUMP: &str = r#"{
  "config": {
    "agent_name": "HF Open Deep Research (gpt-4.1-2025-04-14)",
    "benchmark_name": "gaia",
    "run_id": "gaia_hf_open_deep_research_gpt4120250414_1744843595",
    "agent_args": { "model_name": "gpt-4.1-2025-04-14" }
  },
  "results": {
    "successful_tasks": ["task-aaaa"],
    "failed_tasks": ["task-bbbb"]
  },
  "raw_logging_results": [
    {
      "id": "openai-a-turn2",
      "trace_id": "trace-a",
      "started_at": "2025-04-16T23:46:20.000000+00:00",
      "ended_at": "2025-04-16T23:46:21.000000+00:00",
      "attributes": { "weave_task_id": "task-aaaa" },
      "summary": { "weave": { "trace_name": "openai.chat.completions.create" } },
      "inputs": {
        "self": "<AsyncOpenAI object>",
        "model": "gpt-4.1-2025-04-14",
        "messages": [
          {"role": "system", "content": "sys"},
          {"role": "user", "content": "u1"},
          {"role": "assistant", "content": "a1"},
          {"role": "user", "content": "u2"}
        ],
        "extra_headers": { "authorization": "Bearer sk-SECRET-should-not-survive" }
      },
      "output": {
        "id": "chatcmpl-2",
        "choices": [{"finish_reason": "stop", "index": 0, "message": {"role": "assistant", "content": "answer two"}}],
        "usage": {"prompt_tokens": 200, "completion_tokens": 20, "total_tokens": 220},
        "system_fingerprint": "fp_x"
      }
    },
    {
      "id": "litellm-a-turn2",
      "trace_id": "trace-a",
      "started_at": "2025-04-16T23:46:19.900000+00:00",
      "attributes": { "weave_task_id": "task-aaaa" },
      "summary": { "weave": { "trace_name": "litellm.completion" } },
      "inputs": { "model": "gpt-4.1-2025-04-14", "messages": [{"role": "system", "content": "sys"}] },
      "output": { "id": "modelresp-2", "created": 1, "model": "gpt-4.1", "object": "chat.completion" }
    },
    {
      "id": "openai-a-turn1",
      "trace_id": "trace-a",
      "started_at": "2025-04-16T23:46:16.609285+00:00",
      "ended_at": "2025-04-16T23:46:18.029466+00:00",
      "attributes": { "weave_task_id": "task-aaaa" },
      "summary": { "weave": { "trace_name": "openai.chat.completions.create" } },
      "inputs": {
        "model": "gpt-4.1-2025-04-14",
        "messages": [{"role": "system", "content": "sys"}]
      },
      "output": {
        "id": "chatcmpl-1",
        "choices": [{"finish_reason": "stop", "index": 0, "message": {"role": "assistant", "content": "answer one"}}],
        "usage": {"prompt_tokens": 100, "completion_tokens": 9, "total_tokens": 109}
      }
    },
    {
      "id": "litellm-a-turn1",
      "started_at": "2025-04-16T23:46:16.500000+00:00",
      "attributes": { "weave_task_id": "task-aaaa" },
      "summary": { "weave": { "trace_name": "litellm.completion" } },
      "inputs": { "model": "gpt-4.1-2025-04-14", "messages": [] },
      "output": { "object": "chat.completion" }
    },
    {
      "id": "openai-b-turn1",
      "trace_id": "trace-b",
      "started_at": "2025-04-16T20:00:00.000000+00:00",
      "ended_at": "2025-04-16T20:00:01.000000+00:00",
      "attributes": { "weave_task_id": "task-bbbb" },
      "summary": { "weave": { "trace_name": "openai.chat.completions.create" } },
      "inputs": { "model": "gpt-4.1-2025-04-14", "messages": [{"role": "user", "content": "b-question"}] },
      "output": { "choices": [{"index": 0, "message": {"role": "assistant", "content": "b-answer"}}], "usage": {"total_tokens": 42} }
    },
    {
      "id": "openai-b-turn2-empty",
      "trace_id": "trace-b",
      "started_at": "2025-04-16T20:00:05.000000+00:00",
      "attributes": { "weave_task_id": "task-bbbb" },
      "summary": { "weave": { "trace_name": "openai.chat.completions.create" } },
      "inputs": {},
      "output": {}
    }
  ]
}"#;

#[test]
fn one_run_per_task_sorted_by_task_id() {
    let runs = hal::convert_str(HAL_DUMP).expect("valid HAL dump");
    let ids: Vec<&str> = runs.iter().map(|c| c.meta.gaia_task_id.as_str()).collect();
    assert_eq!(
        ids,
        ["task-aaaa", "task-bbbb"],
        "one run per task, sorted by task id"
    );
}

#[test]
fn litellm_wrappers_dropped_openai_leaves_are_turns() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    let a = &runs[0];
    // task-aaaa has 4 records (2 litellm + 2 openai) but only 2 turns.
    assert_eq!(a.run.steps.len(), 2, "only openai leaves become steps");
    for step in &a.run.steps {
        assert_eq!(step.kind, StepKind::Llm);
        assert_eq!(step.name, "openai.chat.completions.create");
        assert_eq!(step.parent_idx, None, "turns form a linear chain");
    }
}

#[test]
fn turns_ordered_by_started_at_not_array_position() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    let a = &runs[0];
    // turn1 (16:16) precedes turn2 (16:20) despite turn2 appearing first in the array.
    assert_eq!(a.run.steps[0].idx, 0);
    assert_eq!(
        a.run.steps[0].t_start.as_deref(),
        Some("2025-04-16T23:46:16.609285+00:00")
    );
    assert_eq!(
        a.run.steps[0].t_end.as_deref(),
        Some("2025-04-16T23:46:18.029466+00:00")
    );
    assert_eq!(
        a.run.steps[1].t_start.as_deref(),
        Some("2025-04-16T23:46:20.000000+00:00")
    );
}

#[test]
fn inputs_keep_only_model_and_messages_no_secrets() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    // turn2 is the leaf that carried `self` + `extra_headers` with a secret token.
    let Some(Payload::Object(inputs)) = &runs[0].run.steps[1].inputs else {
        panic!("turn2 inputs should be a field-diffable object");
    };
    let mut keys: Vec<&str> = inputs.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["messages", "model"],
        "only semantic request fields survive"
    );
    let serialized = serde_json::to_string(inputs).unwrap();
    assert!(
        !serialized.contains("SECRET"),
        "no auth header leaks into the run"
    );
    assert!(!serialized.contains("authorization"));
}

#[test]
fn outputs_keep_only_choices_and_usage() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    let Some(Payload::Object(output)) = &runs[0].run.steps[0].outputs else {
        panic!("turn outputs should be a field-diffable object");
    };
    let mut keys: Vec<&str> = output.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["choices", "usage"],
        "transport ids dropped, content + cost kept"
    );
}

#[test]
fn verdict_rides_in_meta_never_on_the_run() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    assert!(runs[0].meta.passed, "task-aaaa is in successful_tasks");
    assert!(!runs[1].meta.passed, "task-bbbb is in failed_tasks");
    assert_eq!(
        runs[0].run.outcome, None,
        "adapter never asserts a run verdict"
    );
    assert_eq!(runs[1].run.outcome, None);
    assert_ne!(runs[0].run.outcome, Some(Outcome::Pass));
}

#[test]
fn model_and_run_id_lifted() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    assert_eq!(runs[0].meta.model.as_deref(), Some("gpt-4.1-2025-04-14"));
    // Run id is config-scoped so references from different configs never collide once combined.
    assert_eq!(
        runs[0].run.id,
        "gaia_hf_open_deep_research_gpt4120250414_1744843595/task-aaaa"
    );
}

#[test]
fn content_free_turn_warns_but_does_not_fail() {
    let runs = hal::convert_str(HAL_DUMP).unwrap();
    let b = &runs[1];
    // task-bbbb: one real turn + one content-free turn.
    assert_eq!(b.run.steps.len(), 2);
    assert!(b.run.steps[1].inputs.is_none() && b.run.steps[1].outputs.is_none());
    assert!(
        b.warnings
            .iter()
            .any(|w| w.code == WarningCode::ContentAbsent),
        "a content-free turn raises a content-absent advisory"
    );
}
