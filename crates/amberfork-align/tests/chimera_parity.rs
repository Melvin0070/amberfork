//! Quantitative regression gate: on the committed dev-split chimera pairs, the Rust pipeline
//! must hold each seed's pinned localization baseline (notebook 006/013/014).
//!
//! **Three seeds, per-seed baselines.** Fork localization at the frozen τ=0.3 is seed-sensitive
//! (notebook 014): seed 42 is a favorable draw (6/8), seed 43 a hard one (2/7), seed 44 middling
//! (6/10) — aggregate 14/25 exact, but ±3 localization is a stable 0.95 across n=60. The gate
//! pins each seed's own exact baseline so it cannot rest on one lucky draw; the honest published
//! claim leads with the ±3 window, not seed 42's exact (README, notebook 014).
//!
//! Runs in CI (no `#[ignore]`). Pairs live in `bench/fixtures/chimera_noise_seed{42,43,44}_dev/`
//! — the dev-split subsets of the seed-N n=20 noise sets, GAIA-sanitized per BENCHMARK.md's
//! licensing rule (two-stage redaction; provenance + the re-runnable sanitizer documented beside
//! the data). The test side is deliberately not committed — a committed test set invites
//! tuning-on-test (protocol rule 2).
//!
//! Two guards travel with the gate so it can't rot into a rubber stamp:
//! - `blind_cost_model_fails_the_bar` proves the fixture + scoring actually *discriminate* — a
//!   cost model that sees every step as identical localizes nothing and misses every bar.
//! - `default_params_match_frozen_config` pins `DiffParams::default()` to the frozen bench
//!   config (`bench/params.toml`, sha256:8ebd95ce8f3d) from the align side.

use amberfork_align::{AlignParams, CostModel, DiffParams, ForkParams, LexicalCost, diff};
use amberfork_model::{Run, Step};
use std::path::{Path, PathBuf};

/// `(fixture dir, expected pair count, pinned exact baseline)` per seed (notebook 014).
const SEEDS: &[(&str, usize, usize)] = &[
    ("chimera_noise_seed42_dev", 8, 6),
    ("chimera_noise_seed43_dev", 7, 2),
    ("chimera_noise_seed44_dev", 10, 6),
];

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/fixtures")
        .join(name)
}

fn load_run(path: &Path) -> Run {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn manifests(dir: &Path) -> Vec<PathBuf> {
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(dir)
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

/// Exact-hit count and miss descriptions for one cost model over one seed's committed pairs.
fn score_exact(dir: &Path, manifests: &[PathBuf], cost: &impl CostModel) -> (usize, Vec<String>) {
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
                "{}/{}: predicted {:?}, gold {gold}",
                dir.file_name().unwrap().to_string_lossy(),
                manifest_path.file_name().unwrap().to_string_lossy(),
                result.fork_step_observed()
            ));
        }
    }
    (exact, misses)
}

#[test]
fn chimera_dev_localization_holds_baseline() {
    let (mut agg_exact, mut agg_n) = (0, 0);
    for &(name, expected_pairs, bar) in SEEDS {
        let dir = fixture_dir(name);
        let manifests = manifests(&dir);
        assert_eq!(
            manifests.len(),
            expected_pairs,
            "{name}: expected {expected_pairs} committed dev pairs, found {}",
            manifests.len()
        );

        let (exact, misses) = score_exact(&dir, &manifests, &LexicalCost);
        assert!(
            exact >= bar,
            "{name}: exact {exact}/{expected_pairs} is below the pinned baseline {bar}; misses:\n{}",
            misses.join("\n")
        );
        agg_exact += exact;
        agg_n += expected_pairs;
    }
    println!(
        "chimera dev parity: aggregate exact {agg_exact}/{agg_n} across {} seeds",
        SEEDS.len()
    );
}

/// A cost model that reports every step pair as identical (`0.0`). The aligner then sees no
/// divergence anywhere, so it localizes no fork — the degenerate control that must miss every bar.
struct BlindCost;
impl CostModel for BlindCost {
    fn cost(&self, _a: &Step, _b: &Step) -> f64 {
        0.0
    }
}

#[test]
fn blind_cost_model_fails_the_bar() {
    for &(name, _, bar) in SEEDS {
        let dir = fixture_dir(name);
        let (exact, _) = score_exact(&dir, &manifests(&dir), &BlindCost);
        assert!(
            exact < bar,
            "{name}: a blind (all-identical) cost model scored {exact} >= the bar {bar} — \
             the gate does not discriminate and is vacuous"
        );
    }
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
