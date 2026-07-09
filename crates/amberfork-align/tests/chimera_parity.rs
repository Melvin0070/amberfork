//! Quantitative regression gate: on the committed dev-split chimera pairs, the Rust pipeline
//! must hold the pinned dev baseline of **6/8 exact (0.75)** fork localization — which is also
//! the protocol's ≥0.70 floor (`ceil(0.70·8) = 6`; notebook 003/006/013).
//!
//! Runs in CI (no `#[ignore]`). The pairs live in the repo at
//! `bench/fixtures/chimera_noise_seed42_dev/` — the dev-split subset of the seed-42 n=20 noise
//! set, GAIA-sanitized per BENCHMARK.md's licensing rule (two-stage redaction; provenance and
//! the re-runnable sanitizer are documented beside the data). The test side is deliberately not
//! committed — a committed test set invites tuning-on-test (protocol rule 2). Regenerate/audit
//! the full set with `spike/make_pairs.py` + `spike/sanitize_gaia.py` (see the fixture README).
//!
//! `DiffParams::default()` here equals the frozen bench config `bench/params.toml`
//! (sha256:8ebd95ce8f3d; a bench unit test pins that equality — notebook 007).

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_model::Run;
use std::path::{Path, PathBuf};

/// Pinned dev baseline: 6/8 exact (notebook 006/013). Also the ≥0.70 protocol floor at n=8.
const DEV_BAR: usize = 6;
const EXPECTED_PAIRS: usize = 8;

fn pairs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/fixtures/chimera_noise_seed42_dev")
}

fn load_run(path: &Path) -> Run {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn chimera_dev_localization_holds_baseline() {
    let dir = pairs_dir();
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("pair_") && n.ends_with(".json"))
        })
        .collect();
    manifests.sort();
    assert_eq!(
        manifests.len(),
        EXPECTED_PAIRS,
        "expected the committed dev set of {EXPECTED_PAIRS} pairs; found {}",
        manifests.len()
    );

    let mut exact = 0;
    let mut misses = Vec::new();
    for manifest_path in &manifests {
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;
        let reference = load_run(&dir.join(manifest["reference"].as_str().unwrap()));
        let failing = load_run(&dir.join(manifest["failing"].as_str().unwrap()));

        let result = diff(&reference, &failing, &LexicalCost, &DiffParams::default());
        let pred = result.fork_step_observed();
        if pred == Some(gold) {
            exact += 1;
        } else {
            misses.push(format!(
                "{}: predicted {pred:?}, gold {gold}",
                manifest_path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

    assert!(
        exact >= DEV_BAR,
        "exact {exact}/{EXPECTED_PAIRS} is below the pinned dev baseline {DEV_BAR}/{EXPECTED_PAIRS}; misses:\n{}",
        misses.join("\n")
    );
    println!("chimera dev parity: exact {exact}/{EXPECTED_PAIRS} (baseline {DEV_BAR})");
}
