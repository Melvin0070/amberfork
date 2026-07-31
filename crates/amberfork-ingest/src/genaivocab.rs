//! The native OTel GenAI **vocabulary** layer: mapping a span's `gen_ai.*` semantic-convention
//! attributes onto a canonical [`Step`], independent of the wire envelope ([`crate::genai`]'s
//! [`crate::otlp`] reader) the span arrived in — the `gen_ai.*` counterpart to [`crate::oivocab`].
//!
//! Current spec (`open-telemetry/semantic-conventions-genai`, status Development): span kind
//! rides in `gen_ai.operation.name` (`chat`/`text_completion`/`generate_content`/`execute_tool`/
//! `create_agent`/`invoke_agent`/…), and content rides in `gen_ai.input.messages` /
//! `gen_ai.output.messages` for model-inference spans or `gen_ai.tool.call.arguments` /
//! `gen_ai.tool.call.result` for tool spans — the carrier is kind-conditional, unlike
//! OpenInference's uniform `input.value`/`output.value`. The older per-role event convention
//! (`gen_ai.user.message`/`gen_ai.assistant.message`/…) is superseded upstream and not targeted.

use amberfork_model::{Payload, Step, StepKind, Warning, WarningCode};
use serde_json::{Map, Value};

/// Attribute keys lifted onto canonical [`Step`] fields — never left duplicated in `attrs`.
const CONSUMED_KEYS: &[&str] = &[
    "gen_ai.operation.name",
    "gen_ai.tool.name",
    "gen_ai.agent.name",
    "gen_ai.input.messages",
    "gen_ai.output.messages",
    "gen_ai.tool.call.arguments",
    "gen_ai.tool.call.result",
];

/// Attribute-key prefixes recognized as native OTel GenAI vocabulary. An attribute under one of
/// these is a deliberate, understood part of the mapping (it rides in `attrs`); anything else is
/// genuinely foreign and earns an unmapped-attributes advisory. `error.*`/`server.*` are core OTel
/// semantic conventions the GenAI spec composes with (span errors, the inference endpoint) rather
/// than GenAI-specific, but are common enough on real spans to name here rather than flag as
/// foreign on every export.
const KNOWN_PREFIXES: &[&str] = &["gen_ai.", "error.", "server."];

/// Assemble one canonical [`Step`] from a span's decoded `gen_ai.*` attributes, whatever envelope
/// it came in. Mirrors [`crate::oivocab::map_span_to_step`]'s contract exactly (same
/// [`crate::otlp::SpanToStep`] signature, so [`crate::otlp`] can drive either vocabulary): `idx`/
/// `parent_idx`/`span_name` are already resolved by the caller; `provenance` (raw timing) is
/// stamped into `attrs` after the foreign scan so it is never itself flagged; `t_start`/`t_end`
/// carry RFC3339 timing when the envelope supplies it natively.
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
    let kind = map_kind(attrs.get("gen_ai.operation.name").and_then(|v| v.as_str()));

    // A TOOL span names itself by `gen_ai.tool.name`, an AGENT span by `gen_ai.agent.name`;
    // everything else keeps its span name.
    let name_key = match kind {
        StepKind::Tool => Some("gen_ai.tool.name"),
        StepKind::Agent => Some("gen_ai.agent.name"),
        StepKind::Llm | StepKind::Other => None,
    };
    let name = name_key
        .and_then(|key| attrs.get(key))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map_or_else(|| span_name.to_string(), str::to_string);

    // The content carrier is kind-conditional: a tool span reports its call, everything else
    // reports the model-inference messages.
    let (inputs, outputs) = if kind == StepKind::Tool {
        (
            decode_content(attrs.get("gen_ai.tool.call.arguments")),
            decode_content(attrs.get("gen_ai.tool.call.result")),
        )
    } else {
        (
            decode_content(attrs.get("gen_ai.input.messages")),
            decode_content(attrs.get("gen_ai.output.messages")),
        )
    };

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

/// Map a `gen_ai.operation.name` onto the canonical four. Only the spec's model-inference and
/// tool/agent-lifecycle operations land on a specific kind; everything else (`embeddings`, and
/// any operation the spec adds later) folds to [`StepKind::Other`] — the same "land on the
/// canonical set, don't invent finer structure" rule [`crate::oivocab::map_kind`] applies to
/// OpenInference's non-canonical span kinds.
pub(crate) fn map_kind(operation: Option<&str>) -> StepKind {
    match operation {
        Some("chat" | "text_completion" | "generate_content") => StepKind::Llm,
        Some("execute_tool") => StepKind::Tool,
        Some("create_agent" | "invoke_agent") => StepKind::Agent,
        _ => StepKind::Other,
    }
}

/// Turn a `gen_ai.input.messages`/`output.messages`/`tool.call.arguments`/`tool.call.result`
/// value into a [`Payload`]. Per spec these ride as structured JSON (object or array) or,
/// on an SDK without structured-attribute support, a pre-serialized JSON string. An array (the
/// `messages` shape) is wrapped as `{"messages": [...]}` so it lands on [`Payload::Object`] and
/// stays field-diffable, matching how every other structured carrier in this crate is handled —
/// the bare-array wire shape would otherwise fall to [`Payload::Other`] and diff as an opaque
/// blob. An absent or empty value is no content at all.
pub(crate) fn decode_content(value: Option<&Value>) -> Option<Payload> {
    match value? {
        Value::String(s) if s.is_empty() => None,
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(parsed) => Some(payload_from_json(parsed)),
            Err(_) => Some(Payload::Text(s.clone())),
        },
        Value::Array(arr) if arr.is_empty() => None,
        Value::Object(map) if map.is_empty() => None,
        Value::Null => None,
        other => Some(payload_from_json(other.clone())),
    }
}

/// Classify a parsed JSON value into the payload variant the diff engine reads: a bare string
/// gets text diffing, an object or a `messages` array gets field-level diffing, anything else is
/// preserved verbatim.
fn payload_from_json(value: Value) -> Payload {
    match value {
        Value::String(s) => Payload::Text(s),
        Value::Object(map) => Payload::Object(map),
        Value::Array(items) => {
            let mut map = Map::new();
            map.insert("messages".to_string(), Value::Array(items));
            Payload::Object(map)
        }
        other => Payload::Other(other),
    }
}

/// Whether an attribute key belongs to the recognized native OTel GenAI vocabulary.
fn is_known(key: &str) -> bool {
    KNOWN_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
}
