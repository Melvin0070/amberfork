//! Delta-debugging minimization: reduce a recovering patch-set to its minimal cause.
//!
//! The single-patch counterfactual (issue #37) proves that grafting the good run's response onto
//! the *fork* step recovers the run. But the fork is rarely the whole story: the true fault may sit
//! one step earlier, or a run of downstream steps may merely *propagate* a single upstream break.
//! This is the reducer that tells those apart — given the fork plus its propagation tail as a
//! candidate set, it finds the **smallest subset whose patch still recovers the run** (issue #38).
//!
//! ## The algorithm is classic ddmin, re-pointed
//!
//! Zeller & Hildebrandt's `ddmin` minimizes a *failing* input to a 1-minimal core: a subset that
//! still fails, from which dropping any single element makes it pass. We preserve **recovery**
//! instead of failure — [`Recovery::Recovered`] is the property held invariant — but the shape is
//! identical: partition, try each block, try each complement, refine granularity, stop when no
//! single element can be dropped. treereduce/tree-sitter were dropped (design line 554): they
//! reduce *source code* against a grammar, and an agent-step chain has none.
//!
//! ## Two honesty properties it must keep
//!
//! - **No cause without recovery.** If patching the *whole* candidate set does not recover — the
//!   fork survives even fully patched, or the oracle is too unstable to say — there is no minimal
//!   cause to report. [`minimize`] returns `None`, which the caller renders as
//!   [`Recovery::Unverified`], never a fabricated origin.
//! - **Inconclusive never reduces.** A nondeterministic re-run ([`Recovery::Unverified`] mid-search)
//!   is treated as "not a recovering subset": it can never *cause* a reduction, so an unstable
//!   oracle can only fail to minimize, never mis-minimize.
//!
//! ## Cost
//!
//! For a single-cause set (the fork step is the fault; the tail purely propagates), each level of
//! the search halves the region in at most two oracle calls, so it converges in `O(log n)` calls —
//! the bound issue #38 pins down. Multiple independent faults cost more (classic ddmin is quadratic
//! in the worst case), but that case is real and a plain bisection would miss it, which is why this
//! is ddmin and not a binary search.

// Wired into `verify` in the next slice (issue #38 step 2); until then only the unit tests below
// exercise it, and `clippy -D warnings` would flag the unused `pub(crate)` surface.
#![allow(dead_code)]

use amberfork_model::Recovery;

/// Reduce a candidate set of `n` patchable steps to a 1-minimal recovering subset.
///
/// `recovers` is the oracle: given a subset of candidate indices (always sorted, always a subset of
/// `0..n`), it re-executes with exactly those steps patched and reports whether the run recovered.
/// Returns the minimal subset — sorted indices, none droppable without losing recovery — or `None`
/// when recovery could not be established (the full set does not recover, or the oracle was
/// inconclusive about it). The returned subset is never empty: the empty patch is the untouched bad
/// run, which by definition still forks.
pub(crate) fn minimize<F>(n: usize, mut recovers: F) -> Option<Vec<usize>>
where
    F: FnMut(&[usize]) -> Recovery,
{
    // No candidates is no cause. Also spares the oracle a pointless empty-patch re-run.
    if n == 0 {
        return None;
    }
    // Precondition: the whole region must recover, or there is nothing to minimize. A `NotRecovered`
    // full set means recovery was never on the table; an `Unverified` one means the oracle is too
    // unstable to trust — either way, no cause, not a fabricated one.
    let mut subset: Vec<usize> = (0..n).collect();
    if recovers(&subset) != Recovery::Recovered {
        return None;
    }

    // Classic ddmin: try to reduce to a single block, then to a block's complement, and only when
    // neither shrinks the set refine the partition. The set stays recovering at every step, so what
    // survives is 1-minimal — no element can be dropped without losing recovery.
    let mut granularity = 2;
    while subset.len() >= 2 {
        let blocks = partition(&subset, granularity);

        // Reduce to a block: is one slice of the candidates sufficient on its own?
        if let Some(block) = blocks
            .iter()
            .find(|block| recovers(block) == Recovery::Recovered)
        {
            subset = block.clone();
            granularity = 2;
            continue;
        }

        // Reduce to a complement: can we drop a whole block and still recover? Granularity eases
        // back by one so the coarser split is retried against the smaller set.
        if let Some(complement) = blocks
            .iter()
            .map(|block| complement(&subset, block))
            .find(|complement| recovers(complement) == Recovery::Recovered)
        {
            granularity = (granularity - 1).max(2);
            subset = complement;
            continue;
        }

        // Neither shrank it. If the partition is already down to single elements the set is
        // 1-minimal; otherwise refine to a finer split and try again.
        if granularity >= subset.len() {
            break;
        }
        granularity = (granularity * 2).min(subset.len());
    }
    Some(subset)
}

