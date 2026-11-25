//! The stratified TRAIL↔HAL score registered in notebook 092 (amending 070).
//!
//! 070 found that this corpus is two corpora: pairs whose two sides are comparable trajectories,
//! and pairs where a seven-step summary faces a fifty-step reference. This module assigns each pair
//! its stratum from **step counts and step names alone** — no gold step, no alignment — and then
//! scores the protocol's existing arms on all pairs and on the comparable stratum, side by side.
//!
//! Three things 092 fixed before any score exist here as code, so they cannot drift:
//! - [`is_comparable`]: ratio ≤ [`MAX_STEP_RATIO`] and at least [`MIN_SHARED_NAMES`] shared name;
//! - [`is_constant`]: an arm whose answers take fewer than two distinct values, an abstention
//!   counting as a value;
//! - [`is_null`]: the product arm must beat `random` on exact match with a paired-bootstrap 95%
//!   interval that excludes zero. Higher but not separable is a null.
//!
//! Nothing here is a parameter of the engine, and `bench/params.toml` is read, not changed.

use crate::arms::{self, Arm};
use crate::hash::{bounded, fnv1a64, splitmix64};
use crate::pairs::Pair;
use crate::score::{self, ArmScore};
use amberfork_align::DiffParams;
use amberfork_model::Run;
use serde::Serialize;
use std::collections::BTreeSet;

/// 092, condition 1: the longer run is at most this many times the shorter.
pub const MAX_STEP_RATIO: f64 = 3.0;
/// 092, condition 2: the two runs share at least this many distinct step names.
pub const MIN_SHARED_NAMES: usize = 1;
/// Paired-bootstrap resamples over pairs (092).
pub const RESAMPLES: usize = 2000;
const BOOTSTRAP_SEED: u64 = 0x0921_57A7;

/// A pair's label-free shape: what decides its stratum.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shape {
    pub n_failing: usize,
    pub n_reference: usize,
    pub step_ratio: f64,
    pub shared_names: usize,
    pub comparable: bool,
}

/// Read a pair's shape off its two runs.
#[must_use]
pub fn shape(failing: &Run, reference: &Run) -> Shape {
    let names = |run: &Run| -> BTreeSet<String> {
        run.steps.iter().map(|step| step.name.clone()).collect()
    };
    let (n_failing, n_reference) = (failing.steps.len(), reference.steps.len());
    let step_ratio = step_ratio(n_failing, n_reference);
    let shared_names = names(failing).intersection(&names(reference)).count();
    Shape {
        n_failing,
        n_reference,
        step_ratio,
        shared_names,
        comparable: is_comparable(step_ratio, shared_names),
    }
}

/// Longer ÷ shorter. An empty run compares with nothing, so its ratio is infinite.
fn step_ratio(a: usize, b: usize) -> f64 {
    let (short, long) = (a.min(b), a.max(b));
    if short == 0 {
        f64::INFINITY
    } else {
        long as f64 / short as f64
    }
}

/// 092's comparability rule.
#[must_use]
pub fn is_comparable(step_ratio: f64, shared_names: usize) -> bool {
    step_ratio <= MAX_STEP_RATIO && shared_names >= MIN_SHARED_NAMES
}

/// 092's "constant predictor": fewer than two distinct answers, an abstention counting as one.
#[must_use]
pub fn is_constant(answers: &[Option<usize>]) -> bool {
    answers.iter().collect::<BTreeSet<_>>().len() < 2
}

/// 092's null test for the product arm against `random`, on exact match.
#[must_use]
pub fn is_null(engine_rate: f64, random_rate: f64, difference: &Interval) -> bool {
    !(engine_rate > random_rate && difference.ci95_lo > 0.0)
}

/// A paired difference in exact-match rate with its bootstrap 95% interval.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Interval {
    pub delta: f64,
    pub ci95_lo: f64,
    pub ci95_hi: f64,
}

