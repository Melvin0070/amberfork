//! The record path end to end at the model boundary: a realistic cassette normalizes into a
//! valid `Run` that survives the real aligner and holds a stable shape.
//!
//! The unit tests in `src/normalize.rs` prove the mapping on hand-built cassettes; these prove
//! it against the two things a synthetic cassette can't: the canonical self-align invariant
//! (which would expose an ill-formed trajectory the aligner had to paper over) and a pinned
//! snapshot of the produced run.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_record::{Cassette, normalize};

const REFUND_TRIAGE: &str = include_str!("fixtures/refund-triage.cassette.json");

fn refund_triage() -> Cassette {
    serde_json::from_str(REFUND_TRIAGE).expect("fixture is a valid cassette")
}

#[test]
fn a_recorded_run_does_not_fork_against_itself() {
    // The project's canonical guard: a run aligned against itself has no divergence. If
    // normalization produced a non-contiguous or otherwise ill-formed trajectory, the aligner
    // would have to invent a fork where none exists.
    let run = normalize(&refund_triage());
    let result = diff(&run, &run, &LexicalCost, &DiffParams::default()).expect("self-align");
    assert!(result.fork.is_none(), "a run must not fork against itself");
}

#[test]
fn every_step_of_a_recorded_run_has_content() {
    // The record path's promise, restated on a realistic recording: full content is guaranteed,
    // so the aligner never has to match on metadata alone.
    let run = normalize(&refund_triage());
    assert_eq!(run.steps.len(), 2);
    assert!(
        run.steps
            .iter()
            .all(|s| s.inputs.is_some() && s.outputs.is_some())
    );
}

#[test]
fn normalized_run_shape_is_stable() {
    let run = normalize(&refund_triage());
    insta::assert_json_snapshot!(run);
}
