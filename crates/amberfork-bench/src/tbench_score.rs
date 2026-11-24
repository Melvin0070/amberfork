//! Scoring the Terminal-Bench natural pairs (docs/notebook.md 083, amended by 083a and 083b).
//!
//! There are no step-level cause labels in this corpus, so nothing here is a localization
//! score. Two label-free measurements, both fixed before the engine saw a pair:
//!
//! 1. **The gate.** How often the shipped diff reports a fork (exit 1) on two passing runs of
//!    the same agent — the false-positive rate of gating on two independent live runs — and how
//!    often on a pass×fail pair.
//! 2. **Discrimination.** Whether the *in-sync fraction* — paired steps the fork rule itself
//!    treats as matching (cost ≤ tau), over the alignment's length — separates pass×pass pairs
//!    from pass×fail pairs. Reported as AUROC, against two baselines on identical pairs: the
//!    same fraction computed index-wise with no alignment, and run-length ratio alone.
//!
//! The independent unit is the *task* (five trials of one task are not five observations), so
//! every interval resamples tasks, and an engine-vs-baseline difference is taken inside each
//! resample (BENCHMARK.md rule 9).

use crate::hash::{bounded, fnv1a64, splitmix64};
use crate::score::{Rate, wilson95};
use crate::tbench::{BuildReport, PairClass};
use amberfork_align::{CostModel, DiffParams, LexicalCost, diff};
use amberfork_model::{MoveKind, Run};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Bootstrap resamples, as registered in notebook 083.
pub const RESAMPLES: usize = 2000;
/// Seed for the task bootstrap. Arbitrary, committed, part of the protocol.
const BOOTSTRAP_SEED: u64 = 0x7B_0083;

/// The three scores compared, in table order (baselines first, product last).
pub const ARMS: [&str; 3] = ["length-only", "pos-lexical", "nw-lexical/resync"];

/// Everything measured on one pair. Scores are oriented so that *higher means more alike*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairScore {
    pub class: PairClass,
    pub task: String,
    pub agent: String,
    pub a_trial: String,
    pub b_trial: String,
    /// What `amberfork diff` exits with: 0 converged, 1 forked, 2 trouble.
    pub exit: u8,
    /// `min(len) / max(len)` — the baseline that knows nothing but run length.
    pub length_ratio: f64,
    /// Index-wise in-sync fraction: positions whose two steps cost ≤ tau, over the longer run.
    pub pos_in_sync: f64,
    /// The engine's in-sync fraction: sync moves with cost ≤ tau, over the alignment's length.
    pub engine_in_sync: f64,
    /// Secondary (083b): every `Sync` move, matching or not, over the alignment's length.
    pub engine_paired: f64,
}

impl PairScore {
    fn arm(&self, arm: usize) -> f64 {
        [self.length_ratio, self.pos_in_sync, self.engine_in_sync][arm]
    }
}

/// Score one pair with the frozen engine. `a` is the reference side.
#[must_use]
pub fn score_pair(a: &Run, b: &Run, params: &DiffParams) -> (u8, f64, f64, f64, f64) {
    let (short, long) = {
        let (x, y) = (a.steps.len(), b.steps.len());
        (x.min(y), x.max(y))
    };
    let length_ratio = ratio(short, long);
    let tau = params.fork.tau;
    let pos_matches = (0..short)
        .filter(|&i| LexicalCost.cost(&a.steps[i], &b.steps[i]) <= tau)
        .count();
    let pos_in_sync = ratio(pos_matches, long);

    match diff(a, b, &LexicalCost, params) {
        Ok(result) => {
            let moves = result.alignment.len();
            let paired = result
                .alignment
                .iter()
                .filter(|m| m.kind == MoveKind::Sync)
                .count();
            let in_sync = result
                .alignment
                .iter()
                .filter(|m| m.kind == MoveKind::Sync && m.cost <= tau)
                .count();
            (
                u8::from(result.fork.is_some()),
                length_ratio,
                pos_in_sync,
                ratio(in_sync, moves),
                ratio(paired, moves),
            )
        }
        // The size guard: the CLI's exit 2. The builder excludes over-long pairs, so this is
        // a harness fault if it ever fires — reported, never folded into a rate.
        Err(_) => (2, length_ratio, pos_in_sync, 0.0, 0.0),
    }
}

