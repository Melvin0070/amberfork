//! The canonical guard (CLAUDE.md, "Tests are part of done"): **a run aligned against itself
//! must be all-sync with no fork** — for *any* run, not just the ones we thought of. Property
//! violations here mean the aligner or cost model broke determinism or tie-breaking.

use amberfork_align::{AlignParams, ForkParams, LexicalCost, align, find_fork};
use amberfork_model::{MoveKind, Payload, Step, StepKind};
use proptest::prelude::*;
use serde_json::Map;

/// Arbitrary-ish steps: pooled names (realistic — agents reuse tool names), free-form unicode
/// outputs (stresses the tokenizer), and occasional content-less steps.
fn arb_step(idx: usize) -> impl Strategy<Value = Step> {
    let kind = prop_oneof![
        Just(StepKind::Llm),
        Just(StepKind::Tool),
        Just(StepKind::Agent),
        Just(StepKind::Other),
    ];
    let name = prop_oneof![
        Just("planner".to_string()),
        Just("web.search".to_string()),
        Just("web.fetch".to_string()),
        Just("reader".to_string()),
        "[a-z]{1,8}",
    ];
    let outputs = prop_oneof![
        1 => Just(None),
        4 => any::<String>().prop_map(|s| Some(Payload::Text(s))),
    ];
    (kind, name, outputs).prop_map(move |(kind, name, outputs)| Step {
        idx,
        kind,
        name,
        inputs: None,
        outputs,
        attrs: Map::new(),
        t_start: None,
        t_end: None,
        parent_idx: None,
    })
}

fn arb_run() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(any::<u8>(), 0..25).prop_flat_map(|shape| {
        shape
            .into_iter()
            .enumerate()
            .map(|(i, _)| arb_step(i))
            .collect::<Vec<_>>()
    })
}

proptest! {
    #[test]
    fn self_alignment_is_all_sync_with_no_fork(run in arb_run()) {
        let moves = align(&run, &run, &LexicalCost, &AlignParams::default());

        prop_assert_eq!(moves.len(), run.len());
        for (i, m) in moves.iter().enumerate() {
            prop_assert_eq!(m.kind, MoveKind::Sync);
            prop_assert_eq!((m.a_idx, m.b_idx), (Some(i), Some(i)));
            prop_assert_eq!(m.cost, 0.0);
        }

        // No fork at the default threshold — nor even at tau = 0, the strictest possible.
        prop_assert_eq!(find_fork(&moves, &ForkParams::default()), None);
        let strictest = ForkParams { tau: 0.0, ..ForkParams::default() };
        prop_assert_eq!(find_fork(&moves, &strictest), None);
    }
}
