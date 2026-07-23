//! OpenInference adapter tests: a real-shaped OTLP/JSON span export (Arize Phoenix / LangChain
//! instrumentation) becomes canonical [`amberfork_model::Run`]s. The fixture exercises every
//! mapping branch the adapter must handle:
//! - span kinds `AGENT`/`LLM`/`TOOL`/`CHAIN` → the canonical four (`CHAIN` folds to `other`);
//! - a `TOOL` span named by its `tool.name`, not the span name;
//! - `input.value`/`output.value` honoring `*.mime_type` (JSON → field-diffable object, else text);
//! - a foreign, non-OpenInference attribute preserved to `attrs` and surfaced as an unmapped warning;
//! - a content-free span degrading to a metadata-only step plus a content-absent advisory;
//! - spans emitted out of start-time order and split across two traces, re-ordered by start time
//!   and grouped into one run per `traceId`.
//!
//! The spans deliberately appear out of order in the array (the LLM child precedes its AGENT
//! parent) so the test pins the start-time ordering, not the wire order. A `STATUS_CODE_ERROR` on
//! the LLM span asserts the architecture rule: `outcome` is never inferred from span status.

use amberfork_ingest::from_json_str;
use amberfork_ingest::openinference;
use amberfork_model::{Payload, StepKind, WarningCode};

/// A spec-faithful OpenInference OTLP/JSON export: two traces, the first a four-span ReAct-style
/// agent run, the second a lone LLM span. Attribute values use the OTLP `AnyValue` wire shape
/// (`{"stringValue": …}`, `{"intValue": "42"}`, `{"boolValue": true}`).
const OTLP_EXPORT: &str = r#"{
  "resourceSpans": [
    {
      "resource": { "attributes": [ { "key": "service.name", "value": { "stringValue": "my-agent" } } ] },
      "scopeSpans": [
        {
          "scope": { "name": "openinference.instrumentation.langchain" },
          "spans": [
            {
              "traceId": "trace-aaaa",
              "spanId": "span-llm",
              "parentSpanId": "span-agent",
              "name": "ChatCompletion",
              "startTimeUnixNano": "1700000000000000200",
              "endTimeUnixNano": "1700000000000000250",
              "status": { "code": "STATUS_CODE_ERROR" },
              "attributes": [
                { "key": "openinference.span.kind", "value": { "stringValue": "LLM" } },
                { "key": "llm.model_name", "value": { "stringValue": "gpt-4o" } },
                { "key": "llm.token_count.total", "value": { "intValue": "42" } },
                { "key": "input.mime_type", "value": { "stringValue": "application/json" } },
                { "key": "input.value", "value": { "stringValue": "{\"messages\":[{\"role\":\"user\",\"content\":\"Find the capital of France\"}]}" } },
                { "key": "output.mime_type", "value": { "stringValue": "text/plain" } },
                { "key": "output.value", "value": { "stringValue": "Let me search." } }
              ]
            },
            {
              "traceId": "trace-aaaa",
              "spanId": "span-agent",
              "parentSpanId": "",
              "name": "AgentExecutor",
              "startTimeUnixNano": "1700000000000000100",
              "attributes": [
                { "key": "openinference.span.kind", "value": { "stringValue": "AGENT" } },
                { "key": "input.mime_type", "value": { "stringValue": "text/plain" } },
                { "key": "input.value", "value": { "stringValue": "Find the capital of France" } }
              ]
            },
            {
              "traceId": "trace-aaaa",
              "spanId": "span-tool",
              "parentSpanId": "span-agent",
              "name": "tool_call",
              "startTimeUnixNano": "1700000000000000300",
              "attributes": [
                { "key": "openinference.span.kind", "value": { "stringValue": "TOOL" } },
                { "key": "tool.name", "value": { "stringValue": "web_search" } },
                { "key": "input.mime_type", "value": { "stringValue": "application/json" } },
                { "key": "input.value", "value": { "stringValue": "{\"query\":\"capital of France\"}" } },
                { "key": "output.value", "value": { "stringValue": "Paris" } },
                { "key": "com.example.retry", "value": { "boolValue": true } }
              ]
            },
            {
              "traceId": "trace-aaaa",
              "spanId": "span-chain",
              "parentSpanId": "span-agent",
              "name": "postprocess",
              "startTimeUnixNano": "1700000000000000400",
              "attributes": [
                { "key": "openinference.span.kind", "value": { "stringValue": "CHAIN" } }
              ]
            }
          ]
        }
      ]
    },
    {
      "resource": { "attributes": [] },
      "scopeSpans": [
        {
          "scope": { "name": "openinference" },
          "spans": [
            {
              "traceId": "trace-bbbb",
              "spanId": "span-solo",
              "parentSpanId": "",
              "name": "solo",
              "startTimeUnixNano": "1700000000000009999",
              "attributes": [
                { "key": "openinference.span.kind", "value": { "stringValue": "LLM" } },
                { "key": "output.value", "value": { "stringValue": "hi" } }
              ]
            }
          ]
        }
      ]
    }
  ]
}"#;

