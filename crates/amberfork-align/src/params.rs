//! Parameter validation — the single place the engine's tunables are checked.
//!
//! The params types themselves stay plain data (`AlignParams`, `ForkParams`, [`DiffParams`]):
//! construction cannot fail and the engine never asserts. Whoever accepts *user-supplied*
//! values (the CLI boundary today, `amberfork-bench` sweeps later) calls `validated()` once
//! and maps [`ParamError`] to its own error surface. Decided at the issue #3 review; the
//! invariants encode what the engine actually assumes:
//!
//! - `tau` is a cost threshold on the `[0, 1]` cost-model scale, so it must live there too.
//! - `resync_k = 0` would declare every blip recovered-from, i.e. no fork can ever exist.
//! - Non-positive gap penalties would make the aligner prefer gaps over perfect syncs.
//! - `gap_ext > gap_open` breaks the affine premise (a detour's later steps must be cheaper
//!   than opening a new gap, or detours shred into scattered mismatches — spike 001).

use crate::diff::DiffParams;
use crate::fork::ForkParams;
use crate::nw::AlignParams;
use std::fmt;

/// Why a parameter set was rejected. One variant per violated invariant, carrying the value
/// so the message can be honest about what was seen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamError {
    /// `tau` must be finite and within `[0, 1]` (the cost-model scale).
    TauOutOfRange(f64),
    /// `resync_k` must be at least 1.
    ResyncKZero,
    /// `gap_open` must be finite and positive.
    GapOpenNotPositive(f64),
    /// `gap_ext` must be finite and positive.
    GapExtNotPositive(f64),
    /// `gap_ext` must not exceed `gap_open` (the affine-gap premise).
    GapExtExceedsOpen { gap_open: f64, gap_ext: f64 },
}

impl fmt::Display for ParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TauOutOfRange(tau) => {
                write!(f, "tau must be within [0, 1], got {tau}")
            }
            Self::ResyncKZero => {
                write!(
                    f,
                    "resync_k must be at least 1 (0 would recover every blip)"
                )
            }
            Self::GapOpenNotPositive(v) => {
                write!(f, "gap_open must be positive and finite, got {v}")
            }
            Self::GapExtNotPositive(v) => {
                write!(f, "gap_ext must be positive and finite, got {v}")
            }
            Self::GapExtExceedsOpen { gap_open, gap_ext } => write!(
                f,
                "gap_ext ({gap_ext}) must not exceed gap_open ({gap_open}): extending a gap \
                 costing more than opening one breaks the affine premise"
            ),
        }
    }
}

impl std::error::Error for ParamError {}

impl AlignParams {
    /// Check the aligner's invariants, passing `self` through unchanged when they hold.
    ///
    /// # Errors
    /// The first violated invariant, as a [`ParamError`].
    pub fn validated(self) -> Result<Self, ParamError> {
        if !self.gap_open.is_finite() || self.gap_open <= 0.0 {
            return Err(ParamError::GapOpenNotPositive(self.gap_open));
        }
        if !self.gap_ext.is_finite() || self.gap_ext <= 0.0 {
            return Err(ParamError::GapExtNotPositive(self.gap_ext));
        }
        if self.gap_ext > self.gap_open {
            return Err(ParamError::GapExtExceedsOpen {
                gap_open: self.gap_open,
                gap_ext: self.gap_ext,
            });
        }
        Ok(self)
    }
}

impl ForkParams {
    /// Check the fork rule's invariants, passing `self` through unchanged when they hold.
    ///
    /// # Errors
    /// The first violated invariant, as a [`ParamError`].
    pub fn validated(self) -> Result<Self, ParamError> {
        if !self.tau.is_finite() || !(0.0..=1.0).contains(&self.tau) {
            return Err(ParamError::TauOutOfRange(self.tau));
        }
        if self.resync_k == 0 {
            return Err(ParamError::ResyncKZero);
        }
        Ok(self)
    }
}

impl DiffParams {
    /// Check every engine invariant at once — the one call a boundary makes before handing
    /// user-supplied values to [`crate::diff`].
    ///
    /// # Errors
    /// The first violated invariant, as a [`ParamError`].
    pub fn validated(self) -> Result<Self, ParamError> {
        self.align.validated()?;
        self.fork.validated()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_calibrated_defaults_are_valid() {
        assert_eq!(
            DiffParams::default().validated(),
            Ok(DiffParams::default()),
            "the shipped defaults must satisfy their own invariants"
        );
    }

    #[test]
    fn tau_outside_the_cost_scale_is_rejected() {
        for tau in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
            let params = ForkParams {
                tau,
                ..ForkParams::default()
            };
            match params.validated() {
                Err(ParamError::TauOutOfRange(seen)) => {
                    assert!(seen.is_nan() == tau.is_nan() || seen == tau)
                }
                other => panic!("tau {tau} must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn resync_k_zero_is_rejected() {
        let params = ForkParams {
            resync_k: 0,
            ..ForkParams::default()
        };
        assert_eq!(params.validated(), Err(ParamError::ResyncKZero));
    }

    #[test]
    fn non_positive_or_non_finite_gaps_are_rejected() {
        for gap_open in [0.0, -0.5, f64::NAN] {
            let params = AlignParams {
                gap_open,
                ..AlignParams::default()
            };
            assert!(
                matches!(params.validated(), Err(ParamError::GapOpenNotPositive(_))),
                "gap_open {gap_open} must be rejected"
            );
        }
        for gap_ext in [0.0, -0.5, f64::INFINITY] {
            let params = AlignParams {
                gap_ext,
                ..AlignParams::default()
            };
            assert!(
                matches!(
                    params.validated(),
                    Err(ParamError::GapExtNotPositive(_) | ParamError::GapExtExceedsOpen { .. })
                ),
                "gap_ext {gap_ext} must be rejected"
            );
        }
    }

    #[test]
    fn gap_ext_above_gap_open_breaks_the_affine_premise() {
        let params = AlignParams {
            gap_open: 0.3,
            gap_ext: 0.6,
        };
        assert_eq!(
            params.validated(),
            Err(ParamError::GapExtExceedsOpen {
                gap_open: 0.3,
                gap_ext: 0.6
            })
        );
    }

    #[test]
    fn diff_params_compose_both_validations() {
        let bad_fork = DiffParams {
            fork: ForkParams {
                tau: 2.0,
                ..ForkParams::default()
            },
            ..DiffParams::default()
        };
        assert!(matches!(
            bad_fork.validated(),
            Err(ParamError::TauOutOfRange(_))
        ));

        let bad_align = DiffParams {
            align: AlignParams {
                gap_open: -1.0,
                ..AlignParams::default()
            },
            ..DiffParams::default()
        };
        assert!(matches!(
            bad_align.validated(),
            Err(ParamError::GapOpenNotPositive(_))
        ));
    }

    #[test]
    fn error_messages_name_the_offending_value() {
        let msg = ParamError::TauOutOfRange(1.5).to_string();
        assert!(msg.contains("1.5"), "got: {msg}");
        let msg = ParamError::GapExtExceedsOpen {
            gap_open: 0.3,
            gap_ext: 0.6,
        }
        .to_string();
        assert!(msg.contains("0.3") && msg.contains("0.6"), "got: {msg}");
    }
}
