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
//! Two guards travel with the gate so it can't rot into a rubber stamp:
//! - `blind_cost_model_fails_the_bar` proves the fixture + scoring actually *discriminate* — a
//!   cost model that sees every step as identical localizes nothing and misses the bar. A gate
//!   that only ever passes is indistinguishable from no gate.
//! - `default_params_match_frozen_config` pins `DiffParams::default()` to the frozen bench
//!   config (`bench/params.toml`, sha256:8ebd95ce8f3d) from the align side, so an accidental
//!   default change reddens this crate's own suite — not only the bench crate's mirror test.

use amberfork_align::{AlignParams, CostModel, DiffParams, ForkParams, LexicalCost, diff};
use amberfork_model::{Run, Step};
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

fn manifests() -> Vec<PathBuf> {
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
    manifests
}

/// Exact-hit count and the miss descriptions for one cost model over the committed pairs.
fn score_exact(manifests: &[PathBuf], cost: &impl CostModel) -> (usize, Vec<String>) {
    let dir = pairs_dir();
    let mut exact = 0;
    let mut misses = Vec::new();
    for manifest_path in manifests {
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;
        let reference = load_run(&dir.join(manifest["reference"].as_str().unwrap()));
        let failing = load_run(&dir.join(manifest["failing"].as_str().unwrap()));

        let result = diff(&reference, &failing, cost, &DiffParams::default());
        if result.fork_step_observed() == Some(gold) {
            exact += 1;
        } else {
            misses.push(format!(
                "{}: predicted {:?}, gold {gold}",
                manifest_path.file_name().unwrap().to_string_lossy(),
                result.fork_step_observed()
            ));
        }
    }
    (exact, misses)
}

#[test]
fn chimera_dev_localization_holds_baseline() {
    let manifests = manifests();
    assert_eq!(
        manifests.len(),
        EXPECTED_PAIRS,
        "expected the committed dev set of {EXPECTED_PAIRS} pairs; found {}",
        manifests.len()
    );

    let (exact, misses) = score_exact(&manifests, &LexicalCost);
    assert!(
        exact >= DEV_BAR,
        "exact {exact}/{EXPECTED_PAIRS} is below the pinned dev baseline {DEV_BAR}/{EXPECTED_PAIRS}; misses:\n{}",
        misses.join("\n")
    );
    println!("chimera dev parity: exact {exact}/{EXPECTED_PAIRS} (baseline {DEV_BAR})");
}

/// A cost model that reports every step pair as identical (`0.0`). The aligner then sees no
/// divergence anywhere, so it localizes no fork — the degenerate control that must miss the bar.
struct BlindCost;
impl CostModel for BlindCost {
    fn cost(&self, _a: &Step, _b: &Step) -> f64 {
        0.0
    }
}

#[test]
fn blind_cost_model_fails_the_bar() {
    let manifests = manifests();
    let (exact, _) = score_exact(&manifests, &BlindCost);
    assert!(
        exact < DEV_BAR,
        "a blind (all-identical) cost model scored {exact}/{EXPECTED_PAIRS} >= the bar {DEV_BAR} — \
         the gate does not discriminate and is vacuous"
    );
}

#[test]
fn default_params_match_frozen_config() {
    // The frozen bench config bench/params.toml (sha256:8ebd95ce8f3d; notebook 007). A bench
    // unit test pins file == default(); this pins default() == the documented values from the
    // align side, so neither crate can drift the engine the published number describes alone.
    let p = DiffParams::default();
    assert_eq!(
        p.align,
        AlignParams {
            gap_open: 0.6,
            gap_ext: 0.3
        }
    );
    assert_eq!(
        p.fork,
        ForkParams {
            tau: 0.3,
            resync_k: 2
        }
    );
}