#[test]
fn otlp_export_groups_into_one_run_per_trace_ordered_by_start() {
    let runs = openinference::from_otlp_json_str(OTLP_EXPORT).unwrap();

    // One run per traceId, ordered by each trace's earliest span start (trace-aaaa's 100 <
    // trace-bbbb's 9999), and the traceId becomes the canonical run id.
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].run.id, "trace-aaaa");
    assert_eq!(runs[1].run.id, "trace-bbbb");

    let agent_run = &runs[0].run;
    assert_eq!(agent_run.steps.len(), 4);

    // Span kind → canonical kind. CHAIN is not one of the four, so it folds to Other.
    let kinds: Vec<StepKind> = agent_run.steps.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            StepKind::Agent,
            StepKind::Llm,
            StepKind::Tool,
            StepKind::Other
        ]
    );

    // Structural identity: the TOOL step is named by `tool.name` (`web_search`), not its span
    // name (`tool_call`); every other step keeps its span name.
    let names: Vec<&str> = agent_run.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "AgentExecutor",
            "ChatCompletion",
            "web_search",
            "postprocess"
        ]
    );

    // Steps are re-indexed 0..n by start time even though the LLM child led the wire order,
    // and `parent_idx` is wired from `parentSpanId` regardless of that order.
    let idxs: Vec<usize> = agent_run.steps.iter().map(|s| s.idx).collect();
    assert_eq!(idxs, [0, 1, 2, 3]);
    let parents: Vec<Option<usize>> = agent_run.steps.iter().map(|s| s.parent_idx).collect();
    assert_eq!(parents, [None, Some(0), Some(0), Some(0)]);

    // The architecture rule: a run's verdict is a user assertion, never inferred from span
    // status — the LLM span's STATUS_CODE_ERROR must not become an Outcome.
    assert_eq!(agent_run.outcome, None);
}

#[test]
fn content_honors_mime_and_attrs_carry_the_rest() {
    let runs = openinference::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let steps = &runs[0].run.steps;

    // AGENT: a text/plain input becomes a Text payload; no output was captured.
    assert_eq!(
        steps[0].inputs,
        Some(Payload::Text("Find the capital of France".to_string()))
    );
    assert_eq!(steps[0].outputs, None);

    // LLM: an application/json input becomes a field-diffable object; a text/plain output is text.
    let Some(Payload::Object(input)) = &steps[1].inputs else {
        panic!("expected a JSON object payload for the LLM input");
    };
    assert!(input.contains_key("messages"));
    assert_eq!(
        steps[1].outputs,
        Some(Payload::Text("Let me search.".to_string()))
    );

    // Non-content OpenInference attributes ride in `attrs`, typed: the model name stays a string,
    // the token count decodes from its OTLP string-encoded int into a JSON number.
    assert_eq!(
        steps[1]
            .attrs
            .get("llm.model_name")
            .and_then(|v| v.as_str()),
        Some("gpt-4o")
    );
    assert_eq!(
        steps[1]
            .attrs
            .get("llm.token_count.total")
            .and_then(serde_json::Value::as_i64),
        Some(42)
    );
    // Attributes consumed as canonical fields are lifted out, never left duplicated in `attrs`.
    assert!(!steps[1].attrs.contains_key("input.value"));
    assert!(!steps[1].attrs.contains_key("openinference.span.kind"));

    // TOOL: application/json input → object; the foreign attribute is preserved verbatim as a bool.
    let Some(Payload::Object(query)) = &steps[2].inputs else {
        panic!("expected a JSON object payload for the tool input");
    };
    assert_eq!(
        query.get("query").and_then(|v| v.as_str()),
        Some("capital of France")
    );
    assert_eq!(steps[2].outputs, Some(Payload::Text("Paris".to_string())));
    assert_eq!(
        steps[2].attrs.get("com.example.retry"),
        Some(&serde_json::Value::Bool(true))
    );
}

#[test]
fn foreign_attribute_raises_an_unmapped_warning() {
    let runs = openinference::from_otlp_json_str(OTLP_EXPORT).unwrap();

    let unmapped: Vec<&str> = runs[0]
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::UnmappedAttributes)
        .map(|w| w.msg.as_str())
        .collect();

    // Exactly the foreign key is flagged — recognized OpenInference attributes (model name, token
    // count) are a deliberate part of the mapping and must not trip the warning.
    assert_eq!(unmapped.len(), 1);
    assert!(unmapped[0].contains("com.example.retry"));
    assert!(unmapped[0].contains("web_search"));
    assert!(!unmapped[0].contains("llm.model_name"));
    assert!(!unmapped[0].contains("llm.token_count"));
}

#[test]
fn contentless_span_becomes_a_metadata_only_step() {
    let runs = openinference::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let postprocess = &runs[0].run.steps[3];

    // The CHAIN span carried no input/output value: it becomes a named step with no content.
    assert_eq!(postprocess.inputs, None);
    assert_eq!(postprocess.outputs, None);

    let absent: Vec<&str> = runs[0]
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::ContentAbsent)
        .map(|w| w.msg.as_str())
        .collect();
    assert_eq!(absent.len(), 1);
    assert!(absent[0].contains("postprocess"));
}

#[test]
fn converted_run_roundtrips_through_the_canonical_loader() {
    // The canonical guard for every source adapter: what it emits must be valid canonical input.
    // Serialize the normalized run and re-load it through the plain-JSON loader — same run back.
    let runs = openinference::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let json = serde_json::to_string(&runs[0].run).unwrap();
    let reloaded = from_json_str(&json).unwrap();
    assert_eq!(reloaded.run, runs[0].run);
}

#[test]
fn malformed_json_is_a_parse_error_not_a_panic() {
    let err = openinference::from_otlp_json_str("{ not json").unwrap_err();
    assert!(matches!(err, amberfork_ingest::IngestError::Parse { .. }));
}

#[test]
fn an_export_with_no_spans_yields_no_runs() {
    let runs = openinference::from_otlp_json_str(r#"{ "resourceSpans": [] }"#).unwrap();
    assert!(runs.is_empty());
}
