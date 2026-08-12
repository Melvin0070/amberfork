//! Scoring the LLM-judge baseline arms (issue #46 slice A3a, registered in notebook 069).
//!
//! A separate experiment from `run`, deliberately — the same shape `consensus` took for #45.
//! Protocol rule 10 says a baseline must not disturb the product's numbers: the judge arms do
//! not join [`crate::arms::ALL`], `run`'s four-arm tables keep their exact bytes, and every
//! committed results document published before this slice stands untouched. What this module
//! emits is its own document, with its own denominators named.
//!
//! amberfork's four arms ARE re-scored here, on the identical pairs, from the same frozen
//! params — because rule 9's paired interval needs `nw-lexical`'s hits on the very pairs the
//! judge answered, and quoting a previously published rate computed over a different subset
//! would be the exact sleight of hand the rule exists to prevent.
//!
//! Three registered distinctions the code keeps apart, because collapsing any of them would
//! flatter a baseline:
//!
//! - **A parse failure is a miss**, counted and reported. The arm answered; the answer broke
//!   its own output contract.
//! - **A transport failure is an exclusion** (rule 4), tabulated with its reason and removed
//!   from that arm's denominator. Our infrastructure failed, not the method.
//! - **A missing cassette is neither.** In replay-only mode it stops the run: an operator who
//!   has not recorded the answers has no data, and publishing "0 of 23 evaluated" as though
//!   that were a finding is worse than failing loudly.

use crate::arms::{self, Arm};
use crate::hash::fnv1a64;
use crate::judge_answer;
use crate::judge_cassette::{self, Cassettes, Mode, Question};
use crate::judge_prompt::{JudgeArm, PromptSet};
use crate::judge_provider::{Decoding, Localizer};
use crate::multiref::{PairedInterval, bootstrap_paired};
use crate::pairs::Pair;
use crate::score::{ArmScore, score};
use amberfork_align::DiffParams;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Document version for the judge results artifact.
pub const JUDGE_SCHEMA_VERSION: &str = "0.1";

/// Committed bootstrap seed (rule 5: the published interval must reproduce). Distinct from
/// `multiref`'s so the two experiments cannot accidentally share a resample sequence.
const BOOTSTRAP_SEED: u64 = 0x4A55_4447;

/// Everything one judge run needs besides the pairs themselves.
pub struct Config<'a> {
    pub arms: &'a [JudgeArm],
    pub prompts: &'a PromptSet,
    pub cassettes: &'a Cassettes,
    pub mode: Mode,
    pub localizer: &'a dyn Localizer,
    pub params: &'a DiffParams,
    pub backoff: Duration,
}

