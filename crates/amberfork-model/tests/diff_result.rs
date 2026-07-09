//! Round-trip and invariant tests for the `DiffResult` output schema.

use amberfork_model::{
    Attribution, AttributionMode, Counterfactual, DiffResult, FieldDiff, FieldDiffKind, Fork, Meta,
    Move, MoveKind, Outcome, Recovery, RunPair, RunRef, Source, Warning, WarningCode,
};
use serde_json::json;

/// A representative diagnosed diff: an aligned pair with a benign log-move offset, a fork, a
/// field diff, counterfactual attribution, and a warning — exercises every branch of the schema.
fn diagnosed() -> DiffResult {
    DiffResult {
        runs: RunPair {
            a: RunRef {
                id: "good".into(),
                task: Some("refund #4512".into()),
                outcome: Some(Outcome::Pass),
                n_steps: 2,
            },
            b: RunRef {
                id: "bad".into(),
                task: Some("refund #4512".into()),
                outcome: Some(Outcome::Fail),
                n_steps: 3,
            },
        },
        alignment: vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::log(1, 0.4, 0.6),
            Move::sync(1, 2, 0.9, 0.7),
        ],
        fork: Some(Fork {
            index: 2,
            a_step: Some(1),
            b_step: Some(2),
            confidence: 0.7,
        }),
        field_diffs: vec![FieldDiff {
            step: 2,
            path: "outputs.status".into(),
            before: Some(json!("verified")),
            after: Some(json!("blog-estimate")),
            kind: FieldDiffKind::Changed,
        }],
        attribution: Some(Attribution {
            mode: AttributionMode::Counterfactual,
            origin_step: Some(2),
            propagation: vec![2],
            counterfactual: Some(Counterfactual {
                recovered: Recovery::Recovered,
                runs: 5,
            }),
            cause_label: Some("used unofficial source".into()),
            confidence: 0.8,
        }),
        warnings: vec![Warning {
            code: WarningCode::ContentAbsent,
            msg: "step 1 had no captured content".into(),
        }],
        meta: Meta::current(Source::Passive),
    }
}

#[test]
fn diagnosed_result_roundtrips_idempotently() {
    let result = diagnosed();
    let json = serde_json::to_string(&result).unwrap();
    let back: DiffResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, back);
}

#[test]
fn converged_result_omits_fork_and_roundtrips() {
    // The self-align / converged state: no fork, no diffs, no attribution. `fork` must be
    // absent from the JSON, not `null` — the designed converged state.
    let converged = DiffResult {
        runs: RunPair {
            a: RunRef {
                id: "x".into(),
                task: None,
                outcome: None,
                n_steps: 1,
            },
            b: RunRef {
                id: "x".into(),
                task: None,
                outcome: None,
                n_steps: 1,
            },
        },
        alignment: vec![Move::sync(0, 0, 0.0, 1.0)],
        fork: None,
        field_diffs: vec![],
        attribution: None,
        warnings: vec![],
        meta: Meta::current(Source::Passive),
    };
    let value = serde_json::to_value(&converged).unwrap();
    assert!(value.get("fork").is_none(), "converged diff must omit fork");
    assert!(value.get("attribution").is_none());
    let back: DiffResult = serde_json::from_value(value).unwrap();
    assert_eq!(converged, back);
}

#[test]
fn move_constructors_enforce_index_invariants() {
    let sync = Move::sync(3, 4, 0.1, 0.9);
    assert_eq!(sync.kind, MoveKind::Sync);
    assert_eq!((sync.a_idx, sync.b_idx), (Some(3), Some(4)));

    let log = Move::log(4, 0.5, 0.5);
    assert_eq!(log.kind, MoveKind::Log);
    assert_eq!((log.a_idx, log.b_idx), (None, Some(4)));

    let model = Move::model(3, 0.5, 0.5);
    assert_eq!(model.kind, MoveKind::Model);
    assert_eq!((model.a_idx, model.b_idx), (Some(3), None));
}

