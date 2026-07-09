//! End-to-end tests for `amberfork-bench run` on the committed synthetic fixture set.
//!
//! The fixtures are hand-authored fiction (a restock run spliced with a greenhouse-audit
//! tail) so they can live in the repo — real chimera pairs derive from Who&When/GAIA logs and
//! must not be committed (notebook 001/T30). Three designed pairs: a clean splice, a
//! retry-blip splice, and a benign rewording with no fork. On them the factorial ladder is
//! locked exactly, floor to product (rule 5: exact match on predicted indices):
//!
//! - `random` (committed seed): 0/3 exact, 0 abstentions — the floor always answers.
//! - `pos-lexical`: 1/3 — hits the clean splice, desyncs on the retry insertion (the
//!   designed failure of index-wise diffing), abstains on the rewording.
//! - `nw-structural/resync`: 1/3 exact, 2/3 ±1 — alignment recovers the blip but the
//!   content-blind cost misplaces the clean splice by one (kind+name match across it).
//! - `nw-lexical/resync` (the product): 2/3 exact and the one honest abstention.

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
            "| random | 0.00 [0.00, 0.56] | 0.33 [0.06, 0.79] | 0.67 [0.21, 0.94] | 0.00 | 3 |",
        ))
        .stdout(predicate::str::contains(
            "| pos-lexical | 0.33 [0.06, 0.79] | 0.33 [0.06, 0.79] | 0.67 [0.21, 0.94] | 0.33 | 3 |",
        ))
        .stdout(predicate::str::contains(
            "| nw-structural/resync | 0.33 [0.06, 0.79] | 0.67 [0.21, 0.94] | 0.67 [0.21, 0.94] | 0.33 | 3 |",
        ))
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

    // (arm, exact, w1, w3, no_pred) hit counts — the factorial ladder, floor to product,
    // every arm on the identical three pairs and the identical denominator.
    let expected = [
        ("random", 0, 1, 2, 0),
        ("pos-lexical", 1, 1, 2, 1),
        ("nw-structural/resync", 1, 2, 2, 1),
        ("nw-lexical/resync", 2, 2, 2, 1),
    ];
    let arms = results["arms"].as_array().expect("arms is an array");
    assert_eq!(arms.len(), expected.len(), "one row per protocol arm");
    for (arm, (name, exact, w1, w3, no_pred)) in arms.iter().zip(expected) {
        assert_eq!(arm["arm"], name);
        assert_eq!(arm["exact"]["hits"], exact, "{name} exact");
        assert_eq!(arm["w1"]["hits"], w1, "{name} ±1");
        assert_eq!(arm["w3"]["hits"], w3, "{name} ±3");
        assert_eq!(arm["no_pred"]["hits"], no_pred, "{name} abstentions");
        assert_eq!(
            arm["exact"]["n"], 3,
            "{name}: nothing leaves the denominator"
        );
        assert!(
            arm["exact"]["ci95_lo"].as_f64().is_some(),
            "{name}: every rate carries its interval"
        );
    }
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
