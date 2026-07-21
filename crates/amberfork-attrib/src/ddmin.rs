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

use amberfork_model::Recovery;
use std::future::Future;

/// The outcome of minimizing a candidate set.
///
/// Explicit rather than an `Option` so the caller can report the *full-set precondition* verdict
/// honestly: patching the whole region either recovers (there is a cause to minimize), still forks
/// (`Persisted` — the cause is not in this region), or cannot be decided (`Inconclusive` — a
/// nondeterministic re-run). The `Recovery` the caller ultimately emits follows straight from which
/// variant this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reduction {
    /// The full candidate set recovers; the payload is the 1-minimal cause — sorted candidate
    /// indices, none droppable without losing recovery, never empty.
    Minimized(Vec<usize>),
    /// The full candidate set did not recover: patching the whole region still forks, so there is
    /// no cause within it to minimize.
    Persisted,
    /// The oracle could not decide whether the full set recovers. No trustworthy cause, so the
    /// caller reports `Recovery::Unverified` rather than inventing one.
    Inconclusive,
}

/// Reduce a candidate set of `n` patchable steps to a 1-minimal recovering subset.
///
/// `recovers` is the oracle: given a subset of candidate indices (always sorted, always a subset of
/// `0..n`), it re-executes with exactly those steps patched and reports whether the run recovered.
/// It is `async` because that re-execution is real I/O — a replay server and a driven agent — and
/// fallible because the experiment itself can fail to run (the listener will not bind); such an
/// error aborts minimization rather than being mistaken for an inconclusive result.
///
/// The returned [`Reduction`] tells the three cases apart (see its docs). A `Minimized` subset is
/// never empty: the empty patch is the untouched bad run, which by definition still forks.
///
/// The first oracle call is always the full set `0..n` — the precondition — so the shape here is
/// classic ddmin only once recovery is established.
pub(crate) async fn minimize<F, Fut, E>(n: usize, mut recovers: F) -> Result<Reduction, E>
where
    F: FnMut(&[usize]) -> Fut,
    Fut: Future<Output = Result<Recovery, E>>,
{
    // No candidates is no cause. Also spares the oracle a pointless empty-patch re-run.
    if n == 0 {
        return Ok(Reduction::Persisted);
    }
    // Precondition: the whole region must recover, or there is nothing to minimize.
    let mut subset: Vec<usize> = (0..n).collect();
    match recovers(&subset).await? {
        Recovery::Recovered => {}
        Recovery::NotRecovered => return Ok(Reduction::Persisted),
        Recovery::Unverified => return Ok(Reduction::Inconclusive),
    }

    // Classic ddmin: try to reduce to a single block, then to a block's complement, and only when
    // neither shrinks the set refine the partition. The set stays recovering at every step, so what
    // survives is 1-minimal — no element can be dropped without losing recovery.
    let mut granularity = 2;
    while subset.len() >= 2 {
        let blocks = partition(&subset, granularity);

        // Reduce to a block: is one slice of the candidates sufficient on its own?
        let mut sufficient_block = None;
        for block in &blocks {
            if recovers(block).await? == Recovery::Recovered {
                sufficient_block = Some(block.clone());
                break;
            }
        }
        if let Some(block) = sufficient_block {
            subset = block;
            granularity = 2;
            continue;
        }

        // Reduce to a complement: can we drop a whole block and still recover? Granularity eases
        // back by one so the coarser split is retried against the smaller set.
        let mut reduced = false;
        for block in &blocks {
            let complement = complement(&subset, block);
            if recovers(&complement).await? == Recovery::Recovered {
                subset = complement;
                granularity = (granularity - 1).max(2);
                reduced = true;
                break;
            }
        }
        if reduced {
            continue;
        }

        // Neither shrank it. If the partition is already down to single elements the set is
        // 1-minimal; otherwise refine to a finer split and try again.
        if granularity >= subset.len() {
            break;
        }
        granularity = (granularity * 2).min(subset.len());
    }
    Ok(Reduction::Minimized(subset))
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
    use std::convert::Infallible;

    /// Smallest exponent with `2^e >= n`; `0` for `n <= 1`. The re-run bound is a multiple of this.
    fn ceil_log2(n: usize) -> u32 {
        if n <= 1 {
            0
        } else {
            usize::BITS - (n - 1).leading_zeros()
        }
    }

    #[tokio::test]
    async fn finds_a_single_element_minimal_cause() {
        // Recovery holds exactly when the one true cause `k` is patched: every superset recovers,
        // every subset without it persists. ddmin must peel away to the singleton {k}.
        let k = 5;
        let got = minimize(8, |subset: &[usize]| {
            let recovered = subset.contains(&k);
            async move { Ok::<_, Infallible>(if recovered { Recovered } else { NotRecovered }) }
        })
        .await
        .unwrap();
        assert_eq!(got, Reduction::Minimized(vec![k]));
    }

    #[tokio::test]
    async fn finds_a_multi_element_minimal_cause() {
        // Two independent faults: the run recovers only when BOTH are patched. Neither alone is the
        // cause, so a plain bisection would miss it — full ddmin returns the pair.
        let causes = [2usize, 6];
        let got = minimize(8, |subset: &[usize]| {
            let recovered = causes.iter().all(|c| subset.contains(c));
            async move { Ok::<_, Infallible>(if recovered { Recovered } else { NotRecovered }) }
        })
        .await
        .unwrap();
        assert_eq!(got, Reduction::Minimized(vec![2, 6]));
    }

    #[tokio::test]
    async fn stays_within_the_logarithmic_rerun_bound() {
        // Single-cause reduction visits O(log n) subsets: each level halves the candidate region in
        // at most two oracle calls, plus the one full-set precondition check.
        let n = 16;
        let k = 11;
        let calls = Cell::new(0usize);
        let got = minimize(n, |subset: &[usize]| {
            calls.set(calls.get() + 1);
            let recovered = subset.contains(&k);
            async move { Ok::<_, Infallible>(if recovered { Recovered } else { NotRecovered }) }
        })
        .await
        .unwrap();
        assert_eq!(got, Reduction::Minimized(vec![k]));
        assert!(
            calls.get() as u32 <= 3 * ceil_log2(n),
            "took {} oracle calls, bound is {}",
            calls.get(),
            3 * ceil_log2(n)
        );
    }

    #[tokio::test]
    async fn an_inconclusive_full_set_yields_no_cause() {
        // The oracle cannot even establish that patching everything recovers — a nondeterministic
        // re-run. ddmin must report Inconclusive (-> Recovery::Unverified upstream), never fabricate
        // a cause from an unstable signal.
        let got = minimize(8, |_: &[usize]| async { Ok::<_, Infallible>(Unverified) })
            .await
            .unwrap();
        assert_eq!(got, Reduction::Inconclusive);
    }

    #[tokio::test]
    async fn a_fork_that_never_recovers_persists() {
        // Even fully patched the run still forks — recovery was never on the table, so there is no
        // cause to minimize.
        let got = minimize(8, |_: &[usize]| async { Ok::<_, Infallible>(NotRecovered) })
            .await
            .unwrap();
        assert_eq!(got, Reduction::Persisted);
    }
}