#[test]
fn log_and_model_moves_omit_the_absent_index() {
    // A log move has no `a_idx` key; a model move has no `b_idx` key.
    let log = serde_json::to_value(Move::log(1, 0.4, 0.6)).unwrap();
    assert!(log.get("a_idx").is_none());
    assert_eq!(log["b_idx"], json!(1));

    let model = serde_json::to_value(Move::model(1, 0.4, 0.6)).unwrap();
    assert!(model.get("b_idx").is_none());
    assert_eq!(model["a_idx"], json!(1));
}

#[test]
fn enum_wire_encodings_are_stable() {
    // These strings are the public --json contract; pin them so a rename can't slip through.
    assert_eq!(serde_json::to_value(MoveKind::Sync).unwrap(), json!("sync"));
    assert_eq!(serde_json::to_value(MoveKind::Log).unwrap(), json!("log"));
    assert_eq!(
        serde_json::to_value(MoveKind::Model).unwrap(),
        json!("model")
    );
    assert_eq!(
        serde_json::to_value(AttributionMode::Counterfactual).unwrap(),
        json!("counterfactual")
    );
    assert_eq!(
        serde_json::to_value(Recovery::NotRecovered).unwrap(),
        json!("not_recovered")
    );
    assert_eq!(
        serde_json::to_value(WarningCode::UnmappedAttributes).unwrap(),
        json!("unmapped-attributes")
    );
    assert_eq!(
        serde_json::to_value(FieldDiffKind::Added).unwrap(),
        json!("added")
    );
    assert_eq!(
        serde_json::to_value(Source::Record).unwrap(),
        json!("record")
    );
}

#[test]
fn fork_step_observed_names_the_b_side_directly() {
    // The common case: the fork move touches the observed run, so `Fork::b_step` is the answer.
    assert_eq!(diagnosed().fork_step_observed(), Some(2));
}

#[test]
fn fork_step_observed_is_none_when_converged() {
    let mut result = diagnosed();
    result.fork = None;
    assert_eq!(result.fork_step_observed(), None);
}

#[test]
fn fork_step_observed_falls_back_to_consumed_steps_on_a_model_only_fork() {
    // The observed run skipped steps the reference has: the fork move is model-only (no
    // `b_step`). The nearest observed step is the first one not yet consumed when the gap
    // opens — here moves 0 and 1 consumed observed steps 0 and 1, so the gap points at 2.
    let mut result = diagnosed();
    result.runs.b.n_steps = 4;
    result.alignment = vec![
        Move::sync(0, 0, 0.0, 1.0),
        Move::sync(1, 1, 0.1, 0.9),
        Move::model(2, 0.6, 0.5),
        Move::model(3, 0.3, 0.5),
    ];
    result.fork = Some(Fork {
        index: 2,
        a_step: Some(2),
        b_step: None,
        confidence: 0.5,
    });
    assert_eq!(result.fork_step_observed(), Some(2));
}

#[test]
fn fork_step_observed_clamps_a_trailing_gap_to_the_last_step() {
    // A model-only gap after every observed step was consumed: there is no "next" observed
    // step, so the pointer clamps to the last real one.
    let mut result = diagnosed();
    result.runs.b.n_steps = 2;
    result.alignment = vec![
        Move::sync(0, 0, 0.0, 1.0),
        Move::sync(1, 1, 0.1, 0.9),
        Move::model(2, 0.6, 0.5),
    ];
    result.fork = Some(Fork {
        index: 2,
        a_step: Some(2),
        b_step: None,
        confidence: 0.5,
    });
    assert_eq!(result.fork_step_observed(), Some(1));
}

#[test]
fn field_diff_add_remove_omit_the_absent_side() {
    let added = FieldDiff {
        step: 0,
        path: "outputs.extra".into(),
        before: None,
        after: Some(json!(1)),
        kind: FieldDiffKind::Added,
    };
    let value = serde_json::to_value(&added).unwrap();
    assert!(value.get("before").is_none());
    assert_eq!(value["after"], json!(1));
    let back: FieldDiff = serde_json::from_value(value).unwrap();
    assert_eq!(added, back);
}
