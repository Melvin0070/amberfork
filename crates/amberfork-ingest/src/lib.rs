//! Forgiving loader from plain-JSON trace files into the canonical [`amberfork_model::Run`].
//!
//! This is the input edge of the pipeline. `docs/trace-format.md` promises a *deliberately
//! forgiving* format: a step's unknown fields are preserved into `attrs` and surfaced as an
//! "unmapped attributes" warning rather than failing the parse. That forgiveness lives here,
//! not in the frozen model — [`amberfork_model::Step`] is strict and cannot hold unknown fields,
//! so this crate deserializes into private `Raw*` mirror types that *capture* the extras
//! (`#[serde(flatten)]`) and folds them into `attrs` during conversion.
//!
//! What stays strict: the canonical `kind` vocabulary. A step whose `kind` is not one of
//! `llm`/`tool`/`agent`/`other` is a [`IngestError::NotATrace`], not a forgiven value — mapping
//! framework-specific span kinds down onto the canonical four is a normalizer's job, not this
//! canonical loader's.
//!
//! Sync by design: this is `std::fs` only. `tokio` is reserved for the genuine async I/O edge
//! (record/serve), so the ingest path stays a pure, testable function.

use amberfork_model::{
    Edge, Outcome, Payload, Run, SchemaVersion, Step, StepKind, Warning, WarningCode,
};
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::{Map, Value};
use std::fmt;
use std::path::{Path, PathBuf};

/// Doc pointers baked into error text (issue #20): the first parse error is the product
/// surface for a new user, so it must link out instead of dead-ending. Built from the crate's
/// repository URL so the links can't drift from where the code lives.
const TRACE_FORMAT_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/blob/main/docs/trace-format.md"
);
const CONVERSION_GUIDE_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/blob/main/docs/run-on-your-own-agent.md"
);

/// Framework source adapters. Unlike the canonical loader above, these map a foreign trace shape
/// onto [`Run`]; each is namespaced so its quirks stay isolated from the canonical format.
pub mod tape;
pub mod whowhen;

/// A loaded run together with any non-fatal diagnostics raised while normalizing it. The
/// warnings flow onward into [`amberfork_model::DiffResult::warnings`].
#[derive(Debug, Clone, PartialEq)]
pub struct Ingested {
    pub run: Run,
    pub warnings: Vec<Warning>,
}

/// Everything that can go wrong loading a trace. The library path returns this rather than
/// panicking.
///
/// The parse family carries an optional `path` because `amberfork diff` reads two inputs — an
/// error that doesn't say which file broke is a dead end. [`load_file`] fills it in;
/// [`from_json_str`] has no path to give (embedded demo pair, future stdin).
#[derive(Debug)]
pub enum IngestError {
    /// The file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The input was not parseable JSON. The canonical loader reserves this for malformed
    /// JSON — shape problems classify as [`Self::NotATrace`] — while the format adapters
    /// ([`tape`], [`whowhen`]) use it for any serde failure on their foreign shapes.
    Parse {
        path: Option<PathBuf>,
        source: serde_json::Error,
    },
    /// Valid JSON, but not shaped like a canonical trace (issue #20). The serde detail names
    /// the field or variant that broke; the display adds what a trace needs and points to the
    /// format reference so the first error is not a dead end.
    NotATrace {
        path: Option<PathBuf>,
        source: serde_json::Error,
    },
    /// The input looks like JSON-Lines — the shape of a raw exporter transcript, the
    /// likeliest first mistake (issue #20). amberfork takes a single JSON trace per file;
    /// the display points to the conversion guide.
    JsonLines { path: Option<PathBuf> },
}

