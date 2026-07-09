//! The protocol arms — every method that competes on the fixtures (BENCHMARK.md rule 8:
//! factorial baselines on identical fixtures, same split, same metrics).
//!
//! The factorial design isolates one variable per rung, so the published table can say what
//! each ingredient buys:
//! - [`Arm::Random`] floors the metrics — any arm below it is worse than guessing.
//! - [`Arm::PosLexical`] uses the product's cost model with NO alignment (index-wise
//!   first-mismatch — the agent-replay / `tape_diff.py` approach). The gap between this and
//!   the product is what *alignment* adds over position.
//! - [`Arm::NwStructural`] uses the product's aligner + fork rule with a content-blind 0/1
//!   `(kind, name)` cost. The gap between this and the product is what *content* adds over
//!   structure (notebook 001: agent names cycle, the fork lives in content).
//! - [`Arm::NwLexical`] is the shipped engine, verbatim.
//!
//! Determinism (protocol rule 5): the random arm draws from an in-crate splitmix64 stream
//! seeded by a committed constant mixed with the pair's *name* — not its position — so a
//! reordered or extended pair set never changes another pair's draw, and no external RNG
//! crate can shift the numbers under a version bump.

use crate::hash::fnv1a64;
use crate::pairs::Pair;
use amberfork_align::{CostModel, DiffParams, LexicalCost, diff};
use amberfork_model::Step;

/// Base seed for the random arm. Arbitrary, committed, part of the frozen protocol.
const RANDOM_ARM_SEED: u64 = 0xA6BE_12F0;

/// Every arm the harness runs, in table order (floor first, product last).
pub const ALL: [Arm; 4] = [
    Arm::Random,
    Arm::PosLexical,
    Arm::NwStructural,
    Arm::NwLexical,
];

/// One protocol arm: a named strategy that predicts the failing-run fork step for a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    Random,
    PosLexical,
    NwStructural,
    NwLexical,
}

impl Arm {
    /// The arm's name in tables and results JSON — the spike's vocabulary, kept so numbers
    /// cross-reference against `spike/out/*` grids.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::PosLexical => "pos-lexical",
            Self::NwStructural => "nw-structural/resync",
            Self::NwLexical => "nw-lexical/resync",
        }
    }

    /// Predict the failing-run step where this pair forks, or `None` to abstain.
    #[must_use]
    pub fn predict(self, pair: &Pair, params: &DiffParams) -> Option<usize> {
        match self {
            Self::Random => random_step(pair),
            Self::PosLexical => positional_first_mismatch(pair, params.fork.tau),
            Self::NwStructural => {
                diff(&pair.reference, &pair.failing, &StructuralCost, params).fork_step_observed()
            }
            Self::NwLexical => {
                diff(&pair.reference, &pair.failing, &LexicalCost, params).fork_step_observed()
            }
        }
    }
}

/// The floor: one uniform draw over the failing run's steps, from the pair's own stream.
fn random_step(pair: &Pair) -> Option<usize> {
    let len = pair.failing.steps.len();
    if len == 0 {
        return None;
    }
    let mut state = RANDOM_ARM_SEED ^ fnv1a64(pair.name.as_bytes());
    Some(bounded(splitmix64(&mut state), len))
}

/// The no-alignment control: walk both runs index-wise and call the first step whose cost
/// exceeds `tau` the fork. When the overlap is clean but the lengths differ, the first
/// unmatched tail step is the mismatch (clamped to the last failing step when the failing
/// run is the shorter side). Desyncs on any insertion — that is the point.
fn positional_first_mismatch(pair: &Pair, tau: f64) -> Option<usize> {
    let n_fail = pair.failing.steps.len();
    if n_fail == 0 {
        return None;
    }
    let overlap = pair.reference.steps.len().min(n_fail);
    for i in 0..overlap {
        if LexicalCost.cost(&pair.reference.steps[i], &pair.failing.steps[i]) > tau {
            return Some(i);
        }
    }
    if pair.reference.steps.len() != n_fail {
        return Some(overlap.min(n_fail - 1));
    }
    None
}

/// Content-blind 0/1 cost: two steps are identical when `(kind, name)` match, else maximally
/// far. The "structure-only" arm of notebook 001.
struct StructuralCost;

impl CostModel for StructuralCost {
    fn cost(&self, a: &Step, b: &Step) -> f64 {
        if a.kind == b.kind && a.name == b.name {
            0.0
        } else {
            1.0
        }
    }
}

