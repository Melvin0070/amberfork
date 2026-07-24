//! TRAIL adapter tests: a real-shaped Patronus-SDK trace tree (the wire format of the
//! `patronus-ai/trail-benchmark` GAIA traces) becomes a canonical [`amberfork_model::Run`]. The
//! fixture exercises every mapping branch the adapter must handle:
//! - a nested `child_spans` tree flattened in pre-order, with `parent_idx` wired from the nesting;
//! - the semantic kind read from `span_attributes["openinference.span.kind"]`, NOT the wire
//!   `span_kind` (which every Patronus span reports as `"Internal"`);
//! - a `TOOL` span named by its `tool.name`, not its `span_name`;
//! - `input.value`/`output.value` honoring `*.mime_type` (JSON → field-diffable object, else text);
//! - Patronus/framework attributes (`pat.*`, `smolagents.*`) preserved to `attrs` and surfaced as
//!   an unmapped-attributes warning, while OpenInference vocabulary (`llm.*`, `openinference.*`)
//!   rides silently;
//! - a content-free span degrading to a metadata-only step plus a content-absent advisory;
//! - the source `span_id` retained in `attrs["otel.span_id"]` and the RFC3339 `timestamp` landing
//!   in `t_start`, so a later slice can resolve an error annotation's span id to a step.
//!
//! A `status_code` of `Error` on one span asserts the architecture rule: `outcome` is never
//! inferred from span status.

use amberfork_ingest::trail;
use amberfork_model::{Payload, StepKind, WarningCode};

/// A spec-faithful TRAIL/Patronus trace tree: a root `main` span (no OpenInference kind → Other)
/// with an `AGENT` child that itself has three children — an `LLM` call, a `TOOL` call, and a
/// content-free span. Attribute values are plain JSON (TRAIL does not use the OTLP `AnyValue` wire
/// shape). No GAIA question/answer content appears — real TRAIL traces embed gated GAIA data and
/// are never committed.
const TRAIL_TRACE: &str = r#"{
  "trace_id": "trace-gaia-0001",
  "spans": [
    {
      "span_id": "root0000",
      "parent_span_id": null,
      "span_name": "main",
      "span_kind": "Internal",
      "status_code": "Unset",
      "timestamp": "2025-03-19T16:40:46.830526Z",
      "duration": "PT24.688187S",
      "span_attributes": {
        "pat.app": "GAIA-Samples",
        "pat.project.id": "a69d64fc"
      },
      "child_spans": [
        {
          "span_id": "agent001",
          "parent_span_id": "root0000",
          "span_name": "CodeAgent.run",
          "span_kind": "Internal",
          "status_code": "Error",
          "timestamp": "2025-03-19T16:40:47.204950Z",
          "span_attributes": {
            "openinference.span.kind": "AGENT",
            "input.mime_type": "text/plain",
            "input.value": "Find the capital of France",
            "llm.token_count.total": 512,
            "smolagents.max_steps": 6,
            "pat.app": "GAIA-Samples"
          },
          "child_spans": [
            {
              "span_id": "llm00001",
              "parent_span_id": "agent001",
              "span_name": "LiteLLMModel.__call__",
              "span_kind": "Internal",
              "status_code": "Unset",
              "timestamp": "2025-03-19T16:40:47.300000Z",
              "span_attributes": {
                "openinference.span.kind": "LLM",
                "llm.model_name": "gpt-4o",
                "input.mime_type": "application/json",
                "input.value": "{\"messages\":[{\"role\":\"user\",\"content\":\"Find the capital of France\"}]}",
                "output.mime_type": "text/plain",
                "output.value": "Let me search."
              },
              "child_spans": []
            },
            {
              "span_id": "tool0001",
              "parent_span_id": "agent001",
              "span_name": "tool_call",
              "span_kind": "Internal",
              "status_code": "Unset",
              "timestamp": "2025-03-19T16:40:48.100000Z",
              "span_attributes": {
                "openinference.span.kind": "TOOL",
                "tool.name": "web_search",
                "input.mime_type": "application/json",
                "input.value": "{\"query\":\"capital of France\"}",
                "output.value": "Paris"
              },
              "child_spans": []
            },
            {
              "span_id": "empty001",
              "parent_span_id": "agent001",
              "span_name": "postprocess",
              "span_kind": "Internal",
              "status_code": "Unset",
              "timestamp": "2025-03-19T16:40:48.500000Z",
              "span_attributes": {
                "openinference.span.kind": "CHAIN"
              },
              "child_spans": []
            }
          ]
        }
      ]
    }
  ]
}"#;

