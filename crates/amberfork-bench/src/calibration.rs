//! The reliability curve (pre-registered protocol rule 7): fork confidence binned against
//! empirical exact-match correctness, so a reader can see whether a high-confidence fork is
//! actually right more often — the property the CI `--gate` feature will stand on, measured
//! rather than assumed (the spike version of this measurement is notebook 005).
//!
//! Bins are FIXED-WIDTH over `[0, 1]` — committed code constants like the ±1/±3 windows, not
//! `bench/params.toml` material and not the spike's equal-count terciles: data-derived edges
//! shift with every fixture set, which makes curves incomparable across runs and hands a
//! cherry-picker a knob. An empty bin publishes as empty (`rate: null` / `—`), never
//! disappears — the rule-4 ethos applied to bins. Correctness is the exact-match headline
//! metric, as in the 005 study. Abstentions carry no confidence and are outside the curve;
//! they are already published as the `no_pred` rate on the same denominator.

use crate::arms::Prediction;
use crate::score::{Rate, wilson95};
use serde::{Deserialize, Serialize};

/// Number of fixed-width confidence bins. Five is the standard reliability-curve
/// granularity; the last bin is closed so confidence 1.0 has a home.
pub const N_BINS: usize = 5;

/// One bin of the reliability curve: the half-open confidence interval `[lo, hi)` (closed at
/// 1.0 for the last bin) and the exact-hit rate of the predictions that landed in it —
/// `None` when the bin is empty, which is data, not an omission.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CalibrationBin {
    pub lo: f64,
    pub hi: f64,
    pub rate: Option<Rate>,
}

/// Bin the confidence-carrying predictions of one arm against the gold steps. Predictions
/// without a confidence (abstentions, or an arm that emits none) do not enter any bin.
///
/// # Panics
/// If the slices differ in length — pairs and predictions come from the same loaded set, so
/// a mismatch is a harness bug.
#[must_use]
pub fn calibrate(preds: &[Option<Prediction>], golds: &[usize]) -> Vec<CalibrationBin> {
    assert_eq!(
        preds.len(),
        golds.len(),
        "predictions and golds must pair up"
    );
    let mut counts = [(0usize, 0usize); N_BINS]; // (hits, n) per bin
    for (pred, gold) in preds.iter().zip(golds) {
        let Some(prediction) = pred else { continue };
        let Some(confidence) = prediction.confidence else {
            continue;
        };
        let (hits, n) = &mut counts[bin_of(confidence)];
        *n += 1;
        *hits += usize::from(prediction.step == *gold);
    }
    counts
        .iter()
        .enumerate()
        .map(|(i, &(hits, n))| CalibrationBin {
            lo: i as f64 / N_BINS as f64,
            hi: (i + 1) as f64 / N_BINS as f64,
            rate: (n > 0).then(|| wilson95(hits, n)),
        })
        .collect()
}

/// Which bin a confidence falls in: half-open `[lo, hi)` bins, except the last, which is
/// closed so 1.0 has a home. The engine guarantees confidence in `[0, 1]`; the clamp only
/// covers float grazing past either end, never reordering.
fn bin_of(confidence: f64) -> usize {
    debug_assert!(
        (0.0..=1.0).contains(&confidence),
        "fork confidence out of range: {confidence}"
    );
    let scaled = (confidence.max(0.0) * N_BINS as f64).floor() as usize;
    scaled.min(N_BINS - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pred(step: usize, confidence: f64) -> Option<Prediction> {
        Some(Prediction {
            step,
            confidence: Some(confidence),
        })
    }

    #[test]
    fn bins_tile_the_unit_interval_in_order() {
        let bins = calibrate(&[], &[]);
        assert_eq!(bins.len(), N_BINS);
        assert_eq!(bins[0].lo, 0.0);
        assert_eq!(bins[N_BINS - 1].hi, 1.0);
        for pair in bins.windows(2) {
            assert_eq!(pair[0].hi, pair[1].lo, "bins abut with no gap");
        }
        assert!(
            bins.iter().all(|bin| bin.rate.is_none()),
            "no predictions, every bin empty"
        );
    }

    #[test]
    fn edges_go_to_the_upper_bin_and_the_top_stays_closed() {
        // 0.2 is [0.2, 0.4)'s floor, not [0.0, 0.2)'s ceiling; 1.0 belongs to the last bin
        // (closed top), not a phantom sixth. Both hit gold so hits mirror occupancy.
        let preds = [pred(4, 0.2), pred(4, 1.0)];
        let golds = [4, 4];
        let bins = calibrate(&preds, &golds);
        assert!(bins[0].rate.is_none());
        let second = bins[1].rate.expect("0.2 lands in [0.2, 0.4)");
        assert_eq!((second.hits, second.n), (1, 1));
        let last = bins[N_BINS - 1].rate.expect("1.0 lands in [0.8, 1.0]");
        assert_eq!((last.hits, last.n), (1, 1));
    }

    #[test]
    fn hits_are_exact_match_and_carry_the_wilson_interval() {
        // Three predictions in one bin: exact hit, ±1 near-miss, far miss. Only the exact
        // hit counts — the curve calibrates the headline metric, not the windows.
        let preds = [pred(4, 0.5), pred(5, 0.55), pred(9, 0.59)];
        let golds = [4, 4, 4];
        let bins = calibrate(&preds, &golds);
        let bin = bins[2].rate.expect("all three land in [0.4, 0.6)");
        assert_eq!((bin.hits, bin.n), (1, 3));
        let reference = wilson95(1, 3);
        assert_eq!(bin, reference, "the bin rate IS a Wilson-CI rate");
        assert!(
            bins.iter()
                .enumerate()
                .all(|(i, b)| i == 2 || b.rate.is_none())
        );
    }

    #[test]
    fn predictions_without_confidence_stay_outside_the_curve() {
        // An abstention (None) and a confidence-free prediction (a baseline arm's shape)
        // enter no bin; the one confident prediction is the whole curve.
        let preds = [
            None,
            Some(Prediction {
                step: 4,
                confidence: None,
            }),
            pred(4, 0.9),
        ];
        let golds = [4, 4, 4];
        let bins = calibrate(&preds, &golds);
        let occupied: usize = bins.iter().filter_map(|b| b.rate).map(|r| r.n).sum();
        assert_eq!(occupied, 1, "only the confident prediction is binned");
        let last = bins[N_BINS - 1].rate.expect("0.9 lands in the top bin");
        assert_eq!((last.hits, last.n), (1, 1));
    }
}