/// The paired difference `arm − baseline` in exact-match rate over the same pairs, resampling
/// pairs with replacement. Deterministic: the stream is seeded from `arm_name`.
#[must_use]
pub fn paired_exact_difference(
    arm_name: &str,
    arm: &[Option<usize>],
    baseline: &[Option<usize>],
    golds: &[usize],
) -> Interval {
    let per_pair: Vec<f64> = arm
        .iter()
        .zip(baseline)
        .zip(golds)
        .map(|((arm, baseline), gold)| {
            f64::from(u8::from(*arm == Some(*gold))) - f64::from(u8::from(*baseline == Some(*gold)))
        })
        .collect();
    let n = per_pair.len();
    if n == 0 {
        return Interval {
            delta: 0.0,
            ci95_lo: 0.0,
            ci95_hi: 0.0,
        };
    }
    let mean = |values: &mut dyn Iterator<Item = f64>| values.sum::<f64>() / n as f64;
    let delta = mean(&mut per_pair.iter().copied());
    let mut state = BOOTSTRAP_SEED ^ fnv1a64(arm_name.as_bytes());
    let mut means: Vec<f64> = (0..RESAMPLES)
        .map(|_| mean(&mut (0..n).map(|_| per_pair[bounded(splitmix64(&mut state), n)])))
        .collect();
    means.sort_by(f64::total_cmp);
    let at = |q: f64| means[((RESAMPLES - 1) as f64 * q).round() as usize];
    Interval {
        delta,
        ci95_lo: at(0.025),
        ci95_hi: at(0.975),
    }
}

/// One pair's row: its shape, its gold step, and every arm's answer.
#[derive(Debug, Clone, Serialize)]
pub struct PairRow {
    pub name: String,
    #[serde(flatten)]
    pub shape: Shape,
    pub gold_step: usize,
    /// One answer per arm, in [`arms::ALL`] order; `null` is an abstention.
    pub answers: Vec<Option<usize>>,
}

/// One arm on one set of pairs.
#[derive(Debug, Clone, Serialize)]
pub struct ArmSummary {
    pub arm: String,
    #[serde(flatten)]
    pub score: ArmScore,
    pub constant_predictor: bool,
    /// Exact-match difference against `random`; absent for `random` itself.
    pub vs_random: Option<Interval>,
}

/// Every arm on one set of pairs.
#[derive(Debug, Clone, Serialize)]
pub struct StratumSummary {
    pub n_pairs: usize,
    pub arms: Vec<ArmSummary>,
}

/// The published document.
#[derive(Debug, Clone, Serialize)]
pub struct Results {
    pub protocol: &'static str,
    pub notebook_registration: u32,
    pub split: String,
    pub params_sha256: String,
    pub max_step_ratio: f64,
    pub min_shared_names: usize,
    pub resamples: usize,
    pub arms: Vec<String>,
    pub all_pairs: StratumSummary,
    /// Absent when no pair is comparable.
    pub comparable: Option<StratumSummary>,
    /// 092's first check: the product arm is not a constant predictor on the comparable stratum.
    pub product_discriminates: bool,
    /// 092's retirement test on the comparable stratum.
    pub null: bool,
    pub pairs: Vec<PairRow>,
}

/// The arm that stands for the product.
const PRODUCT: Arm = Arm::NwLexical;

/// Ask every arm about every pair.
#[must_use]
pub fn rows(pairs: &[Pair], params: &DiffParams) -> Vec<PairRow> {
    pairs
        .iter()
        .map(|pair| PairRow {
            name: pair.name.clone(),
            shape: shape(&pair.failing, &pair.reference),
            gold_step: pair.gold_step,
            answers: arms::ALL
                .iter()
                .map(|arm| arm.predict(pair, params).map(|prediction| prediction.step))
                .collect(),
        })
        .collect()
}

fn summarize_stratum(rows: &[&PairRow]) -> StratumSummary {
    let golds: Vec<usize> = rows.iter().map(|row| row.gold_step).collect();
    let column = |index: usize| -> Vec<Option<usize>> {
        rows.iter().map(|row| row.answers[index]).collect()
    };
    let random_index = arms::ALL
        .iter()
        .position(|arm| *arm == Arm::Random)
        .expect("the protocol has a random arm");
    let random = column(random_index);
    let arms = arms::ALL
        .iter()
        .enumerate()
        .map(|(index, arm)| {
            let answers = column(index);
            ArmSummary {
                arm: arm.name().to_string(),
                score: score::score(&answers, &golds),
                constant_predictor: is_constant(&answers),
                vs_random: (index != random_index)
                    .then(|| paired_exact_difference(arm.name(), &answers, &random, &golds)),
            }
        })
        .collect();
    StratumSummary {
        n_pairs: rows.len(),
        arms,
    }
}

