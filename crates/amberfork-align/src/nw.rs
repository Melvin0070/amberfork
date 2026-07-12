//! Affine-gap global alignment (Gotoh three-state Needleman–Wunsch) over step costs.
//!
//! Produces the move-typed alignment the rest of the product reads: a sequence of
//! [`Move`]s in run order. Side convention matches the `DiffResult` contract, not the spike:
//! **`a` is the reference (“model” side), `b` is the observed/failing run (“log” side)** — so
//! a [`MoveKind::Log`](amberfork_model::MoveKind::Log) move is a step extra in the observed
//! run and a [`MoveKind::Model`](amberfork_model::MoveKind::Model) move is a step the observed
//! run skipped.
//!
//! Affine gaps (open ≠ extend) are the empirically load-bearing choice: a retry or an inserted
//! detour is *one* event, so its second and later steps must be cheaper than opening a new gap,
//! or the aligner shreds detours into scattered mismatches (spike 001). Gap runs re-enter
//! through a sync state before switching direction (standard Gotoh simplification, as
//! validated in the spike).
//!
//! Ties prefer the sync state, so two identical runs align as pure sync — the self-align
//! invariant the fork rule builds on.

use crate::cost::CostModel;
use amberfork_model::{Move, Step};

/// Affine-gap penalties, on the same scale as [`CostModel`] costs (`[0, 1]` per step).
/// Defaults are the spike-validated values (notebook 001–003).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlignParams {
    /// Cost of the first step of a gap run.
    pub gap_open: f64,
    /// Cost of each further step extending an open gap run.
    pub gap_ext: f64,
}

impl Default for AlignParams {
    fn default() -> Self {
        Self {
            gap_open: 0.6,
            gap_ext: 0.3,
        }
    }
}

/// Globally align reference `a` against observed `b`, minimizing total cost.
///
/// Returns the full move-typed alignment in run order. Every step of `a` appears exactly once
/// across sync/model moves and every step of `b` exactly once across sync/log moves, both in
/// increasing index order.
///
/// Move fields: a sync move's `cost` is the cost-model value for the pair and its `confidence`
/// is `1 - cost` (the similarity itself). A gap move's `cost` is the gap penalty it actually
/// paid (open or extend), and its `confidence` is the *minimum* cost between the gapped step
/// and any step on the other side — “even the closest counterpart looks this different” — or
/// `1.0` when the other side is empty.
pub fn align(
    a: &[Step],
    b: &[Step],
    cost_model: &impl CostModel,
    params: &AlignParams,
) -> Vec<Move> {
    let (n, m) = (a.len(), b.len());
    // Prepare each side once — O(n+m) per-step digests — so the O(n·m) fill below spends
    // nothing on re-deriving per-step state (issue #16).
    let a_prep: Vec<_> = a.iter().map(|s| cost_model.prepare(s)).collect();
    let b_prep: Vec<_> = b.iter().map(|s| cost_model.prepare(s)).collect();
    let cost = Matrix::from_fn(n, m, |i, j| {
        cost_model.cost_prepared(&a_prep[i], &b_prep[j])
    });

    // Three-state Gotoh DP over (n+1)×(m+1). `sync[i][j]` = best cost ending in a sync of
    // a[i-1]/b[j-1]; `gap_a` = ending in a gapped a-step (model move); `gap_b` = ending in a
    // gapped b-step (log move). Gap states re-enter only from sync or themselves.
    let mut sync = Matrix::filled(n + 1, m + 1, f64::INFINITY);
    let mut gap_a = Matrix::filled(n + 1, m + 1, f64::INFINITY);
    let mut gap_b = Matrix::filled(n + 1, m + 1, f64::INFINITY);
    *sync.at_mut(0, 0) = 0.0;
    for i in 1..=n {
        *gap_a.at_mut(i, 0) = params.gap_open + (i - 1) as f64 * params.gap_ext;
    }
    for j in 1..=m {
        *gap_b.at_mut(0, j) = params.gap_open + (j - 1) as f64 * params.gap_ext;
    }
    for i in 1..=n {
        for j in 1..=m {
            let best_prev = sync
                .at(i - 1, j - 1)
                .min(gap_a.at(i - 1, j - 1))
                .min(gap_b.at(i - 1, j - 1));
            *sync.at_mut(i, j) = cost.at(i - 1, j - 1) + best_prev;
            *gap_a.at_mut(i, j) =
                (sync.at(i - 1, j) + params.gap_open).min(gap_a.at(i - 1, j) + params.gap_ext);
            *gap_b.at_mut(i, j) =
                (sync.at(i, j - 1) + params.gap_open).min(gap_b.at(i, j - 1) + params.gap_ext);
        }
    }

    // Confidence of a gap move: even the closest counterpart on the other side costs this
    // much. An empty other side means fully confident (1.0).
    let gap_a_confidence = |i: usize| (0..m).map(|j| cost.at(i, j)).fold(1.0f64, f64::min);
    let gap_b_confidence = |j: usize| (0..n).map(|i| cost.at(i, j)).fold(1.0f64, f64::min);

    // Traceback by recomputation (no pointer matrices): at each cell, re-derive which state
    // the minimum came from. Ties prefer sync, so identical runs come out pure sync.
    let mut moves = Vec::new();
    let (mut i, mut j) = (n, m);
    let mut state = best_state(sync.at(i, j), gap_a.at(i, j), gap_b.at(i, j));
    while i > 0 || j > 0 {
        match state {
            State::Sync if i > 0 && j > 0 => {
                let pair_cost = cost.at(i - 1, j - 1);
                moves.push(Move::sync(i - 1, j - 1, pair_cost, 1.0 - pair_cost));
                state = best_state(
                    sync.at(i - 1, j - 1),
                    gap_a.at(i - 1, j - 1),
                    gap_b.at(i - 1, j - 1),
                );
                i -= 1;
                j -= 1;
            }
            State::GapA if i > 0 => {
                let opened =
                    sync.at(i - 1, j) + params.gap_open <= gap_a.at(i - 1, j) + params.gap_ext;
                let paid = if opened {
                    params.gap_open
                } else {
                    params.gap_ext
                };
                moves.push(Move::model(i - 1, paid, gap_a_confidence(i - 1)));
                state = if opened { State::Sync } else { State::GapA };
                i -= 1;
            }
            State::GapB if j > 0 => {
                let opened =
                    sync.at(i, j - 1) + params.gap_open <= gap_b.at(i, j - 1) + params.gap_ext;
                let paid = if opened {
                    params.gap_open
                } else {
                    params.gap_ext
                };
                moves.push(Move::log(j - 1, paid, gap_b_confidence(j - 1)));
                state = if opened { State::Sync } else { State::GapB };
                j -= 1;
            }
            // Safety: a side is exhausted but the state disagrees (cannot happen with the
            // ∞-initialized edges above; kept so a bug degrades instead of hanging).
            _ if i > 0 => {
                moves.push(Move::model(i - 1, params.gap_ext, gap_a_confidence(i - 1)));
                i -= 1;
            }
            _ => {
                moves.push(Move::log(j - 1, params.gap_ext, gap_b_confidence(j - 1)));
                j -= 1;
            }
        }
    }
    moves.reverse();
    moves
}