#[test]
fn tree_flattens_pre_order_with_parents_and_the_trace_id_as_run_id() {
    let ingested = trail::from_trace_json_str(TRAIL_TRACE).unwrap();
    let run = &ingested.run;

    assert_eq!(run.id, "trace-gaia-0001");
    // Pre-order DFS: root, then its AGENT child, then that agent's three children in order.
    assert_eq!(run.steps.len(), 5);

    let names: Vec<&str> = run.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "main",
            "CodeAgent.run",
            "LiteLLMModel.__call__",
            "web_search", // named by tool.name, not its span_name "tool_call"
            "postprocess"
        ]
    );

    // Steps are indexed 0..n in the walk order; parent_idx comes from the tree nesting.
    let idxs: Vec<usize> = run.steps.iter().map(|s| s.idx).collect();
    assert_eq!(idxs, [0, 1, 2, 3, 4]);
    let parents: Vec<Option<usize>> = run.steps.iter().map(|s| s.parent_idx).collect();
    assert_eq!(parents, [None, Some(0), Some(1), Some(1), Some(1)]);

    // Semantic kind is the OpenInference attribute, never the wire span_kind ("Internal"). The
    // root has none (→ Other); CHAIN is outside the canonical four (→ Other).
    let kinds: Vec<StepKind> = run.steps.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            StepKind::Other,
            StepKind::Agent,
            StepKind::Llm,
            StepKind::Tool,
            StepKind::Other
        ]
    );

    // The architecture rule: a run's verdict is a user assertion, never inferred from span status —
    // the AGENT span's status_code "Error" must not become an Outcome.
    assert_eq!(run.outcome, None);
}

#[test]
fn content_honors_mime_and_provenance_is_retained() {
    let ingested = trail::from_trace_json_str(TRAIL_TRACE).unwrap();
    let steps = &ingested.run.steps;

    // AGENT: a text/plain input becomes a Text payload; no output was captured.
    assert_eq!(
        steps[1].inputs,
        Some(Payload::Text("Find the capital of France".to_string()))
    );
    assert_eq!(steps[1].outputs, None);

    // LLM: an application/json input parses into a field-diffable Object; text/plain output is Text.
    match steps[2].inputs.as_ref().expect("llm input present") {
        Payload::Object(map) => assert!(map.contains_key("messages")),
        other => panic!("expected an Object input payload, got {other:?}"),
    }
    assert_eq!(
        steps[2].outputs,
        Some(Payload::Text("Let me search.".to_string()))
    );

    // The source span id is retained for annotation → step resolution, and the RFC3339 timestamp
    // lands natively in t_start (display-only, never an alignment signal).
    assert_eq!(
        steps[3].attrs.get("otel.span_id").and_then(|v| v.as_str()),
        Some("tool0001")
    );
    assert_eq!(
        steps[0].t_start.as_deref(),
        Some("2025-03-19T16:40:46.830526Z")
    );
    // Duration is preserved as display-only provenance.
    assert_eq!(
        steps[0].attrs.get("otel.duration").and_then(|v| v.as_str()),
        Some("PT24.688187S")
    );
}

#[test]
fn foreign_attrs_are_flagged_and_openinference_vocab_rides_silently() {
    let ingested = trail::from_trace_json_str(TRAIL_TRACE).unwrap();

    // The AGENT span carries pat.* and smolagents.* (foreign) plus llm.* (known). Only the foreign
    // ones are named in a single unmapped-attributes warning for that step.
    let agent_warning = ingested
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::UnmappedAttributes && w.msg.contains("CodeAgent.run"))
        .expect("agent step raises an unmapped-attributes warning");
    assert!(agent_warning.msg.contains("pat.app"));
    assert!(agent_warning.msg.contains("smolagents.max_steps"));
    assert!(
        !agent_warning.msg.contains("llm.token_count"),
        "known OpenInference vocabulary must not be flagged as foreign"
    );

    // Both known and foreign attributes are still preserved to attrs — nothing is dropped.
    let agent = &ingested.run.steps[1];
    assert!(agent.attrs.contains_key("llm.token_count.total"));
    assert!(agent.attrs.contains_key("smolagents.max_steps"));
    assert!(agent.attrs.contains_key("pat.app"));
}

