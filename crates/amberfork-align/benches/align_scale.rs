//! Scale benchmark for issue #16: `diff()` wall time as run length grows.
//!
//! `LexicalCost` re-tokenizes both steps on every cost call, so the aligner's cost matrix does
//! O(n·m) tokenizations where O(n+m) would do (documented in `cost.rs`; measured in the wild in
//! notebook 020: 0.20s at 133×123 real steps, 12.6s at 1000×999). This benchmark is the
//! reproducible version of that curve — the "before" any caching change must be judged against,
//! and the harness that measures the "after".
//!
//! Method: long runs are stitched from the committed seed-42 dev fixture
//! (`bench/fixtures/chimera_noise_seed42_dev/`) by concatenating each side's runs in filename
//! order and cycling to the target length — real GAIA-derived step content, not synthetic
//! filler, so payload-serialization cost is represented. Steps are cloned (owned deep copies —
//! the notebook-020 scale probe was bitten by Python shallow copies; issue #16 comment) and
//! re-indexed; `parent_idx` is cleared because stitching invalidates cross-run parentage.
//! Everything is deterministic: same fixture, same order, no randomness.
//!
//! Run with `cargo bench -p amberfork-align`. The full baseline sweep takes a few minutes at
//! the pre-cache 12.6s/iteration top size; criterion may warn that the target time is too low
//! for 10 samples — it still collects them, just tells you it will be slow.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_model::{Run, SchemaVersion, Step};
use criterion::{BenchmarkId, Criterion, SamplingMode, criterion_group, criterion_main};
use std::path::{Path, PathBuf};

/// Stitched run lengths (steps per side). 125 approximates the real 133×123 pair notebook 020
/// timed at 0.20s; each next size doubles, ending at the 1000-step point measured at 12.6s.
const SIZES: &[usize] = &[125, 250, 500, 1000];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/fixtures/chimera_noise_seed42_dev")
}

/// All steps of one fixture side (`a` or `b`), runs concatenated in filename order.
fn side_steps(side: char) -> Vec<Step> {
    let dir = fixture_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{side}_")) && n.ends_with(".json"))
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no {side}_*.json runs in {}",
        dir.display()
    );
    files
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let run: Run = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            run.steps
        })
        .collect()
}

/// A well-formed `n`-step run cycled out of `source`: deep-copied steps, re-indexed from 0,
/// parentage cleared (a parent index from one fixture run is meaningless after stitching).
fn stitched_run(id: &str, source: &[Step], n: usize) -> Run {
    let steps = source
        .iter()
        .cycle()
        .take(n)
        .cloned()
        .enumerate()
        .map(|(idx, mut step)| {
            step.idx = idx;
            step.parent_idx = None;
            step
        })
        .collect();
    Run {
        schema_version: SchemaVersion::current(),
        id: id.to_string(),
        task: Some("scale probe (issue #16)".to_string()),
        outcome: None,
        steps,
        edges: None,
    }
}

fn bench_diff_scale(c: &mut Criterion) {
    let a_source = side_steps('a');
    let b_source = side_steps('b');
    let mut group = c.benchmark_group("diff_scale");
    // Long-running benchmark: flat sampling and the 10-sample minimum keep the sweep honest
    // without criterion's default 100 samples × 12.6s at the top size.
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    for &n in SIZES {
        let reference = stitched_run("stitched-a", &a_source, n);
        let observed = stitched_run("stitched-b", &b_source, n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| diff(&reference, &observed, &LexicalCost, &DiffParams::default()));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_diff_scale);
criterion_main!(benches);
