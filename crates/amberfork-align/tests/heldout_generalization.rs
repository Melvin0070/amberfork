//! Generalization probe: the frozen fork params (τ=0.3, resync_k=2, gaps 0.6/0.3), calibrated
//! *only* on the Who&When-derived chimera family (notebook 001/007), run **unchanged** against a
//! trajectory of a deliberately different structural shape — a single-agent ReAct tool loop
//! (`llm` think → `tool` act/observe), where the chimera set is all multi-agent Magentic-One
//! orchestration (`kind: agent` throughout). Issue #42.
//!
//! **This is a held-out probe, not a dev set.** The params are never tuned against these pairs;
//! the fixture is built by the same mechanical injection as the chimera set (splice a reference
//! prefix + a different run's suffix at a known `gold_step`, plus one duplicated retry step and
//! light token-dropout rewording on the shared prefix), the frozen engine is run once, and the
//! numbers are reported honestly. The fixture is **synthetic and hand-authored** to vary the
//! *shape* — weaker evidence than a real third-party log; the strong version arrives with the
//! OpenInference/OTel adapter + TRAIL (#39/#41). See the fixture README for the honest caveat.
//!
//! Two guards keep the probe from rotting into a rubber stamp:
//! - `heldout_react_generalization_holds` reports exact / ±1 / ±3 and asserts a **conservative**
//!   ±3 floor — a regression guard set at-or-below the first honest frozen run, never a tuned
//!   parameter. A drop below it is a red CI that forces a notebook finding (issue #42 acceptance:
//!   no silent parameter tweak).
//! - `blind_cost_model_localizes_nothing` proves the fixture actually discriminates — an
//!   all-identical cost model sees no divergence and localizes no fork, hitting nothing.

use amberfork_align::{CostModel, DiffParams, LexicalCost, diff};
use amberfork_model::{Run, Step};
use std::path::{Path, PathBuf};

/// `(fixture dir, expected pair count)`. The held-out ReAct set is a single directory; unlike the
/// chimera gate there is no per-seed baseline — the probe reports and floors the ±3 window only.
const HELDOUT: (&str, usize) = ("heldout_react_v1", 6);

/// ±3 regression floor, pinned at the first honest frozen run (notebook 041): the ReAct set
/// localizes **6/6 within ±3** — exact 5/6, with `pair_06` two steps early. Unlike the chimera
/// gate this fixture is deterministic (no seed draw), so the floor sits *at* the observed value,
/// mirroring `chimera_parity`'s pin-at-baseline convention: any drop out of ±3 is a real
/// generalization regression that must trip CI and force a notebook entry, never a silent retune
/// (issue #42 acceptance). **Not** a tuned parameter — the engine stays frozen; this only guards it.
const PM3_FLOOR: usize = 6;

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

/// Windowed localization tally over one fixture set for one cost model: how many predictions land
/// exactly on gold, within ±1, and within ±3. An abstention (no fork predicted) is a miss in every
/// window with the denominator intact — the same honest reading BENCHMARK.md pins.
#[derive(Default)]
struct Windows {
    exact: usize,
    pm1: usize,
    pm3: usize,
    misses: Vec<String>,
}

fn score(dir: &Path, manifests: &[PathBuf], cost: &impl CostModel) -> Windows {
    let mut w = Windows::default();
    for manifest_path in manifests {
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;
        let reference = load_run(&dir.join(manifest["reference"].as_str().unwrap()));
        let failing = load_run(&dir.join(manifest["failing"].as_str().unwrap()));

        let result =
            diff(&reference, &failing, cost, &DiffParams::default()).expect("under the size guard");
        match result.fork_step_observed() {
            Some(pred) => {
                let delta = pred.abs_diff(gold);
                if delta == 0 {
                    w.exact += 1;
                }
                if delta <= 1 {
                    w.pm1 += 1;
                }
                if delta <= 3 {
                    w.pm3 += 1;
                } else {
                    w.misses.push(format!(
                        "{}: predicted {pred}, gold {gold} (Δ{delta})",
                        manifest_path.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
            None => w.misses.push(format!(
                "{}: no fork predicted, gold {gold}",
                manifest_path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    w
}

#[test]
fn heldout_react_generalization_holds() {
    let (name, expected_pairs) = HELDOUT;
    let dir = fixture_dir(name);
    let manifests = manifests(&dir);
    assert_eq!(
        manifests.len(),
        expected_pairs,
        "{name}: expected {expected_pairs} held-out pairs, found {}",
        manifests.len()
    );

    let w = score(&dir, &manifests, &LexicalCost);
    println!(
        "held-out ReAct generalization (frozen params): exact {}/{expected_pairs}, \
         ±1 {}/{expected_pairs}, ±3 {}/{expected_pairs}",
        w.exact, w.pm1, w.pm3
    );
    assert!(
        w.pm3 >= PM3_FLOOR,
        "{name}: ±3 localization {}/{expected_pairs} fell below the frozen-param floor {PM3_FLOOR} \
         — the fork rule regressed on the held-out shape; record it in the notebook, do not retune. \
         Misses:\n{}",
        w.pm3,
        w.misses.join("\n")
    );
}

/// A cost model that reports every step pair as identical (`0.0`): no divergence anywhere, so the
/// aligner localizes no fork. The degenerate control that must hit nothing — proof the fixture and
/// scoring actually discriminate rather than rubber-stamping any input.
struct BlindCost;
impl CostModel for BlindCost {
    type Prepared = ();

    fn prepare(&self, _step: &Step) {}

    fn cost_prepared(&self, _a: &(), _b: &()) -> f64 {
        0.0
    }
}

#[test]
fn blind_cost_model_localizes_nothing() {
    let (name, _) = HELDOUT;
    let dir = fixture_dir(name);
    let w = score(&dir, &manifests(&dir), &BlindCost);
    assert_eq!(
        w.pm3, 0,
        "{name}: a blind (all-identical) cost model localized {} pair(s) within ±3 — the fixture \
         does not discriminate and the probe is vacuous",
        w.pm3
    );
}
