//! The multi-reference consensus experiment (issue #45 slice B).
//!
//! Pre-registered in `docs/notebook.md` 065 **before this module existed**. Everything binding
//! is declared there and mirrored in the constants below; nothing here may be tuned after a
//! number has been seen without invalidating the registration.
//!
//! Three arms on identical fixtures, identical gold, identical frozen params:
//! - [`Arm3::Pristine`] — the shipped engine against the committed reference. The **ceiling**:
//!   consensus over jittered references can at best recover what the clean one already said.
//! - [`Arm3::Single`] — against jittered reference *i*, all [`N_REFERENCES`] of them. The
//!   "unlucky draw" condition the issue is about.
//! - [`Arm3::Consensus`] — [`amberfork_align::consensus`] over all of them; modal fork step.
//!
//! The decision is **paired** (BENCHMARK.md rule 9): both arms score the same 25 dev pairs, and
//! at n=25 an unpaired Wilson comparison returns "inconclusive" whether or not there is an
//! effect, which would let a real null and a real win look identical. See [`bootstrap_paired`].

use crate::hash::{bounded, fnv1a64, splitmix64};
use crate::jitter::jitter_reference;
use crate::pairs::Pair;
use crate::score::{ArmScore, score};
use amberfork_align::{DiffParams, LexicalCost, consensus, diff};
use serde::{Deserialize, Serialize};

/// References jittered per suspect. Fixed at 10 in notebook 065: no sweep, no best-N column.
/// If consensus needs a tuned N to win, that fragility is the finding.
pub const N_REFERENCES: usize = 10;
/// Bootstrap resamples for the paired interval. Declared in advance (rule 9).
pub const BOOTSTRAP_RESAMPLES: usize = 10_000;
/// Seed for the bootstrap's own stream. Committed so the interval reproduces exactly.
const BOOTSTRAP_SEED: u64 = 0x9E45_C0DE;

/// The three arms, in table order (ceiling, condition, treatment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arm3 {
    Pristine,
    Single,
    Consensus,
}

impl Arm3 {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Pristine => "pristine",
            Self::Single => "single",
            Self::Consensus => "consensus",
        }
    }
}

/// One pair's full record — every arm's prediction, kept per-pair so the paired statistic can
/// be recomputed from the results document without re-running the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairRecord {
    /// Namespaced `dir/pair_NN` — the jitter key, unique across fixture dirs.
    pub key: String,
    pub gold_step: usize,
    pub pristine: Option<usize>,
    /// One prediction per jittered reference, in variant order.
    pub single: Vec<Option<usize>>,
    pub consensus: Option<usize>,
    /// How many of the [`N_REFERENCES`] backed the modal step.
    pub support: usize,
    /// How many forked at all — `support / forked` is the agreement rate.
    pub forked: usize,
}

impl PairRecord {
    /// Whether consensus hit the gold step exactly.
    #[must_use]
    pub fn consensus_hit(&self) -> f64 {
        f64::from(u8::from(self.consensus == Some(self.gold_step)))
    }

    /// The *expected* single-draw hit: the mean over all [`N_REFERENCES`] draws. This, not one
    /// arbitrary draw, is what "align against a randomly chosen good run" is worth.
    #[must_use]
    pub fn single_hit_rate(&self) -> f64 {
        if self.single.is_empty() {
            return 0.0;
        }
        let hits = self
            .single
            .iter()
            .filter(|p| **p == Some(self.gold_step))
            .count();
        hits as f64 / self.single.len() as f64
    }

    /// The registered per-pair difference statistic `d_p ∈ [−1, 1]`.
    #[must_use]
    pub fn paired_delta(&self) -> f64 {
        self.consensus_hit() - self.single_hit_rate()
    }
}

/// A bootstrap interval on a mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PairedInterval {
    pub mean: f64,
    pub ci95_lo: f64,
    pub ci95_hi: f64,
    pub resamples: usize,
    /// Pairs resampled — the independent unit, and the reason this interval is honest.
    pub n_pairs: usize,
}

impl PairedInterval {
    /// The registered decision rule: consensus pays iff the interval excludes 0 with a positive
    /// point estimate. A positive mean whose interval straddles 0 is a **null**, by prior
    /// agreement, and a null kills the POA milestone.
    #[must_use]
    pub fn pays(&self) -> bool {
        self.mean > 0.0 && self.ci95_lo > 0.0
    }
}

/// The complete experiment result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultirefResults {
    pub n_references: usize,
    pub n_pairs: usize,
    /// Per-arm windowed scores with Wilson intervals (rule 6, unchanged).
    pub pristine: ArmScore,
    pub consensus: ArmScore,
    /// The single arm over all `n_pairs × n_references` draws. Its Wilson interval is
    /// **optimistic** — draws within a pair share a suspect and are not independent — so it is
    /// reported for completeness and `single_per_pair` is the one to read.
    pub single_pooled: ArmScore,
    /// Bootstrap interval on the mean per-pair single-draw hit rate, resampling pairs.
    pub single_per_pair: PairedInterval,
    /// The decision statistic: bootstrap interval on `mean(d_p)`.
    pub paired: PairedInterval,
    /// Mean agreement (`support / forked`) over pairs where any reference forked.
    pub mean_agreement: Option<f64>,
    pub records: Vec<PairRecord>,
}

