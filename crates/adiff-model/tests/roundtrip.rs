//! Round-trip and semantic tests against the public contract in `docs/trace-format.md`.
//!
//! The canonical example there *is* the spec, so it is embedded verbatim below. If the model
//! ever disagrees with the documented format, these tests go red first.

use adiff_model::{Edge, Outcome, Payload, Run, SchemaVersion, Step, StepKind};

/// The exact canonical example from `docs/trace-format.md` (## Shape).
const TRACE_FORMAT_EXAMPLE: &str = r#"{
  "schema_version": "0.1",
  "id": "refund-triage_2026-07-07_bad",
  "task": "refund-triage #4512",
  "outcome": "fail",
  "steps": [
    {
      "idx": 0,
      "kind": "llm",
      "name": "planner",
      "inputs": { "messages": [{ "role": "user", "content": "Handle refund for order 8841" }] },
      "outputs": { "content": "I'll look up the order first." },
      "attrs": { "model": "claude-sonnet-5" },
      "t_start": null,
      "t_end": null,
      "parent_idx": null
    },
    {
      "idx": 1,
      "kind": "tool",
      "name": "lookup_order",
      "inputs": { "order_id": "8841" },
      "outputs": { "status": "shipped", "total": 129.0 },
      "attrs": {},
      "parent_idx": 0
    }
  ],
  "edges": [[0, 1]]
}"#;

#[test]
fn trace_format_example_parses_with_expected_semantics() {
    let run: Run = serde_json::from_str(TRACE_FORMAT_EXAMPLE).unwrap();

    assert_eq!(run.schema_version, SchemaVersion::current());
    assert_eq!(run.id, "refund-triage_2026-07-07_bad");
    assert_eq!(run.task.as_deref(), Some("refund-triage #4512"));
    assert_eq!(run.outcome, Some(Outcome::Fail));
    assert_eq!(run.steps.len(), 2);
    assert_eq!(run.edges, Some(vec![Edge(0, 1)]));

    let planner = &run.steps[0];
    assert_eq!(planner.idx, 0);
    assert_eq!(planner.kind, StepKind::Llm);
    assert_eq!(planner.name, "planner");
    assert_eq!(planner.parent_idx, None);
    // An object payload must land in the Object variant so it gets field-level diffing.
    assert!(matches!(planner.outputs, Some(Payload::Object(_))));
    assert_eq!(
        planner.attrs.get("model").and_then(|v| v.as_str()),
        Some("claude-sonnet-5")
    );

    let lookup = &run.steps[1];
    assert_eq!(lookup.kind, StepKind::Tool);
    assert_eq!(lookup.parent_idx, Some(0));
    assert!(lookup.attrs.is_empty());
}

#[test]
fn trace_format_example_roundtrips_idempotently() {
    // parse -> serialize -> parse must yield an equal struct. We omit `null`/empty fields on
    // re-emit, so this asserts the stronger *struct* invariant rather than byte identity.
    let run: Run = serde_json::from_str(TRACE_FORMAT_EXAMPLE).unwrap();
    let reserialized = serde_json::to_string(&run).unwrap();
    let run2: Run = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(run, run2);
}

#[test]
fn minimal_step_is_accepted() {
    // Per the contract: the minimal valid step is idx + kind + name + one of inputs/outputs.
    let json = r#"{
      "schema_version": "0.1",
      "id": "minimal",
      "steps": [{ "idx": 0, "kind": "other", "name": "x", "outputs": "done" }]
    }"#;
    let run: Run = serde_json::from_str(json).unwrap();
    assert_eq!(run.task, None);
    assert_eq!(run.outcome, None);
    assert_eq!(run.edges, None);
    let step = &run.steps[0];
    assert_eq!(step.kind, StepKind::Other);
    assert_eq!(step.inputs, None);
    assert_eq!(step.outputs, Some(Payload::Text("done".to_string())));
}

#[test]
fn payload_text_and_object_are_distinguished() {
    // A JSON string becomes Text (text diffing); a JSON object becomes Object (field diffing).
    let text: Payload = serde_json::from_str(r#""hello""#).unwrap();
    assert_eq!(text, Payload::Text("hello".to_string()));

    let object: Payload = serde_json::from_str(r#"{"k": 1}"#).unwrap();
    assert!(matches!(object, Payload::Object(_)));
}

#[test]
fn payload_other_preserves_non_stringy_shapes() {
    // Arrays/numbers must round-trip verbatim rather than fail the parse.
    let array: Payload = serde_json::from_str("[1, 2, 3]").unwrap();
    assert!(matches!(array, Payload::Other(_)));
    let reserialized = serde_json::to_string(&array).unwrap();
    assert_eq!(reserialized, "[1,2,3]");
}

#[test]
fn all_step_kinds_and_outcomes_roundtrip() {
    for (kind, wire) in [
        (StepKind::Llm, "\"llm\""),
        (StepKind::Tool, "\"tool\""),
        (StepKind::Agent, "\"agent\""),
        (StepKind::Other, "\"other\""),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), wire);
        assert_eq!(serde_json::from_str::<StepKind>(wire).unwrap(), kind);
    }
    for (outcome, wire) in [
        (Outcome::Pass, "\"pass\""),
        (Outcome::Fail, "\"fail\""),
        (Outcome::Unknown, "\"unknown\""),
    ] {
        assert_eq!(serde_json::to_string(&outcome).unwrap(), wire);
        assert_eq!(serde_json::from_str::<Outcome>(wire).unwrap(), outcome);
    }
}

#[test]
fn edges_serialize_as_pairs() {
    let step = Step {
        idx: 0,
        kind: StepKind::Llm,
        name: "a".to_string(),
        inputs: None,
        outputs: Some(Payload::Text("hi".to_string())),
        attrs: serde_json::Map::new(),
        t_start: None,
        t_end: None,
        parent_idx: None,
    };
    let run = Run {
        schema_version: SchemaVersion::current(),
        id: "e".to_string(),
        task: None,
        outcome: None,
        steps: vec![step],
        edges: Some(vec![Edge(0, 1), Edge(1, 2)]),
    };
    let json = serde_json::to_string(&run).unwrap();
    assert!(json.contains(r#""edges":[[0,1],[1,2]]"#), "got: {json}");
}
