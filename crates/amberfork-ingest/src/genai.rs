//! Adapter from native OTel GenAI (`gen_ai.*`) OTLP/JSON span exports into canonical
//! [`amberfork_model::Run`]s.
//!
//! Sibling adapter to [`crate::openinference`]: the exact same [`crate::otlp`] envelope
//! (resource/scope flattening, `AnyValue` decoding, start-time ordering, `parentSpanId` →
//! `parent_idx`, `outcome` never inferred from span status), a different attribute vocabulary
//! ([`crate::genaivocab`], the native OTel GenAI semantic conventions rather than OpenInference's).
//! Deliberately not folded into `openinference` — notebook 042 drew this slice boundary before
//! either adapter was built: "OpenInference now, native `gen_ai.*` next — same envelope, additive
//! vocabulary."
//!
//! Slice boundary: targets the current `gen_ai.input.messages`/`gen_ai.output.messages` /
//! `gen_ai.tool.call.arguments`/`gen_ai.tool.call.result` span-attribute convention. The
//! superseded per-role event convention (`gen_ai.user.message`/`gen_ai.assistant.message`/…) is
//! not read.

use crate::{IngestError, Ingested, genaivocab, otlp};
use std::path::Path;

/// Parse a native OTel GenAI OTLP/JSON span export into one canonical run per `traceId`.
///
/// Returns the runs ordered by their earliest span's start time, each paired with any non-fatal
/// diagnostics raised while normalizing it. An export with no spans yields an empty vector.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid OTLP/JSON. Everything past a
/// successful parse is forgiving: a missing operation name, absent content, or a foreign
/// attribute yields a canonical fallback plus a warning, never an error.
pub fn from_otlp_json_str(s: &str) -> Result<Vec<Ingested>, IngestError> {
    otlp::from_otlp_json_str(s, genaivocab::map_span_to_step)
}

/// Load a native OTel GenAI OTLP/JSON span export from a file on disk.
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