impl MultirefResults {
    /// The registered verdict.
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        if self.paired.pays() {
            "PAYS — consensus beats the expected single draw; the POA milestone is justified"
        } else {
            "NULL — consensus does not beat the expected single draw; the POA milestone dies"
        }
    }
}

/// Run all three arms over `pairs` and decide.
///
/// `keys` namespaces each pair by its fixture directory (`pair_00` exists in every dev seed).
///
/// # Panics
/// If `pairs` is empty — an empty fixture set is a loader error long before scoring.
#[must_use]
pub fn run_experiment(pairs: &[(String, Pair)], params: &DiffParams) -> MultirefResults {
    assert!(!pairs.is_empty(), "the experiment needs fixtures");

    let records: Vec<PairRecord> = pairs
        .iter()
        .map(|(key, pair)| {
            let references: Vec<_> = (0..N_REFERENCES)
                .map(|i| jitter_reference(&pair.reference, key, i))
                .collect();

            let pristine = predict(&pair.reference, pair, params);
            let single: Vec<Option<usize>> = references
                .iter()
                .map(|r| predict(r, pair, params))
                .collect();
            let tally = consensus(&references, &pair.failing, &LexicalCost, params)
                .expect("bench pairs stay within the default size guard");

            PairRecord {
                key: key.clone(),
                gold_step: pair.gold_step,
                pristine,
                single,
                consensus: tally.modal_step,
                support: tally.support,
                forked: tally.forked(),
            }
        })
        .collect();

    let golds: Vec<usize> = records.iter().map(|r| r.gold_step).collect();
    let pooled_golds: Vec<usize> = golds
        .iter()
        .flat_map(|g| std::iter::repeat_n(*g, N_REFERENCES))
        .collect();
    let pooled_preds: Vec<Option<usize>> = records.iter().flat_map(|r| r.single.clone()).collect();

    let deltas: Vec<f64> = records.iter().map(PairRecord::paired_delta).collect();
    let single_rates: Vec<f64> = records.iter().map(PairRecord::single_hit_rate).collect();

    let agreements: Vec<f64> = records
        .iter()
        .filter(|r| r.forked > 0)
        .map(|r| r.support as f64 / r.forked as f64)
        .collect();

    MultirefResults {
        n_references: N_REFERENCES,
        n_pairs: records.len(),
        pristine: score(
            &records.iter().map(|r| r.pristine).collect::<Vec<_>>(),
            &golds,
        ),
        consensus: score(
            &records.iter().map(|r| r.consensus).collect::<Vec<_>>(),
            &golds,
        ),
        single_pooled: score(&pooled_preds, &pooled_golds),
        single_per_pair: bootstrap_paired(&single_rates, BOOTSTRAP_SEED ^ 0x5115),
        paired: bootstrap_paired(&deltas, BOOTSTRAP_SEED),
        mean_agreement: (!agreements.is_empty())
            .then(|| agreements.iter().sum::<f64>() / agreements.len() as f64),
        records,
    }
}

fn predict(reference: &amberfork_model::Run, pair: &Pair, params: &DiffParams) -> Option<usize> {
    diff(reference, &pair.failing, &LexicalCost, params)
        .expect("bench pairs stay within the default size guard")
        .fork_step_observed()
}

/// Percentile bootstrap 95% CI on the mean of `values`, resampling with replacement.
///
/// The resampling unit is the *value*, and every caller passes one value per fixture pair — the
/// independent unit. Resampling predictions instead would treat ten jittered draws of one
/// suspect as ten independent observations and shrink the interval by roughly √10 for free.
#[must_use]
pub fn bootstrap_paired(values: &[f64], seed: u64) -> PairedInterval {
    let n = values.len();
    assert!(n > 0, "bootstrap on an empty sample");
    let mean = values.iter().sum::<f64>() / n as f64;

    let mut state = seed ^ fnv1a64(b"multiref-bootstrap");
    let mut means = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    for _ in 0..BOOTSTRAP_RESAMPLES {
        let mut total = 0.0;
        for _ in 0..n {
            total += values[bounded(splitmix64(&mut state), n)];
        }
        means.push(total / n as f64);
    }
    means.sort_by(f64::total_cmp);

    PairedInterval {
        mean,
        ci95_lo: percentile(&means, 0.025),
        ci95_hi: percentile(&means, 0.975),
        resamples: BOOTSTRAP_RESAMPLES,
        n_pairs: n,
    }
}

