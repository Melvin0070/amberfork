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
//! A third set, `mode_a_prime_synthetic`, locks the issue-#7 cross-system disclosure: two
//! pairs whose manifests declare `cross_system: true` (a CaptainAgent-style failing run vs a
//! smolagents-style passing reference on the same task). On them `run` labels the protocol
//! `mode-a-prime` and prints the disclosure banner — cross-system references diverge from step
//! 0, so the windowed metrics are the ones of record and step-exact is not claimed. The
//! chimera set is the control: `cross_system: 0`, no banner, table byte-identical to before.
//!
//! Rule 2 (parameter freeze) is locked here too: every run names its params file
//! (`--params`, default `bench/params.toml`), the published artifact carries the file's
//! sha256 — recomputed in-test from the committed bytes, never hardcoded, so a changelog
//! edit doesn't break the suite — and a missing or invalid file is trouble, never a silent
//! fall back to code defaults.
//!
//! Rule 7 (calibration): the reliability curve publishes under the main table for exactly
//! the confidence-bearing arms. On the designed splices both aligner forks are near-certain
//! (top bin), the product hits 2/2 there against the content-blind arm's 1/2, and the four
//! empty bins print as `—` / `rate: null` — published, never dropped.
//!
//! `report` closes the loop (BENCHMARK.md's definition of done: the table reproduces
//! offline): it renders a committed results document — no pairs, no engine, no fetch — and
//! its stdout is byte-identical to the `run` that wrote the document, because both modes
//! print through one renderer. The insta snapshot locks the committed dev-split artifact
//! (`bench/results/`); a document the renderer cannot vouch for (missing, or a foreign
//! `bench_schema_version`) is trouble, never a partial table.

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

/// The synthetic Mode A′ set (issue #7): two cross-system pairs — a CaptainAgent-style failing
/// run against a smolagents-style passing reference on the same task, rosters and step shapes
/// diverging from step 0. Hand-authored fiction, same as `chimera_synthetic`; the manifests
/// carry `cross_system: true`, which is what drives the table's cross-system disclosure.
fn mode_a_prime_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mode_a_prime_synthetic")
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
        ))
        // Rule 7: the reliability curve publishes with the table. Both aligner arms fork
        // with near-certain confidence on the designed splices (top bin), where the product
        // hits 2/2 and the content-blind arm 1/2; the four lower bins publish as empty
        // rather than vanishing. Abstentions carry no confidence, so each curve sums to 2
        // of the 3 scored pairs — the third is the no-pred column of the main table.
        .stdout(predicate::str::contains(
            "calibration: exact-hit rate by fork confidence (abstentions carry no confidence)",
        ))
        .stdout(predicate::str::contains(
            "| confidence | nw-structural/resync | nw-lexical/resync |",
        ))
        .stdout(predicate::str::contains("| [0.0, 0.2) | — | — |"))
        .stdout(predicate::str::contains(
            "| [0.8, 1.0] | 1/2 · 0.50 [0.09, 0.91] | 2/2 · 1.00 [0.34, 1.00] |",
        ))
        // The cross-system disclosure seam is inert for a same-system set: no banner appears,
        // and the table below is exactly what it was before Mode A′ existed.
        .stdout(predicate::str::contains("cross-system:").not());

    let text = std::fs::read_to_string(&json_path).expect("results JSON written");
    let results: serde_json::Value = serde_json::from_str(&text).expect("results JSON parses");
    assert_eq!(results["bench_schema_version"], "0.5");
    assert_eq!(results["protocol"], "chimera");
    assert_eq!(results["split"], "all");
    assert_eq!(results["n_pairs"], 3);
    assert_eq!(results["cross_system"], 0, "no cross-system pairs here");

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

    // Rule 7 in the committed document: the curve rides on exactly the confidence-bearing
    // arms — five fixed-width bins each, empty bins explicit (`rate: null`), occupied bins
    // a full Wilson-CI rate. The baselines carry no `calibration` key at all: a method
    // without a confidence has nothing to calibrate, and a fabricated curve would be
    // decorative.
    for arm in &arms[..2] {
        assert!(
            arm.get("calibration").is_none(),
            "{}: baselines do not calibrate",
            arm["arm"]
        );
    }
    for (arm, top_hits) in arms[2..].iter().zip([1, 2]) {
        let name = &arm["arm"];
        let bins = arm["calibration"]
            .as_array()
            .expect("aligner arms calibrate");
        assert_eq!(bins.len(), 5, "{name}: five fixed-width bins");
        assert_eq!(bins[0]["lo"], 0.0);
        assert_eq!(bins[4]["hi"], 1.0);
        for bin in &bins[..4] {
            assert_eq!(
                bin["rate"],
                serde_json::Value::Null,
                "{name}: empty is data"
            );
        }
        assert_eq!(bins[4]["rate"]["hits"], top_hits, "{name} top-bin hits");
        assert_eq!(
            bins[4]["rate"]["n"], 2,
            "{name}: both forks are near-certain"
        );
    }
}

