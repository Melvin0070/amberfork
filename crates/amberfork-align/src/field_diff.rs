//! Field-level diffs inside aligned pairs — the producer for `DiffResult::field_diffs`
//! (issue #13, the content-diff pane's data).
//!
//! The aligner answers *which steps pair up* and the fork rule answers *where the runs
//! diverge*; this pass answers *what exactly changed inside a pair*. Per the design's
//! granularity split (OV-6): attribution evidence is an explicit, typed, field-level diff of
//! values — never a similarity score. So only synchronous moves are diffed (a log/model move
//! has no counterpart to compare), and equal payloads emit nothing: on a converged self-diff
//! this produces the empty vec by construction, keeping the canonical invariant honest now
//! that the field is no longer hardcoded empty.
//!
//! Comparison happens on the wire representation ([`serde_json::Value`]), not the [`Payload`]
//! variant: the JSON contract is the seam, so `Text("x")` and an `Other` holding `"x"` are
//! the same value, not a spurious diff. Paths are rooted at the slot (`inputs`/`outputs`) and
//! extend one `.key` per object level; arrays and mixed-type values are leaves (one `Changed`
//! with both values) — element-wise array alignment is its own problem and not this pass's.

use amberfork_model::{FieldDiff, FieldDiffKind, Move, MoveKind, Payload, Step};
use serde_json::Value;
use std::collections::BTreeSet;

/// Diff the payloads of every synchronous pair in `alignment`. `step` on each emitted
/// [`FieldDiff`] is the *alignment index* of the pair (the renderer's filter key), not a run
/// step index.
pub(crate) fn field_diffs(
    reference: &[Step],
    observed: &[Step],
    alignment: &[Move],
) -> Vec<FieldDiff> {
    let mut out = Vec::new();
    for (index, mv) in alignment.iter().enumerate() {
        if mv.kind != MoveKind::Sync {
            continue;
        }
        // The Move constructors guarantee a sync move has both indices in bounds; `get`
        // keeps this pass total on a hand-built (e.g. deserialized) alignment anyway.
        let pair = mv
            .a_idx
            .and_then(|i| reference.get(i))
            .zip(mv.b_idx.and_then(|i| observed.get(i)));
        let Some((a, b)) = pair else {
            continue;
        };
        slot_diffs(
            index,
            "inputs",
            a.inputs.as_ref(),
            b.inputs.as_ref(),
            &mut out,
        );
        slot_diffs(
            index,
            "outputs",
            a.outputs.as_ref(),
            b.outputs.as_ref(),
            &mut out,
        );
    }
    out
}

/// Diff one payload slot of an aligned pair. A slot present on only one side is a whole-slot
/// `Added`/`Removed`; present on both, the values are compared on the wire representation.
fn slot_diffs(
    step: usize,
    slot: &str,
    a: Option<&Payload>,
    b: Option<&Payload>,
    out: &mut Vec<FieldDiff>,
) {
    match (a, b) {
        (None, None) => {}
        (None, Some(b)) => out.push(FieldDiff {
            step,
            path: slot.to_string(),
            before: None,
            after: Some(payload_value(b)),
            kind: FieldDiffKind::Added,
        }),
        (Some(a), None) => out.push(FieldDiff {
            step,
            path: slot.to_string(),
            before: Some(payload_value(a)),
            after: None,
            kind: FieldDiffKind::Removed,
        }),
        (Some(a), Some(b)) => {
            value_diffs(step, slot, &payload_value(a), &payload_value(b), out);
        }
    }
}

/// Recursive core: equal values emit nothing; two objects diff per key (union, sorted — the
/// deterministic order snapshots rely on); anything else is a leaf `Changed`. Depth is
/// bounded by serde_json's own parse-time recursion limit, so no explicit guard is needed.
fn value_diffs(step: usize, path: &str, before: &Value, after: &Value, out: &mut Vec<FieldDiff>) {
    if before == after {
        return;
    }
    if let (Value::Object(a), Value::Object(b)) = (before, after) {
        let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
        for key in keys {
            let child_path = format!("{path}.{key}");
            match (a.get(key), b.get(key)) {
                (Some(before), Some(after)) => value_diffs(step, &child_path, before, after, out),
                (Some(before), None) => out.push(FieldDiff {
                    step,
                    path: child_path,
                    before: Some(before.clone()),
                    after: None,
                    kind: FieldDiffKind::Removed,
                }),
                (None, Some(after)) => out.push(FieldDiff {
                    step,
                    path: child_path,
                    before: None,
                    after: Some(after.clone()),
                    kind: FieldDiffKind::Added,
                }),
                (None, None) => unreachable!("key came from the union of both maps"),
            }
        }
        return;
    }
    out.push(FieldDiff {
        step,
        path: path.to_string(),
        before: Some(before.clone()),
        after: Some(after.clone()),
        kind: FieldDiffKind::Changed,
    });
}

