//! TapeAgents adapter tests: a raw tape (`metadata.task` + a `steps` array of typed nodes)
//! becomes a canonical reference run, the step body survives as a field-diffable object, the
//! run-level outcome is decided *honestly* from result-vs-gold (not hardcoded), and — the
//! canonical guard — the converted run re-loads cleanly through the plain-JSON loader.

use amberfork_ingest::from_json_str;
use amberfork_ingest::tape;
use amberfork_model::{Outcome, Payload, StepKind};

/// A passing tape: the produced `result` matches the GAIA `Final answer`. Exercises every step
/// shape the converter must handle — a leading node with no `agent` (name is the bare kind), agent
/// nodes with distinct kinds, and an agent node with no `kind` at all (defaults to `step`). Every
/// node carries content, as real tapes do, so the run round-trips through the canonical loader
/// without a content-absent advisory.
const PASS_TAPE: &str = r#"{
  "metadata": {
    "task": {
      "Question": "How many r's are in strawberry?",
      "task_id": "gaia-task-42",
      "Final answer": "3"
    },
    "result": "3"
  },
  "steps": [
    { "kind": "question", "content": "How many r's are in strawberry?", "filename": null },
    { "kind": "reasoning_thought", "reasoning": "Count the letters.", "metadata": { "agent": "web_agent" } },
    { "kind": "search_action", "query": "strawberry spelling", "source": "wiki", "metadata": { "agent": "web_agent" } },
    { "kind": "set_next_node", "next_node": "act", "metadata": { "agent": "web_agent" } },
    { "reasoning": "done", "metadata": { "agent": "web_agent" } }
  ]
}"#;

#[test]
fn passing_tape_converts_to_canonical_reference_run() {
    let converted = tape::convert_str(PASS_TAPE, "l1_task007").unwrap();

    // Run identity: the stem is folded into the id, the task is the GAIA question, and the
    // outcome is Pass because the tape's result matches the gold Final answer.
    assert_eq!(converted.run.id, "tape_l1_task007");
    assert_eq!(
        converted.run.task.as_deref(),
        Some("How many r's are in strawberry?")
    );
    assert_eq!(converted.run.outcome, Some(Outcome::Pass));

    // Every tape node becomes one agent-kind step; the tape's own node kind rides in the name,
    // prefixed by the acting agent when one is annotated (bare kind when not).
    let names: Vec<&str> = converted
        .run
        .steps
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "question",
            "web_agent:reasoning_thought",
            "web_agent:search_action",
            "web_agent:set_next_node",
            "web_agent:step",
        ]
    );
    assert!(
        converted
            .run
            .steps
            .iter()
            .all(|s| s.kind == StepKind::Agent)
    );

    // The node body survives as a field-diffable object, not a stringified blob: the leading
    // `question` node keeps both of its fields.
    let Some(Payload::Object(body)) = &converted.run.steps[0].outputs else {
        panic!("expected an object payload for the question node");
    };
    assert_eq!(
        body.get("content").and_then(|v| v.as_str()),
        Some("How many r's are in strawberry?")
    );
    assert!(body.contains_key("filename"));
    // `kind` and `metadata` are consumed into the step's identity, never left in the body.
    assert!(!body.contains_key("kind"));
    assert!(!body.contains_key("metadata"));

    // Tape steps carry no separate inputs.
    assert!(converted.run.steps.iter().all(|s| s.inputs.is_none()));

    // The pairing metadata lands beside the run, never inside it.
    assert_eq!(converted.meta.task_id.as_deref(), Some("gaia-task-42"));
    assert_eq!(converted.meta.final_answer, "3");
    assert_eq!(converted.meta.result, "3");
    assert!(converted.meta.is_success());
}

#[test]
fn bookkeeping_only_node_has_no_outputs() {
    // A node whose only fields are `kind` + `metadata` has an empty body: it becomes a named step
    // with no outputs at all, so a contentless bookkeeping node never masquerades as content.
    let tape = r#"{
      "metadata": { "task": { "task_id": "t", "Final answer": "x" }, "result": "x" },
      "steps": [ { "kind": "stop", "metadata": { "agent": "web_agent" } } ]
    }"#;
    let converted = tape::convert_str(tape, "l1_task008").unwrap();
    assert_eq!(converted.run.steps[0].name, "web_agent:stop");
    assert_eq!(converted.run.steps[0].outputs, None);
}

#[test]
fn tape_whose_result_misses_the_gold_answer_is_not_a_success() {
    let tape = PASS_TAPE.replace("\"result\": \"3\"", "\"result\": \"42\"");
    let converted = tape::convert_str(&tape, "l1_task007").unwrap();

    // A reference that did not actually solve the task is honestly a failure, and the pairing
    // filter can see it — so it is never used as a "good" reference.
    assert!(!converted.meta.is_success());
    assert_eq!(converted.run.outcome, Some(Outcome::Fail));
}

#[test]
fn success_check_normalizes_whitespace_and_case() {
    // GAIA answers are graded after trimming and case-folding (the spike's .strip().lower()).
    let tape = PASS_TAPE
        .replace("\"Final answer\": \"3\"", "\"Final answer\": \"Paris\"")
        .replace("\"result\": \"3\"", "\"result\": \"  paris \"");
    let converted = tape::convert_str(&tape, "l1_task007").unwrap();
    assert!(converted.meta.is_success());
    assert_eq!(converted.run.outcome, Some(Outcome::Pass));
}

#[test]
fn non_object_task_metadata_degrades_gracefully() {
    // Some tapes carry a non-dict `task` (the spike guarded with isinstance): there is then no
    // question and no task_id, but conversion still succeeds — it does not panic or error.
    let tape = r#"{
      "metadata": { "task": "unstructured", "result": "" },
      "steps": [ { "kind": "question", "content": "hi" } ]
    }"#;
    let converted = tape::convert_str(tape, "l1_task099").unwrap();
    assert_eq!(converted.run.task, None);
    assert_eq!(converted.meta.task_id, None);
    assert_eq!(converted.run.steps.len(), 1);
}

#[test]
fn converted_run_roundtrips_through_the_canonical_loader() {
    // The canonical guard for this adapter: what it emits must be valid canonical input.
    // Serialize the converted run and re-load it through the plain-JSON loader — same run, zero
    // warnings (a field-diffable object body must survive the round trip intact).
    let converted = tape::convert_str(PASS_TAPE, "l1_task007").unwrap();
    let json = serde_json::to_string(&converted.run).unwrap();
    let reloaded = from_json_str(&json).unwrap();
    assert_eq!(reloaded.run, converted.run);
    assert!(
        reloaded.warnings.is_empty(),
        "converted run should re-load clean, got: {:?}",
        reloaded.warnings
    );
}