/// Why a run could not produce a document at all.
#[derive(Debug)]
pub enum RunError {
    /// Replay-only, and these questions have no recorded answer. Every one is listed: an
    /// operator preparing a live run wants the whole list, not the first of twenty-three.
    MissingCassettes(Vec<PathBuf>),
    /// A cassette exists but cannot be trusted (malformed, or filed under a foreign key).
    Cassette(String),
    /// A prompt could not be rendered for a pair.
    Render(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCassettes(paths) => {
                writeln!(
                    f,
                    "{} question(s) have no recorded answer; replay-only mode does not call a \
                     provider. Re-run with --live and an API key to record them:",
                    paths.len()
                )?;
                for path in paths {
                    writeln!(f, "  {}", path.display())?;
                }
                Ok(())
            }
            Self::Cassette(msg) | Self::Render(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for RunError {}

/// One arm's outcome on one pair.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// A step index the arm named.
    Predicted(usize),
    /// The arm answered but broke its output contract, or (stepwise) never said `true`.
    Miss(MissReason),
    /// The arm could not be asked. Rule 4: counted, tabulated, out of the denominator.
    Excluded(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MissReason {
    Parse(String),
    /// `judge-stepwise` reached the end of the run without calling any step decisive.
    NoDecisiveStep,
    /// An aligner arm found no fork. Reported as the existing `no_pred` rate, not as a
    /// judge-style parse failure.
    NoFork,
}

/// One pair's row in the published document.
#[derive(Debug, Serialize)]
pub struct PairRow {
    pub name: String,
    pub task_key: String,
    pub gold_step: usize,
    /// Arm name → predicted step, or null for a miss/exclusion. The two are distinguished by
    /// the arm's own counts below; this field is the prediction record rule 5 pins.
    pub predictions: BTreeMap<String, Option<usize>>,
}

/// One arm's published result.
#[derive(Debug, Serialize)]
pub struct ArmRow {
    pub arm: String,
    pub score: ArmScore,
    /// Answers that broke the output contract. Scored as misses, counted here so a reader can
    /// see whether a low rate is bad localization or bad instruction-following.
    pub parse_failures: usize,
    /// Pairs dropped from this arm's denominator, with reasons (rule 4).
    pub exclusions: Vec<String>,
    /// How many answers came off disk. A table whose rows all replayed is reproducible.
    pub replayed: usize,
    /// How many answers were recorded live in this run.
    pub recorded: usize,
}

/// One judge arm against the product, on the pairs where both scored (rule 9).
#[derive(Debug, Serialize)]
pub struct PairedRow {
    pub judge_arm: String,
    pub product_arm: String,
    /// `d_p = product_hit − judge_hit`, per the registration.
    pub delta: PairedInterval,
    pub verdict: String,
}

/// The published judge-baseline document.
#[derive(Debug, Serialize)]
pub struct JudgeResults {
    pub judge_schema_version: String,
    pub provider: String,
    pub model: String,
    pub decoding: Decoding,
    pub split: String,
    pub n_pairs: usize,
    pub params_sha256: String,
    /// Which prompt revision each scored arm ran under (rule 10's own-freeze clause).
    pub prompts: BTreeMap<String, String>,
    pub arms: Vec<ArmRow>,
    pub paired: Vec<PairedRow>,
    pub pairs: Vec<PairRow>,
}

/// Score the configured judge arms plus amberfork's four arms on `pairs`.
///
/// # Errors
/// [`RunError`] if any question has no recorded answer in replay-only mode, or a cassette or
/// render problem makes the run unsound. A *provider* failure is not an error here — it is an
/// exclusion, which is data.
pub fn run_experiment(
    pairs: &[(String, Pair)],
    split: &str,
    params_sha256: &str,
    config: &Config<'_>,
) -> Result<JudgeResults, RunError> {
    assert!(!pairs.is_empty(), "the experiment needs pairs");

    let mut missing = Vec::new();
    let mut outcomes: BTreeMap<String, Vec<Outcome>> = BTreeMap::new();
    let mut replayed: BTreeMap<String, usize> = BTreeMap::new();
    let mut recorded: BTreeMap<String, usize> = BTreeMap::new();

    for (_, pair) in pairs {
        for &arm in config.arms {
            let mut tally = Tally::default();
            let outcome = ask_arm(pair, arm, config, &mut missing, &mut tally)?;
            outcomes
                .entry(arm.name().to_string())
                .or_default()
                .push(outcome);
            *replayed.entry(arm.name().to_string()).or_default() += tally.replayed;
            *recorded.entry(arm.name().to_string()).or_default() += tally.recorded;
        }
        // The product's arms, re-scored unchanged on the identical pairs (rule 10).
        for arm in arms::ALL {
            let outcome = match arm.predict(pair, config.params) {
                Some(prediction) => Outcome::Predicted(prediction.step),
                None => Outcome::Miss(MissReason::NoFork),
            };
            outcomes
                .entry(arm.name().to_string())
                .or_default()
                .push(outcome);
        }
    }

    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        return Err(RunError::MissingCassettes(missing));
    }

    let golds: Vec<usize> = pairs.iter().map(|(_, pair)| pair.gold_step).collect();
    // Deliberate table order, matching `run`'s: the factorial ladder floor-first, then the
    // baselines under test. Alphabetical would put `judge-paired` above `random`, which reads
    // as a ranking rather than a design.
    let order: Vec<String> = arms::ALL
        .iter()
        .map(|arm| arm.name().to_string())
        .chain(config.arms.iter().map(|arm| arm.name().to_string()))
        .collect();
    let arm_rows = score_arms(&outcomes, &order, &golds, &replayed, &recorded);
    let paired = paired_rows(&outcomes, &golds, config.arms);
    let pair_rows = pair_rows(pairs, &outcomes);

    let prompts = config
        .arms
        .iter()
        .map(|arm| {
            (
                arm.name().to_string(),
                config.prompts.prompt(*arm).sha256.clone(),
            )
        })
        .collect();

    Ok(JudgeResults {
        judge_schema_version: JUDGE_SCHEMA_VERSION.to_string(),
        provider: config.localizer.provider().to_string(),
        model: config.localizer.model().to_string(),
        decoding: config.localizer.decoding(),
        split: split.to_string(),
        n_pairs: pairs.len(),
        params_sha256: params_sha256.to_string(),
        prompts,
        arms: arm_rows,
        paired,
        pairs: pair_rows,
    })
}

#[derive(Default)]
struct Tally {
    replayed: usize,
    recorded: usize,
}

/// Ask one arm about one pair. Collects (rather than raises) a replay-only cassette miss, so
/// the operator learns about every unrecorded question in one pass.
fn ask_arm(
    pair: &Pair,
    arm: JudgeArm,
    config: &Config<'_>,
    missing: &mut Vec<PathBuf>,
    tally: &mut Tally,
) -> Result<Outcome, RunError> {
    let n_steps = pair.failing.steps.len();
    match arm {
        JudgeArm::Single | JudgeArm::Paired => {
            let rendered = if arm == JudgeArm::Single {
                config.prompts.render_single(&pair.failing)
            } else {
                config.prompts.render_paired(&pair.reference, &pair.failing)
            }
            .map_err(|err| RunError::Render(err.to_string()))?;

            match obtain(config, arm, &rendered, missing, tally)? {
                None => Ok(Outcome::Excluded("cassette pending".to_string())),
                Some(Asked::Failed(reason)) => Ok(Outcome::Excluded(reason)),
                Some(Asked::Answered(text)) => Ok(match judge_answer::parse_step(&text, n_steps) {
                    Ok(step) => Outcome::Predicted(step),
                    Err(failure) => Outcome::Miss(MissReason::Parse(failure.to_string())),
                }),
            }
        }
        JudgeArm::Stepwise => {
            // Candidates in order; the first `true` is the prediction and the rest of the run
            // is never asked about — that IS the Who&When step-by-step method, and asking on
            // would both cost more and let hindsight leak in.
            let mut parse_failures = 0usize;
            for candidate in 0..n_steps {
                let rendered = config
                    .prompts
                    .render_stepwise(&pair.failing, candidate)
                    .map_err(|err| RunError::Render(err.to_string()))?;
                match obtain(config, arm, &rendered, missing, tally)? {
                    None => return Ok(Outcome::Excluded("cassette pending".to_string())),
                    Some(Asked::Failed(reason)) => return Ok(Outcome::Excluded(reason)),
                    Some(Asked::Answered(text)) => match judge_answer::parse_decisive(&text) {
                        Ok(true) => return Ok(Outcome::Predicted(candidate)),
                        Ok(false) => {}
                        // An unreadable verdict cannot be a `true`, so the sweep continues.
                        // Aborting the pair would turn one bad response into an exclusion and
                        // quietly shrink the arm's denominator (rule 4).
                        Err(_) => parse_failures += 1,
                    },
                }
            }
            Ok(Outcome::Miss(if parse_failures > 0 {
                MissReason::Parse(format!(
                    "{parse_failures} unreadable verdict(s), no decisive step"
                ))
            } else {
                MissReason::NoDecisiveStep
            }))
        }
    }
}

enum Asked {
    Answered(String),
    Failed(String),
}

/// `Ok(None)` means "recorded as missing, keep going" — never "no answer exists".
fn obtain(
    config: &Config<'_>,
    arm: JudgeArm,
    rendered: &crate::judge_prompt::Rendered,
    missing: &mut Vec<PathBuf>,
    tally: &mut Tally,
) -> Result<Option<Asked>, RunError> {
    let question = Question {
        arm,
        prompt_sha256: &config.prompts.prompt(arm).sha256,
        rendered_prompt_sha256: &rendered.sha256,
        prompt: &rendered.text,
    };
    match judge_cassette::obtain(
        config.cassettes,
        config.mode,
        config.localizer,
        question,
        config.backoff,
    ) {
        Ok(answer) => {
            if answer.replayed {
                tally.replayed += 1;
            } else {
                tally.recorded += 1;
            }
            Ok(Some(Asked::Answered(answer.text)))
        }
        Err(judge_cassette::ObtainError::Cassette(judge_cassette::CassetteError::Missing {
            file,
            ..
        })) => {
            missing.push(file);
            Ok(None)
        }
        Err(judge_cassette::ObtainError::Cassette(err)) => Err(RunError::Cassette(err.to_string())),
        Err(err @ judge_cassette::ObtainError::Transport { .. }) => {
            Ok(Some(Asked::Failed(err.to_string())))
        }
    }
}

fn score_arms(
    outcomes: &BTreeMap<String, Vec<Outcome>>,
    order: &[String],
    golds: &[usize],
    replayed: &BTreeMap<String, usize>,
    recorded: &BTreeMap<String, usize>,
) -> Vec<ArmRow> {
    order
        .iter()
        .filter_map(|arm| outcomes.get(arm).map(|results| (arm, results)))
        .map(|(arm, results)| {
            let mut preds = Vec::new();
            let mut kept_golds = Vec::new();
            let mut parse_failures = 0;
            let mut exclusions = Vec::new();
            for (outcome, gold) in results.iter().zip(golds) {
                match outcome {
                    Outcome::Predicted(step) => {
                        preds.push(Some(*step));
                        kept_golds.push(*gold);
                    }
                    Outcome::Miss(reason) => {
                        if matches!(reason, MissReason::Parse(_)) {
                            parse_failures += 1;
                        }
                        preds.push(None);
                        kept_golds.push(*gold);
                    }
                    Outcome::Excluded(reason) => exclusions.push(reason.clone()),
                }
            }
            ArmRow {
                arm: arm.clone(),
                score: score(&preds, &kept_golds),
                parse_failures,
                exclusions,
                replayed: replayed.get(arm).copied().unwrap_or_default(),
                recorded: recorded.get(arm).copied().unwrap_or_default(),
            }
        })
        .collect()
}

/// The registered comparison: `d_p = nw-lexical hit − judge hit`, over the pairs where both
/// arms produced a scored outcome. An exclusion on either side breaks the pairing for that
/// pair, so it leaves the sample rather than being counted as a zero difference.
fn paired_rows(
    outcomes: &BTreeMap<String, Vec<Outcome>>,
    golds: &[usize],
    judge_arms: &[JudgeArm],
) -> Vec<PairedRow> {
    let product = Arm::NwLexical.name();
    let Some(product_outcomes) = outcomes.get(product) else {
        return Vec::new();
    };

    judge_arms
        .iter()
        .filter_map(|arm| {
            let judge_outcomes = outcomes.get(arm.name())?;
            let deltas: Vec<f64> = judge_outcomes
                .iter()
                .zip(product_outcomes)
                .zip(golds)
                .filter_map(|((judge, product), gold)| {
                    match (hit(judge, *gold), hit(product, *gold)) {
                        (Some(judge_hit), Some(product_hit)) => {
                            Some(f64::from(u8::from(product_hit)) - f64::from(u8::from(judge_hit)))
                        }
                        _ => None,
                    }
                })
                .collect();
            if deltas.is_empty() {
                return None;
            }
            let delta = bootstrap_paired(&deltas, arm_seed(*arm));
            Some(PairedRow {
                judge_arm: arm.name().to_string(),
                product_arm: product.to_string(),
                verdict: verdict(&delta).to_string(),
                delta,
            })
        })
        .collect()
}

/// Each arm resamples from its own stream. Sharing one sequence across arms would correlate
/// their intervals for no reason — the arms are separate experiments that happen to run in one
/// invocation.
fn arm_seed(arm: JudgeArm) -> u64 {
    BOOTSTRAP_SEED ^ fnv1a64(arm.name().as_bytes())
}

/// Whether an outcome hit gold exactly. `None` for an exclusion — it has no verdict, which is
/// different from having a wrong one.
fn hit(outcome: &Outcome, gold: usize) -> Option<bool> {
    match outcome {
        Outcome::Predicted(step) => Some(*step == gold),
        Outcome::Miss(_) => Some(false),
        Outcome::Excluded(_) => None,
    }
}

/// The registered reading of a paired interval. A CI straddling zero is a tie and must be
/// reported as one — at these fixture counts a tie is the likeliest outcome, and reading it in
/// whichever direction flatters the product is exactly what registering in advance forbids.
fn verdict(delta: &PairedInterval) -> &'static str {
    if delta.ci95_lo > 0.0 {
        "amberfork localizes better on these pairs"
    } else if delta.ci95_hi < 0.0 {
        "the judge localizes better on these pairs"
    } else {
        "tie — the interval includes zero"
    }
}

fn pair_rows(pairs: &[(String, Pair)], outcomes: &BTreeMap<String, Vec<Outcome>>) -> Vec<PairRow> {
    pairs
        .iter()
        .enumerate()
        .map(|(idx, (name, pair))| PairRow {
            name: name.clone(),
            task_key: pair.task_key.clone(),
            gold_step: pair.gold_step,
            predictions: outcomes
                .iter()
                .map(|(arm, results)| {
                    let step = match results.get(idx) {
                        Some(Outcome::Predicted(step)) => Some(*step),
                        _ => None,
                    };
                    (arm.clone(), step)
                })
                .collect(),
        })
        .collect()
}

/// Render the document as the markdown table the notebook entry quotes.
#[must_use]
pub fn render(results: &JudgeResults) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "LLM-judge baseline — {} pairs · split={} · {} {} · params sha256:{}\n",
        results.n_pairs,
        results.split,
        results.provider,
        results.model,
        &results.params_sha256[..12.min(results.params_sha256.len())],
    ));
    out.push_str(&format!(
        "decoding: temperature {} · max_output_tokens {}\n\n",
        results
            .decoding
            .temperature
            .map_or_else(|| "unset".to_string(), |t| format!("{t}")),
        results.decoding.max_output_tokens,
    ));

    out.push_str("| arm | exact | ±1 | ±3 | no-pred | parse-fail | excl | n |\n");
    out.push_str("|---|---|---|---|---|---|---|---|\n");
    for row in &results.arms {
        out.push_str(&format!(
            "| {} | {:.3} [{:.3}, {:.3}] | {:.3} | {:.3} | {:.3} | {} | {} | {} |\n",
            row.arm,
            row.score.exact.rate,
            row.score.exact.ci95_lo,
            row.score.exact.ci95_hi,
            row.score.w1.rate,
            row.score.w3.rate,
            row.score.no_pred.rate,
            row.parse_failures,
            row.exclusions.len(),
            row.score.exact.n,
        ));
    }

    if !results.paired.is_empty() {
        out.push_str(
            "\npaired comparison (rule 9): d_p = product hit − judge hit, bootstrap 95%\n\n",
        );
        out.push_str("| judge arm | vs | mean d | 95% CI | pairs | verdict |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for row in &results.paired {
            out.push_str(&format!(
                "| {} | {} | {:+.3} | [{:+.3}, {:+.3}] | {} | {} |\n",
                row.judge_arm,
                row.product_arm,
                row.delta.mean,
                row.delta.ci95_lo,
                row.delta.ci95_hi,
                row.delta.n_pairs,
                row.verdict,
            ));
        }
    }

    for row in &results.arms {
        for exclusion in &row.exclusions {
            out.push_str(&format!("\nexcluded ({}): {exclusion}", row.arm));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_positive_interval_that_clears_zero_reads_as_a_product_win() {
        let delta = PairedInterval {
            mean: 0.30,
            ci95_lo: 0.10,
            ci95_hi: 0.50,
            resamples: 10_000,
            n_pairs: 23,
        };
        assert_eq!(verdict(&delta), "amberfork localizes better on these pairs");
    }

    #[test]
    fn a_negative_interval_that_clears_zero_reads_as_a_judge_win() {
        // The outcome this registration expects on the natural pairs. It must render as
        // plainly as a win would.
        let delta = PairedInterval {
            mean: -0.26,
            ci95_lo: -0.45,
            ci95_hi: -0.08,
            resamples: 10_000,
            n_pairs: 23,
        };
        assert_eq!(verdict(&delta), "the judge localizes better on these pairs");
    }

    #[test]
    fn an_interval_touching_zero_is_a_tie_however_large_the_point_estimate() {
        let delta = PairedInterval {
            mean: 0.22,
            ci95_lo: -0.01,
            ci95_hi: 0.44,
            resamples: 10_000,
            n_pairs: 23,
        };
        assert_eq!(verdict(&delta), "tie — the interval includes zero");
    }

    #[test]
    fn an_exclusion_has_no_verdict_while_a_miss_has_a_wrong_one() {
        assert_eq!(hit(&Outcome::Predicted(4), 4), Some(true));
        assert_eq!(hit(&Outcome::Predicted(5), 4), Some(false));
        assert_eq!(
            hit(&Outcome::Miss(MissReason::NoDecisiveStep), 4),
            Some(false)
        );
        assert_eq!(hit(&Outcome::Excluded("timeout".into()), 4), None);
    }

    #[test]
    fn a_parse_failure_is_scored_as_a_miss_and_counted_separately() {
        let outcomes = BTreeMap::from([(
            "judge-paired".to_string(),
            vec![
                Outcome::Predicted(1),
                Outcome::Miss(MissReason::Parse("no JSON object".into())),
            ],
        )]);

        let order = vec!["judge-paired".to_string()];
        let rows = score_arms(
            &outcomes,
            &order,
            &[1, 1],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );

        let row = &rows[0];
        assert_eq!(row.parse_failures, 1);
        // The denominator keeps both pairs: a broken answer is a wrong answer.
        assert_eq!(row.score.exact.n, 2);
        assert_eq!(row.score.exact.hits, 1);
        assert_eq!(row.score.no_pred.hits, 1);
    }

    #[test]
    fn an_exclusion_leaves_the_denominator_and_is_tabulated() {
        let outcomes = BTreeMap::from([(
            "judge-paired".to_string(),
            vec![
                Outcome::Predicted(1),
                Outcome::Excluded("provider unreachable".into()),
            ],
        )]);

        let order = vec!["judge-paired".to_string()];
        let rows = score_arms(
            &outcomes,
            &order,
            &[1, 1],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );

        let row = &rows[0];
        // Rule 4: the excluded pair is counted with its reason, never scored as a miss —
        // a rate over a silently shrunk denominator is a lie, and so is one padded with our
        // own network failures.
        assert_eq!(row.score.exact.n, 1);
        assert_eq!(row.exclusions, vec!["provider unreachable".to_string()]);
    }

    #[test]
    fn a_pair_excluded_on_either_side_leaves_the_paired_sample() {
        let outcomes = BTreeMap::from([
            (
                "judge-paired".to_string(),
                vec![
                    Outcome::Predicted(0),
                    Outcome::Excluded("timeout".into()),
                    Outcome::Predicted(9),
                ],
            ),
            (
                Arm::NwLexical.name().to_string(),
                vec![
                    Outcome::Predicted(0),
                    Outcome::Predicted(1),
                    Outcome::Predicted(2),
                ],
            ),
        ]);

        let rows = paired_rows(&outcomes, &[0, 1, 2], &[JudgeArm::Paired]);

        // Two pairs survive: both hit on the first (d=0), only the product hits on the third
        // (d=+1). The excluded middle pair is absent, not a zero.
        assert_eq!(rows[0].delta.n_pairs, 2);
        assert!(
            (rows[0].delta.mean - 0.5).abs() < 1e-9,
            "{:?}",
            rows[0].delta
        );
    }

    #[test]
    fn the_paired_interval_reproduces_from_the_committed_seed() {
        let outcomes = BTreeMap::from([
            (
                "judge-paired".to_string(),
                vec![Outcome::Predicted(0), Outcome::Predicted(5)],
            ),
            (
                Arm::NwLexical.name().to_string(),
                vec![Outcome::Predicted(0), Outcome::Predicted(1)],
            ),
        ]);

        let once = paired_rows(&outcomes, &[0, 1], &[JudgeArm::Paired]);
        let twice = paired_rows(&outcomes, &[0, 1], &[JudgeArm::Paired]);

        assert_eq!(once[0].delta.ci95_lo, twice[0].delta.ci95_lo);
        assert_eq!(once[0].delta.ci95_hi, twice[0].delta.ci95_hi);
        assert_eq!(once[0].delta.resamples, 10_000);
    }

    #[test]
    fn each_judge_arm_bootstraps_from_its_own_stream() {
        // Asserted on the seeds rather than on two intervals: at small n the percentile bounds
        // saturate at ±1 whatever the stream, so comparing intervals would pass or fail for
        // reasons unrelated to the property.
        let seeds: Vec<u64> = JudgeArm::ALL.into_iter().map(arm_seed).collect();
        let mut unique = seeds.clone();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(
            unique.len(),
            seeds.len(),
            "two arms share a resample stream"
        );
        assert!(
            !seeds.contains(&BOOTSTRAP_SEED),
            "an arm reuses the base seed unmixed"
        );
    }
}
