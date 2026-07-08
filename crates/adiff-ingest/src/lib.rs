//! Forgiving loader from plain-JSON trace files into the canonical [`adiff_model::Run`].
//!
//! This is the input edge of the pipeline. `docs/trace-format.md` promises a *deliberately
//! forgiving* format: a step's unknown fields are preserved into `attrs` and surfaced as an
//! "unmapped attributes" warning rather than failing the parse. That forgiveness lives here,
//! not in the frozen model — [`adiff_model::Step`] is strict and cannot hold unknown fields,
//! so this crate deserializes into private `Raw*` mirror types that *capture* the extras
//! (`#[serde(flatten)]`) and folds them into `attrs` during conversion.
//!
//! What stays strict: the canonical `kind` vocabulary. A step whose `kind` is not one of
//! `llm`/`tool`/`agent`/`other` is a [`IngestError::Parse`], not a forgiven value — mapping
//! framework-specific span kinds down onto the canonical four is a normalizer's job, not this
//! canonical loader's.
//!
//! Sync by design: this is `std::fs` only. `tokio` is reserved for the genuine async I/O edge
//! (record/serve), so the ingest path stays a pure, testable function.

use adiff_model::{
    Edge, Outcome, Payload, Run, SchemaVersion, Step, StepKind, Warning, WarningCode,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::fmt;
use std::path::{Path, PathBuf};

/// Framework source adapters. Unlike the canonical loader above, these map a foreign trace shape
/// onto [`Run`]; each is namespaced so its quirks stay isolated from the canonical format.
pub mod whowhen;

/// A loaded run together with any non-fatal diagnostics raised while normalizing it. The
/// warnings flow onward into [`adiff_model::DiffResult::warnings`].
#[derive(Debug, Clone, PartialEq)]
pub struct Ingested {
    pub run: Run,
    pub warnings: Vec<Warning>,
}

/// Everything that can go wrong loading a trace. The library path returns this rather than
/// panicking.
#[derive(Debug)]
pub enum IngestError {
    /// The file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The bytes were not valid JSON for the trace format.
    Parse(serde_json::Error),
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read trace file {}: {source}", path.display())
            }
            Self::Parse(source) => write!(f, "failed to parse trace JSON: {source}"),
        }
    }
}

impl std::error::Error for IngestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(source) => Some(source),
        }
    }
}

/// Parse a canonical trace from a JSON string.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid trace JSON (malformed, or a
/// non-canonical `kind`).
pub fn from_json_str(s: &str) -> Result<Ingested, IngestError> {
    let raw: RawRun = serde_json::from_str(s).map_err(IngestError::Parse)?;
    let (run, warnings) = raw.into_run();
    Ok(Ingested { run, warnings })
}

/// Load a canonical trace from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or [`IngestError::Parse`] if its
/// contents are not valid trace JSON.
pub fn load_file(path: impl AsRef<Path>) -> Result<Ingested, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_json_str(&text)
}

/// Deserialization mirror of [`Run`] that captures unmapped top-level fields.
#[derive(Deserialize)]
struct RawRun {
    schema_version: SchemaVersion,
    id: String,
    #[serde(default)]
    task: Option<String>,
    #[serde(default)]
    outcome: Option<Outcome>,
    steps: Vec<RawStep>,
    #[serde(default)]
    edges: Option<Vec<Edge>>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// Deserialization mirror of [`Step`] that captures unmapped fields into `extra` so the loader
/// can preserve them into `attrs` instead of dropping them.
#[derive(Deserialize)]
struct RawStep {
    idx: usize,
    kind: StepKind,
    name: String,
    #[serde(default)]
    inputs: Option<Payload>,
    #[serde(default)]
    outputs: Option<Payload>,
    #[serde(default)]
    attrs: Map<String, Value>,
    #[serde(default)]
    t_start: Option<String>,
    #[serde(default)]
    t_end: Option<String>,
    #[serde(default)]
    parent_idx: Option<usize>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl RawRun {
    fn into_run(self) -> (Run, Vec<Warning>) {
        let mut warnings = Vec::new();
        if !self.extra.is_empty() {
            warnings.push(Warning {
                code: WarningCode::UnmappedAttributes,
                msg: format!(
                    "run-level unmapped fields (no run-level attrs to hold them, dropped): {}",
                    sorted_keys(&self.extra)
                ),
            });
        }
        let steps = self
            .steps
            .into_iter()
            .map(|step| step.into_step(&mut warnings))
            .collect();
        let run = Run {
            schema_version: self.schema_version,
            id: self.id,
            task: self.task,
            outcome: self.outcome,
            steps,
            edges: self.edges,
        };
        (run, warnings)
    }
}

impl RawStep {
    fn into_step(self, warnings: &mut Vec<Warning>) -> Step {
        let mut attrs = self.attrs;
        if !self.extra.is_empty() {
            warnings.push(Warning {
                code: WarningCode::UnmappedAttributes,
                msg: format!(
                    "step {} ({}): unmapped fields moved to attrs: {}",
                    self.idx,
                    self.name,
                    sorted_keys(&self.extra)
                ),
            });
            for (key, value) in self.extra {
                attrs.insert(key, value);
            }
        }
        if self.inputs.is_none() && self.outputs.is_none() {
            warnings.push(Warning {
                code: WarningCode::ContentAbsent,
                msg: format!(
                    "step {} ({}): no input or output content captured",
                    self.idx, self.name
                ),
            });
        }
        Step {
            idx: self.idx,
            kind: self.kind,
            name: self.name,
            inputs: self.inputs,
            outputs: self.outputs,
            attrs,
            t_start: self.t_start,
            t_end: self.t_end,
            parent_idx: self.parent_idx,
        }
    }
}

/// Comma-joined keys of a map, in sorted order (the backing map is a `BTreeMap`, so warning
/// text is deterministic).
fn sorted_keys(map: &Map<String, Value>) -> String {
    map.keys().cloned().collect::<Vec<_>>().join(", ")
}