#[test]
fn content_free_span_degrades_to_metadata_only_with_an_advisory() {
    let ingested = trail::from_trace_json_str(TRAIL_TRACE).unwrap();

    let postprocess = &ingested.run.steps[4];
    assert_eq!(postprocess.inputs, None);
    assert_eq!(postprocess.outputs, None);
    assert!(
        ingested
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::ContentAbsent && w.msg.contains("postprocess")),
        "a content-free span raises a content-absent advisory"
    );
}

#[test]
fn normalized_run_round_trips_through_the_canonical_loader() {
    // The canonical guard: a TRAIL-normalized run must re-serialize and re-load through the plain
    // JSON loader unchanged — proof the adapter lands squarely in the canonical model.
    let ingested = trail::from_trace_json_str(TRAIL_TRACE).unwrap();
    let json = serde_json::to_string(&ingested.run).expect("run serializes");
    let reloaded = amberfork_ingest::from_json_str(&json).expect("canonical reload");
    assert_eq!(reloaded.run, ingested.run);
}

#[test]
fn a_trace_with_no_spans_yields_a_run_with_no_steps() {
    let ingested = trail::from_trace_json_str(r#"{"trace_id": "empty", "spans": []}"#).unwrap();
    assert_eq!(ingested.run.id, "empty");
    assert!(ingested.run.steps.is_empty());
    assert!(ingested.warnings.is_empty());
}

#[test]
fn malformed_json_is_a_parse_error() {
    let err = trail::from_trace_json_str("{not json").expect_err("malformed input must fail");
    assert!(matches!(err, amberfork_ingest::IngestError::Parse { .. }));
}

// --- Gold annotations (S2) ------------------------------------------------------------------
//
// A TRAIL error-annotation file (`processed_annotations_gaia/<id>.json`) locates each decisive
// error at a span id. These tests reuse the `TRAIL_TRACE` fixture above so the span ids resolve
// against a real normalized run: `agent001` → step 1, `tool0001` → step 3, and so on. One error
// points at a span id the run does not contain, and one carries an out-of-vocabulary impact.

/// A spec-faithful annotation for `TRAIL_TRACE`: three errors that resolve (agent, tool, root),
/// one that does not (`ghost999`), and one with an unexpected impact. The `scores` block mirrors
/// the real files and must be ignored, not fail the parse.
const TRAIL_ANNOTATIONS: &str = r#"{
  "trace_id": "trace-gaia-0001",
  "errors": [
    {
      "category": "Instruction Non-compliance",
      "location": "agent001",
      "evidence": "missing <end_plan> tag",
      "description": "plan not terminated as instructed",
      "impact": "LOW"
    },
    {
      "category": "Poor Information Retrieval",
      "location": "tool0001",
      "evidence": "tool returned an unrelated result",
      "description": "search did not match the query",
      "impact": "MEDIUM"
    },
    {
      "category": "Goal Deviation",
      "location": "root0000",
      "evidence": "drifted from the task",
      "description": "top-level goal abandoned",
      "impact": "HIGH"
    },
    {
      "category": "Tool-related",
      "location": "ghost999",
      "evidence": "references a span not in this run",
      "description": "unresolvable location",
      "impact": "CRITICAL"
    }
  ],
  "scores": [
    { "reliability_score": 3, "overall": 3.5 }
  ]
}"#;

#[test]
fn annotations_parse_with_typed_impact_and_file_order() {
    let gold = trail::annotations_from_json_str(TRAIL_ANNOTATIONS).unwrap();

    assert_eq!(gold.trace_id, "trace-gaia-0001");
    assert_eq!(gold.errors.len(), 4);

    // File order is preserved, category is carried, and the closed impact vocabulary is typed —
    // with an out-of-vocabulary value kept losslessly rather than dropped or failing the parse.
    let impacts: Vec<&trail::Impact> = gold.errors.iter().map(|e| &e.impact).collect();
    assert_eq!(
        impacts,
        [
            &trail::Impact::Low,
            &trail::Impact::Medium,
            &trail::Impact::High,
            &trail::Impact::Other("CRITICAL".to_string()),
        ]
    );
    assert_eq!(gold.errors[0].category, "Instruction Non-compliance");
    assert_eq!(gold.errors[1].location, "tool0001");
}

