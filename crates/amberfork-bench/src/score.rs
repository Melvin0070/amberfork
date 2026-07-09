//! Localization scoring: the windowed metrics BENCHMARK.md names, each as a rate with its
//! Wilson 95% interval (pre-registered protocol rule 6 — no headline rate without a CI).
//!
//! Predictions are failing-run step indices; `None` means the arm found no fork. A `None`
//! counts as a miss in every window with the denominator intact — protocol rule 4 forbids
//! silently shrinking the denominator — and is additionally reported as its own `no_pred`
//! rate so a method can't hide behind abstention.

use serde::Serialize;

/// z for a two-sided 95% normal interval (Φ⁻¹(0.975)).
const Z95: f64 = 1.959_963_984_540_054;

/// A binomial rate with its Wilson 95% score interval. `rate` is `hits / n`; the interval is
/// asymmetric near the boundaries, which is exactly why Wilson over normal approximation on
/// small n (Who&When-scale fixture sets).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rate {
    pub hits: usize,
    pub n: usize,
    pub rate: f64,
    pub ci95_lo: f64,
    pub ci95_hi: f64,
}

/// One arm's localization scores on one fixture set: step exact-match, within ±1, within ±3
/// (the honest windows — "first divergence" and "decisive error" can legitimately differ by a
/// step), and the abstention rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ArmScore {
    pub exact: Rate,
    pub w1: Rate,
    pub w3: Rate,
    pub no_pred: Rate,
}

/// The Wilson 95% score interval around `hits / n`.
///
/// # Panics
/// If `n` is zero or `hits > n` — an empty fixture set is a loader error long before scoring.
#[must_use]
pub fn wilson95(hits: usize, n: usize) -> Rate {
    assert!(n > 0, "wilson95 on an empty set");
    assert!(hits <= n, "wilson95 with hits {hits} > n {n}");
    let nf = n as f64;
    let p = hits as f64 / nf;
    let z2 = Z95 * Z95;
    let denom = 1.0 + z2 / nf;
    let center = (p + z2 / (2.0 * nf)) / denom;
    let half = Z95 * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt() / denom;
    Rate {
        hits,
        n,
        rate: p,
        // Mathematically already within [0, 1]; the clamp only swallows float error at the
        // 0/n and n/n boundaries, where the bound must be exact.
        ci95_lo: (center - half).max(0.0),
        ci95_hi: (center + half).min(1.0),
    }
}

/// Score one arm's predictions against the gold steps, position-wise.
///
/// # Panics
/// If the slices differ in length or are empty — pairs and predictions come from the same
/// loaded set, so a mismatch is a harness bug.
#[must_use]
pub fn score(preds: &[Option<usize>], golds: &[usize]) -> ArmScore {
    assert_eq!(
        preds.len(),
        golds.len(),
        "predictions and golds must pair up"
    );
    let n = golds.len();
    let (mut exact, mut w1, mut w3, mut no_pred) = (0, 0, 0, 0);
    for (pred, gold) in preds.iter().zip(golds) {
        match pred {
            None => no_pred += 1,
            Some(p) => {
                let delta = p.abs_diff(*gold);
                exact += usize::from(delta == 0);
                w1 += usize::from(delta <= 1);
                w3 += usize::from(delta <= 3);
            }
        }
    }
    ArmScore {
        exact: wilson95(exact, n),
        w1: wilson95(w1, n),
        w3: wilson95(w3, n),
        no_pred: wilson95(no_pred, n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values computed independently (Python, same z = Φ⁻¹(0.975)); the test locks
    /// the implementation to the formula rather than to itself.
    #[test]
    fn wilson95_matches_independent_reference_values() {
        let cases = [
            (15, 20, 0.75, 0.531_299_122_381_256, 0.888_138_298_592_334_3),
            (
                14,
                20,
                0.70,
                0.481_027_181_646_476_5,
                0.854_522_755_132_395_6,
            ),
            (
                2,
                3,
                2.0 / 3.0,
                0.207_659_600_802_047_7,
                0.938_508_055_279_603_7,
            ),
        ];
        for (hits, n, rate, lo, hi) in cases {
            let r = wilson95(hits, n);
            assert_eq!((r.hits, r.n), (hits, n));
            assert!((r.rate - rate).abs() < 1e-12, "{hits}/{n} rate: {}", r.rate);
            assert!(
                (r.ci95_lo - lo).abs() < 1e-12,
                "{hits}/{n} lo: {}",
                r.ci95_lo
            );
            assert!(
                (r.ci95_hi - hi).abs() < 1e-12,
                "{hits}/{n} hi: {}",
                r.ci95_hi
            );
        }
    }

    #[test]
    fn wilson95_is_exact_at_the_boundaries() {
        // 0/n and n/n must pin the corresponding bound exactly (no floating drift past the
        // [0, 1] scale): a reader must never see a negative rate or one above 1.
        let zero = wilson95(0, 20);
        assert_eq!(zero.rate, 0.0);
        assert_eq!(zero.ci95_lo, 0.0);
        assert!((zero.ci95_hi - 0.161_125_158_052_819_38).abs() < 1e-12);

        let full = wilson95(20, 20);
        assert_eq!(full.rate, 1.0);
        assert_eq!(full.ci95_hi, 1.0);
        assert!((full.ci95_lo - 0.838_874_841_947_180_6).abs() < 1e-12);
    }

    #[test]
    fn score_windows_and_abstentions_share_one_denominator() {
        // Deltas: 0 (exact), 1 (±1), 3 (±3), 5 (miss even at ±3), None (abstention — a miss
        // everywhere). Every rate is out of n = 5.
        let preds = [Some(4), Some(5), Some(7), Some(9), None];
        let golds = [4, 4, 4, 4, 4];
        let s = score(&preds, &golds);
        assert_eq!((s.exact.hits, s.exact.n), (1, 5));
        assert_eq!((s.w1.hits, s.w1.n), (2, 5));
        assert_eq!((s.w3.hits, s.w3.n), (3, 5));
        assert_eq!((s.no_pred.hits, s.no_pred.n), (1, 5));
    }

    #[test]
    fn score_windows_are_symmetric_around_gold() {
        // A prediction below gold counts the same as one above it.
        let preds = [Some(3), Some(1)];
        let golds = [4, 4];
        let s = score(&preds, &golds);
        assert_eq!(s.w1.hits, 1);
        assert_eq!(s.w3.hits, 2);
    }
}