fn ratio(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// Area under the ROC curve for telling `positives` from `negatives` by score: the probability
/// a random positive scores above a random negative, ties counting half. `None` when either
/// class is empty.
#[must_use]
pub fn auroc(positives: &[f64], negatives: &[f64]) -> Option<f64> {
    if positives.is_empty() || negatives.is_empty() {
        return None;
    }
    // Rank-sum form: sort the negatives once, then count by binary search.
    let mut sorted = negatives.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut wins = 0.0;
    for &p in positives {
        let below = sorted.partition_point(|&n| n < p);
        let at_or_below = sorted.partition_point(|&n| n <= p);
        wins += below as f64 + 0.5 * (at_or_below - below) as f64;
    }
    Some(wins / (positives.len() as f64 * negatives.len() as f64))
}

/// A point estimate with its task-bootstrap 95% interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub value: f64,
    pub ci95_lo: f64,
    pub ci95_hi: f64,
}

/// The exit-code rates for one class of pair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GateRates {
    /// Pairs on which the diff reported a fork (exit 1), Wilson 95%.
    pub forked: Rate,
    /// Pairs the diff could not process (exit 2) — reported apart, never in `forked`.
    pub trouble: usize,
}

/// One scaffold's point estimates — no intervals; most scaffolds are too few tasks to carry one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScaffoldRow {
    pub agent: String,
    pub pass_fail: usize,
    pub pass_pass: usize,
    /// AUROC per arm, in [`ARMS`] order; `None` when a class is empty.
    pub auroc: [Option<f64>; 3],
}

/// The committed results document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TbenchResults {
    pub protocol: String,
    pub split: String,
    pub params_sha256: String,
    pub tau: f64,
    pub tasks: usize,
    pub pass_fail: usize,
    pub pass_pass: usize,
    pub excluded_pairs: usize,
    pub gate_pass_pass: GateRates,
    pub gate_pass_fail: GateRates,
    pub arms: Vec<String>,
    /// AUROC of each arm for separating pass×pass (positive) from pass×fail, in `arms` order.
    pub auroc: Vec<Interval>,
    /// Engine AUROC minus each baseline's, paired within each resample.
    pub engine_minus_length: Interval,
    pub engine_minus_pos: Interval,
    /// Secondary (083b): AUROC of the every-`Sync`-move fraction. No decision rests on it.
    pub secondary_engine_paired_auroc: Option<f64>,
    pub resamples: usize,
    pub by_scaffold: Vec<ScaffoldRow>,
    pub pairs: Vec<PairScore>,
}

/// Score every built pair on `split`, loading runs from `pairs_dir`.
///
/// # Errors
/// A run file that cannot be read or parsed. `pairs.json` itself is the caller's to load.
pub fn score_split(
    report: &BuildReport,
    pairs_dir: &Path,
    split: &str,
    params: &DiffParams,
) -> Result<Vec<PairScore>, Box<dyn std::error::Error>> {
    let load = |file: &str| -> Result<Run, Box<dyn std::error::Error>> {
        let path = pairs_dir.join(file);
        let text =
            std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        Ok(serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?)
    };
    let mut scores = Vec::new();
    for pair in report.pairs.iter().filter(|p| p.spec.split == split) {
        let (a, b) = (load(&pair.a)?, load(&pair.b)?);
        let (exit, length_ratio, pos_in_sync, engine_in_sync, engine_paired) =
            score_pair(&a, &b, params);
        scores.push(PairScore {
            class: pair.spec.class,
            task: pair.spec.task.clone(),
            agent: pair.spec.agent.clone(),
            a_trial: pair.spec.a_trial.clone(),
            b_trial: pair.spec.b_trial.clone(),
            exit,
            length_ratio,
            pos_in_sync,
            engine_in_sync,
            engine_paired,
        });
    }
    Ok(scores)
}

/// Per-arm AUROCs over a set of pairs, `None` when a class is empty.
fn aurocs(pairs: &[&PairScore]) -> Option<[f64; 3]> {
    let mut out = [0.0; 3];
    for (arm, slot) in out.iter_mut().enumerate() {
        let of = |class| -> Vec<f64> {
            pairs
                .iter()
                .filter(|p| p.class == class)
                .map(|p| p.arm(arm))
                .collect()
        };
        *slot = auroc(&of(PairClass::PassPass), &of(PairClass::PassFail))?;
    }
    Some(out)
}

