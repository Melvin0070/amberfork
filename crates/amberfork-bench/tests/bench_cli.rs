//! End-to-end tests for `amberfork-bench run` on the committed synthetic fixture sets.
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
//!
//! A second set, `exclusion_zoo`, locks protocol rule 4 (exclusions are data): one evaluable
//! pair among four designed failures — a missing run file, a gold step outside the failing
//! run, a manifest that is not JSON, and an empty run — each counted and tabulated with its
//! reason, never silently dropped and never fatal. The dev/test split (rule 1) keys on the
//! reference run's id: `restock-good` hashes to test, `greenhouse-good` to dev, so the two
//! sets between them lock both assignments end-to-end.
//!
//! Rule 2 (parameter freeze) is locked here too: every run names its params file
//! (`--params`, default `bench/params.toml`), the published artifact carries the file's
//! sha256 — recomputed in-test from the committed bytes, never hardcoded, so a changelog
//! edit doesn't break the suite — and a missing or invalid file is trouble, never a silent
//! fall back to code defaults.

use assert_cmd::Command;
use predicates::prelude::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chimera_synthetic")
}

fn zoo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exclusion_zoo")
}

/// The committed frozen-params file (rule 2), reached from the crate root — integration
/// tests run with the package as working directory, so the in-repo default `bench/params.toml`
/// does not resolve here and every invocation passes the file explicitly.
fn frozen_params() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/params.toml")
}

/// The sha256 hex of the committed params file, recomputed from its exact bytes — what the
/// published artifact must echo.
fn frozen_params_sha256() -> String {
    let bytes = std::fs::read(frozen_params()).expect("committed bench/params.toml reads");
    format!("{:x}", Sha256::digest(&bytes))
}

fn bench() -> Command {
    Command::cargo_bin("amberfork-bench").expect("amberfork-bench binary builds")
}

#[test]
fn run_scores_the_synthetic_set_and_writes_the_results_json() {
    let json_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("results.json");
    let sha = frozen_params_sha256();

    bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .arg("--params")
        .arg(frozen_params())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "coverage: 3/3 pairs evaluated · split=all (dev 0, test 3) · scored 3",
        ))
        // Rule 2: the table publishes with the config that produced it — file, hash, values.
        .stdout(predicate::str::contains(format!(
            "params: {} sha256:{} · tau 0.3 · resync_k 2 · gap 0.6+0.3",
            frozen_params().display(),
            &sha[..12]
        )))
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
    assert_eq!(results["bench_schema_version"], "0.3");
    assert_eq!(results["protocol"], "chimera");
    assert_eq!(results["split"], "all");
    assert_eq!(results["n_pairs"], 3);

    // Rule 2 in the committed document: the params carry their identity (source + sha256),
    // not just their values.
    assert_eq!(
        results["params"]["source"],
        frozen_params().display().to_string()
    );
    assert_eq!(results["params"]["sha256"], sha);
    assert_eq!(results["params"]["tau"], 0.3);
    assert_eq!(results["params"]["resync_k"], 2);
    assert_eq!(results["params"]["gap_open"], 0.6);
    assert_eq!(results["params"]["gap_ext"], 0.3);

    // Rule 4: coverage rides in the results document, exclusions tabulated (none here).
    assert_eq!(results["coverage"]["total"], 3);
    assert_eq!(results["coverage"]["evaluated"], 3);
    assert_eq!(results["coverage"]["dev"], 0);
    assert_eq!(results["coverage"]["test"], 3);
    assert_eq!(results["coverage"]["reasons"], serde_json::json!({}));
    assert_eq!(results["coverage"]["exclusions"], serde_json::json!([]));

    // Rule 1: the split manifest — every evaluated pair with its task key and assignment.
    // All three synthetic pairs share one reference task, so all land on one side (the
    // leakage guard: same task, same side). `restock-good` hashes to test.
    let pairs = results["pairs"].as_array().expect("pairs is an array");
    assert_eq!(pairs.len(), 3, "one manifest record per evaluated pair");
    for (record, name) in pairs.iter().zip(["pair_00", "pair_01", "pair_02"]) {
        assert_eq!(record["name"], name);
        assert_eq!(record["task_key"], "restock-good");
        assert_eq!(record["split"], "test");
    }

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
fn split_test_selects_the_synthetic_pairs() {
    // The whole synthetic set shares the `restock-good` task, which hashes to test — so
    // `--split test` scores all three and the table matches the all-split run exactly.
    bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .arg("--params")
        .arg(frozen_params())
        .args(["--split", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "coverage: 3/3 pairs evaluated · split=test (dev 0, test 3) · scored 3",
        ))
        .stdout(predicate::str::contains(
            "| nw-lexical/resync | 0.67 [0.21, 0.94] | 0.67 [0.21, 0.94] | 0.67 [0.21, 0.94] | 0.33 | 3 |",
        ));
}