impl IngestError {
    /// Attach the originating file to a parse-family error — [`load_file`] knows the path,
    /// [`from_json_str`] doesn't. `Io` already names its file.
    fn with_path(self, path: &Path) -> Self {
        let path = Some(path.to_path_buf());
        match self {
            Self::Io { .. } => self,
            Self::Parse { source, .. } => Self::Parse { path, source },
            Self::NotATrace { source, .. } => Self::NotATrace { path, source },
            Self::JsonLines { .. } => Self::JsonLines { path },
        }
    }
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Parse-family messages open with the offending file when known, `path: message`
        // style, so a two-input `diff` error is attributable at a glance.
        let name = |path: &Option<PathBuf>| {
            path.as_ref()
                .map_or_else(String::new, |p| format!("{}: ", p.display()))
        };
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read trace file {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(f, "{}failed to parse trace JSON: {source}", name(path))
            }
            Self::NotATrace { path, source } => write!(
                f,
                "{}valid JSON, but not a canonical trace: {source}\n  \
                 a canonical trace is a single JSON object with `schema_version`, `id`, and `steps`\n  \
                 format reference: {TRACE_FORMAT_URL}\n  \
                 conversion guide: {CONVERSION_GUIDE_URL}",
                name(path)
            ),
            Self::JsonLines { path } => write!(
                f,
                "{}this looks like JSON-Lines (one JSON value per line) — amberfork takes a \
                 single JSON trace per file\n  \
                 conversion guide: {CONVERSION_GUIDE_URL}",
                name(path)
            ),
        }
    }
}

impl std::error::Error for IngestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } | Self::NotATrace { source, .. } => Some(source),
            Self::JsonLines { .. } => None,
        }
    }
}

/// Parse a canonical trace from a JSON string.
///
/// # Errors
/// Returns [`IngestError::Parse`] if the string is not valid JSON, [`IngestError::NotATrace`]
/// if it is valid JSON that isn't shaped like a canonical trace (including a non-canonical
/// `kind`), or [`IngestError::JsonLines`] if it looks like a raw JSONL transcript.
pub fn from_json_str(s: &str) -> Result<Ingested, IngestError> {
    let raw: RawRun =
        serde_json::from_str(s).map_err(|source| classify_parse_failure(s, source))?;
    let (run, warnings) = raw.into_run();
    Ok(Ingested { run, warnings })
}

/// Load a canonical trace from a file on disk.
///
/// # Errors
/// Returns [`IngestError::Io`] if the file cannot be read, or a parse-family error (see
/// [`from_json_str`]) — with the path attached — if its contents are not a canonical trace.
pub fn load_file(path: impl AsRef<Path>) -> Result<Ingested, IngestError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_json_str(&text).map_err(|err| err.with_path(path))
}

/// Decide what a failed top-level parse actually means (issue #20). Runs only on the error
/// path, so the extra whole-input scan costs nothing on a clean load. Order matters: a JSONL
/// file is not valid JSON as a whole, so its check must precede the valid-JSON one.
fn classify_parse_failure(text: &str, source: serde_json::Error) -> IngestError {
    if looks_like_json_lines(text) {
        return IngestError::JsonLines { path: None };
    }
    if serde_json::from_str::<IgnoredAny>(text).is_ok() {
        return IngestError::NotATrace { path: None, source };
    }
    IngestError::Parse { path: None, source }
}

/// The issue-#20 heuristic, verbatim: the first non-empty line is a complete JSON value and
/// more lines follow. A pretty-printed JSON document cannot false-positive — its first line
/// (`{`) is not complete JSON on its own. `IgnoredAny` validates without building a value.
fn looks_like_json_lines(text: &str) -> bool {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    lines.next().is_some() && serde_json::from_str::<IgnoredAny>(first).is_ok()
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
        if !self.schema_version.is_current() {
            warnings.push(Warning {
                code: WarningCode::SchemaVersionMismatch,
                msg: format!(
                    "trace declares schema_version {:?}; this build is native to {} — fields may be read under the wrong contract",
                    self.schema_version.0,
                    SchemaVersion::CURRENT
                ),
            });
        }
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

/// Comma-joined keys of a map, explicitly sorted so warning text is deterministic — the
/// workspace's `serde_json` map preserves insertion order (issue #17), so the sort cannot be
/// inherited from the map type.
fn sorted_keys(map: &Map<String, Value>) -> String {
    let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys.join(", ")
}