#[test]
fn resolve_maps_span_ids_to_step_indices_and_flags_the_unresolvable() {
    let run = trail::from_trace_json_str(TRAIL_TRACE).unwrap().run;
    let gold = trail::annotations_from_json_str(TRAIL_ANNOTATIONS).unwrap();

    let resolved = gold.resolve(&run);
    assert_eq!(resolved.len(), 4);

    // Each error's span-located gold resolves to the step index the adapter assigned it: agent001
    // is step 1, tool0001 is step 3, root0000 is step 0. `ghost999` is absent from the run, so it
    // resolves to None — data, per protocol rule 4, never a silent drop.
    let steps: Vec<Option<usize>> = resolved.iter().map(|g| g.step).collect();
    assert_eq!(steps, [Some(1), Some(3), Some(0), None]);

    // Category and impact ride through resolution for later per-category coverage.
    assert_eq!(resolved[1].category, "Poor Information Retrieval");
    assert_eq!(resolved[2].impact, trail::Impact::High);
    assert_eq!(resolved[3].location, "ghost999");
}

#[test]
fn a_clean_trace_has_empty_gold() {
    // A trace the annotators found no fault in has an empty `errors` array — a valid file, and a
    // run with no gold fork (a candidate reference, not a failing side).
    let gold =
        trail::annotations_from_json_str(r#"{"trace_id": "clean", "errors": [], "scores": []}"#)
            .unwrap();
    assert_eq!(gold.trace_id, "clean");
    assert!(gold.errors.is_empty());

    let run = trail::from_trace_json_str(TRAIL_TRACE).unwrap().run;
    assert!(gold.resolve(&run).is_empty());
}

#[test]
fn malformed_annotations_are_a_parse_error() {
    let err = trail::annotations_from_json_str("{not json").expect_err("malformed input must fail");
    assert!(matches!(err, amberfork_ingest::IngestError::Parse { .. }));
}

// --- Pairing metadata (S4a) -----------------------------------------------------------------
//
// Every TRAIL GAIA trace is a smolagents run that embeds its canonical GAIA `task_id` (a UUID)
// inside the harness spans' structured `logs`: `get_examples_to_answer` carries the loaded
// dataset row under `logs[].body["function.output"]`, and `answer_single_question` carries the
// specific example under `logs[].body["function.arguments"].example`. That id is the join key a
// same-task reference is matched on (issue #41). It sits *beside* the gated GAIA
// question/answer/annotator-steps, so `convert_str` lifts only the UUID into `TrailMeta` and
// never the content around it — the same "identity beside the run, never inside it" rule the
// `tape` adapter's `TapeMeta` holds.

/// A harness-bearing trace: `main` → `get_examples_to_answer` (the dataset load, task_id in
/// `function.output`) and `answer_single_question` (task_id in the answered `example`). The gated
/// fields carry distinctive `SECRET-*` markers, present ONLY in the harness log body, so any leak
/// into the trajectory is detectable.
const TRAIL_TRACE_WITH_TASK_ID: &str = r#"{
  "trace_id": "trace-gaia-meta",
  "spans": [
    {
      "span_id": "root0000",
      "parent_span_id": null,
      "span_name": "main",
      "span_kind": "Internal",
      "status_code": "Unset",
      "span_attributes": { "pat.app": "GAIA-Samples" },
      "child_spans": [
        {
          "span_id": "harness0",
          "parent_span_id": "root0000",
          "span_name": "get_examples_to_answer",
          "span_kind": "Internal",
          "status_code": "Unset",
          "span_attributes": {},
          "logs": [
            {
              "body": {
                "function.name": "get_examples_to_answer",
                "function.arguments": {
                  "answers_file": "output/validation/gaia.jsonl",
                  "eval_ds": "Dataset({ features: ['task_id', 'question'], num_rows: 1 })"
                },
                "function.output": [
                  {
                    "task_id": "11111111-2222-3333-4444-555555555555",
                    "question": "SECRET-HARNESS-QUESTION",
                    "true_answer": "SECRET-GOLD-ANSWER",
                    "Annotator Metadata": { "Steps": "SECRET-STEPS" }
                  }
                ]
              }
            }
          ],
          "child_spans": []
        },
        {
          "span_id": "answer00",
          "parent_span_id": "root0000",
          "span_name": "answer_single_question",
          "span_kind": "Internal",
          "status_code": "Unset",
          "span_attributes": {},
          "logs": [
            {
              "body": {
                "function.name": "answer_single_question",
                "function.arguments": {
                  "example": {
                    "task_id": "11111111-2222-3333-4444-555555555555",
                    "question": "SECRET-HARNESS-QUESTION"
                  }
                }
              }
            }
          ],
          "child_spans": []
        }
      ]
    }
  ]
}"#;

