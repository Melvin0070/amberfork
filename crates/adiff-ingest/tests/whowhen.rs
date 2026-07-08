//! Who&When adapter tests: the two annotation splits, key/casing normalization, the
//! string-encoded 0-indexed `mistake_step` (and its unusable variants), and the canonical
//! guard — a converted run must re-load cleanly through the plain-JSON loader.

use adiff_ingest::from_json_str;
use adiff_ingest::whowhen::{self, GoldStep, Split};
use adiff_model::{Outcome, Payload, StepKind};

/// Hand-Crafted split: history entries carry `role` (not `name`), the reference answer is under
/// the `groundtruth` key, and one agent name has the `Websurfer` casing drift.
const HAND_LOG: &str = r#"{
  "question": "Which national park has the tallest waterfall?",
  "question_ID": "hand-q-1",
  "history": [
    { "role": "human",        "content": "Which national park has the tallest waterfall?" },
    { "role": "Orchestrator", "content": "Delegating the search." },
    { "role": "Websurfer",    "content": "I found it is in Iceland." },
    { "role": "Assistant",    "content": "Final answer: Iceland." }
  ],
  "mistake_agent": "Websurfer",
  "mistake_step": "2",
  "groundtruth": "Yosemite National Park"
}"#;

/// Algorithm-Generated split: entries carry BOTH `name` and `role` (name must win), the
/// reference answer is under `ground_truth`, and `mistake_step` is the string `"1"`.
const ALGO_LOG: &str = r#"{
  "question": "Who painted the Mona Lisa?",
  "question_ID": "algo-q-9",
  "history": [
    { "role": "user",      "name": "human",     "content": "Who painted the Mona Lisa?" },
    { "role": "assistant", "name": "Websurfer", "content": "Searching the web..." },
    { "role": "assistant", "name": "Assistant", "content": "It was Michelangelo." }
  ],
  "mistake_agent": "Assistant",
  "mistake_step": "1",
  "ground_truth": "Leonardo da Vinci"
}"#;

#[test]
fn hand_crafted_log_converts_to_canonical_run() {
    let converted = whowhen::convert_str(HAND_LOG, Split::HandCrafted, "1").unwrap();

    // Run identity: slug carries the split, task is the question, and every Who&When log is a
    // failure by construction.
    assert_eq!(converted.run.id, "whowhen_hand_1");
    assert_eq!(
        converted.run.task.as_deref(),
        Some("Which national park has the tallest waterfall?")
    );
    assert_eq!(converted.run.outcome, Some(Outcome::Fail));

    // Every history entry becomes one agent step; `role` supplies the name; `Websurfer` is
    // normalized to `WebSurfer`.
    let names: Vec<&str> = converted
        .run
        .steps
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["human", "Orchestrator", "WebSurfer", "Assistant"]);
    assert!(
        converted
            .run
            .steps
            .iter()
            .all(|s| s.kind == StepKind::Agent)
    );
    assert_eq!(
        converted.run.steps[0].outputs,
        Some(Payload::Text(
            "Which national park has the tallest waterfall?".to_string()
        ))
    );

    // Gold: the annotation is kept beside the run, the string `"2"` resolves to an in-range
    // 0-indexed target, and the mistake agent gets the same casing fix as the step names.
    assert_eq!(converted.gold.split, Split::HandCrafted);
    assert_eq!(converted.gold.question_id, "hand-q-1");
    assert_eq!(converted.gold.gold_step, GoldStep::Valid(2));
    assert_eq!(converted.gold.mistake_agent.as_deref(), Some("WebSurfer"));
    assert_eq!(converted.gold.ground_truth, "Yosemite National Park");
}

#[test]
fn algorithm_generated_prefers_name_over_role() {
    let converted = whowhen::convert_str(ALGO_LOG, Split::AlgorithmGenerated, "9").unwrap();

    assert_eq!(converted.run.id, "whowhen_algo_9");
    // `name` wins over `role`: step 0 is "human" (name), not "user" (role).
    let names: Vec<&str> = converted
        .run
        .steps
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["human", "WebSurfer", "Assistant"]);

    // Reference answer read from the `ground_truth` key on this split.
    assert_eq!(converted.gold.ground_truth, "Leonardo da Vinci");
    assert_eq!(converted.gold.split, Split::AlgorithmGenerated);
    assert_eq!(converted.gold.gold_step, GoldStep::Valid(1));
    assert_eq!(converted.gold.question_id, "algo-q-9");
}

