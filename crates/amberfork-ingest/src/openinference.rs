//! Adapter from OpenInference OTLP/JSON span exports into canonical [`amberfork_model::Run`]s.
//!
//! OpenInference is the most-deployed OpenTelemetry GenAI convention (Arize Phoenix, and the
//! LangSmith / Langfuse instrumentations built on it): a run is an OTLP span tree whose semantic
//! meaning rides in `openinference.*` / `llm.*` / `tool.*` attributes. This is the real on-ramp
//! for the "point it at traces I already have" persona, and — like [`crate::whowhen`] and
//! [`crate::tape`] — a *source adapter* kept apart from the canonical loader
//! ([`crate::from_json_str`]): an OTLP export's shape is nothing like the canonical trace.
//!
//! Sibling convention `gen_ai.*` (native OTel GenAI) shares this exact OTLP envelope but a
//! different attribute vocabulary; it is a deliberate follow-up slice, not folded in here.
//!
//! Four boundaries the adapter makes explicit:
//! - **One run per `traceId`.** A single OTLP export can carry several independent traces; each
//!   becomes its own [`Run`], and the runs come back ordered by their earliest span so the output
//!   is deterministic.
//! - **Order is start time, not wire order.** Exporters do not promise spans in execution order,
//!   so steps are re-indexed by `startTimeUnixNano` (stable on the wire order for ties) before
//!   `parent_idx` is wired from `parentSpanId`.
//! - **`outcome` is never inferred from span status.** An OTLP `STATUS_CODE_ERROR` is not a run
//!   verdict (architecture rule, `docs/trace-format.md`): a run's outcome is a user assertion, so
//!   this adapter always leaves it `None`.
//! - **Known vocabulary is mapped; foreign attributes are flagged.** Recognized OpenInference/OTel
//!   attributes map onto canonical fields or ride in `attrs` silently; an attribute outside that
//!   vocabulary is preserved to `attrs` *and* raises an [`WarningCode::UnmappedAttributes`]
//!   advisory, mirroring the canonical loader's forgiveness.
//!
//! Slice boundary: content is read from the `input.value` / `output.value` carriers (which
//! OpenInference sets on every content-bearing span); reconstructing structured messages from the
//! flattened `llm.input_messages.*` attributes is deferred — those attributes ride in `attrs`
//! meanwhile, losing nothing. Timing is preserved as the raw OTLP nanos in `attrs`
//! (`otel.*_time_unix_nano`) rather than fabricated into RFC3339, since timing is display-only and
//! never an alignment signal.

use crate::{IngestError, Ingested};
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind, Warning, WarningCode};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

/// Attribute keys lifted onto canonical [`Step`] fields — never left duplicated in `attrs`.
const CONSUMED_KEYS: &[&str] = &[
    "openinference.span.kind",
    "tool.name",
    "input.value",
    "input.mime_type",
    "output.value",
    "output.mime_type",
];

/// Attribute-key prefixes this adapter recognizes as OpenInference/OTel GenAI vocabulary. An
/// attribute under one of these is a deliberate, understood part of the mapping (it rides in
/// `attrs`); anything else is genuinely foreign and earns an unmapped-attributes advisory.
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

/// Parse an OpenInference OTLP/JSON span export into one canonical run per `traceId`.
///
/// Returns the runs ordered by their earliest span's start time, each paired with any non-fatal
/// diagnostics raised while normalizing it. An export with no spans yields an empty vector.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid OTLP/JSON. Everything past a
/// successful parse is forgiving: a missing span kind, absent content, or a foreign attribute
/// yields a canonical fallback plus a warning, never an error.
pub fn from_otlp_json_str(s: &str) -> Result<Vec<Ingested>, IngestError> {
    let export: RawExport =
        serde_json::from_str(s).map_err(|source| IngestError::Parse { path: None, source })?;
    Ok(normalize(export))
}

/// Load an OpenInference OTLP/JSON span export from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] (with the path
/// attached) if its contents are not valid OTLP/JSON.
pub fn load_file(path: impl AsRef<Path>) -> Result<Vec<Ingested>, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_otlp_json_str(&text).map_err(|err| err.with_path(path))
}

/// A span with its attributes decoded from the OTLP `AnyValue` wire shape into plain JSON, plus the
/// wire position it arrived in (the stable tiebreak for equal start times).
struct DecodedSpan {
    span_id: String,
    parent_span_id: String,
    name: String,
    start: Option<u64>,
    start_raw: Option<String>,
    end_raw: Option<String>,
    attrs: Map<String, Value>,
    wire_pos: usize,
}

