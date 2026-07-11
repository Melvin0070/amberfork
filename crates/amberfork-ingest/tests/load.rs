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
fn foreign_schema_version_warns_but_still_loads() {
    let json = r#"{
      "schema_version": "9.9",
      "id": "future",
      "steps": [{ "idx": 0, "kind": "llm", "name": "a", "outputs": "x" }]
    }"#;
    let ingested = from_json_str(json).unwrap();
    assert_eq!(ingested.run.steps.len(), 1, "permissive: still loads");
    let mismatch = ingested
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::SchemaVersionMismatch)
        .expect("a foreign schema_version must warn");
    assert!(mismatch.msg.contains("9.9"), "got: {}", mismatch.msg);
}

#[test]
fn malformed_json_is_a_parse_error() {
    let err = from_json_str("{ not json").unwrap_err();
    assert!(matches!(err, IngestError::Parse { .. }));
    // The error chains its serde source rather than swallowing it.
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn valid_json_that_is_not_a_trace_says_what_a_trace_needs() {
    // The likeliest first mistake (issue #20): valid JSON from some exporter that isn't a
    // canonical trace. The error must keep serde's field detail and stop being a dead end:
    // say what a trace is, point to the format reference and the conversion guide.
    let err = from_json_str(r#"{"hello": "world"}"#).unwrap_err();
    assert!(matches!(err, IngestError::NotATrace { .. }));
    let msg = err.to_string();
    assert!(
        msg.contains("missing field `schema_version`"),
        "keeps serde's detail: {msg}"
    );
    assert!(
        msg.contains("docs/trace-format.md"),
        "points to the format reference: {msg}"
    );
    assert!(
        msg.contains("docs/run-on-your-own-agent.md"),
        "points to the conversion guide: {msg}"
    );
}

#[test]
fn json_lines_input_is_named_and_points_to_the_conversion_guide() {
    // Raw exporter transcripts are JSONL — one JSON value per line. Serde's own error for
    // this ("trailing characters") never names the shape; the classifier must (issue #20).
    let jsonl = "{\"role\": \"assistant\", \"content\": \"hi\"}\n{\"role\": \"tool\", \"name\": \"web.search\"}\n";
    let err = from_json_str(jsonl).unwrap_err();
    assert!(matches!(err, IngestError::JsonLines { .. }));
    let msg = err.to_string();
    assert!(msg.contains("JSON-Lines"), "names the shape: {msg}");
    assert!(
        msg.contains("docs/run-on-your-own-agent.md"),
        "points to the conversion guide: {msg}"
    );
}

#[test]
fn pretty_printed_wrong_shape_is_not_mistaken_for_json_lines() {
    // A pretty-printed JSON document also spans many lines, but its first line (`{`) is not
    // a complete JSON value — the JSONL heuristic must not fire.
    let err = from_json_str("{\n  \"hello\": \"world\"\n}").unwrap_err();
    assert!(matches!(err, IngestError::NotATrace { .. }));
}

#[test]
fn non_canonical_kind_is_not_a_trace() {
    // `chain` is an OpenInference span kind, not one of the canonical four — the canonical
    // loader rejects it rather than silently coercing. Valid JSON with the wrong vocabulary
    // classifies as NotATrace, so the user gets serde's unknown-variant detail plus the
    // format pointer.
    let json = r#"{
      "schema_version": "0.1", "id": "k",
      "steps": [{ "idx": 0, "kind": "chain", "name": "a", "outputs": "x" }]
    }"#;
    let err = from_json_str(json).unwrap_err();
    assert!(matches!(err, IngestError::NotATrace { .. }));
    assert!(err.to_string().contains("chain"), "got: {err}");
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
fn load_file_parse_errors_name_the_offending_file() {
    // `amberfork diff` loads two files; an error that doesn't say which one failed is a
    // dead end (issue #20). Io errors already carry their path — the parse family must too.
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("raw_transcript.jsonl");
    std::fs::write(&path, "{\"a\": 1}\n{\"a\": 2}\n").unwrap();

    let err = load_file(&path).unwrap_err();
    assert!(matches!(err, IngestError::JsonLines { .. }));
    assert!(
        err.to_string().contains("raw_transcript.jsonl"),
        "names which input failed: {err}"
    );
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
