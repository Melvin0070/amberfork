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

use crate::{IngestError, Ingested, oivocab};
use amberfork_model::{Run, SchemaVersion, Step, Warning};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

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
        // Timing is preserved as the raw OTLP nanos (display-only, never an alignment signal) — not
        // fabricated into RFC3339, so `t_start`/`t_end` stay `None`. The nanos ride in `attrs` as
        // provenance, stamped after the foreign scan so they are never flagged.
        let mut provenance = Vec::new();
        if let Some(start) = self.start_raw {
            provenance.push(("otel.start_time_unix_nano", Value::from(start)));
        }
        if let Some(end) = self.end_raw {
            provenance.push(("otel.end_time_unix_nano", Value::from(end)));
        }
        oivocab::map_span_to_step(
            idx, parent_idx, &self.name, self.attrs, provenance, None, None, warnings,
        )
    }
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
