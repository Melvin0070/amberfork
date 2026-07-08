//! Loader behavior tests: clean parse, the two forgiving warnings, strictness, and I/O errors.

use amberfork_ingest::{IngestError, from_json_str, load_file};
use amberfork_model::{Outcome, Payload, StepKind, WarningCode};

const MINIMAL_TRACE: &str = r#"{
  "schema_version": "0.1",
  "id": "minimal",
  "outcome": "fail",
  "steps": [
    { "idx": 0, "kind": "llm", "name": "planner", "outputs": "hi", "attrs": {} }
  ]
}"#;

#[test]
fn clean_trace_loads_with_no_warnings() {
    let ingested = from_json_str(MINIMAL_TRACE).unwrap();
    assert!(ingested.warnings.is_empty());
    assert_eq!(ingested.run.id, "minimal");
    assert_eq!(ingested.run.outcome, Some(Outcome::Fail));
    let step = &ingested.run.steps[0];
    assert_eq!(step.kind, StepKind::Llm);
    assert_eq!(step.outputs, Some(Payload::Text("hi".to_string())));
}

#[test]
fn unmapped_step_fields_move_to_attrs_and_warn() {
    // `retries` and `latency_ms` are not canonical Step fields; they must be preserved into
    // attrs (not dropped) and reported once.
    let json = r#"{
      "schema_version": "0.1",
      "id": "extra",
      "steps": [{
        "idx": 0, "kind": "tool", "name": "web.search", "outputs": "ok",
        "attrs": { "model": "x" },
        "retries": 2, "latency_ms": 40
      }]
    }"#;
    let ingested = from_json_str(json).unwrap();

    let step = &ingested.run.steps[0];
    assert_eq!(step.attrs.get("retries").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(
        step.attrs.get("latency_ms").and_then(|v| v.as_i64()),
        Some(40)
    );
    assert_eq!(step.attrs.get("model").and_then(|v| v.as_str()), Some("x"));

    assert_eq!(ingested.warnings.len(), 1);
    let warning = &ingested.warnings[0];
    assert_eq!(warning.code, WarningCode::UnmappedAttributes);
    // Keys are reported in deterministic (sorted) order.
    assert!(
        warning.msg.contains("latency_ms, retries"),
        "got: {}",
        warning.msg
    );
}

#[test]
fn content_absent_step_warns_but_still_loads() {
    let json = r#"{
      "schema_version": "0.1",
      "id": "meta-only",
      "steps": [{ "idx": 0, "kind": "agent", "name": "orchestrator" }]
    }"#;
    let ingested = from_json_str(json).unwrap();

    assert_eq!(ingested.run.steps.len(), 1);
    assert_eq!(ingested.warnings.len(), 1);
    assert_eq!(ingested.warnings[0].code, WarningCode::ContentAbsent);
}

#[test]
fn run_level_unmapped_field_warns() {
    let json = r#"{
      "schema_version": "0.1",
      "id": "runextra",
      "provenance": "otel-collector-1",
      "steps": [{ "idx": 0, "kind": "llm", "name": "a", "outputs": "x" }]
    }"#;
    let ingested = from_json_str(json).unwrap();
    assert_eq!(ingested.warnings.len(), 1);
    assert_eq!(ingested.warnings[0].code, WarningCode::UnmappedAttributes);
    assert!(ingested.warnings[0].msg.contains("provenance"));
}

#[test]
fn malformed_json_is_a_parse_error() {
    let err = from_json_str("{ not json").unwrap_err();
    assert!(matches!(err, IngestError::Parse(_)));
    // The error chains its serde source rather than swallowing it.
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn non_canonical_kind_is_a_parse_error() {
    // `chain` is an OpenInference span kind, not one of the canonical four — the canonical
    // loader rejects it rather than silently coercing.
    let json = r#"{
      "schema_version": "0.1", "id": "k",
      "steps": [{ "idx": 0, "kind": "chain", "name": "a", "outputs": "x" }]
    }"#;
    assert!(matches!(
        from_json_str(json).unwrap_err(),
        IngestError::Parse(_)
    ));
}

#[test]
fn load_file_reads_a_trace_from_disk() {
    // CARGO_TARGET_TMPDIR is a cargo-provided per-target temp dir for integration tests, so
    // this exercises the real file path hermetically — no coupling to spike/.
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("load_file_reads.json");
    std::fs::write(&path, MINIMAL_TRACE).unwrap();

    let ingested = load_file(&path).unwrap();
    assert_eq!(ingested.run.id, "minimal");
    assert_eq!(ingested.run.steps.len(), 1);
}

#[test]
fn missing_file_is_an_io_error() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("does-not-exist.json");
    let err = load_file(&path).unwrap_err();
    match err {
        IngestError::Io { path: p, .. } => assert!(p.ends_with("does-not-exist.json")),
        other => panic!("expected Io error, got {other:?}"),
    }
}