#[test]
fn run_discloses_cross_system_pairs() {
    // Issue #7 (Mode A′): a set whose pairs align a failing run against a reference from a
    // *different* agent system must SAY SO in the published table. Cross-system references
    // legitimately diverge from step 0 (different rosters, different plan shapes), so step-exact
    // gold is murky and the windowed ±1/±3 metrics are the ones of record (BENCHMARK.md,
    // notebook 002). The disclosure is derived from the pairs' own `cross_system: true`, not an
    // operator flag — the data declares its nature, so it cannot be mislabeled.
    let json_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mode_a_prime_results.json");

    bench()
        .arg("run")
        .arg("--pairs")
        .arg(mode_a_prime_dir())
        .arg("--params")
        .arg(frozen_params())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success()
        // Both scored pairs are cross-system, so the banner reads 2/2 and states the honest
        // reading of the numbers below it.
        .stdout(predicate::str::contains(
            "cross-system: 2/2 scored pairs align a failing run against a reference from a \
             different agent system — cross-system references diverge from step 0, so ±1/±3 \
             are the metric of record and step-exact is not claimed.",
        ));

    let text = std::fs::read_to_string(&json_path).expect("results JSON written");
    let results: serde_json::Value = serde_json::from_str(&text).expect("results JSON parses");
    assert_eq!(results["bench_schema_version"], "0.5");
    // A set carrying cross-system pairs is Mode A′, not controlled-injection chimera — the
    // coarse protocol label follows the data, and the banner carries the detail.
    assert_eq!(results["protocol"], "mode-a-prime");
    assert_eq!(results["n_pairs"], 2);
    assert_eq!(
        results["cross_system"], 2,
        "both scored pairs are cross-system"
    );
    assert_eq!(results["coverage"]["total"], 2);
    assert_eq!(results["coverage"]["evaluated"], 2);
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

/// The canonical committed results document (the dev-split run on the real seed-42 noise
/// set; the test split stays sealed until a release tag — rule 2).
fn committed_results() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/results/chimera_noise_seed42_dev.json")
}

#[test]
fn report_reproduces_the_committed_dev_results_offline() {
    // The whole point of the committed document: anyone can re-render the published table
    // from the repo alone. The snapshot IS the published artifact — if either the document
    // or the renderer drifts, this goes red before a stale table reaches a reader.
    let output = bench()
        .arg("report")
        .arg("--results")
        .arg(committed_results())
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8 stdout");
    insta::assert_snapshot!("report_committed_dev", stdout);
}

/// The committed Mode A′ results document (all 4 real cross-system pairs under the frozen
/// params — the full published set; n=4 is everything the public tape data yields, so there
/// is no held-back split to seal). Identifiers only: pair names and tape stems, no GAIA
/// content (notebook 016).
fn committed_mode_a_prime_results() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/results/mode_a_prime_realpairs_all.json")
}

#[test]
fn report_reproduces_the_committed_mode_a_prime_results_offline() {
    // Same guarantee as the chimera document, plus the seam's promise: the re-rendered
    // table must carry the cross-system disclosure and introduce itself as Mode A′ —
    // a reader can never mistake the directional cross-system table for the controlled
    // chimera protocol.
    let output = bench()
        .arg("report")
        .arg("--results")
        .arg(committed_mode_a_prime_results())
        .assert()
        .success()
        .stderr(predicate::str::contains("mode-a-prime protocol"));
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8 stdout");
    assert!(
        stdout.contains("cross-system: 4/4 scored pairs"),
        "the committed table must disclose its cross-system nature"
    );
    insta::assert_snapshot!("report_committed_mode_a_prime", stdout);
}

#[test]
fn report_output_is_identical_to_the_run_that_wrote_the_document() {
    // One renderer, two modes: `run` prints the artifact it just computed, `report` prints
    // the artifact a document carries. Byte-identical stdout is the guarantee that a
    // committed table never diverges from what a live run would have published.
    let json_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("roundtrip.json");
    let run = bench()
        .arg("run")
        .arg("--pairs")
        .arg(fixtures_dir())
        .arg("--params")
        .arg(frozen_params())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success();
    let report = bench()
        .arg("report")
        .arg("--results")
        .arg(&json_path)
        .assert()
        .success();
    assert_eq!(
        String::from_utf8(run.get_output().stdout.clone()).expect("utf-8 stdout"),
        String::from_utf8(report.get_output().stdout.clone()).expect("utf-8 stdout"),
        "report must reproduce the run's published artifact byte for byte"
    );
}

#[test]
fn report_on_a_missing_results_file_is_trouble() {
    bench()
        .args(["report", "--results", "does/not/exist.json"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does/not/exist.json"));
}

#[test]
fn report_on_an_unsupported_schema_version_is_trouble() {
    // A renderer that silently draws a table from a document shaped by different rules
    // would misrepresent it. The version gate speaks before any shape error can.
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("foreign_version.json");
    std::fs::write(&path, r#"{ "bench_schema_version": "0.4" }"#).expect("write scratch doc");

    bench()
        .arg("report")
        .arg("--results")
        .arg(&path)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("0.4").and(predicate::str::contains("0.5")));
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