#[test]
fn out_of_range_mistake_step_is_unusable_not_valid() {
    // A blame index past the end of a 3-step trajectory is not a usable target; the raw value
    // is preserved for the sanity report rather than silently kept as `Valid`.
    let log = r#"{
      "question": "q", "question_ID": "x",
      "history": [
        { "role": "a", "content": "1" },
        { "role": "b", "content": "2" },
        { "role": "c", "content": "3" }
      ],
      "mistake_step": "9", "groundtruth": "gt"
    }"#;
    let converted = whowhen::convert_str(log, Split::HandCrafted, "0").unwrap();
    assert_eq!(converted.run.steps.len(), 3);
    assert_eq!(
        converted.gold.gold_step,
        GoldStep::Unusable("9".to_string())
    );
}

#[test]
fn absent_mistake_step_is_absent() {
    let log = r#"{
      "question": "q", "question_ID": "x",
      "history": [ { "role": "a", "content": "1" } ],
      "groundtruth": "gt"
    }"#;
    let converted = whowhen::convert_str(log, Split::HandCrafted, "0").unwrap();
    assert_eq!(converted.gold.gold_step, GoldStep::Absent);
}

#[test]
fn negative_and_nonnumeric_mistake_step_are_unusable() {
    let base = r#"{
      "question": "q", "question_ID": "x",
      "history": [ { "role": "a", "content": "1" }, { "role": "b", "content": "2" } ],
      "MISTAKE", "groundtruth": "gt"
    }"#;
    // "-1" is out of range for `usize` (Python's int("-1") slips past the >= n_steps check;
    // parsing to usize rejects it honestly).
    let neg = base.replace(r#""MISTAKE""#, r#""mistake_step": "-1""#);
    assert_eq!(
        whowhen::convert_str(&neg, Split::HandCrafted, "0")
            .unwrap()
            .gold
            .gold_step,
        GoldStep::Unusable("-1".to_string())
    );
    let text = base.replace(r#""MISTAKE""#, r#""mistake_step": "foo""#);
    assert_eq!(
        whowhen::convert_str(&text, Split::HandCrafted, "0")
            .unwrap()
            .gold
            .gold_step,
        GoldStep::Unusable("foo".to_string())
    );
}

#[test]
fn empty_content_becomes_no_output() {
    let log = r#"{
      "question": "q", "question_ID": "x",
      "history": [
        { "role": "a", "content": "" },
        { "role": "b", "content": "hi" }
      ],
      "mistake_step": "0", "groundtruth": "gt"
    }"#;
    let converted = whowhen::convert_str(log, Split::HandCrafted, "0").unwrap();
    assert_eq!(converted.run.steps[0].outputs, None);
    assert_eq!(
        converted.run.steps[1].outputs,
        Some(Payload::Text("hi".to_string()))
    );
}

#[test]
fn empty_name_falls_through_to_role() {
    // Empty string is falsy in the Python reference (`name or role`), so an empty `name` must
    // yield the `role`, not "".
    let log = r#"{
      "question": "q", "question_ID": "x",
      "history": [ { "name": "", "role": "orchestrator", "content": "x" } ],
      "mistake_step": "0", "groundtruth": "gt"
    }"#;
    let converted = whowhen::convert_str(log, Split::HandCrafted, "0").unwrap();
    assert_eq!(converted.run.steps[0].name, "orchestrator");
}

#[test]
fn converted_run_roundtrips_through_the_canonical_loader() {
    // The canonical guard for this adapter: what it emits must be valid canonical input. Serialize
    // the converted run and re-load it through the plain-JSON loader — same run, zero warnings.
    let converted = whowhen::convert_str(HAND_LOG, Split::HandCrafted, "1").unwrap();
    let json = serde_json::to_string(&converted.run).unwrap();
    let reloaded = from_json_str(&json).unwrap();
    assert_eq!(reloaded.run, converted.run);
    assert!(
        reloaded.warnings.is_empty(),
        "converted run should re-load clean, got: {:?}",
        reloaded.warnings
    );
}
