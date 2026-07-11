//! End-to-end contract of `amberfork diff <bad> --against <good>` (issue #4, slice 1).
//!
//! These tests run the real binary against the committed smoke fixtures, which makes them the
//! first place `amberfork-ingest` and `amberfork-align` are exercised together. The contract
//! under test:
//! - exit codes follow the `diff(1)` precedent — 0 converged, 1 forked, 2 trouble;
//! - `--json` emits a deserializable [`amberfork_model::DiffResult`] on stdout (the machine
//!   contract), locating the fork at the fixture manifest's gold step;
//! - errors go to stderr, never stdout, and name the offending path.

use amberfork_model::DiffResult;
use assert_cmd::Command;
use std::path::{Path, PathBuf};

/// Exit codes under test, named once (see `diff(1)`: 0 same, 1 differ, 2 trouble).
const EXIT_CONVERGED: i32 = 0;
const EXIT_FORKED: i32 = 1;
const EXIT_TROUBLE: i32 = 2;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/fixtures/smoke")
}

fn amberfork() -> Command {
    Command::cargo_bin("amberfork").expect("amberfork binary builds")
}

/// The committed manifest: which fixture is the failing side and where the fork truly is.
fn manifest() -> (PathBuf, PathBuf, usize) {
    let dir = fixture_dir();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pair_smoke.json")).unwrap())
            .unwrap();
    let bad = dir.join(manifest["failing"].as_str().unwrap());
    let good = dir.join(manifest["reference"].as_str().unwrap());
    let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;
    (bad, good, gold)
}

#[test]
fn forked_pair_exits_1_and_json_locates_the_gold_step() {
    let (bad, good, gold) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .assert()
        .code(EXIT_FORKED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult =
        serde_json::from_str(&stdout).expect("--json stdout must be a valid DiffResult");

    let fork = result.fork.expect("smoke pair encodes a real divergence");
    assert_eq!(
        fork.b_step,
        Some(gold),
        "fork must land on failing-run step {gold} (manifest gold)"
    );
    // Side convention of the contract: <good> is reference = a, <bad> is observed = b.
    assert_eq!(
        result.runs.b.n_steps,
        result
            .alignment
            .iter()
            .filter(|m| m.b_idx.is_some())
            .count()
    );
    // The content-diff pane's data (issue #13): the fork pair carries field-level evidence.
    assert!(
        result.field_diffs.iter().any(|fd| fd.step == fork.index),
        "--json must carry field diffs at the fork's alignment index"
    );
}

#[test]
fn self_diff_exits_0_and_json_reports_converged() {
    let (bad, _, _) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&bad)
        .arg("--json")
        .assert()
        .code(EXIT_CONVERGED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        result.fork, None,
        "a run against itself is the converged state"
    );
}

#[test]
fn jsonl_input_exits_2_and_stderr_names_the_shape_the_file_and_the_guide() {
    // The likeliest first mistake (issue #20): pointing `diff` at a raw exporter transcript
    // (JSONL) instead of a converted canonical trace. The first error message is the product
    // surface for exactly that user — it must say which file, what shape it detected, and
    // where the conversion guide lives, all without dirtying stdout.
    let (_, good, _) = manifest();
    let jsonl = Path::new(env!("CARGO_TARGET_TMPDIR")).join("raw_transcript.jsonl");
    std::fs::write(
        &jsonl,
        "{\"role\": \"assistant\", \"content\": \"hi\"}\n{\"role\": \"tool\", \"name\": \"web.search\"}\n",
    )
    .unwrap();

    amberfork()
        .arg("diff")
        .arg(&jsonl)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("raw_transcript.jsonl"))
        .stderr(predicates::str::contains("JSON-Lines"))
        .stderr(predicates::str::contains("docs/run-on-your-own-agent.md"));
}

#[test]
fn missing_file_exits_2_with_the_path_on_stderr_and_clean_stdout() {
    let (_, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg("no/such/trace.json")
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("no/such/trace.json"));
}