/// Group every span by `traceId`, order the groups by earliest start, and normalize each into a
/// [`Run`]. Pure and infallible — the parse already succeeded, so every remaining decision degrades
/// to a canonical fallback plus a warning.
fn normalize(export: RawExport) -> Vec<Ingested> {
    // Flatten the resource/scope nesting into per-trace span lists, decoding attributes as we go
    // and stamping each span's wire position (a monotonic counter) for a deterministic tie-break.
    let mut traces: Vec<(String, Vec<DecodedSpan>)> = Vec::new();
    let mut wire_pos = 0usize;
    for resource in export.resource_spans {
        for scope in resource.scope_spans {
            for span in scope.spans {
                let decoded = span.decode(wire_pos);
                wire_pos += 1;
                let trace_id = span_trace_id(&span);
                match traces.iter_mut().find(|(id, _)| id == &trace_id) {
                    Some((_, spans)) => spans.push(decoded),
                    None => traces.push((trace_id, vec![decoded])),
                }
            }
        }
    }

    let mut runs: Vec<Ingested> = traces
        .into_iter()
        .map(|(trace_id, spans)| build_run(trace_id, spans))
        .collect();
    // Deterministic output: earliest-starting trace first, traceId as the tiebreak.
    runs.sort_by(|a, b| {
        earliest_start(&a.run)
            .cmp(&earliest_start(&b.run))
            .then_with(|| a.run.id.cmp(&b.run.id))
    });
    runs
}

/// The smallest raw start-nanos preserved on any of a run's steps, for ordering runs against each
/// other. A run with no timed steps sorts first (`None` < `Some`).
fn earliest_start(run: &Run) -> Option<u64> {
    run.steps
        .iter()
        .filter_map(|s| s.attrs.get("otel.start_time_unix_nano"))
        .filter_map(|v| v.as_str())
        .filter_map(|s| s.parse::<u64>().ok())
        .min()
}

/// Turn one trace's spans into a canonical run: order by start time, re-index, wire parents.
fn build_run(trace_id: String, mut spans: Vec<DecodedSpan>) -> Ingested {
    // Order by start time; equal (or absent) starts keep wire order via the stable tiebreak.
    spans.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.wire_pos.cmp(&b.wire_pos))
    });

    // spanId -> assigned idx, so `parentSpanId` can resolve to `parent_idx` after re-indexing.
    let idx_of: Map<String, Value> = spans
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.span_id.is_empty())
        .map(|(idx, s)| (s.span_id.clone(), Value::from(idx)))
        .collect();

    let mut warnings = Vec::new();
    let steps = spans
        .into_iter()
        .enumerate()
        .map(|(idx, span)| {
            let parent_idx = idx_of
                .get(&span.parent_span_id)
                .and_then(serde_json::Value::as_u64)
                .and_then(|u| usize::try_from(u).ok());
            span.into_step(idx, parent_idx, &mut warnings)
        })
        .collect();

    let run = Run {
        schema_version: SchemaVersion::current(),
        id: trace_id,
        task: None,
        outcome: None,
        steps,
        edges: None,
    };
    Ingested { run, warnings }
}

