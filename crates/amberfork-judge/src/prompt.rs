//! Renders an [`ExplainContext`](crate::ExplainContext) window into the prompt a local model
//! narrates. Pure and offline-testable — the network call lives in `ollama.rs`.

use crate::context::{Side, StepSnapshot};
use amberfork_model::Payload;

/// A local model's context window is small; a raw tool payload can be arbitrarily large. Cap
/// each preview so one oversized step can't blow the prompt up (or push the actual divergence
/// content out of the window entirely).
const MAX_PAYLOAD_PREVIEW_CHARS: usize = 400;

fn preview(payload: &Payload) -> String {
    let raw = match payload {
        Payload::Text(text) => text.clone(),
        Payload::Object(map) => serde_json::to_string(map).unwrap_or_default(),
        Payload::Other(value) => value.to_string(),
    };
    if raw.chars().count() > MAX_PAYLOAD_PREVIEW_CHARS {
        let truncated: String = raw.chars().take(MAX_PAYLOAD_PREVIEW_CHARS).collect();
        format!("{truncated}…")
    } else {
        raw
    }
}

fn step_line(step: &StepSnapshot) -> String {
    let side = match step.side {
        Side::A => "reference",
        Side::B => "observed",
    };
    let inputs = step
        .inputs
        .as_ref()
        .map_or_else(|| "(none)".to_string(), preview);
    let outputs = step
        .outputs
        .as_ref()
        .map_or_else(|| "(none)".to_string(), preview);
    format!(
        "[{side} · {:?} · {}] inputs: {inputs} | outputs: {outputs}",
        step.kind, step.name
    )
}

/// Build the prompt for a forked result's window. Only ever called on a real fork —
/// [`crate::OllamaJudge`] short-circuits the converged case before reaching here — so the model
/// is never asked to confirm or deny that a fork exists, only to describe the one it's shown.
/// Deliberately never asks the model to name a step index: [`Explanation::fork_index`]
/// (`crate::judge::Explanation`) is set by the caller from data it already has, not parsed from
/// the model's answer, which is why the grounding guard can never be defeated by a plausible
/// wrong number.
pub fn build(window: &[StepSnapshot]) -> String {
    let lines: String = window
        .iter()
        .map(|s| format!("- {}\n", step_line(s)))
        .collect();
    format!(
        "You are narrating why two AI agent runs diverged, using only the step content below. \
         Never mention step numbers, indices, or claim a specific location in the trace — just \
         describe what functionally changed. Answer in 2-4 plain sentences.\n\n{lines}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::StepKind;

    fn snapshot(side: Side, idx: usize, outputs: Option<Payload>) -> StepSnapshot {
        StepSnapshot {
            side,
            idx,
            kind: StepKind::Tool,
            name: "search".to_string(),
            inputs: None,
            outputs,
        }
    }

    #[test]
    fn the_prompt_never_mentions_a_step_index() {
        let window = vec![snapshot(Side::A, 5, Some(Payload::Text("ok".into())))];

        let prompt = build(&window);

        assert!(!prompt.contains('5'));
    }

    #[test]
    fn a_long_payload_is_truncated() {
        let long = "x".repeat(MAX_PAYLOAD_PREVIEW_CHARS * 2);
        let window = vec![snapshot(Side::B, 0, Some(Payload::Text(long)))];

        let prompt = build(&window);

        assert!(prompt.contains('…'));
        assert!(prompt.len() < MAX_PAYLOAD_PREVIEW_CHARS * 2);
    }

    #[test]
    fn both_sides_are_labeled_reference_and_observed() {
        let window = vec![
            snapshot(Side::A, 0, Some(Payload::Text("ref".into()))),
            snapshot(Side::B, 0, Some(Payload::Text("obs".into()))),
        ];

        let prompt = build(&window);

        assert!(prompt.contains("reference"));
        assert!(prompt.contains("observed"));
    }
}
