//! Issue #3's quantitative bar: on the seed-42 n=20 noise chimera pairs, the Rust pipeline
//! must **match or beat the spike's 70% exact** fork localization (notebook 003 measured the
//! token-level cost model at 75% via the Python harness; this test checks the Rust port).
//!
//! `#[ignore]` because the pairs are NOT in the repo and must not be: their content derives
//! from Who&When logs whose questions originate in GAIA (gated upstream — redistribution
//! unresolved, notebook 001 / T30). Regenerate locally with `python3 spike/make_pairs.py`,
//! then run `cargo test -p amberfork-align --test chimera_parity -- --ignored`.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_model::{DiffResult, Run};
use std::path::{Path, PathBuf};

/// Spike's recorded number on these pairs: 14/20 exact (spike/out/noise, notebook 001–003).
const SPIKE_BAR: usize = 14;
const EXPECTED_PAIRS: usize = 20;

fn pairs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/data/pairs_noise")
}

fn load_run(path: &Path) -> Run {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// The failing-run step a fork points at, for scoring against a failing-side gold index.
/// A fork move that has a `b` side names it directly; a model-only fork (the failing run
/// skipped steps) counts the failing steps consumed before it — the spike's `a_pos` rule.
fn predicted_failing_step(result: &DiffResult, n_failing: usize) -> Option<usize> {
    let fork = result.fork?;
    let fork_move = result.alignment[fork.index];
    fork_move.b_idx.or_else(|| {
        let consumed = result.alignment[..fork.index]
            .iter()
            .filter(|m| m.b_idx.is_some())
            .count();
        Some(consumed.min(n_failing.saturating_sub(1)))
    })
}

#[test]
#[ignore = "needs local spike/data/pairs_noise (python3 spike/make_pairs.py); not committed per GAIA licensing (notebook 001/T30)"]
fn chimera_noise_localization_meets_spike_bar() {
    let dir = pairs_dir();
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "read {}: {e} — regenerate via spike/make_pairs.py",
                dir.display()
            )
        })
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
        "expected the seed-42 n=20 set; found {} manifests",
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
        let pred = predicted_failing_step(&result, failing.steps.len());
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
        exact >= SPIKE_BAR,
        "exact {exact}/{EXPECTED_PAIRS} is below the spike bar {SPIKE_BAR}/{EXPECTED_PAIRS}; misses:\n{}",
        misses.join("\n")
    );
    println!("chimera parity: exact {exact}/{EXPECTED_PAIRS} (spike bar {SPIKE_BAR})");
}
