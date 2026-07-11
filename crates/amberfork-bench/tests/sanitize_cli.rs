//! End-to-end tests for `amberfork-bench sanitize` (issue #17) — the Rust home of the
//! scenarios `spike/test_sanitize.py` proved for the Python original, on the same fabricated
//! questions/answers (never benchmark-derived data — notebook 001/T30).
//!
//! The redaction primitives' invariants (space-count preservation, no residue, determinism,
//! idempotence) live as unit tests in `src/sanitize.rs`; this file exercises the two CLI
//! stages whole: files in, files out, receipts on stderr, artifact-only exit codes. The
//! centerpiece is the cross-log sweep — the leak class that forced the two-stage design
//! (notebook 013): a chimera whose tail (from log Y) quotes log X's question must come out
//! clean when swept against BOTH source questions, which canonical-only sanitization
//! structurally cannot see.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

fn run(id: &str, question: &str, steps: &[&str]) -> Value {
    json!({
        "schema_version": "0.1",
        "id": id,
        "task": question,
        "outcome": "fail",
        "steps": steps
            .iter()
            .enumerate()
            .map(|(i, s)| json!({"idx": i, "kind": "agent", "name": "a", "outputs": s}))
            .collect::<Vec<_>>(),
    })
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_string(value).unwrap()).unwrap();
}

fn lower_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            current.push(c.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn four_grams(text: &str) -> HashSet<Vec<String>> {
    let tokens = lower_tokens(text);
    if tokens.len() < 4 {
        return HashSet::new();
    }
    tokens.windows(4).map(<[String]>::to_vec).collect()
}

fn step_body(dir: &Path, file: &str) -> String {
    let run: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(file)).unwrap()).unwrap();
    run["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["outputs"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn canonical_stage_redacts_question_answer_and_index() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("canonical");
    let out = temp.path().join("sanitized");
    std::fs::create_dir(&src).unwrap();

    let question = "what is the total penguin population on dream island at the end of 2012";
    write_json(
        &src.join("x.json"),
        &run(
            "x",
            question,
            &[&format!("restating: {question}"), "and the count is 42000"],
        ),
    );
    write_json(
        &src.join("index.json"),
        &json!([{"file": "x.json", "ground_truth": "42000"}]),
    );

    Command::cargo_bin("amberfork-bench")
        .unwrap()
        .args(["sanitize", "canonical"])
        .arg("--src")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("sanitized 1 canonical log(s)"))
        .stderr(predicate::str::contains("verify OK"));

    let sanitized: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("x.json")).unwrap()).unwrap();
    let task = sanitized["task"].as_str().unwrap();
    assert!(
        task.starts_with("[GAIA question redacted; sha256:") && task.ends_with(']'),
        "task is not a redaction marker: {task}"
    );
    let body = step_body(&out, "x.json");
    assert!(
        four_grams(&body)
            .intersection(&four_grams(question))
            .next()
            .is_none(),
        "question 4-gram survived the canonical stage"
    );
    assert!(
        !body.contains("42000"),
        "answer survived the canonical stage"
    );

    let index: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("index.json")).unwrap()).unwrap();
    let gold = index[0]["ground_truth"].as_str().unwrap();
    assert!(
        gold.starts_with("sha256:"),
        "index ground_truth is not hash-redacted: {gold}"
    );
}

/// The cross-log sweep, end to end: a chimera whose tail (from log Y) quotes log X's whole
/// question must come out clean when swept against BOTH source questions — the exact leak
/// canonical-only sanitization cannot see.
#[test]
fn pairs_stage_sweeps_cross_log_question_leaks() {
    let temp = tempfile::tempdir().unwrap();
    let canon = temp.path().join("canonical");
    let pairs = temp.path().join("pairs");
    let out = temp.path().join("swept");
    std::fs::create_dir(&canon).unwrap();
    std::fs::create_dir(&pairs).unwrap();

    let question_x = "count the penguins on dream island in the attached file";
    let question_y = "list the highest grossing movies released in the year 2020";
    write_json(&canon.join("x.json"), &run("x", question_x, &["intro"]));
    write_json(&canon.join("y.json"), &run("y", question_y, &["intro"]));
    write_json(
        &canon.join("index.json"),
        &json!([
            {"file": "x.json", "ground_truth": "7"},
            {"file": "y.json", "ground_truth": "tenet"},
        ]),
    );

    // failing = X prefix + Y tail, and Y's tail happens to quote X's whole question.
    write_json(
        &pairs.join("a_00.json"),
        &run(
            "a",
            question_x,
            &["intro step", &format!("agent restated: {question_x}")],
        ),
    );
    write_json(
        &pairs.join("b_00.json"),
        &run("b", question_x, &["intro step"]),
    );
    write_json(
        &pairs.join("pair_00.json"),
        &json!({
            "failing": "a_00.json",
            "reference": "b_00.json",
            "gold_step": 1,
            "meta": {"x": "x.json", "y": "y.json"},
        }),
    );

    Command::cargo_bin("amberfork-bench")
        .unwrap()
        .args(["sanitize", "pairs"])
        .arg("--pairs")
        .arg(&pairs)
        .arg("--canonical")
        .arg(&canon)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("swept 1 pair(s)"));

    let body = format!(
        "{} {}",
        step_body(&out, "a_00.json"),
        step_body(&out, "b_00.json")
    );
    assert!(
        four_grams(&body)
            .intersection(&four_grams(question_x))
            .next()
            .is_none(),
        "X's question leaked through Y's tail"
    );
    // The manifest travels verbatim.
    assert_eq!(
        std::fs::read_to_string(pairs.join("pair_00.json")).unwrap(),
        std::fs::read_to_string(out.join("pair_00.json")).unwrap(),
    );
}

/// A pair set naming a source log the canonical index does not know is trouble (exit 2), not
/// a partial sweep: the sanitizer certifies an artifact, so an input it cannot fully account
/// for stops the run.
#[test]
fn pairs_stage_refuses_an_unindexed_source_log() {
    let temp = tempfile::tempdir().unwrap();
    let canon = temp.path().join("canonical");
    let pairs = temp.path().join("pairs");
    std::fs::create_dir(&canon).unwrap();
    std::fs::create_dir(&pairs).unwrap();

    write_json(&canon.join("x.json"), &run("x", "q", &["intro"]));
    write_json(&canon.join("index.json"), &json!([]));
    write_json(&pairs.join("a_00.json"), &run("a", "q", &["s"]));
    write_json(&pairs.join("b_00.json"), &run("b", "q", &["s"]));
    write_json(
        &pairs.join("pair_00.json"),
        &json!({
            "failing": "a_00.json",
            "reference": "b_00.json",
            "gold_step": 0,
            "meta": {"x": "x.json", "y": "x.json"},
        }),
    );

    Command::cargo_bin("amberfork-bench")
        .unwrap()
        .args(["sanitize", "pairs"])
        .arg("--pairs")
        .arg(&pairs)
        .arg("--canonical")
        .arg(&canon)
        .arg("--out")
        .arg(temp.path().join("swept"))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no index entry for x.json"));
}
