//! Adapter from OpenInference OTLP/JSON span exports into canonical [`amberfork_model::Run`]s.
//!
//! OpenInference is the most-deployed OpenTelemetry GenAI convention (Arize Phoenix, and the
//! LangSmith / Langfuse instrumentations built on it): a run is an OTLP span tree whose semantic
//! meaning rides in `openinference.*` / `llm.*` / `tool.*` attributes. This is the real on-ramp
//! for the "point it at traces I already have" persona, and — like [`crate::whowhen`] and
//! [`crate::tape`] — a *source adapter* kept apart from the canonical loader
//! ([`crate::from_json_str`]): an OTLP export's shape is nothing like the canonical trace.
//!
//! Sibling convention `gen_ai.*` (native OTel GenAI, [`crate::genai`]) shares the exact same
//! [`crate::otlp`] envelope but a different attribute vocabulary ([`crate::genaivocab`]).
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

use crate::{IngestError, Ingested, oivocab, otlp};
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
    otlp::from_otlp_json_str(s, oivocab::map_span_to_step)
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