/// Split `subset` into `k` contiguous, near-equal blocks (the last few absorb the remainder). `k` is
/// clamped to `subset.len()`, so every returned block is non-empty.
fn partition(subset: &[usize], k: usize) -> Vec<Vec<usize>> {
    let len = subset.len();
    let k = k.clamp(1, len);
    (0..k)
        .map(|i| subset[i * len / k..(i + 1) * len / k].to_vec())
        .collect()
}

/// The elements of `subset` not in `block`. `block` is a contiguous slice of `subset`, so the result
/// stays sorted.
fn complement(subset: &[usize], block: &[usize]) -> Vec<usize> {
    subset
        .iter()
        .filter(|element| !block.contains(element))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::Recovery::{NotRecovered, Recovered, Unverified};
    use std::cell::Cell;

    /// Smallest exponent with `2^e >= n`; `0` for `n <= 1`. The re-run bound is a multiple of this.
    fn ceil_log2(n: usize) -> u32 {
        if n <= 1 {
            0
        } else {
            usize::BITS - (n - 1).leading_zeros()
        }
    }

    #[test]
    fn finds_a_single_element_minimal_cause() {
        // Recovery holds exactly when the one true cause `k` is patched: every superset recovers,
        // every subset without it persists. ddmin must peel away to the singleton {k}.
        let k = 5;
        let got = minimize(8, |subset| {
            if subset.contains(&k) {
                Recovered
            } else {
                NotRecovered
            }
        });
        assert_eq!(got, Some(vec![k]));
    }

    #[test]
    fn finds_a_multi_element_minimal_cause() {
        // Two independent faults: the run recovers only when BOTH are patched. Neither alone is the
        // cause, so a plain bisection would miss it — full ddmin returns the pair.
        let causes = [2usize, 6];
        let got = minimize(8, |subset| {
            if causes.iter().all(|c| subset.contains(c)) {
                Recovered
            } else {
                NotRecovered
            }
        });
        assert_eq!(got, Some(vec![2, 6]));
    }

    #[test]
    fn stays_within_the_logarithmic_rerun_bound() {
        // Single-cause reduction visits O(log n) subsets: each level halves the candidate region in
        // at most two oracle calls, plus the one full-set precondition check.
        let n = 16;
        let k = 11;
        let calls = Cell::new(0usize);
        let got = minimize(n, |subset| {
            calls.set(calls.get() + 1);
            if subset.contains(&k) {
                Recovered
            } else {
                NotRecovered
            }
        });
        assert_eq!(got, Some(vec![k]));
        assert!(
            calls.get() as u32 <= 3 * ceil_log2(n),
            "took {} oracle calls, bound is {}",
            calls.get(),
            3 * ceil_log2(n)
        );
    }

    #[test]
    fn an_inconclusive_full_set_yields_no_cause() {
        // The oracle cannot even establish that patching everything recovers — a nondeterministic
        // re-run. ddmin must report "no minimal cause" (None -> Recovery::Unverified upstream),
        // never fabricate one from an unstable signal.
        let got = minimize(8, |_subset| Unverified);
        assert_eq!(got, None);
    }

    #[test]
    fn a_fork_that_never_recovers_yields_no_cause() {
        // Even fully patched the run still forks — recovery was never on the table, so there is no
        // cause to minimize.
        let got = minimize(8, |_subset| NotRecovered);
        assert_eq!(got, None);
    }
}