/// A trace whose only task-id source is the answered example under `function.arguments` — no
/// `function.output` dataset-load span — exercising the `answer_single_question` path alone.
const TRAIL_TRACE_EXAMPLE_ONLY: &str = r#"{
  "trace_id": "trace-gaia-example-only",
  "spans": [
    {
      "span_id": "root0000",
      "parent_span_id": null,
      "span_name": "main",
      "span_kind": "Internal",
      "status_code": "Unset",
      "span_attributes": {},
      "child_spans": [
        {
          "span_id": "answer00",
          "parent_span_id": "root0000",
          "span_name": "answer_single_question",
          "span_kind": "Internal",
          "status_code": "Unset",
          "span_attributes": {},
          "logs": [
            {
              "body": {
                "function.arguments": {
                  "example": { "task_id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" }
                }
              }
            }
          ],
          "child_spans": []
        }
      ]
    }
  ]
}"#;

#[test]
fn convert_str_extracts_the_gaia_task_id() {
    let converted = trail::convert_str(TRAIL_TRACE_WITH_TASK_ID).unwrap();
    assert_eq!(
        converted.meta.gaia_task_id.as_deref(),
        Some("11111111-2222-3333-4444-555555555555"),
    );
}

#[test]
fn the_task_id_can_come_from_the_answered_example_alone() {
    let converted = trail::convert_str(TRAIL_TRACE_EXAMPLE_ONLY).unwrap();
    assert_eq!(
        converted.meta.gaia_task_id.as_deref(),
        Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
    );
}

#[test]
fn a_trace_without_a_harness_span_has_no_task_id() {
    // The S1 fixture is a bare agent trace with no smolagents dataset-load harness, so there is no
    // task_id to lift — a trace we cannot join, which is data (protocol rule 4), never a failure.
    let converted = trail::convert_str(TRAIL_TRACE).unwrap();
    assert_eq!(converted.meta.gaia_task_id, None);
}

#[test]
fn only_the_task_id_is_lifted_never_the_gated_content_beside_it() {
    // The harness log body holds the GAIA question, true answer, and annotator steps next to the
    // task_id. Only the UUID may reach `TrailMeta`; the gated content must never ride into the
    // trajectory. The `SECRET-*` markers appear ONLY in that log body, so their absence from the
    // serialized run is the guard.
    let converted = trail::convert_str(TRAIL_TRACE_WITH_TASK_ID).unwrap();
    assert_eq!(
        converted.meta.gaia_task_id.as_deref(),
        Some("11111111-2222-3333-4444-555555555555"),
    );

    let serialized = serde_json::to_string(&converted.run).expect("run serializes");
    for gated in [
        "SECRET-HARNESS-QUESTION",
        "SECRET-GOLD-ANSWER",
        "SECRET-STEPS",
    ] {
        assert!(
            !serialized.contains(gated),
            "gated harness-log content {gated} leaked into the trajectory",
        );
    }
}

#[test]
fn convert_str_normalizes_the_trajectory_like_the_content_only_path() {
    // `convert_str` layers task-id extraction over the same normalization the content-only
    // `from_trace_json_str` does. The run and its warnings must be byte-identical — the meta seam
    // adds a join key beside the run and changes nothing inside it.
    let converted = trail::convert_str(TRAIL_TRACE).unwrap();
    let ingested = trail::from_trace_json_str(TRAIL_TRACE).unwrap();
    assert_eq!(converted.run, ingested.run);
    assert_eq!(converted.warnings, ingested.warnings);
}