/// The three DP states of the Gotoh alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Sync,
    GapA,
    GapB,
}

/// The state holding the minimum of the three cell values; ties prefer sync, then the
/// model-side gap — the same order the spike validated.
fn best_state(sync: f64, gap_a: f64, gap_b: f64) -> State {
    let mut best = (sync, State::Sync);
    if gap_a < best.0 {
        best = (gap_a, State::GapA);
    }
    if gap_b < best.0 {
        best = (gap_b, State::GapB);
    }
    best.1
}

/// Dense row-major matrix of `f64`. A `Vec<Vec<f64>>` would work, but one contiguous buffer
/// keeps the three DP tables cache-friendly, and the accessors keep index math in one place.
struct Matrix {
    cols: usize,
    cells: Vec<f64>,
}

impl Matrix {
    fn filled(rows: usize, cols: usize, value: f64) -> Self {
        Self {
            cols,
            cells: vec![value; rows * cols],
        }
    }

    fn from_fn(rows: usize, cols: usize, f: impl Fn(usize, usize) -> f64) -> Self {
        let mut m = Self::filled(rows, cols, 0.0);
        for i in 0..rows {
            for j in 0..cols {
                *m.at_mut(i, j) = f(i, j);
            }
        }
        m
    }

    fn at(&self, i: usize, j: usize) -> f64 {
        self.cells[i * self.cols + j]
    }

