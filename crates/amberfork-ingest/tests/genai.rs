//! Native OTel GenAI adapter tests: a real-shaped OTLP/JSON span export using `gen_ai.*`
//! semantic-convention attributes becomes canonical [`amberfork_model::Run`]s. Mirrors
//! `tests/openinference.rs`'s discipline over the different vocabulary and its
//! kind-conditional content carrier:
//! - `gen_ai.operation.name` `invoke_agent`/`chat`/`execute_tool`/`embeddings` → the canonical
//!   four (`embeddings` folds to `other`, same as OpenInference's non-canonical kinds);
//! - an AGENT span named by `gen_ai.agent.name`, a TOOL span by `gen_ai.tool.name`, not the span
//!   name;
//! - `gen_ai.input.messages`/`output.messages` (LLM/AGENT spans) vs.
//!   `gen_ai.tool.call.arguments`/`tool.call.result` (TOOL spans) — the carrier is kind-conditional;
//! - a structured array carrier and a pre-serialized JSON-string carrier both decode, and both
//!   wrap into a field-diffable `{"messages": [...]}` object; a non-JSON string stays text;
//! - a foreign, non-`gen_ai.*` attribute preserved to `attrs` and surfaced as an unmapped warning;
//! - a content-free span degrading to a metadata-only step plus a content-absent advisory;
//! - spans emitted out of start-time order and split across two traces, re-ordered by start time
//!   and grouped into one run per `traceId`.
//!
//! The spans deliberately appear out of order in the array (the LLM child precedes its AGENT
//! parent) so the test pins the start-time ordering, not the wire order. A `STATUS_CODE_ERROR` on
//! the LLM span asserts the architecture rule: `outcome` is never inferred from span status.

use amberfork_ingest::from_json_str;
use amberfork_ingest::genai;
use amberfork_model::{Payload, StepKind, WarningCode};

