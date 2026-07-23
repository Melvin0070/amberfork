//! The shared OpenInference **vocabulary** layer: mapping a span's semantic attributes onto a
//! canonical [`Step`], independent of the wire envelope the span arrived in.
//!
//! OpenInference (`openinference.*` / `llm.*` / `tool.*` attributes) is emitted through more than
//! one envelope: the OTLP/JSON span export [`crate::openinference`] reads, and the nested
//! Patronus-SDK trace tree [`crate::trail`] reads. Both carry the *same* attribute vocabulary, so
//! the kind/name/content mapping and the known-vs-foreign attribute split live here once and each
//! adapter supplies only its envelope-specific parts (how spans are found and ordered, and what
//! provenance to stamp). This is the factor-out the second consumer earns — notebook 042 flagged
//! the OTLP envelope reader as the first shared substrate; the vocabulary is the second.

use amberfork_model::{Payload, Step, StepKind, Warning, WarningCode};
use serde_json::{Map, Value};

/// Attribute keys lifted onto canonical [`Step`] fields — never left duplicated in `attrs`.
const CONSUMED_KEYS: &[&str] = &[
    "openinference.span.kind",
    "tool.name",
    "input.value",
    "input.mime_type",
    "output.value",
    "output.mime_type",
];

/// Attribute-key prefixes recognized as OpenInference/OTel GenAI vocabulary. An attribute under
/// one of these is a deliberate, understood part of the mapping (it rides in `attrs`); anything
/// else is genuinely foreign and earns an unmapped-attributes advisory.
const KNOWN_PREFIXES: &[&str] = &[
    "openinference.",
    "llm.",
    "tool.",
    "input.",
    "output.",
    "embedding.",
    "reranker.",
    "retrieval.",
    "document.",
    "message.",
    "prompt.",
    "metadata",
    "tag.",
    "session.",
    "user.",
    "graph.",
    "gen_ai.",
    "exception.",
];

/// Assemble one canonical [`Step`] from a span's decoded attributes, whatever envelope it came in.
///
/// The caller has already located the span (its position `idx`, its `parent_idx` within the run,
/// its `span_name`) and decoded its attributes to plain JSON (`attrs`); this maps that onto the
/// canonical fields. `provenance` holds envelope-specific keys (raw timing, source span id) stamped
/// into `attrs` *after* the foreign scan, so they can never be flagged as unmapped. `t_start` /
/// `t_end` carry RFC3339 timing when the envelope supplies it natively (else `None`, with raw
/// timing preserved via `provenance`). Any [`WarningCode::UnmappedAttributes`] /
/// [`WarningCode::ContentAbsent`] advisory is pushed onto `warnings`.
#[expect(
    clippy::too_many_arguments,
    reason = "the envelope-independent inputs of a span-to-step mapping; grouping them into a \
              struct would only move the argument list, not shorten it"
)]
pub(crate) fn map_span_to_step(
    idx: usize,
    parent_idx: Option<usize>,
    span_name: &str,
    attrs: Map<String, Value>,
    provenance: Vec<(&'static str, Value)>,
    t_start: Option<String>,
    t_end: Option<String>,
    warnings: &mut Vec<Warning>,
) -> Step {
    let kind = map_kind(attrs.get("openinference.span.kind"));
    // A TOOL span names itself by `tool.name`; everything else keeps its span name.
    let name = attrs
        .get("tool.name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map_or_else(|| span_name.to_string(), str::to_string);

    let inputs = decode_content(attrs.get("input.value"), attrs.get("input.mime_type"));
    let outputs = decode_content(attrs.get("output.value"), attrs.get("output.mime_type"));

    // Everything not lifted onto a canonical field rides in `attrs`; a foreign key (outside the
    // known vocabulary) is preserved too but flagged.
    let mut out_attrs = Map::new();
    let mut foreign = Vec::new();
    for (key, value) in attrs {
        if CONSUMED_KEYS.contains(&key.as_str()) {
            continue;
        }
        if !is_known(&key) {
            foreign.push(key.clone());
        }
        out_attrs.insert(key, value);
    }
    // Provenance is stamped after the foreign scan so it is never itself flagged.
    for (key, value) in provenance {
        out_attrs.insert(key.to_string(), value);
    }

    if !foreign.is_empty() {
        foreign.sort_unstable();
        warnings.push(Warning {
            code: WarningCode::UnmappedAttributes,
            msg: format!(
                "step {idx} ({name}): foreign attributes preserved to attrs: {}",
                foreign.join(", ")
            ),
        });
    }
    if inputs.is_none() && outputs.is_none() {
        warnings.push(Warning {
            code: WarningCode::ContentAbsent,
            msg: format!("step {idx} ({name}): no input or output content captured"),
        });
    }

    Step {
        idx,
        kind,
        name,
        inputs,
        outputs,
        attrs: out_attrs,
        t_start,
        t_end,
        parent_idx,
    }
}

/// Map an OpenInference `openinference.span.kind` onto the canonical four. The kinds outside the
/// canonical vocabulary (CHAIN, RETRIEVER, EMBEDDING, RERANKER, GUARDRAIL, EVALUATOR) and a missing
/// kind all fold to [`StepKind::Other`] — a normalizer's job is to land on the canonical set, not
/// to invent finer structure than the model carries.
pub(crate) fn map_kind(value: Option<&Value>) -> StepKind {
    match value
        .and_then(|v| v.as_str())
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("LLM") => StepKind::Llm,
        Some("TOOL") => StepKind::Tool,
        Some("AGENT") => StepKind::Agent,
        _ => StepKind::Other,
    }
}

/// Turn an `input.value`/`output.value` (plus its `*.mime_type`) into a [`Payload`]. A JSON
/// mime-type whose value parses becomes a field-diffable [`Payload::Object`] (or [`Payload::Other`]
/// for a non-object JSON); everything else is text. An absent or empty value is no content at all.
pub(crate) fn decode_content(value: Option<&Value>, mime: Option<&Value>) -> Option<Payload> {
    let text = match value? {
        Value::String(s) => s.clone(),
        // A non-string carrier (already-structured value) is taken verbatim.
        other => return Some(payload_from_json(other.clone())),
    };
    if text.is_empty() {
        return None;
    }
    let is_json = mime
        .and_then(|m| m.as_str())
        .is_some_and(|m| m.contains("json"));
    if is_json {
        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
            return Some(payload_from_json(parsed));
        }
    }
    Some(Payload::Text(text))
}

/// Classify a parsed JSON value into the payload variant the diff engine reads: an object gets
/// field-level diffing, a bare string gets text diffing, anything else is preserved verbatim.
fn payload_from_json(value: Value) -> Payload {
    match value {
        Value::String(s) => Payload::Text(s),
        Value::Object(map) => Payload::Object(map),
        other => Payload::Other(other),
    }
}

/// Whether an attribute key belongs to the recognized OpenInference/OTel vocabulary.
fn is_known(key: &str) -> bool {
    KNOWN_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
}