/// Score all pairs and the comparable stratum, and apply 092's two tests.
///
/// # Panics
/// If `rows` is empty — an empty split is a caller error, as in the protocol's own scorer.
#[must_use]
pub fn summarize(rows: Vec<PairRow>, split: &str, params_sha256: &str) -> Results {
    let all: Vec<&PairRow> = rows.iter().collect();
    let comparable_rows: Vec<&PairRow> = rows.iter().filter(|row| row.shape.comparable).collect();
    let all_pairs = summarize_stratum(&all);
    let comparable = (!comparable_rows.is_empty()).then(|| summarize_stratum(&comparable_rows));

    let product = |summary: &StratumSummary| -> ArmSummary {
        summary
            .arms
            .iter()
            .find(|arm| arm.arm == PRODUCT.name())
            .expect("the product arm is scored")
            .clone()
    };
    let random_rate = |summary: &StratumSummary| -> f64 {
        summary
            .arms
            .iter()
            .find(|arm| arm.arm == Arm::Random.name())
            .expect("the random arm is scored")
            .score
            .exact
            .rate
    };
    let (product_discriminates, null) = comparable.as_ref().map_or((false, true), |summary| {
        let engine = product(summary);
        let difference = engine
            .vs_random
            .as_ref()
            .expect("the product is not random");
        (
            !engine.constant_predictor,
            is_null(engine.score.exact.rate, random_rate(summary), difference),
        )
    });

    Results {
        protocol: "trail-strata",
        notebook_registration: 92,
        split: split.to_string(),
        params_sha256: params_sha256.to_string(),
        max_step_ratio: MAX_STEP_RATIO,
        min_shared_names: MIN_SHARED_NAMES,
        resamples: RESAMPLES,
        arms: arms::ALL.iter().map(|arm| arm.name().to_string()).collect(),
        all_pairs,
        comparable,
        product_discriminates,
        null,
        pairs: rows,
    }
}

