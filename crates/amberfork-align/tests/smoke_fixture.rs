//! End-to-end fixture parity with the spike (issue #3 acceptance): on the committed smoke
//! pair — benign retry offset + reworded prefix, then a genuinely wrong `web.fetch` — the
//! Rust pipeline must localize the fork at the gold step, exactly as `spike/test_smoke.py`
//! does. The gold value lives in `pair_smoke.json`, the same manifest the spike reads.

use amberfork_align::{AlignParams, ForkParams, LexicalCost, align, find_fork};
use amberfork_model::Run;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/fixtures/smoke")
}

fn load_run(path: &Path) -> Run {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn smoke_pair_forks_at_gold_step() {
    let dir = fixture_dir();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pair_smoke.json")).unwrap())
            .unwrap();
    let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;

    // Manifest sides map onto the DiffResult convention: reference = a, failing = b.
    let reference = load_run(&dir.join(manifest["reference"].as_str().unwrap()));
    let failing = load_run(&dir.join(manifest["failing"].as_str().unwrap()));

    let moves = align(
        &reference.steps,
        &failing.steps,
        &LexicalCost,
        &AlignParams::default(),
    );
    let fork = find_fork(&moves, &ForkParams::default())
        .expect("smoke pair must fork — it encodes a real divergence");

    assert_eq!(
        fork.b_step,
        Some(gold),
        "fork must land on failing-run step {gold} (see spike/test_smoke.py)"
    );
    assert!(
        fork.confidence > 0.0,
        "a real divergence must not read as a marginal call"
    );
}

#[test]
fn smoke_runs_self_align_clean() {
    // Same data, converged case: each run against itself.
    for file in ["run_a.json", "run_b.json"] {
        let run = load_run(&fixture_dir().join(file));
        let moves = align(
            &run.steps,
            &run.steps,
            &LexicalCost,
            &AlignParams::default(),
        );
        assert_eq!(
            find_fork(&moves, &ForkParams::default()),
            None,
            "{file} vs itself must converge"
        );
    }
}
