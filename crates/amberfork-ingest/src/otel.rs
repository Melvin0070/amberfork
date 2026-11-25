//! One entry point for "some OTLP/JSON export": recognize it, pick the attribute vocabulary from
//! the spans themselves, and hand it to the matching adapter.
//!
//! [`crate::openinference`] and [`crate::genai`] share the [`crate::otlp`] envelope and differ only
//! in vocabulary, but a caller holding a file rarely knows which instrumentation wrote it. Nothing
//! in the envelope says, so the choice is read off the span attributes: a span carrying any
//! `gen_ai.*` key votes for the native OTel GenAI conventions, one carrying any `openinference.*`
//! key votes for OpenInference, and the majority wins. A tie, or an export with neither marker,
//! goes to OpenInference — the older and more widely deployed of the two, and the one whose
//! fallbacks (`input.value` / `output.value`) are most likely to find content.
//!
//! The OTel GenAI semantic conventions are still at "Development" status upstream; the `gen_ai.*`
//! side of this tracks a moving target.

use crate::{IngestError, Ingested, genai, openinference};
use serde_json::Value;

/// Which attribute vocabulary an export's spans are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vocabulary {
    /// `openinference.*` / `llm.*` / `tool.*` — [`crate::openinference`].
    OpenInference,
    /// Native OTel GenAI `gen_ai.*` — [`crate::genai`].
    GenAi,
}

/// Whether `text` is an OTLP/JSON span export: a JSON object with a top-level `resourceSpans`.
/// Presence-based, like the cassette sniff — a malformed export still routes here and fails as
/// an export.
#[must_use]
pub fn looks_like_export(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|value| value.get("resourceSpans").is_some())
}

/// Read the vocabulary off the spans: each span votes once, by whether it carries a `gen_ai.*`
/// or an `openinference.*` attribute key. Unparseable input is [`Vocabulary::OpenInference`]; the
/// adapter it routes to reports the parse error.
#[must_use]
pub fn detect_vocabulary(text: &str) -> Vocabulary {
    let Ok(export) = serde_json::from_str::<Value>(text) else {
        return Vocabulary::OpenInference;
    };
    let (mut genai_spans, mut openinference_spans) = (0usize, 0usize);
    for span in spans(&export) {
        let has = |prefix: &str| {
            list(span, "attributes")
                .filter_map(|attr| attr.get("key").and_then(Value::as_str))
                .any(|key| key.starts_with(prefix))
        };
        if has("gen_ai.") {
            genai_spans += 1;
        }
        if has("openinference.") {
            openinference_spans += 1;
        }
    }
    if genai_spans > openinference_spans {
        Vocabulary::GenAi
    } else {
        Vocabulary::OpenInference
    }
}

/// Parse an OTLP/JSON span export into one canonical run per `traceId`, choosing the vocabulary
/// with [`detect_vocabulary`].
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid OTLP/JSON.
pub fn from_otlp_json_str(text: &str) -> Result<Vec<Ingested>, IngestError> {
    match detect_vocabulary(text) {
        Vocabulary::GenAi => genai::from_otlp_json_str(text),
        Vocabulary::OpenInference => openinference::from_otlp_json_str(text),
    }
}

fn list<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn spans(export: &Value) -> impl Iterator<Item = &Value> {
    list(export, "resourceSpans")
        .flat_map(|resource| list(resource, "scopeSpans"))
        .flat_map(|scope| list(scope, "spans"))
}