/// The human-readable report: per-pair rows, then both strata, then the two verdicts.
#[must_use]
pub fn render(results: &Results) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let answer = |a: Option<usize>| a.map_or_else(|| "—".to_string(), |step| step.to_string());

    let _ = writeln!(
        out,
        "comparable = step ratio <= {} and >= {} shared step name(s)\n",
        results.max_step_ratio, results.min_shared_names
    );
    let _ = writeln!(
        out,
        "{:<10} {:>5} {:>5} {:>6} {:>6} {:>5} {:>5}  {}",
        "pair",
        "fail",
        "ref",
        "ratio",
        "shared",
        "strat",
        "gold",
        results.arms.join(" | ")
    );
    for row in &results.pairs {
        let answers: Vec<String> = row.answers.iter().map(|a| answer(*a)).collect();
        let _ = writeln!(
            out,
            "{:<10} {:>5} {:>5} {:>6.2} {:>6} {:>5} {:>5}  {}",
            row.name,
            row.shape.n_failing,
            row.shape.n_reference,
            row.shape.step_ratio,
            row.shape.shared_names,
            if row.shape.comparable { "comp" } else { "—" },
            row.gold_step,
            answers.join(" | ")
        );
    }

    let mut stratum = |title: &str, summary: &StratumSummary| {
        let _ = writeln!(out, "\n{title} (n={})", summary.n_pairs);
        let _ = writeln!(
            out,
            "{:<22} {:>18} {:>6} {:>6} {:>8} {:>9}  exact vs random [95%]",
            "arm", "exact [95%]", "±1", "±3", "abstain", "constant"
        );
        for arm in &summary.arms {
            let score = &arm.score;
            let versus = arm.vs_random.as_ref().map_or_else(String::new, |d| {
                format!("{:+.3} [{:+.3}, {:+.3}]", d.delta, d.ci95_lo, d.ci95_hi)
            });
            let _ = writeln!(
                out,
                "{:<22} {:>5.3} [{:.2}, {:.2}] {:>6.3} {:>6.3} {:>8.3} {:>9}  {}",
                arm.arm,
                score.exact.rate,
                score.exact.ci95_lo,
                score.exact.ci95_hi,
                score.w1.rate,
                score.w3.rate,
                score.no_pred.rate,
                if arm.constant_predictor { "YES" } else { "no" },
                versus
            );
        }
    };
    stratum("all pairs", &results.all_pairs);
    match &results.comparable {
        Some(summary) => stratum("comparable stratum", summary),
        None => {
            let _ = writeln!(out, "\ncomparable stratum: no pair qualifies");
        }
    }

    let _ = writeln!(
        out,
        "\ncheck 1 — product arm discriminates on the comparable stratum: {}",
        if results.product_discriminates {
            "PASS"
        } else {
            "FAIL"
        }
    );
    let _ = writeln!(
        out,
        "check 2 — exact match beats random with a paired 95% interval above zero: {}",
        if results.null { "NO (null)" } else { "YES" }
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{SchemaVersion, Step, StepKind};
    use serde_json::Map;

    fn run(names: &[&str]) -> Run {
        Run {
            schema_version: SchemaVersion::current(),
            id: "r".to_string(),
            task: None,
            outcome: None,
            steps: names
                .iter()
                .enumerate()
                .map(|(idx, name)| Step {
                    idx,
                    kind: StepKind::Tool,
                    name: (*name).to_string(),
                    inputs: None,
                    outputs: None,
                    attrs: Map::new(),
                    t_start: None,
                    t_end: None,
                    parent_idx: None,
                })
                .collect(),
            edges: None,
        }
    }

    #[test]
    fn shape_counts_steps_and_distinct_shared_names() {
        let failing = run(&["search", "search", "visit", "answer"]);
        let reference = run(&["search", "visit", "visit", "read", "read", "read"]);
        let got = shape(&failing, &reference);
        assert_eq!((got.n_failing, got.n_reference), (4, 6));
        assert!((got.step_ratio - 1.5).abs() < 1e-12);
        assert_eq!(got.shared_names, 2, "search and visit, each counted once");
        assert!(got.comparable);
    }

    #[test]
    fn the_ratio_is_longer_over_shorter_whichever_side_is_longer() {
        let short = run(&["a"]);
        let long = run(&["a", "b", "c", "d"]);
        assert!((shape(&short, &long).step_ratio - 4.0).abs() < 1e-12);
        assert!((shape(&long, &short).step_ratio - 4.0).abs() < 1e-12);
    }

    #[test]
    fn comparability_needs_both_conditions_and_the_ratio_bound_is_inclusive() {
        assert!(is_comparable(3.0, 1), "exactly at the bound is comparable");
        assert!(
            !is_comparable(3.01, 5),
            "too lopsided, however much is shared"
        );
        assert!(!is_comparable(1.0, 0), "same length, nothing shared");
        assert!(is_comparable(1.0, 1));
    }

    #[test]
    fn an_empty_run_is_never_comparable() {
        let got = shape(&run(&[]), &run(&["a"]));
        assert!(got.step_ratio.is_infinite());
        assert!(!got.comparable);
    }

    #[test]
    fn a_constant_predictor_gives_fewer_than_two_distinct_answers() {
        assert!(is_constant(&[Some(0), Some(0), Some(0)]));
        assert!(
            is_constant(&[None, None]),
            "abstaining on everything is constant"
        );
        assert!(is_constant(&[]));
        assert!(!is_constant(&[Some(0), Some(4)]));
        assert!(
            !is_constant(&[Some(0), None]),
            "an abstention is a distinct answer"
        );
    }

    #[test]
    fn the_paired_difference_is_the_gap_in_exact_hits_and_is_deterministic() {
        let golds = [1, 2, 3, 4];
        let arm = [Some(1), Some(2), Some(3), None];
        let baseline = [Some(9), Some(2), None, Some(9)];
        let first = paired_exact_difference("arm", &arm, &baseline, &golds);
        // Arm hits 3 of 4, baseline 1 of 4.
        assert!((first.delta - 0.5).abs() < 1e-12);
        assert!(first.ci95_lo <= first.delta && first.delta <= first.ci95_hi);
        assert_eq!(
            first,
            paired_exact_difference("arm", &arm, &baseline, &golds)
        );
    }

    #[test]
    fn identical_arms_have_a_zero_difference_with_a_zero_interval() {
        let golds = [1, 2, 3];
        let answers = [Some(1), Some(9), None];
        let same = paired_exact_difference("arm", &answers, &answers, &golds);
        assert_eq!((same.delta, same.ci95_lo, same.ci95_hi), (0.0, 0.0, 0.0));
    }

    #[test]
    fn higher_but_not_separable_is_a_null() {
        let separable = Interval {
            delta: 0.4,
            ci95_lo: 0.1,
            ci95_hi: 0.7,
        };
        let straddles_zero = Interval {
            delta: 0.2,
            ci95_lo: -0.1,
            ci95_hi: 0.5,
        };
        let touches_zero = Interval {
            delta: 0.2,
            ci95_lo: 0.0,
            ci95_hi: 0.5,
        };
        assert!(!is_null(0.5, 0.1, &separable));
        assert!(is_null(0.3, 0.1, &straddles_zero));
        assert!(
            is_null(0.3, 0.1, &touches_zero),
            "the interval must exclude zero"
        );
        assert!(
            is_null(0.1, 0.1, &separable),
            "the rate must actually be higher"
        );
    }

    fn row(name: &str, comparable: bool, gold: usize, answers: [Option<usize>; 4]) -> PairRow {
        PairRow {
            name: name.to_string(),
            shape: Shape {
                n_failing: 10,
                n_reference: 10,
                step_ratio: 1.0,
                shared_names: usize::from(comparable),
                comparable,
            },
            gold_step: gold,
            answers: answers.to_vec(),
        }
    }

    /// The column of [`arms::ALL`] that holds `arm`.
    fn column_of(arm: Arm) -> usize {
        arms::ALL.iter().position(|a| *a == arm).unwrap()
    }

    fn answers(product: Option<usize>, random: Option<usize>) -> [Option<usize>; 4] {
        let mut row = [Some(0); 4];
        row[column_of(PRODUCT)] = product;
        row[column_of(Arm::Random)] = random;
        row
    }

    #[test]
    fn the_summary_scores_both_strata_and_keeps_them_apart() {
        let rows = vec![
            row("p0", true, 3, answers(Some(3), Some(0))),
            row("p1", true, 5, answers(Some(5), Some(1))),
            row("p2", false, 2, answers(None, Some(2))),
        ];
        let results = summarize(rows, "dev", "abc");
        assert_eq!(results.all_pairs.n_pairs, 3);
        let comparable = results
            .comparable
            .as_ref()
            .expect("two pairs are comparable");
        assert_eq!(comparable.n_pairs, 2);

        let product = |summary: &StratumSummary| {
            summary
                .arms
                .iter()
                .find(|arm| arm.arm == PRODUCT.name())
                .unwrap()
                .clone()
        };
        assert_eq!(product(comparable).score.exact.hits, 2);
        assert_eq!(product(&results.all_pairs).score.exact.hits, 2);
        assert_eq!(product(&results.all_pairs).score.no_pred.hits, 1);
        assert!(results.product_discriminates);
        assert!(
            results
                .all_pairs
                .arms
                .iter()
                .find(|arm| arm.arm == Arm::Random.name())
                .unwrap()
                .vs_random
                .is_none()
        );
    }

    #[test]
    fn a_product_arm_that_answers_one_thing_fails_the_first_check() {
        let rows = vec![
            row("p0", true, 3, answers(Some(0), Some(1))),
            row("p1", true, 5, answers(Some(0), Some(2))),
        ];
        let results = summarize(rows, "dev", "abc");
        assert!(!results.product_discriminates);
        assert!(results.null);
    }

    #[test]
    fn no_comparable_pair_is_a_failed_check_and_a_null_not_a_panic() {
        let results = summarize(
            vec![row("p0", false, 1, answers(Some(1), Some(0)))],
            "dev",
            "abc",
        );
        assert!(results.comparable.is_none());
        assert!(!results.product_discriminates);
        assert!(results.null);
        assert!(render(&results).contains("no pair qualifies"));
    }
}
