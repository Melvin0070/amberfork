//! Normalizing a captured cassette into the canonical [`amberfork_model::Run`].
//!
//! The record path's second half. [`crate::proxy`] captures provider exchanges into a
//! [`Cassette`]; this turns that cassette into the one shape the aligner reads, so a recorded
//! run diffs through exactly the same engine as a passively-ingested trace.
//! `docs/cassette-format.md` states the promise this keeps: *"A cassette becomes a `Run` by
//! normalization; the trace format stays the one seam the aligner reads."*
//!
//! The mapping is deliberately provider-agnostic. Each captured exchange becomes one
//! [`StepKind::Llm`] step whose `inputs` is the full request body and whose `outputs` is the
//! full response body — carried structurally when they are JSON, verbatim text otherwise. That
//! is the record path's whole reason to exist: the passive path's *full content guaranteed*
//! cell is **no** and this one's is **yes**, so a normalized cassette never yields a
//! content-absent step. What this stage does *not* do is re-parse one provider's message schema
//! (pulling the last user turn out of an OpenAI `messages` array, expanding `tool_calls` into
//! [`StepKind::Tool`] steps): the field-level diff already compares the full bodies, so that
//! extraction is a later refinement, not a prerequisite for the payoff.

use crate::cassette::{Body, Cassette, Exchange};
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind};
use serde_json::{Map, Value};

/// Step name for an exchange whose request body does not name a model. A recorded exchange is
/// always an LLM call, so the fallback stays in that vocabulary.
const UNNAMED_LLM: &str = "llm";

/// Parse a cassette from JSON and normalize it into a [`Run`].
///
/// The string entry to the record path's normalizer — what the CLI reaches for once it has
/// sniffed a `cassette_version` and routed a file here, paralleling
/// [`amberfork_ingest::from_json_str`] on the passive path. Kept separate from [`normalize`] so
/// the in-memory mapping stays a pure `&Cassette -> Run` function with no serde in its signature.
///
/// # Errors
/// Returns the [`serde_json::Error`] if the input is not a well-formed cassette (malformed JSON,
/// or valid JSON whose shape does not match the cassette contract). The caller owns turning that
/// into a file-attributed, doc-linked message.
pub fn normalize_str(s: &str) -> Result<Run, serde_json::Error> {
    let cassette: Cassette = serde_json::from_str(s)?;
    Ok(normalize(&cassette))
}

/// Normalize a captured cassette into the canonical [`Run`] the aligner consumes.
///
/// Each [`Exchange`] becomes one step, in capture order, as a linear chain (no `parent_idx`, no
/// explicit edges — a boundary recording observes sequence, not causality). The run carries no
/// `outcome`: a verdict is user-supplied and never derived from a recording, exactly as on the
/// passive path.
#[must_use]
pub fn normalize(cassette: &Cassette) -> Run {
    let steps = cassette
        .exchanges
        .iter()
        .enumerate()
        .map(|(idx, exchange)| step_from_exchange(idx, exchange))
        .collect();
    Run {
        schema_version: SchemaVersion::current(),
        id: cassette.id.clone(),
        task: None,
        outcome: None,
        steps,
        edges: None,
    }
}

/// Map one captured exchange onto one canonical step.
///
/// `idx` is the trajectory position taken from enumeration, not the cassette's own
/// [`Exchange::idx`]: the model requires a contiguous 0-based trajectory, so a cassette with a
/// gap in its capture indices still yields a valid run.
fn step_from_exchange(idx: usize, exchange: &Exchange) -> Step {
    let mut attrs = Map::new();
    // The status is a genuine divergence signal (a 200 vs a 429 is a real fork) and the one
    // piece of the round trip that lives outside the bodies, so it is worth carrying across.
    attrs.insert("status".to_string(), exchange.response.status.into());
    Step {
        idx,
        kind: StepKind::Llm,
        name: model_name(&exchange.request.body),
        inputs: Some(payload_from_body(&exchange.request.body)),
        outputs: Some(payload_from_body(&exchange.response.body)),
        attrs,
        t_start: None,
        t_end: None,
        parent_idx: None,
    }
}

/// The model named in a request body, or [`UNNAMED_LLM`]. The model is an LLM call's most
/// telling structural identity, and the aligner keys on `name`, so surface it when the provider
/// sent it.
fn model_name(body: &Body) -> String {
    let Body::Json(Value::Object(map)) = body else {
        return UNNAMED_LLM.to_string();
    };
    map.get("model")
        .and_then(Value::as_str)
        .unwrap_or(UNNAMED_LLM)
        .to_string()
}