/// Nearest-rank percentile of a sorted slice. No interpolation: at 10k resamples the
/// difference is far below the precision anyone should read into a bootstrap bound, and an
/// exact rank is reproducible across platforms without float-ordering subtleties.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let idx = ((sorted.len() as f64) * q) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Render the results as the fixed-width table the notebook entry quotes.
#[must_use]
pub fn render(results: &MultirefResults) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "multi-reference consensus — {} dev pairs, N={} jittered references each\n\n",
        results.n_pairs, results.n_references
    ));
    out.push_str("arm         exact  Wilson 95%        ±1     n\n");
    for (arm, s) in [
        (Arm3::Pristine, &results.pristine),
        (Arm3::Single, &results.single_pooled),
        (Arm3::Consensus, &results.consensus),
    ] {
        out.push_str(&format!(
            "{:<11} {:.3}  [{:.3}, {:.3}]  {:.3}  {}\n",
            arm.name(),
            s.exact.rate,
            s.exact.ci95_lo,
            s.exact.ci95_hi,
            s.w1.rate,
            s.exact.n
        ));
    }
    out.push_str(&format!(
        "\nsingle, per-pair mean (bootstrap): {:.3}  [{:.3}, {:.3}]\n",
        results.single_per_pair.mean,
        results.single_per_pair.ci95_lo,
        results.single_per_pair.ci95_hi
    ));
    out.push_str(&format!(
        "paired  mean(d_p) = consensus − E[single]: {:+.3}  [{:+.3}, {:+.3}]  ({} resamples, {} pairs)\n",
        results.paired.mean,
        results.paired.ci95_lo,
        results.paired.ci95_hi,
        results.paired.resamples,
        results.paired.n_pairs
    ));
    if let Some(agreement) = results.mean_agreement {
        out.push_str(&format!(
            "mean agreement (support/forked): {agreement:.3}\n"
        ));
    }
    out.push_str(&format!("\n{}\n", results.verdict()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interval(values: &[f64]) -> PairedInterval {
        bootstrap_paired(values, BOOTSTRAP_SEED)
    }

    #[test]
    fn a_constant_sample_has_a_degenerate_interval() {
        let i = interval(&[0.5; 25]);
        assert!((i.mean - 0.5).abs() < 1e-12);
        assert!(
            (i.ci95_lo - 0.5).abs() < 1e-12 && (i.ci95_hi - 0.5).abs() < 1e-12,
            "every resample of a constant is the constant"
        );
    }

    #[test]
    fn a_sample_straddling_zero_yields_an_interval_containing_zero() {
        // Half +1, half −1: the mean is 0 and no honest interval excludes it.
        let values: Vec<f64> = (0..26)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let i = interval(&values);
        assert!(i.ci95_lo < 0.0 && i.ci95_hi > 0.0);
        assert!(!i.pays(), "a coin flip must never read as a win");
    }

    #[test]
    fn an_unambiguous_effect_clears_zero() {
        // 25 pairs where consensus wins outright: the interval must exclude 0.
        let i = interval(&[1.0; 25]);
        assert!(i.ci95_lo > 0.0);
        assert!(i.pays());
    }

    #[test]
    fn a_negative_effect_never_pays() {
        let i = interval(&[-1.0; 25]);
        assert!(i.ci95_hi < 0.0);
        assert!(!i.pays(), "consensus losing must not be reported as paying");
    }

    #[test]
    fn the_interval_is_reproducible_from_the_committed_seed() {
        let values: Vec<f64> = (0..25).map(|i| f64::from(i % 3) / 2.0 - 0.5).collect();
        assert_eq!(
            interval(&values),
            interval(&values),
            "a published interval that moves between runs is not a published interval"
        );
    }

    #[test]
    fn the_paired_delta_is_consensus_minus_the_expected_draw() {
        let record = PairRecord {
            key: "seed42/pair_00".into(),
            gold_step: 4,
            pristine: Some(4),
            // 3 of 10 draws hit gold.
            single: vec![
                Some(4),
                Some(4),
                Some(4),
                Some(2),
                Some(2),
                None,
                Some(7),
                Some(1),
                Some(9),
                Some(3),
            ],
            consensus: Some(4),
            support: 3,
            forked: 9,
        };
        assert!((record.single_hit_rate() - 0.3).abs() < 1e-12);
        assert!((record.consensus_hit() - 1.0).abs() < 1e-12);
        assert!(
            (record.paired_delta() - 0.7).abs() < 1e-12,
            "consensus hitting where 3/10 draws hit is worth +0.7, not +1"
        );
    }

    #[test]
    fn a_consensus_miss_where_draws_hit_is_a_negative_delta() {
        let record = PairRecord {
            key: "seed42/pair_01".into(),
            gold_step: 4,
            pristine: Some(4),
            single: vec![Some(4); 10],
            consensus: Some(5),
            support: 10,
            forked: 10,
        };
        assert!(
            (record.paired_delta() + 1.0).abs() < 1e-12,
            "unanimous draws hitting while the modal step misses is the worst case, −1"
        );
    }

    #[test]
    fn percentile_bounds_stay_inside_the_sample() {
        let sorted: Vec<f64> = (0..1000).map(f64::from).collect();
        assert_eq!(percentile(&sorted, 0.0), 0.0);
        assert_eq!(percentile(&sorted, 0.975), 975.0);
        assert_eq!(
            percentile(&sorted, 1.0),
            999.0,
            "q=1 must not index past the end"
        );
    }
}