#[test]
fn a_split_with_no_pairs_is_trouble() {
    // No synthetic pair hashes to dev; asking to score dev is a job that cannot be done,
    // not a silent empty table.
    bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .arg("--params")
        .arg(frozen_params())
        .args(["--split", "dev"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "no pairs to score in split dev (evaluated: dev 0, test 3)",
        ));
}

#[test]
fn exclusions_are_counted_and_tabulated_not_fatal() {
    let json_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("zoo_results.json");

    bench()
        .arg("run")
        .arg("--pairs")
        .arg(zoo_dir())
        .arg("--params")
        .arg(frozen_params())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "coverage: 1/5 pairs evaluated (excluded: empty-run 1, gold-out-of-range 1, \
             manifest-invalid 1, run-unloadable 1) · split=all (dev 1, test 0) · scored 1",
        ))
        // The one evaluable pair forks where the wrong sensor gets read; the product hits it.
        .stdout(predicate::str::contains(
            "| nw-lexical/resync | 1.00 [0.21, 1.00] | 1.00 [0.21, 1.00] | 1.00 [0.21, 1.00] | 0.00 | 1 |",
        ))
        // Every exclusion is named and explained on stderr, one line each.
        .stderr(predicate::str::contains("excluded pair_01"))
        .stderr(predicate::str::contains("excluded pair_02"))
        .stderr(predicate::str::contains("excluded pair_03"))
        .stderr(predicate::str::contains("excluded pair_04"));

    let text = std::fs::read_to_string(&json_path).expect("results JSON written");
    let results: serde_json::Value = serde_json::from_str(&text).expect("results JSON parses");
    assert_eq!(results["coverage"]["total"], 5);
    assert_eq!(results["coverage"]["evaluated"], 1);
    assert_eq!(results["coverage"]["dev"], 1);
    assert_eq!(results["coverage"]["test"], 0);
    assert_eq!(
        results["coverage"]["reasons"],
        serde_json::json!({
            "empty-run": 1,
            "gold-out-of-range": 1,
            "manifest-invalid": 1,
            "run-unloadable": 1
        })
    );
    // Per-case records, in manifest order, each naming the offending file dir-relative so a
    // committed results document stays machine-portable.
    assert_eq!(
        results["coverage"]["exclusions"],
        serde_json::json!([
            { "name": "pair_01", "reason": "run-unloadable", "file": "missing.json" },
            { "name": "pair_02", "reason": "gold-out-of-range", "file": "good_fail.json" },
            { "name": "pair_03", "reason": "manifest-invalid", "file": "pair_03.json" },
            { "name": "pair_04", "reason": "empty-run", "file": "empty_fail.json" }
        ])
    );
    assert_eq!(
        results["pairs"],
        serde_json::json!([
            { "name": "pair_00", "task_key": "greenhouse-good", "split": "dev" }
        ])
    );
    assert_eq!(results["n_pairs"], 1);
}

#[test]
fn a_missing_params_file_is_trouble_and_names_the_default_path() {
    // No --params given: the default is `bench/params.toml` relative to the working
    // directory, which for this test process is the crate root — nothing there. Rule 2 has
    // no code-default fallback: a run that cannot name its config must not print a table.
    bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("bench/params.toml"));
}

#[test]
fn an_invalid_params_file_is_trouble_not_a_fallback() {
    // A frozen file that violates an engine invariant is rejected through the engine's own
    // validation, naming the offending value.
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bad_params.toml");
    std::fs::write(
        &path,
        "[align]\ngap_open = 0.6\ngap_ext = 0.3\n\n[fork]\ntau = 2.0\nresync_k = 2\n",
    )
    .expect("write scratch params");

    bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .arg("--params")
        .arg(&path)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("tau must be within [0, 1]"));
}

#[test]
fn a_missing_pairs_dir_is_trouble() {
    bench()
        .args(["run", "--pairs", "does/not/exist"])
        .arg("--params")
        .arg(frozen_params())
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
        .arg("--params")
        .arg(frozen_params())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no pair manifests"));
}