/// A payload as its wire representation — what the untagged serde derive would emit.
fn payload_value(payload: &Payload) -> Value {
    match payload {
        Payload::Text(text) => Value::String(text.clone()),
        Payload::Object(map) => Value::Object(map.clone()),
        Payload::Other(value) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support;
    use serde_json::json;

    /// One shared name across all fixtures — field diffing reads only the payloads.
    fn step(idx: usize, inputs: Option<Payload>, outputs: Option<Payload>) -> Step {
        test_support::step(idx, "tool")
            .inputs(inputs)
            .outputs(outputs)
            .build()
    }

    fn text(s: &str) -> Option<Payload> {
        Some(Payload::Text(s.to_string()))
    }

    fn object(value: serde_json::Value) -> Option<Payload> {
        match value {
            Value::Object(map) => Some(Payload::Object(map)),
            other => panic!("test fixture must be a JSON object, got {other}"),
        }
    }

    #[test]
    fn identical_payloads_yield_no_diffs() {
        let a = [step(0, text("query"), text("9 results"))];
        let b = [step(0, text("query"), text("9 results"))];
        let alignment = [Move::sync(0, 0, 0.0, 1.0)];

        assert_eq!(field_diffs(&a, &b, &alignment), Vec::new());
    }

    #[test]
    fn changed_text_body_diffs_at_the_slot_path_with_the_alignment_index() {
        // A leading log move so the alignment index (1) differs from the b step index (1)
        // *and* from the a step index (0) — the emitted `step` must be the alignment index.
        let a = [step(0, None, text("census.gov page"))];
        let b = [
            step(0, None, text("stray retry")),
            step(1, None, text("blogspot page")),
        ];
        let alignment = [Move::log(0, 0.6, 0.9), Move::sync(0, 1, 0.8, 0.2)];

        assert_eq!(
            field_diffs(&a, &b, &alignment),
            vec![FieldDiff {
                step: 1,
                path: "outputs".to_string(),
                before: Some(json!("census.gov page")),
                after: Some(json!("blogspot page")),
                kind: FieldDiffKind::Changed,
            }]
        );
    }

    #[test]
    fn object_payloads_diff_per_key_in_sorted_order() {
        let a = [step(
            0,
            None,
            object(json!({"gone": true, "same": 1, "status": "ok"})),
        )];
        let b = [step(
            0,
            None,
            object(json!({"new": 2, "same": 1, "status": "error"})),
        )];
        let alignment = [Move::sync(0, 0, 0.5, 0.5)];

        assert_eq!(
            field_diffs(&a, &b, &alignment),
            vec![
                FieldDiff {
                    step: 0,
                    path: "outputs.gone".to_string(),
                    before: Some(json!(true)),
                    after: None,
                    kind: FieldDiffKind::Removed,
                },
                FieldDiff {
                    step: 0,
                    path: "outputs.new".to_string(),
                    before: None,
                    after: Some(json!(2)),
                    kind: FieldDiffKind::Added,
                },
                FieldDiff {
                    step: 0,
                    path: "outputs.status".to_string(),
                    before: Some(json!("ok")),
                    after: Some(json!("error")),
                    kind: FieldDiffKind::Changed,
                },
            ]
        );
    }

    #[test]
    fn nested_objects_recurse_to_the_changed_leaf() {
        let a = [step(
            0,
            None,
            object(json!({"response": {"body": "x", "meta": {"tokens": 10}}})),
        )];
        let b = [step(
            0,
            None,
            object(json!({"response": {"body": "x", "meta": {"tokens": 12}}})),
        )];
        let alignment = [Move::sync(0, 0, 0.1, 0.9)];

        assert_eq!(
            field_diffs(&a, &b, &alignment),
            vec![FieldDiff {
                step: 0,
                path: "outputs.response.meta.tokens".to_string(),
                before: Some(json!(10)),
                after: Some(json!(12)),
                kind: FieldDiffKind::Changed,
            }]
        );
    }

    #[test]
    fn one_sided_slots_are_added_or_removed_inputs_before_outputs() {
        // Reference has inputs only, observed has outputs only: the whole `inputs` slot was
        // removed and the whole `outputs` slot added — reported in slot order.
        let a = [step(0, object(json!({"q": "census"})), None)];
        let b = [step(0, None, text("blog page"))];
        let alignment = [Move::sync(0, 0, 0.9, 0.1)];

        assert_eq!(
            field_diffs(&a, &b, &alignment),
            vec![
                FieldDiff {
                    step: 0,
                    path: "inputs".to_string(),
                    before: Some(json!({"q": "census"})),
                    after: None,
                    kind: FieldDiffKind::Removed,
                },
                FieldDiff {
                    step: 0,
                    path: "outputs".to_string(),
                    before: None,
                    after: Some(json!("blog page")),
                    kind: FieldDiffKind::Added,
                },
            ]
        );
    }

    #[test]
    fn non_sync_moves_carry_no_field_diffs() {
        let a = [step(0, None, text("only in reference"))];
        let b = [step(0, None, text("only in observed"))];
        let alignment = [Move::model(0, 0.7, 0.8), Move::log(0, 0.7, 0.8)];

        assert_eq!(field_diffs(&a, &b, &alignment), Vec::new());
    }

    #[test]
    fn payload_variants_compare_by_wire_value() {
        // Text and an untagged Other holding the same string serialize identically; the
        // contract is the wire, so this is sameness, not a diff.
        let a = [step(0, None, text("same"))];
        let b = [step(0, None, Some(Payload::Other(json!("same"))))];
        let alignment = [Move::sync(0, 0, 0.0, 1.0)];

        assert_eq!(field_diffs(&a, &b, &alignment), Vec::new());
    }
}