impl DecodedSpan {
    fn into_step(self, idx: usize, parent_idx: Option<usize>, warnings: &mut Vec<Warning>) -> Step {
        let kind = map_kind(self.attrs.get("openinference.span.kind"));
        let name = self
            .attrs
            .get("tool.name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map_or_else(|| self.name.clone(), str::to_string);

        let inputs = decode_content(
            self.attrs.get("input.value"),
            self.attrs.get("input.mime_type"),
        );
        let outputs = decode_content(
            self.attrs.get("output.value"),
            self.attrs.get("output.mime_type"),
        );

        // Everything not lifted onto a canonical field rides in `attrs`; a foreign key (outside the
        // known vocabulary) is preserved too but flagged.
        let mut attrs = Map::new();
        let mut foreign = Vec::new();
        for (key, value) in self.attrs {
            if CONSUMED_KEYS.contains(&key.as_str()) {
                continue;
            }
            if !is_known(&key) {
                foreign.push(key.clone());
            }
            attrs.insert(key, value);
        }
        // Timing is preserved as the raw OTLP nanos (display-only, never an alignment signal) — not
        // fabricated into RFC3339. Inserted after the foreign scan so it can never be flagged.
        if let Some(start) = self.start_raw {
            attrs.insert("otel.start_time_unix_nano".to_string(), Value::from(start));
        }
        if let Some(end) = self.end_raw {
            attrs.insert("otel.end_time_unix_nano".to_string(), Value::from(end));
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
            attrs,
            t_start: None,
            t_end: None,
            parent_idx,
        }
    }
}

/// Map an OpenInference `openinference.span.kind` onto the canonical four. The kinds outside the
/// canonical vocabulary (CHAIN, RETRIEVER, EMBEDDING, RERANKER, GUARDRAIL, EVALUATOR) and a missing
/// kind all fold to [`StepKind::Other`] — a normalizer's job is to land on the canonical set, not
/// to invent finer structure than the model carries.
fn map_kind(value: Option<&Value>) -> StepKind {
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
fn decode_content(value: Option<&Value>, mime: Option<&Value>) -> Option<Payload> {
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

/// The `traceId` of a raw span, defaulting to a single synthetic bucket when a span omits it (so a
/// trace-id-less export still yields one run rather than being silently dropped).
fn span_trace_id(span: &RawSpan) -> String {
    if span.trace_id.is_empty() {
        "trace-unknown".to_string()
    } else {
        span.trace_id.clone()
    }
}

// --- OTLP/JSON wire types -------------------------------------------------------------------

/// The OTLP/JSON export envelope. Every level defaults to empty so a partial or unfamiliar export
/// degrades to fewer spans rather than a parse failure.
#[derive(Deserialize)]
struct RawExport {
    #[serde(default, rename = "resourceSpans")]
    resource_spans: Vec<RawResourceSpans>,
}

#[derive(Deserialize)]
struct RawResourceSpans {
    #[serde(default, rename = "scopeSpans")]
    scope_spans: Vec<RawScopeSpans>,
}

#[derive(Deserialize)]
struct RawScopeSpans {
    #[serde(default)]
    spans: Vec<RawSpan>,
}

/// One OTLP span. Only the fields the adapter reads are named; `status`, `kind`, `links`, and the
/// rest are ignored (serde's default), never a parse failure.
#[derive(Deserialize)]
struct RawSpan {
    #[serde(default, rename = "traceId")]
    trace_id: String,
    #[serde(default, rename = "spanId")]
    span_id: String,
    #[serde(default, rename = "parentSpanId")]
    parent_span_id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "startTimeUnixNano")]
    start_time_unix_nano: Option<String>,
    #[serde(default, rename = "endTimeUnixNano")]
    end_time_unix_nano: Option<String>,
    #[serde(default)]
    attributes: Vec<RawAttr>,
}

impl RawSpan {
    fn decode(&self, wire_pos: usize) -> DecodedSpan {
        let mut attrs = Map::new();
        for attr in &self.attributes {
            attrs.insert(attr.key.clone(), decode_any_value(&attr.value));
        }
        DecodedSpan {
            span_id: self.span_id.clone(),
            parent_span_id: self.parent_span_id.clone(),
            name: self.name.clone(),
            start: self
                .start_time_unix_nano
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok()),
            start_raw: self.start_time_unix_nano.clone(),
            end_raw: self.end_time_unix_nano.clone(),
            attrs,
            wire_pos,
        }
    }
}

/// One OTLP key/value attribute. The value is an `AnyValue`, decoded on conversion.
#[derive(Deserialize)]
struct RawAttr {
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: Value,
}

/// Decode an OTLP `AnyValue` object (`{"stringValue": …}`, `{"intValue": "42"}`, `{"boolValue":
/// true}`, `{"doubleValue": …}`, `{"arrayValue": {"values": […]}}`, `{"kvlistValue": {"values":
/// […]}}`) into plain JSON. `intValue` is an int64 rendered as a string in proto3 JSON — decoded to
/// a JSON number when it parses. An unrecognized shape is returned verbatim so nothing is lost.
fn decode_any_value(value: &Value) -> Value {
    let Value::Object(obj) = value else {
        return value.clone();
    };
    if let Some(s) = obj.get("stringValue") {
        return s.clone();
    }
    if let Some(b) = obj.get("boolValue") {
        return b.clone();
    }
    if let Some(d) = obj.get("doubleValue") {
        return d.clone();
    }
    if let Some(int_value) = obj.get("intValue") {
        return match int_value {
            Value::String(s) => s
                .parse::<i64>()
                .map_or_else(|_| int_value.clone(), Value::from),
            other => other.clone(),
        };
    }
    if let Some(array) = obj.get("arrayValue") {
        let values = array
            .get("values")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().map(decode_any_value).collect())
            .unwrap_or_default();
        return Value::Array(values);
    }
    if let Some(kvlist) = obj.get("kvlistValue") {
        let mut map = Map::new();
        if let Some(items) = kvlist.get("values").and_then(|v| v.as_array()) {
            for item in items {
                if let (Some(key), Some(val)) =
                    (item.get("key").and_then(|k| k.as_str()), item.get("value"))
                {
                    map.insert(key.to_string(), decode_any_value(val));
                }
            }
        }
        return Value::Object(map);
    }
    value.clone()
}