/// Carry a captured body into the payload the diff engine reads: a JSON object gets field-level
/// diffing, a non-object JSON value is preserved verbatim, and a non-JSON body degrades to text
/// — content shape is lost, content never is.
fn payload_from_body(body: &Body) -> Payload {
    match body {
        Body::Json(Value::Object(map)) => Payload::Object(map.clone()),
        Body::Json(value) => Payload::Other(value.clone()),
        Body::Text(text) => Payload::Text(text.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cassette::{CapturedRequest, CapturedResponse, CassetteVersion};
    use serde_json::json;

    fn exchange(idx: usize, request: Body, status: u16, response: Body) -> Exchange {
        Exchange {
            idx,
            request: CapturedRequest {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                headers: Vec::new(),
                body: request,
            },
            response: CapturedResponse {
                status,
                headers: Vec::new(),
                body: response,
            },
        }
    }

    fn cassette(exchanges: Vec<Exchange>) -> Cassette {
        Cassette {
            cassette_version: CassetteVersion::current(),
            id: "test-run".to_string(),
            exchanges,
        }
    }

    #[test]
    fn each_exchange_becomes_one_llm_step_in_capture_order() {
        let cass = cassette(vec![
            exchange(
                0,
                Body::Json(json!({"model": "claude-sonnet-5"})),
                200,
                Body::Json(json!({"n": 1})),
            ),
            exchange(
                1,
                Body::Json(json!({"model": "claude-sonnet-5"})),
                200,
                Body::Json(json!({"n": 2})),
            ),
        ]);
        let run = normalize(&cass);
        assert_eq!(run.id, "test-run");
        assert_eq!(run.steps.len(), 2);
        assert_eq!(run.steps[0].idx, 0);
        assert_eq!(run.steps[1].idx, 1);
        assert!(run.steps.iter().all(|s| s.kind == StepKind::Llm));
        // A boundary recording is a linear chain: sequence is observed, causality is not.
        assert!(run.edges.is_none());
        assert!(run.steps.iter().all(|s| s.parent_idx.is_none()));
    }

    #[test]
    fn every_recorded_step_carries_both_inputs_and_outputs() {
        // The record path's reason to exist. A passive trace can be metadata-only (a
        // content-absent step); a cassette guarantees full content, so no normalized step is
        // ever missing its inputs or its outputs. This is the invariant the +76% number rests on.
        let cass = cassette(vec![exchange(
            0,
            Body::Json(
                json!({"messages": [{"role": "user", "content": "Handle refund for 8841"}]}),
            ),
            200,
            Body::Json(json!({"choices": [{"message": {"content": "Looking up the order."}}]})),
        )]);
        let run = normalize(&cass);
        assert!(
            run.steps
                .iter()
                .all(|s| s.inputs.is_some() && s.outputs.is_some())
        );
    }

    #[test]
    fn json_object_body_becomes_a_field_diffable_object_payload() {
        let cass = cassette(vec![exchange(
            0,
            Body::Json(json!({"model": "claude-sonnet-5", "temperature": 0})),
            200,
            Body::Json(json!({"choices": []})),
        )]);
        let run = normalize(&cass);
        let inputs = run.steps[0].inputs.as_ref().expect("inputs present");
        let Payload::Object(map) = inputs else {
            panic!("a JSON object request must map to Payload::Object, got {inputs:?}");
        };
        assert_eq!(map["temperature"], json!(0));
    }

    #[test]
    fn non_json_response_degrades_to_text_not_silence() {
        // A provider's HTML 502 is exactly the run worth having recorded. The fidelity loss must
        // show up as shape (text, not object), never as a dropped output.
        let cass = cassette(vec![exchange(
            0,
            Body::Json(json!({"model": "claude-sonnet-5"})),
            502,
            Body::Text("<html>502 Bad Gateway</html>".to_string()),
        )]);
        let run = normalize(&cass);
        let outputs = run.steps[0].outputs.as_ref().expect("output present");
        assert_eq!(
            *outputs,
            Payload::Text("<html>502 Bad Gateway</html>".to_string())
        );
    }

    #[test]
    fn step_name_is_the_model_when_the_request_names_one() {
        // The model is an LLM call's most telling structural identity, and the aligner keys on
        // `name`: a run that swaps models has genuinely diverged.
        let cass = cassette(vec![exchange(
            0,
            Body::Json(json!({"model": "claude-opus-4-8"})),
            200,
            Body::Json(json!({})),
        )]);
        assert_eq!(normalize(&cass).steps[0].name, "claude-opus-4-8");
    }

    #[test]
    fn step_name_falls_back_when_no_model_is_named() {
        let cass = cassette(vec![exchange(
            0,
            Body::Text("not json".to_string()),
            200,
            Body::Json(json!({})),
        )]);
        assert_eq!(normalize(&cass).steps[0].name, UNNAMED_LLM);
    }

    #[test]
    fn normalize_str_parses_then_normalizes_a_well_formed_cassette() {
        let json = r#"{
            "cassette_version": "0.1",
            "id": "from-str",
            "exchanges": [
                {
                    "idx": 0,
                    "request": { "method": "POST", "path": "/v1/messages",
                        "body": { "model": "claude-sonnet-5" } },
                    "response": { "status": 200, "body": { "ok": true } }
                }
            ]
        }"#;
        let run = normalize_str(json).expect("well-formed cassette normalizes");
        assert_eq!(run.id, "from-str");
        assert_eq!(run.steps.len(), 1);
        assert_eq!(run.steps[0].name, "claude-sonnet-5");
    }

    #[test]
    fn normalize_str_errors_on_a_broken_cassette_shape() {
        // Valid JSON, but `exchanges` is the wrong type — the string entry surfaces the serde
        // error rather than silently yielding a run, so the caller can attribute it to a file.
        let json = r#"{"cassette_version": "0.1", "id": "broken", "exchanges": "not-an-array"}"#;
        assert!(normalize_str(json).is_err());
    }

    #[test]
    fn response_status_is_kept_as_a_step_attribute() {
        let cass = cassette(vec![exchange(
            0,
            Body::Json(json!({"model": "claude-sonnet-5"})),
            429,
            Body::Json(json!({"error": "rate_limited"})),
        )]);
        let run = normalize(&cass);
        assert_eq!(run.steps[0].attrs["status"], json!(429));
    }
}