fn gate(pairs: &[PairScore], class: PairClass) -> GateRates {
    let of: Vec<&PairScore> = pairs.iter().filter(|p| p.class == class).collect();
    let processed = of.iter().filter(|p| p.exit != 2).count();
    GateRates {
        forked: wilson95(of.iter().filter(|p| p.exit == 1).count(), processed.max(1)),
        trouble: of.len() - processed,
    }
}

/// Assemble the results document from per-pair scores.
///
/// # Panics
/// If either class is empty on this split — there is then nothing to separate, and a table
/// would be decoration.
#[must_use]
pub fn summarize(
    pairs: Vec<PairScore>,
    split: &str,
    params_sha256: &str,
    tau: f64,
    excluded_pairs: usize,
) -> TbenchResults {
    let all: Vec<&PairScore> = pairs.iter().collect();
    let point = aurocs(&all).expect("both classes present on the scored split");

    let mut by_task: BTreeMap<&str, Vec<&PairScore>> = BTreeMap::new();
    for pair in &pairs {
        by_task.entry(&pair.task).or_default().push(pair);
    }
    let tasks: Vec<&Vec<&PairScore>> = by_task.values().collect();

    let mut state = BOOTSTRAP_SEED ^ fnv1a64(b"tbench-task-bootstrap");
    let mut draws: [Vec<f64>; 5] = Default::default();
    while draws[0].len() < RESAMPLES {
        let mut sample: Vec<&PairScore> = Vec::new();
        for _ in 0..tasks.len() {
            sample.extend(tasks[bounded(splitmix64(&mut state), tasks.len())].iter());
        }
        // A resample that drew no pair of one class has no AUROC; redraw rather than impute.
        let Some(a) = aurocs(&sample) else { continue };
        for (slot, value) in draws
            .iter_mut()
            .zip([a[0], a[1], a[2], a[2] - a[0], a[2] - a[1]])
        {
            slot.push(value);
        }
    }
    let interval = |value: f64, draws: &mut Vec<f64>| {
        draws.sort_by(f64::total_cmp);
        let at = |q: f64| draws[((draws.len() as f64 * q) as usize).min(draws.len() - 1)];
        Interval {
            value,
            ci95_lo: at(0.025),
            ci95_hi: at(0.975),
        }
    };
    let [d_len, d_pos, d_eng, d_eng_len, d_eng_pos] = &mut draws;
    let auroc_intervals = vec![
        interval(point[0], d_len),
        interval(point[1], d_pos),
        interval(point[2], d_eng),
    ];
    let engine_minus_length = interval(point[2] - point[0], d_eng_len);
    let engine_minus_pos = interval(point[2] - point[1], d_eng_pos);

    let mut by_agent: BTreeMap<&str, Vec<&PairScore>> = BTreeMap::new();
    for pair in &pairs {
        by_agent.entry(&pair.agent).or_default().push(pair);
    }
    let by_scaffold = by_agent
        .into_iter()
        .map(|(agent, of)| {
            let scores = aurocs(&of);
            ScaffoldRow {
                agent: agent.to_owned(),
                pass_fail: of.iter().filter(|p| p.class == PairClass::PassFail).count(),
                pass_pass: of.iter().filter(|p| p.class == PairClass::PassPass).count(),
                auroc: [0, 1, 2].map(|arm| scores.map(|s| s[arm])),
            }
        })
        .collect();

    let class_scores = |class| -> Vec<f64> {
        pairs
            .iter()
            .filter(|p| p.class == class)
            .map(|p| p.engine_paired)
            .collect()
    };
    let secondary = auroc(
        &class_scores(PairClass::PassPass),
        &class_scores(PairClass::PassFail),
    );

    TbenchResults {
        protocol: "tbench-natural-pairs (notebook 083 + 083a + 083b)".to_owned(),
        split: split.to_owned(),
        params_sha256: params_sha256.to_owned(),
        tau,
        tasks: by_task.len(),
        pass_fail: pairs
            .iter()
            .filter(|p| p.class == PairClass::PassFail)
            .count(),
        pass_pass: pairs
            .iter()
            .filter(|p| p.class == PairClass::PassPass)
            .count(),
        excluded_pairs,
        gate_pass_pass: gate(&pairs, PairClass::PassPass),
        gate_pass_fail: gate(&pairs, PairClass::PassFail),
        arms: ARMS.iter().map(|&arm| arm.to_owned()).collect(),
        auroc: auroc_intervals,
        engine_minus_length,
        engine_minus_pos,
        secondary_engine_paired_auroc: secondary,
        resamples: RESAMPLES,
        by_scaffold,
        pairs,
    }
}

