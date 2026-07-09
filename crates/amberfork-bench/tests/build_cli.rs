//! End-to-end test for `amberfork-bench build-pairs` (issue #7 slice 3).
//!
//! The vertical slice, proven whole: raw upstream shapes — a ServiceNow TapeAgents tape and a
//! Who&When failure log, on the same GAIA task — go in; the honest cross-system table comes out.
//! `build-pairs` converts both through the `amberfork-ingest` adapters, matches them on the shared
//! `task_id`, and writes the `pair_*.json` + `a_*`/`b_*` triples the slice-1 seam reads; `run` then
//! scores the generated set and prints the Mode A′ disclosure banner. One test spans the whole
//! join, so a break anywhere between the adapters and the banner is a red test.
//!
//! Inputs are hand-authored fiction (a 6×7 arithmetic task), written to a scratch tree under
//! `CARGO_TARGET_TMPDIR` — nothing benchmark-derived is committed (notebook 001/T30). A second,
//! unsuccessful tape exercises the "a tape earns reference status" boundary: it is counted as an
//! unpaired drop, never silently skipped.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

fn bench() -> Command {
    Command::cargo_bin("amberfork-bench").expect("amberfork-bench binary builds")
}

/// The committed frozen-params file, reached from the crate root (integration tests run with the
/// package as working directory, so the in-repo default `bench/params.toml` does not resolve here).
fn frozen_params() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/params.toml")
}

/// A clean scratch tree for this test under Cargo's per-crate temp dir — removed first so a rerun
/// never inherits a stale output set.
fn work_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_pairs");
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, contents).expect("write fixture file");
}

/// A successful TapeAgents tape: its produced `result` matches the gold `Final answer`, so it can
/// serve as a Mode A′ reference for GAIA task `gaia-abc`.
const WINNING_TAPE: &str = r#"{
  "metadata": {
    "task": {
      "Question": "What is 6 times 7?",
      "task_id": "gaia-abc",
      "Final answer": "42"
    },
    "result": "42"
  },
  "steps": [
    {"kind": "plan", "metadata": {"agent": "Planner"}, "text": "multiply 6 by 7"},
    {"kind": "tool", "metadata": {"agent": "Calculator"}, "tool": "mul", "args": "6,7"},
    {"kind": "observe", "metadata": {"agent": "Calculator"}, "result": "42"},
    {"kind": "final", "metadata": {"agent": "Planner"}, "answer": "42"}
  ]
}"#;

/// An unsuccessful tape on the same task: its `result` differs from the gold answer, so it must be
/// counted as an unpaired drop rather than serve as a reference.
const LOSING_TAPE: &str = r#"{
  "metadata": {
    "task": {
      "Question": "What is 6 times 7?",
      "task_id": "gaia-abc",
      "Final answer": "42"
    },
    "result": "41"
  },
  "steps": [
    {"kind": "plan", "metadata": {"agent": "Planner"}, "text": "guess"},
    {"kind": "final", "metadata": {"agent": "Planner"}, "answer": "41"}
  ]
}"#;

/// A Who&When failure log on GAIA task `gaia-abc` — matches the winning tape's `task_id`. Its
/// annotated `mistake_step` (2, a valid index into the 5-turn history) is the pair's gold fork.
const FAILING_LOG: &str = r#"{
  "question": "What is 6 times 7?",
  "question_ID": "gaia-abc",
  "mistake_agent": "Calculator",
  "mistake_step": "2",
  "ground_truth": "42",
  "history": [
    {"role": "user", "content": "What is 6 times 7?"},
    {"name": "Planner", "content": "I will multiply 6 by 7."},
    {"name": "Calculator", "content": "6 times 7 is 41."},
    {"name": "Planner", "content": "The answer is 41."},
    {"name": "Judge", "content": "That is incorrect."}
  ]
}"#;

#[test]
fn build_pairs_constructs_a_cross_system_set_that_flows_through_the_seam() {
    let work = work_dir();
    let tapes = work.join("tapes");
    let logs = work.join("logs");
    let out = work.join("out");
    write(&tapes.join("win.json"), WINNING_TAPE);
    write(&tapes.join("lose.json"), LOSING_TAPE);
    write(&logs.join("Hand-Crafted/4.json"), FAILING_LOG);

    // Build: the winning tape pairs with the log; the losing tape is a counted, named drop.
    bench()
        .arg("build-pairs")
        .arg("--tapes")
        .arg(&tapes)
        .arg("--logs")
        .arg(&logs)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("built 1 cross-system pair(s) -> "))
        .stderr(predicate::str::contains("tapes: 2, logs: 1"))
        .stderr(predicate::str::contains(
            "unpaired tape lose: tape did not solve its task",
        ));

    // The manifest carries the seam's cross-system contract: gold fork step and the flag that
    // drives the Mode A′ disclosure, pointing at the two generated run files.
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("pair_00.json")).expect("pair_00.json written"))
            .expect("pair_00.json is valid JSON");
    assert_eq!(manifest["cross_system"], true);
    assert_eq!(manifest["gold_step"], 2);
    assert_eq!(manifest["failing"], "a_00.json");
    assert_eq!(manifest["reference"], "b_00.json");
    assert_eq!(manifest["meta"]["task_id"], "gaia-abc");
    assert!(
        out.join("a_00.json").is_file() && out.join("b_00.json").is_file(),
        "both run files are written beside the manifest"
    );

    // Score the generated set: the seam labels it Mode A′ and prints the cross-system banner,
    // and the results document records the protocol and the cross-system count.
    let json_path = out.join("results.json");
    bench()
        .arg("run")
        .arg("--pairs")
        .arg(&out)
        .arg("--params")
        .arg(frozen_params())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "cross-system: 1/1 scored pairs align a failing run against a reference from a \
             different agent system",
        ));

    let results: serde_json::Value =
        serde_json::from_slice(&fs::read(&json_path).expect("results.json written"))
            .expect("results.json is valid JSON");
    assert_eq!(results["protocol"], "mode-a-prime");
    assert_eq!(results["cross_system"], 1);
    assert_eq!(results["n_pairs"], 1);
}