/// One splitmix64 output (Vigna's mixer): advances `state` and returns the next value.
/// Chosen because it is tiny, public-domain, and fixed for all time — the stream is part of
/// the reproducibility promise.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Map a full-width draw onto `[0, len)` by widening multiply (Lemire). Bias is O(len/2⁶⁴) —
/// immaterial at run lengths — and it needs no rejection loop.
fn bounded(draw: u64, len: usize) -> usize {
    let wide = u128::from(draw) * (len as u128);
    (wide >> 64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Run, SchemaVersion, StepKind};
    use serde_json::Map;

    fn step(idx: usize, kind: StepKind, name: &str, out: &str) -> Step {
        Step {
            idx,
            kind,
            name: name.to_string(),
            inputs: None,
            outputs: Some(amberfork_model::Payload::Text(out.to_string())),
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        }
    }

    fn run(id: &str, names_outs: &[(&str, &str)]) -> Run {
        Run {
            schema_version: SchemaVersion::current(),
            id: id.to_string(),
            task: None,
            outcome: None,
            steps: names_outs
                .iter()
                .enumerate()
                .map(|(i, (n, o))| step(i, StepKind::Tool, n, o))
                .collect(),
            edges: None,
        }
    }

    fn pair(name: &str, reference: Run, failing: Run) -> Pair {
        let task_key = reference.id.clone();
        Pair {
            name: name.to_string(),
            split: crate::split::Split::of(&task_key),
            task_key,
            reference,
            failing,
            gold_step: 0,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn splitmix64_matches_the_published_vectors() {
        // First outputs for seeds 0 and 1234567, from Vigna's reference implementation.
        let mut state = 0u64;
        assert_eq!(splitmix64(&mut state), 0xE220_A839_7B1D_CDAF);
        let mut state = 1_234_567u64;
        assert_eq!(splitmix64(&mut state), 0x599E_D017_FB08_FC85);
    }

    #[test]
    fn bounded_draws_stay_in_range_and_split_evenly_at_the_edges() {
        assert_eq!(bounded(0, 10), 0);
        assert_eq!(bounded(u64::MAX, 10), 9);
        // Widening multiply: the draw maps proportionally, so mid-range lands mid-interval.
        assert_eq!(bounded(u64::MAX / 2 + 1, 2), 1);
        for len in [1, 7, 100] {
            assert!(bounded(0x9E37_79B9_7F4A_7C15, len) < len);
        }
    }

    #[test]
    fn random_arm_is_deterministic_per_pair_name_not_position() {
        let reference = run("ref", &[("a", "x"), ("b", "y"), ("c", "z")]);
        let failing = run("fail", &[("a", "x"), ("b", "y"), ("c", "z")]);
        let p = pair("pair_00", reference.clone(), failing.clone());
        let first = Arm::Random.predict(&p, &DiffParams::default());
        let again = Arm::Random.predict(&p, &DiffParams::default());
        assert_eq!(first, again, "same name + same length = same draw");
        assert!(first.expect("non-empty run always draws") < 3);

        let renamed = pair("pair_99", reference, failing);
        // Different name seeds a different stream. (Equal draws are possible in principle;
        // these two names differ — pinned so a seeding regression can't hide.)
        assert_ne!(Arm::Random.predict(&renamed, &DiffParams::default()), first);
    }

    #[test]
    fn positional_flags_the_first_costly_index() {
        let reference = run(
            "ref",
            &[("plan", "search the census"), ("fetch", "census.gov")],
        );
        let failing = run(
            "fail",
            &[
                ("plan", "search the census"),
                ("fetch", "a blog post instead"),
            ],
        );
        let p = pair("p", reference, failing);
        assert_eq!(Arm::PosLexical.predict(&p, &DiffParams::default()), Some(1));
    }

    #[test]
    fn positional_abstains_on_an_identical_equal_length_pair() {
        let reference = run("ref", &[("plan", "x"), ("fetch", "y")]);
        let failing = run("fail", &[("plan", "x"), ("fetch", "y")]);
        let p = pair("p", reference, failing);
        assert_eq!(Arm::PosLexical.predict(&p, &DiffParams::default()), None);
    }

    #[test]
    fn positional_calls_a_clean_overlap_with_extra_failing_tail_at_the_first_extra_step() {
        let reference = run("ref", &[("plan", "x"), ("fetch", "y")]);
        let failing = run("fail", &[("plan", "x"), ("fetch", "y"), ("retry", "z")]);
        let p = pair("p", reference, failing);
        assert_eq!(
            Arm::PosLexical.predict(&p, &DiffParams::default()),
            Some(2),
            "the first unmatched failing step is the mismatch"
        );
    }

    #[test]
    fn positional_clamps_a_truncated_failing_run_to_its_last_step() {
        let reference = run("ref", &[("plan", "x"), ("fetch", "y"), ("answer", "z")]);
        let failing = run("fail", &[("plan", "x"), ("fetch", "y")]);
        let p = pair("p", reference, failing);
        assert_eq!(
            Arm::PosLexical.predict(&p, &DiffParams::default()),
            Some(1),
            "the missing tail is the mismatch, pointed at the last real failing step"
        );
    }

    #[test]
    fn structural_cost_reads_kind_and_name_only() {
        let a = step(0, StepKind::Tool, "web.search", "nine results");
        let same_shape = step(
            3,
            StepKind::Tool,
            "web.search",
            "completely different output",
        );
        let other_name = step(0, StepKind::Tool, "web.fetch", "nine results");
        let other_kind = step(0, StepKind::Llm, "web.search", "nine results");
        assert_eq!(StructuralCost.cost(&a, &same_shape), 0.0);
        assert_eq!(StructuralCost.cost(&a, &other_name), 1.0);
        assert_eq!(StructuralCost.cost(&a, &other_kind), 1.0);
    }

    #[test]
    fn arm_names_are_the_spike_vocabulary() {
        let names: Vec<&str> = ALL.iter().map(|arm| arm.name()).collect();
        assert_eq!(
            names,
            [
                "random",
                "pos-lexical",
                "nw-structural/resync",
                "nw-lexical/resync"
            ]
        );
    }
}