/// Render the results as the fixed-width table the notebook quotes.
#[must_use]
pub fn render(r: &TbenchResults) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let rate = |g: &GateRates| {
        format!(
            "{:.3} [{:.3}, {:.3}]  ({}/{}, {} trouble)",
            g.forked.rate, g.forked.ci95_lo, g.forked.ci95_hi, g.forked.hits, g.forked.n, g.trouble
        )
    };
    let _ = writeln!(
        out,
        "split {} · {} tasks · {} pass×fail · {} pass×pass · {} excluded",
        r.split, r.tasks, r.pass_fail, r.pass_pass, r.excluded_pairs
    );
    let _ = writeln!(
        out,
        "\ngate — fraction of pairs on which diff exits 1 (Wilson 95%)"
    );
    let _ = writeln!(
        out,
        "  pass×pass (false positives)  {}",
        rate(&r.gate_pass_pass)
    );
    let _ = writeln!(
        out,
        "  pass×fail                    {}",
        rate(&r.gate_pass_fail)
    );
    let _ = writeln!(
        out,
        "\ndiscrimination — AUROC, pass×pass vs pass×fail (95% CI, bootstrap over tasks, {} resamples)",
        r.resamples
    );
    for (arm, i) in r.arms.iter().zip(&r.auroc) {
        let _ = writeln!(
            out,
            "  {arm:<20} {:.3} [{:.3}, {:.3}]",
            i.value, i.ci95_lo, i.ci95_hi
        );
    }
    for (label, i) in [
        ("engine − length-only", &r.engine_minus_length),
        ("engine − pos-lexical", &r.engine_minus_pos),
    ] {
        let _ = writeln!(
            out,
            "  {label:<20} {:+.3} [{:+.3}, {:+.3}]",
            i.value, i.ci95_lo, i.ci95_hi
        );
    }
    let _ = writeln!(
        out,
        "\nby scaffold — AUROC point estimates (length · pos · engine)"
    );
    for row in &r.by_scaffold {
        let cell = |v: Option<f64>| v.map_or_else(|| "  —  ".to_owned(), |v| format!("{v:.3}"));
        let _ = writeln!(
            out,
            "  {:<18} {:>4} pf {:>4} pp   {} · {} · {}",
            row.agent,
            row.pass_fail,
            row.pass_pass,
            cell(row.auroc[0]),
            cell(row.auroc[1]),
            cell(row.auroc[2]),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support::{run, step};

    #[test]
    fn auroc_is_the_probability_a_positive_outscores_a_negative() {
        assert_eq!(auroc(&[0.9, 0.8], &[0.1, 0.2]), Some(1.0));
        assert_eq!(auroc(&[0.1, 0.2], &[0.9, 0.8]), Some(0.0));
        // 0.5 beats 0.4 and loses to 0.6; 0.7 beats both: 3 of 4.
        assert_eq!(auroc(&[0.5, 0.7], &[0.4, 0.6]), Some(0.75));
    }

    #[test]
    fn auroc_counts_a_tie_as_half_and_refuses_an_empty_class() {
        assert_eq!(auroc(&[0.5], &[0.5]), Some(0.5));
        assert_eq!(auroc(&[0.5, 0.5], &[0.5, 0.1]), Some(0.75));
        assert_eq!(auroc(&[], &[0.5]), None);
        assert_eq!(auroc(&[0.5], &[]), None);
    }

    fn text_run(id: &str, outputs: &[&str]) -> Run {
        run(
            id,
            outputs
                .iter()
                .enumerate()
                .map(|(i, text)| step(i, "tool").text_output(*text).build())
                .collect(),
        )
        .build()
    }

    #[test]
    fn identical_runs_converge_with_everything_in_sync() {
        let a = text_run("a", &["alpha one", "beta two", "gamma three"]);
        let (exit, length, pos, engine, paired) = score_pair(&a, &a, &DiffParams::default());
        assert_eq!((exit, length, pos, engine, paired), (0, 1.0, 1.0, 1.0, 1.0));
    }

    #[test]
    fn an_inserted_step_desyncs_position_but_not_alignment() {
        // The whole reason alignment exists: one extra step at the front shifts every index.
        let a = text_run(
            "a",
            &[
                "alpha one two",
                "beta three four",
                "gamma five six",
                "delta seven eight",
            ],
        );
        let b = text_run(
            "b",
            &[
                "retry notice zzz",
                "alpha one two",
                "beta three four",
                "gamma five six",
                "delta seven eight",
            ],
        );
        let (_, length, pos, engine, _) = score_pair(&a, &b, &DiffParams::default());
        assert_eq!(length, 0.8);
        assert_eq!(pos, 0.0, "index-wise, no step lines up");
        assert_eq!(engine, 0.8, "four of five moves are in sync");
    }

    fn pair(class: PairClass, task: &str, agent: &str, exit: u8, scores: [f64; 3]) -> PairScore {
        PairScore {
            class,
            task: task.to_owned(),
            agent: agent.to_owned(),
            a_trial: "a".to_owned(),
            b_trial: "b".to_owned(),
            exit,
            length_ratio: scores[0],
            pos_in_sync: scores[1],
            engine_in_sync: scores[2],
            engine_paired: scores[2],
        }
    }

    fn sample() -> Vec<PairScore> {
        // Length cannot separate the classes, position half can, the engine fully can.
        let mut pairs = Vec::new();
        for (i, task) in ["t1", "t2", "t3", "t4"].into_iter().enumerate() {
            let lift = i as f64 * 0.01;
            pairs.push(pair(
                PairClass::PassPass,
                task,
                "x",
                1,
                [0.5, 0.6 + lift, 0.9 + lift],
            ));
            pairs.push(pair(
                PairClass::PassFail,
                task,
                "x",
                1,
                [0.5, 0.6 + lift, 0.2 + lift],
            ));
        }
        pairs[0].exit = 0;
        pairs[1].exit = 2;
        pairs
    }

    #[test]
    fn the_summary_reports_gate_rates_with_trouble_kept_apart() {
        let r = summarize(sample(), "dev", "sha", 0.3, 7);
        assert_eq!(
            (r.tasks, r.pass_fail, r.pass_pass, r.excluded_pairs),
            (4, 4, 4, 7)
        );
        assert_eq!(
            (r.gate_pass_pass.forked.hits, r.gate_pass_pass.forked.n),
            (3, 4)
        );
        assert_eq!(r.gate_pass_pass.trouble, 0);
        // One pass×fail pair exited 2: out of the denominator, counted as trouble.
        assert_eq!(
            (r.gate_pass_fail.forked.hits, r.gate_pass_fail.forked.n),
            (3, 3)
        );
        assert_eq!(r.gate_pass_fail.trouble, 1);
    }

    #[test]
    fn the_summary_ranks_the_arms_and_pairs_the_differences() {
        let r = summarize(sample(), "dev", "sha", 0.3, 0);
        assert_eq!(r.auroc[0].value, 0.5, "length-only: all ties");
        assert_eq!(r.auroc[2].value, 1.0, "engine: perfect separation");
        assert_eq!(r.engine_minus_length.value, 0.5);
        assert_eq!(
            (r.engine_minus_length.ci95_lo, r.engine_minus_length.ci95_hi),
            (0.5, 0.5),
            "every resample has the same difference, so the paired interval is a point"
        );
        assert_eq!(r.by_scaffold.len(), 1);
        assert_eq!(r.by_scaffold[0].auroc[2], Some(1.0));
    }

    #[test]
    fn the_bootstrap_is_deterministic() {
        assert_eq!(
            summarize(sample(), "dev", "sha", 0.3, 0),
            summarize(sample(), "dev", "sha", 0.3, 0)
        );
    }
}
