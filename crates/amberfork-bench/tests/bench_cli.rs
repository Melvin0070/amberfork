//! End-to-end tests for `amberfork-bench run` on the committed synthetic fixture set.
//!
//! The fixtures are hand-authored fiction (a restock run spliced with a greenhouse-audit
//! tail) so they can live in the repo — real chimera pairs derive from Who&When/GAIA logs and
//! must not be committed (notebook 001/T30). Three designed pairs: two the product arm must
//! hit exactly (a clean splice and a retry-blip splice) and one benign rewording where it
//! must abstain. That fixes every headline number: exact 2/3, ±1 2/3, ±3 2/3, no-pred 1/3.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chimera_synthetic")
}

fn bench() -> Command {
    Command::cargo_bin("amberfork-bench").expect("amberfork-bench binary builds")
}

#[test]
fn run_scores_the_synthetic_set_and_writes_the_results_json() {
    let json_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("results.json");

    bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "| nw-lexical/resync | 0.67 [0.21, 0.94] | 0.67 [0.21, 0.94] | 0.67 [0.21, 0.94] | 0.33 | 3 |",
        ));

    let text = std::fs::read_to_string(&json_path).expect("results JSON written");
    let results: serde_json::Value = serde_json::from_str(&text).expect("results JSON parses");
    assert_eq!(results["bench_schema_version"], "0.1");
    assert_eq!(results["protocol"], "chimera");
    assert_eq!(results["n_pairs"], 3);
    assert_eq!(results["params"]["tau"], 0.3);
    assert_eq!(results["params"]["resync_k"], 2);

    let arm = &results["arms"][0];
    assert_eq!(arm["arm"], "nw-lexical/resync");
    assert_eq!(
        arm["exact"]["hits"], 2,
        "clean splice + retry splice hit gold"
    );
    assert_eq!(
        arm["exact"]["n"], 3,
        "the abstention stays in the denominator"
    );
    assert_eq!(arm["w1"]["hits"], 2);
    assert_eq!(arm["w3"]["hits"], 2);
    assert_eq!(arm["no_pred"]["hits"], 1, "the reworded pair abstains");
    assert!(
        arm["exact"]["ci95_lo"].as_f64().is_some(),
        "every rate carries its interval"
    );
}

#[test]
fn a_missing_pairs_dir_is_trouble() {
    bench()
        .args(["run", "--pairs", "does/not/exist"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does/not/exist"));
}

#[test]
fn an_empty_pairs_dir_is_trouble() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("empty_pairs");
    std::fs::create_dir_all(&dir).expect("create scratch dir");

    bench()
        .arg("run")
        .arg("--pairs")
        .arg(&dir)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no pair manifests"));
}
