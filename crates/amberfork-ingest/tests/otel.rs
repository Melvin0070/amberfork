//! OTLP/JSON export routing: one entry point that recognizes an export and picks the attribute
//! vocabulary ([`amberfork_ingest::openinference`] or [`amberfork_ingest::genai`]) from the spans
//! themselves, so a caller holding "some OTLP file" does not have to know which instrumentation
//! wrote it.
//!
//! The contract under test:
//! - an export is recognized by its top-level `resourceSpans`, and nothing else is;
//! - spans carrying `gen_ai.*` attributes select the native OTel GenAI vocabulary, spans carrying
//!   `openinference.*` select OpenInference;
//! - a mixed export goes with the majority of its spans, and a tie or an export with neither
//!   marker falls back to OpenInference (the older, more widely deployed convention);
//! - the routed result is exactly what calling the chosen adapter directly returns.

use amberfork_ingest::otel::{self, Vocabulary};
use amberfork_ingest::{genai, openinference};
use amberfork_model::StepKind;

fn export(spans: &[serde_json::Value]) -> String {
    serde_json::json!({ "resourceSpans": [ { "scopeSpans": [ { "spans": spans } ] } ] }).to_string()
}

fn span(id: &str, start: u64, attrs: &[(&str, &str)]) -> serde_json::Value {
    let attributes: Vec<serde_json::Value> = attrs
        .iter()
        .map(|(key, value)| serde_json::json!({ "key": key, "value": { "stringValue": value } }))
        .collect();
    serde_json::json!({
        "traceId": "trace-1", "spanId": id, "name": id,
        "startTimeUnixNano": start.to_string(), "attributes": attributes,
    })
}

fn genai_span(id: &str, start: u64) -> serde_json::Value {
    span(
        id,
        start,
        &[
            ("gen_ai.operation.name", "execute_tool"),
            ("gen_ai.tool.name", "search"),
        ],
    )
}

fn openinference_span(id: &str, start: u64) -> serde_json::Value {
    span(
        id,
        start,
        &[("openinference.span.kind", "TOOL"), ("tool.name", "search")],
    )
}

#[test]
fn an_export_is_recognized_by_resource_spans_and_nothing_else_is() {
    assert!(otel::looks_like_export(&export(&[])));
    assert!(!otel::looks_like_export(
        r#"{"schema_version":"0.1","id":"x","steps":[]}"#
    ));
    assert!(!otel::looks_like_export(r#"{"cassette_version":"0.1"}"#));
    assert!(!otel::looks_like_export(r#"[{"resourceSpans":[]}]"#));
    assert!(!otel::looks_like_export("{not json"));
}

#[test]
fn the_vocabulary_is_read_off_the_span_attributes() {
    assert_eq!(
        otel::detect_vocabulary(&export(&[genai_span("a", 1)])),
        Vocabulary::GenAi
    );
    assert_eq!(
        otel::detect_vocabulary(&export(&[openinference_span("a", 1)])),
        Vocabulary::OpenInference
    );
}

#[test]
fn a_mixed_export_goes_with_the_majority_and_a_tie_or_no_marker_is_openinference() {
    let mostly_genai = export(&[
        genai_span("a", 1),
        genai_span("b", 2),
        openinference_span("c", 3),
    ]);
    assert_eq!(otel::detect_vocabulary(&mostly_genai), Vocabulary::GenAi);

    let tie = export(&[genai_span("a", 1), openinference_span("b", 2)]);
    assert_eq!(otel::detect_vocabulary(&tie), Vocabulary::OpenInference);

    let unmarked = export(&[span("a", 1, &[("http.method", "POST")])]);
    assert_eq!(
        otel::detect_vocabulary(&unmarked),
        Vocabulary::OpenInference
    );
}

#[test]
fn routing_returns_what_the_chosen_adapter_returns() {
    let genai_export = export(&[genai_span("a", 1)]);
    let routed = otel::from_otlp_json_str(&genai_export).expect("a gen_ai export loads");
    let direct = genai::from_otlp_json_str(&genai_export).unwrap();
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].run, direct[0].run);
    // The gen_ai vocabulary names a tool span by `gen_ai.tool.name`; OpenInference would not.
    assert_eq!(routed[0].run.steps[0].kind, StepKind::Tool);
    assert_eq!(routed[0].run.steps[0].name, "search");

    let oi_export = export(&[openinference_span("a", 1)]);
    let routed = otel::from_otlp_json_str(&oi_export).expect("an OpenInference export loads");
    let direct = openinference::from_otlp_json_str(&oi_export).unwrap();
    assert_eq!(routed[0].run, direct[0].run);
    assert_eq!(routed[0].run.steps[0].kind, StepKind::Tool);
}