    fn at_mut(&mut self, i: usize, j: usize) -> &mut f64 {
        &mut self.cells[i * self.cols + j]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::LexicalCost;
    use amberfork_model::{MoveKind, test_support};

    /// A trivially hand-checkable cost model: 0 if names match, 1 otherwise. Exercising the
    /// trait seam with a mock is the point — the aligner must not care what costs mean.
    struct NameEq;

    impl CostModel for NameEq {
        type Prepared = String;

        fn prepare(&self, step: &Step) -> String {
            step.name.clone()
        }

        fn cost_prepared(&self, a: &String, b: &String) -> f64 {
            if a == b { 0.0 } else { 1.0 }
        }
    }

    fn step(idx: usize, name: &str, out: &str) -> Step {
        test_support::step(idx, name).text_output(out).build()
    }

    fn run_of(names: &[&str]) -> Vec<Step> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| step(i, n, &format!("{n} output")))
            .collect()
    }

    /// The structural well-formedness invariant: each side's indices appear exactly once, in
    /// increasing order, across the move kinds that carry them.
    fn assert_covers_in_order(moves: &[Move], n_a: usize, n_b: usize) {
        let a_seen: Vec<usize> = moves.iter().filter_map(|m| m.a_idx).collect();
        let b_seen: Vec<usize> = moves.iter().filter_map(|m| m.b_idx).collect();
        assert_eq!(a_seen, (0..n_a).collect::<Vec<_>>(), "a coverage/order");
        assert_eq!(b_seen, (0..n_b).collect::<Vec<_>>(), "b coverage/order");
    }

    #[test]
    fn identical_runs_align_all_sync() {
        let run = run_of(&["plan", "search", "fetch", "answer"]);
        let moves = align(&run, &run, &LexicalCost, &AlignParams::default());
        assert_eq!(moves.len(), 4);
        for (i, m) in moves.iter().enumerate() {
            assert_eq!(m.kind, MoveKind::Sync);
            assert_eq!((m.a_idx, m.b_idx), (Some(i), Some(i)));
            assert_eq!(m.cost, 0.0);
            assert_eq!(m.confidence, 1.0);
        }
        assert_covers_in_order(&moves, 4, 4);
    }

    #[test]
    fn extra_observed_step_is_a_log_move() {
        let a = run_of(&["plan", "search", "answer"]);
        let b = run_of(&["plan", "retry", "search", "answer"]);
        let moves = align(&a, &b, &NameEq, &AlignParams::default());
        let kinds: Vec<MoveKind> = moves.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            [
                MoveKind::Sync,
                MoveKind::Log,
                MoveKind::Sync,
                MoveKind::Sync
            ]
        );
        assert_eq!(moves[1].b_idx, Some(1));
        assert_eq!(moves[1].cost, AlignParams::default().gap_open);
        assert_covers_in_order(&moves, 3, 4);
    }

    #[test]
    fn missing_observed_step_is_a_model_move() {
        let a = run_of(&["plan", "verify", "answer"]);
        let b = run_of(&["plan", "answer"]);
        let moves = align(&a, &b, &NameEq, &AlignParams::default());
        let kinds: Vec<MoveKind> = moves.iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [MoveKind::Sync, MoveKind::Model, MoveKind::Sync]);
        assert_eq!(moves[1].a_idx, Some(1));
        assert_covers_in_order(&moves, 3, 2);
    }

    #[test]
    fn contiguous_gap_pays_open_then_extend() {
        let a = run_of(&["start", "finish"]);
        let b = run_of(&["start", "junk1", "junk2", "finish"]);
        let params = AlignParams::default();
        let moves = align(&a, &b, &NameEq, &params);
        let kinds: Vec<MoveKind> = moves.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            [MoveKind::Sync, MoveKind::Log, MoveKind::Log, MoveKind::Sync]
        );
        // Affine semantics: the detour is ONE event — first gapped step opens, second extends.
        assert_eq!(moves[1].cost, params.gap_open);
        assert_eq!(moves[2].cost, params.gap_ext);
        assert_covers_in_order(&moves, 2, 4);
    }

    #[test]
    fn empty_sides_become_pure_gap_runs() {
        let some = run_of(&["only", "steps"]);
        let none: Vec<Step> = Vec::new();
        let params = AlignParams::default();

        let all_log = align(&none, &some, &NameEq, &params);
        assert_eq!(
            all_log.iter().map(|m| m.kind).collect::<Vec<_>>(),
            [MoveKind::Log, MoveKind::Log]
        );
        // No other side at all: fully confident these are unmatched.
        assert!(all_log.iter().all(|m| m.confidence == 1.0));

        let all_model = align(&some, &none, &NameEq, &params);
        assert_eq!(
            all_model.iter().map(|m| m.kind).collect::<Vec<_>>(),
            [MoveKind::Model, MoveKind::Model]
        );

        assert!(align(&none, &none, &NameEq, &params).is_empty());
    }

    #[test]
    fn gap_confidence_is_distance_to_closest_counterpart() {
        // The extra observed step shares tokens with "search" — the aligner still gaps it,
        // but must report low confidence that it is truly unmatched.
        let a = vec![step(0, "search", "query census population figures")];
        let b = vec![
            step(0, "search", "query census population figures"),
            step(1, "search", "query census population figures again"),
        ];
        let moves = align(&a, &b, &LexicalCost, &AlignParams::default());
        assert_eq!(
            moves.iter().map(|m| m.kind).collect::<Vec<_>>(),
            [MoveKind::Sync, MoveKind::Log]
        );
        let expected = LexicalCost.cost(&a[0], &b[1]);
        assert!(expected > 0.0 && expected < 0.5, "fixture sanity");
        assert_eq!(moves[1].confidence, expected);
    }
}