/// A spec-faithful native OTel GenAI OTLP/JSON export: two traces, the first a four-span
/// agent→llm/tool/embedding run, the second a lone LLM span. Attribute values use the OTLP
/// `AnyValue` wire shape (`{"stringValue": …}`, `{"intValue": "42"}`, `{"arrayValue": …}`).
const OTLP_EXPORT: &str = r#"{
  "resourceSpans": [
    {
      "resource": { "attributes": [ { "key": "service.name", "value": { "stringValue": "my-agent" } } ] },
      "scopeSpans": [
        {
          "scope": { "name": "opentelemetry.instrumentation.genai" },
          "spans": [
            {
              "traceId": "trace-aaaa",
              "spanId": "span-llm",
              "parentSpanId": "span-agent",
              "name": "ChatCompletion",
              "startTimeUnixNano": "1700000000000000200",
              "status": { "code": "STATUS_CODE_ERROR" },
              "attributes": [
                { "key": "gen_ai.operation.name", "value": { "stringValue": "chat" } },
                { "key": "gen_ai.usage.input_tokens", "value": { "intValue": "42" } },
                { "key": "gen_ai.input.messages", "value": { "arrayValue": { "values": [
                  { "kvlistValue": { "values": [
                    { "key": "role", "value": { "stringValue": "user" } },
                    { "key": "content", "value": { "stringValue": "Find the capital of France" } }
                  ] } }
                ] } } },
                { "key": "gen_ai.output.messages", "value": { "stringValue": "Let me search." } }
              ]
            },
            {
              "traceId": "trace-aaaa",
              "spanId": "span-agent",
              "parentSpanId": "",
              "name": "AgentExecutor",
              "startTimeUnixNano": "1700000000000000100",
              "attributes": [
                { "key": "gen_ai.operation.name", "value": { "stringValue": "invoke_agent" } },
                { "key": "gen_ai.agent.name", "value": { "stringValue": "Coordinator" } },
                { "key": "gen_ai.input.messages", "value": { "stringValue": "[{\"role\":\"user\",\"content\":\"Find the capital of France\"}]" } }
              ]
            },
            {
              "traceId": "trace-aaaa",
              "spanId": "span-tool",
              "parentSpanId": "span-agent",
              "name": "tool_call",
              "startTimeUnixNano": "1700000000000000300",
              "attributes": [
                { "key": "gen_ai.operation.name", "value": { "stringValue": "execute_tool" } },
                { "key": "gen_ai.tool.name", "value": { "stringValue": "web_search" } },
                { "key": "gen_ai.tool.call.arguments", "value": { "kvlistValue": { "values": [
                  { "key": "query", "value": { "stringValue": "capital of France" } }
                ] } } },
                { "key": "gen_ai.tool.call.result", "value": { "stringValue": "Paris" } },
                { "key": "com.example.retry", "value": { "boolValue": true } }
              ]
            },
            {
              "traceId": "trace-aaaa",
              "spanId": "span-embed",
              "parentSpanId": "span-agent",
              "name": "embed_batch",
              "startTimeUnixNano": "1700000000000000400",
              "attributes": [
                { "key": "gen_ai.operation.name", "value": { "stringValue": "embeddings" } }
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
          "scope": { "name": "gen_ai" },
          "spans": [
            {
              "traceId": "trace-bbbb",
              "spanId": "span-solo",
              "parentSpanId": "",
              "name": "solo",
              "startTimeUnixNano": "1700000000000009999",
              "attributes": [
                { "key": "gen_ai.operation.name", "value": { "stringValue": "chat" } },
                { "key": "gen_ai.output.messages", "value": { "arrayValue": { "values": [
                  { "kvlistValue": { "values": [
                    { "key": "role", "value": { "stringValue": "assistant" } },
                    { "key": "content", "value": { "stringValue": "hi" } }
                  ] } }
                ] } } }
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
    let runs = genai::from_otlp_json_str(OTLP_EXPORT).unwrap();

    // One run per traceId, ordered by each trace's earliest span start (trace-aaaa's 100 <
    // trace-bbbb's 9999), and the traceId becomes the canonical run id.
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].run.id, "trace-aaaa");
    assert_eq!(runs[1].run.id, "trace-bbbb");

    let agent_run = &runs[0].run;
    assert_eq!(agent_run.steps.len(), 4);

    // Operation name → canonical kind. `embeddings` is not one of the four, so it folds to Other.
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

    // Structural identity: the AGENT step is named by `gen_ai.agent.name` (`Coordinator`), the
    // TOOL step by `gen_ai.tool.name` (`web_search`) — neither keeps its span name; LLM and Other
    // steps do.
    let names: Vec<&str> = agent_run.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["Coordinator", "ChatCompletion", "web_search", "embed_batch"]
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
fn content_carrier_is_kind_conditional_and_attrs_carry_the_rest() {
    let runs = genai::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let steps = &runs[0].run.steps;

    // AGENT: `input.messages` arrived as a pre-serialized JSON string (an array) — it still
    // decodes and wraps into a field-diffable object; no output was captured.
    let Some(Payload::Object(agent_in)) = &steps[0].inputs else {
        panic!("expected a JSON object payload for the agent input");
    };
    assert!(agent_in.contains_key("messages"));
    assert_eq!(steps[0].outputs, None);

    // LLM: `input.messages` arrived as a native structured array — same wrap, no string parse
    // needed. `output.messages` is a plain non-JSON string, so it stays text.
    let Some(Payload::Object(llm_in)) = &steps[1].inputs else {
        panic!("expected a JSON object payload for the llm input");
    };
    assert!(llm_in.contains_key("messages"));
    assert_eq!(
        steps[1].outputs,
        Some(Payload::Text("Let me search.".to_string()))
    );
    // Recognized gen_ai attributes ride in `attrs`, typed: the OTLP string-encoded int decodes
    // into a JSON number.
    assert_eq!(
        steps[1]
            .attrs
            .get("gen_ai.usage.input_tokens")
            .and_then(serde_json::Value::as_i64),
        Some(42)
    );
    // Attributes consumed as canonical fields are lifted out, never left duplicated in `attrs`.
    assert!(!steps[1].attrs.contains_key("gen_ai.input.messages"));
    assert!(!steps[1].attrs.contains_key("gen_ai.operation.name"));

    // TOOL: the carrier switches to `tool.call.arguments`/`tool.call.result`, not
    // `input.messages`/`output.messages` — a structured object input, a plain-text result.
    let Some(Payload::Object(query)) = &steps[2].inputs else {
        panic!("expected a JSON object payload for the tool arguments");
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
fn a_structured_array_carrier_wraps_to_a_field_diffable_object() {
    let runs = genai::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let solo = &runs[1].run.steps[0];

    // `output.messages` arrived as a native OTLP array — wrapped as {"messages": [...]}, not left
    // as an un-field-diffable bare array (`Payload::Other`).
    let Some(Payload::Object(out)) = &solo.outputs else {
        panic!("expected a JSON object payload for a structured messages array");
    };
    let Some(serde_json::Value::Array(messages)) = out.get("messages") else {
        panic!("expected the wrapped array under the messages key");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(solo.inputs, None);
}

#[test]
fn foreign_attribute_raises_an_unmapped_warning() {
    let runs = genai::from_otlp_json_str(OTLP_EXPORT).unwrap();

    let unmapped: Vec<&str> = runs[0]
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::UnmappedAttributes)
        .map(|w| w.msg.as_str())
        .collect();

    // Exactly the foreign key is flagged — recognized gen_ai attributes (usage tokens) are a
    // deliberate part of the mapping and must not trip the warning.
    assert_eq!(unmapped.len(), 1);
    assert!(unmapped[0].contains("com.example.retry"));
    assert!(unmapped[0].contains("web_search"));
    assert!(!unmapped[0].contains("gen_ai.usage.input_tokens"));
}

#[test]
fn contentless_span_becomes_a_metadata_only_step() {
    let runs = genai::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let embed = &runs[0].run.steps[3];

    // The embeddings span carried no message/tool-call content: it becomes a named step with no
    // content.
    assert_eq!(embed.inputs, None);
    assert_eq!(embed.outputs, None);

    let absent: Vec<&str> = runs[0]
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::ContentAbsent)
        .map(|w| w.msg.as_str())
        .collect();
    assert_eq!(absent.len(), 1);
    assert!(absent[0].contains("embed_batch"));
}

#[test]
fn converted_run_roundtrips_through_the_canonical_loader() {
    // The canonical guard for every source adapter: what it emits must be valid canonical input.
    // Serialize the normalized run and re-load it through the plain-JSON loader — same run back.
    let runs = genai::from_otlp_json_str(OTLP_EXPORT).unwrap();
    let json = serde_json::to_string(&runs[0].run).unwrap();
    let reloaded = from_json_str(&json).unwrap();
    assert_eq!(reloaded.run, runs[0].run);
}

#[test]
fn malformed_json_is_a_parse_error_not_a_panic() {
    let err = genai::from_otlp_json_str("{ not json").unwrap_err();
    assert!(matches!(err, amberfork_ingest::IngestError::Parse { .. }));
}

#[test]
fn an_export_with_no_spans_yields_no_runs() {
    let runs = genai::from_otlp_json_str(r#"{ "resourceSpans": [] }"#).unwrap();
    assert!(runs.is_empty());
}
