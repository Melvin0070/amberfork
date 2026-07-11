//! Ingest warnings must survive the full pipeline (issue #4, slice 3 / carry-over from #3):
//! `amberfork-ingest` raises them per run, the CLI merges them into
//! `DiffResult.warnings` with a `run a:`/`run b:` side prefix, `--json` carries them in the
//! machine contract, and the human render routes them to stderr (stdout is the result).
//!
//! The traces are written by the test itself: the good run carries a run-level unmapped
//! field, the bad run a step-level unmapped field plus a content-absent step — one specimen
//! per warning code, split across sides so the prefixes are actually exercised.

use amberfork_model::{DiffResult, WarningCode};
use assert_cmd::Command;
use std::path::PathBuf;

/// A run-level unknown field (`run_tag`) → `unmapped-attributes` on side `a`.
const GOOD_TRACE: &str = r#"{
  "schema_version": "0.1",
  "id": "warny_good",
  "run_tag": "nightly",
  "steps": [
    {"idx": 0, "kind": "tool", "name": "fetch", "outputs": "ok"},
    {"idx": 1, "kind": "llm", "name": "planner", "outputs": "plan: done"}
  ]
}"#;

/// A step-level unknown field (`latency_ms`) and a content-absent step → both codes on
/// side `b`.
const BAD_TRACE: &str = r#"{
  "schema_version": "0.1",
  "id": "warny_bad",
  "steps": [
    {"idx": 0, "kind": "tool", "name": "fetch", "outputs": "ok", "latency_ms": 812},
    {"idx": 1, "kind": "llm", "name": "planner"}
  ]
}"#;

/// Write both traces into the cargo-provided per-crate tmp dir; suffix keeps parallel tests
/// from clobbering each other.
fn write_pair(suffix: &str) -> (PathBuf, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let good = dir.join(format!("warny_good_{suffix}.json"));
    let bad = dir.join(format!("warny_bad_{suffix}.json"));
    std::fs::write(&good, GOOD_TRACE).unwrap();
    std::fs::write(&bad, BAD_TRACE).unwrap();
    (bad, good)
}

#[test]
fn json_carries_both_sides_warnings_with_run_prefixes() {
    let (bad, good) = write_pair("json");

    let output = Command::cargo_bin("amberfork")
        .unwrap()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let result: DiffResult = serde_json::from_str(&stdout).expect("valid DiffResult JSON");

    let find = |code: WarningCode, prefix: &str| {
        result
            .warnings
            .iter()
            .find(|w| w.code == code && w.msg.starts_with(prefix))
            .unwrap_or_else(|| {
                panic!(
                    "expected a {code:?} warning prefixed {prefix:?}: {:#?}",
                    result.warnings
                )
            })
    };

    // Side a (reference/good): the run-level unmapped field.
    let a_unmapped = find(WarningCode::UnmappedAttributes, "run a: ");
    assert!(
        a_unmapped.msg.contains("run_tag"),
        "got: {}",
        a_unmapped.msg
    );

    // Side b (observed/bad): the step-level unmapped field and the content-absent step.
    let b_unmapped = find(WarningCode::UnmappedAttributes, "run b: ");
    assert!(
        b_unmapped.msg.contains("latency_ms"),
        "got: {}",
        b_unmapped.msg
    );
    let b_absent = find(WarningCode::ContentAbsent, "run b: ");
    assert!(b_absent.msg.contains("step 1"), "got: {}", b_absent.msg);
}

#[test]
fn human_mode_reports_warnings_on_stderr_not_stdout() {
    let (bad, good) = write_pair("human");

    let output = Command::cargo_bin("amberfork")
        .unwrap()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--no-color")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(
        stderr.contains("amberfork: warning: run a: ")
            && stderr.contains("amberfork: warning: run b: "),
        "both sides' warnings belong on stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("warning:"),
        "stdout is the result, not the diagnostics channel:\n{stdout}"
    );
}
