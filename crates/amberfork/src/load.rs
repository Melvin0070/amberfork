//! Loading a run from a file, auto-detecting which amberfork artifact it is.
//!
//! `amberfork diff` takes two files, and each may be a canonical trace (the passive path,
//! `docs/trace-format.md`) or a captured cassette (the record path, `docs/cassette-format.md`).
//! The founder chose auto-detection over an explicit `convert` step (issue #33's deferred
//! checkpoint): a cassette is a first-party, self-versioning amberfork artifact, not a foreign
//! shape, and the two carry mutually-exclusive version keys — so a single unambiguous sniff on
//! the top-level `cassette_version` routes each file with zero ceremony.
//!
//! This dispatch lives at the CLI (the composition root, already an I/O edge), not inside
//! `amberfork-ingest`: the pure ingest crate stays canonical-only and free of the record
//! crate's runtime half, so the tokio quarantine holds. The aligner still only ever sees a
//! `Run` — normalization happens here, before the engine, so "the trace format stays the one
//! seam the aligner reads" is preserved.

use amberfork_ingest::{IngestError, Ingested};
use amberfork_model::Run;
use amberfork_record::Cassette;
use std::fmt;
use std::path::{Path, PathBuf};

/// The cassette-format reference, baked into the cassette error the same way
/// `amberfork-ingest` builds its trace-format links — so a first parse failure on a cassette
/// links out to its contract instead of dead-ending.
const CASSETTE_FORMAT_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/blob/main/docs/cassette-format.md"
);

/// Everything that can go wrong loading either artifact. The canonical/foreign path keeps its
/// own rich [`IngestError`] verbatim; the cassette path gets its own arm so a file the sniff
/// routed to the record path fails as a *cassette*, never as the misleading "not a canonical
/// trace" — routing by a version key means owning the error text for what that key selected.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The canonical/foreign loader rejected the file (already path-attributed and doc-linked).
    Ingest(IngestError),
    /// The file carried a `cassette_version` but is not a well-formed cassette.
    Cassette {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// `diff --verify` needs a recorded cassette to re-execute, but this file is a canonical
    /// trace (no `cassette_version`). Only a cassette carries the recorded exchanges a re-run
    /// replays — a passive trace has nothing to re-execute.
    NotACassette { path: PathBuf },
    /// An OTLP/JSON export with no spans — there is no run in it to load.
    EmptyExport { path: PathBuf },
    /// An OTLP/JSON export holding several traces, and nothing said which one is the run.
    ManyTraces {
        path: PathBuf,
        traces: Vec<TraceSummary>,
        flag: &'static str,
    },
    /// A trace id was asked for that the export does not contain (or that matches several).
    UnknownTrace {
        path: PathBuf,
        wanted: String,
        traces: Vec<TraceSummary>,
        flag: &'static str,
    },
    /// A trace id was given for a file that is not an OTLP export. Ignoring it would let a
    /// mistyped path compare a run the user did not mean.
    TraceIdWithoutExport { path: PathBuf, flag: &'static str },
}

/// One trace in an OTLP export, as listed in an error: its id and how many steps it has.
#[derive(Debug)]
pub struct TraceSummary {
    pub id: String,
    pub steps: usize,
}

/// Which trace of a multi-trace OTLP export a side of the diff means, and the flag that says so
/// — carried together so an error can name the flag for the side that needs it.
#[derive(Debug, Clone, Copy)]
pub struct TraceChoice<'a> {
    pub id: Option<&'a str>,
    pub flag: &'static str,
}

fn list_traces(f: &mut fmt::Formatter<'_>, traces: &[TraceSummary]) -> fmt::Result {
    for trace in traces {
        write!(f, "\n    {}  ({} steps)", trace.id, trace.steps)?;
    }
    Ok(())
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read trace file {}: {source}", path.display())
            }
            Self::Ingest(err) => write!(f, "{err}"),
            Self::Cassette { path, source } => write!(
                f,
                "{}has a `cassette_version` but is not a valid cassette: {source}\n  \
                 a cassette is a single JSON object with `cassette_version`, `id`, and \
                 `exchanges`\n  \
                 format reference: {CASSETTE_FORMAT_URL}",
                format_args!("{}: ", path.display())
            ),
            Self::NotACassette { path } => write!(
                f,
                "{}: diff --verify re-executes recorded exchanges, so both runs must be cassettes \
                 — this is a canonical trace (no `cassette_version`)\n  \
                 record one with `amberfork record -- <cmd>`\n  \
                 format reference: {CASSETTE_FORMAT_URL}",
                path.display()
            ),
            Self::EmptyExport { path } => write!(
                f,
                "{}: this is an OTLP export (`resourceSpans`) with no spans in it — there is no \
                 run to load",
                path.display()
            ),
            Self::ManyTraces { path, traces, flag } => {
                write!(
                    f,
                    "{}: this OTLP export holds {} traces, and each trace is one run — pick one \
                     with `{flag} <ID>` (a unique prefix is enough):",
                    path.display(),
                    traces.len()
                )?;
                list_traces(f, traces)
            }
            Self::UnknownTrace {
                path,
                wanted,
                traces,
                flag,
            } => {
                write!(
                    f,
                    "{}: `{flag} {wanted}` does not name exactly one trace in this OTLP export, \
                     which holds:",
                    path.display()
                )?;
                list_traces(f, traces)
            }
            Self::TraceIdWithoutExport { path, flag } => write!(
                f,
                "{}: `{flag}` picks a trace out of an OTLP export, but this file is not one (no \
                 `resourceSpans`)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Ingest(err) => Some(err),
            Self::Cassette { source, .. } => Some(source),
            Self::NotACassette { .. }
            | Self::EmptyExport { .. }
            | Self::ManyTraces { .. }
            | Self::UnknownTrace { .. }
            | Self::TraceIdWithoutExport { .. } => None,
        }
    }
}

/// Whether a file's bytes name it a cassette. The whole discriminator is the presence of a
/// top-level `cassette_version`: a canonical trace never carries it, a cassette always does, and
/// the two contracts version independently. Presence-based, not value-based: a file that sets
/// `cassette_version` to a wrong type is still a *cassette attempt* and belongs on the cassette
/// path (where it earns a cassette-specific error), not silently rerouted to the trace loader.
/// A malformed-JSON file fails to parse and falls through to the canonical loader, which owns
/// the richer JSON-Lines / not-a-trace classification.
fn is_cassette(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .is_ok_and(|value| value.get("cassette_version").is_some())
}

/// Pick the one run a side of the diff means out of an OTLP export's traces: the only one, or the
/// one `choice` names by id or unique id prefix.
fn pick_trace(
    path: &Path,
    mut runs: Vec<Ingested>,
    choice: TraceChoice<'_>,
) -> Result<Ingested, LoadError> {
    let summaries = |runs: &[Ingested]| -> Vec<TraceSummary> {
        runs.iter()
            .map(|ingested| TraceSummary {
                id: ingested.run.id.clone(),
                steps: ingested.run.steps.len(),
            })
            .collect()
    };
    let Some(wanted) = choice.id else {
        return match runs.len() {
            0 => Err(LoadError::EmptyExport {
                path: path.to_path_buf(),
            }),
            1 => Ok(runs.remove(0)),
            _ => Err(LoadError::ManyTraces {
                path: path.to_path_buf(),
                traces: summaries(&runs),
                flag: choice.flag,
            }),
        };
    };
    let exact = runs.iter().position(|ingested| ingested.run.id == wanted);
    let by_prefix = || {
        let mut matches = runs
            .iter()
            .enumerate()
            .filter(|(_, ingested)| ingested.run.id.starts_with(wanted));
        match (matches.next(), matches.next()) {
            (Some((index, _)), None) => Some(index),
            _ => None,
        }
    };
    match exact.or_else(by_prefix) {
        Some(index) => Ok(runs.remove(index)),
        None => Err(LoadError::UnknownTrace {
            path: path.to_path_buf(),
            wanted: wanted.to_owned(),
            traces: summaries(&runs),
            flag: choice.flag,
        }),
    }
}

/// Load a run from a file, routing by artifact shape. Reads the bytes once, sniffs, and hands
/// them to the matching loader; the cassette path normalizes into the same [`Run`] the passive
/// path produces, so everything downstream is identical.
///
/// # Errors
/// Returns [`LoadError`] if the file cannot be read, is not a valid cassette (when routed
/// there), is not a canonical/foreign trace (when routed there), or is an OTLP export from
/// which `choice` does not select exactly one trace.
pub fn load_run(path: &Path, choice: TraceChoice<'_>) -> Result<Ingested, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    // An OTLP export is the one shape that can hold several runs, so it is the one shape the
    // trace choice applies to; on anything else a trace id is a mistake, not a no-op.
    if amberfork_ingest::otel::looks_like_export(&text) {
        let runs = amberfork_ingest::otel::from_otlp_json_str(&text)
            .map_err(|err| LoadError::Ingest(err.with_path(path)))?;
        return pick_trace(path, runs, choice);
    }
    if choice.id.is_some() {
        return Err(LoadError::TraceIdWithoutExport {
            path: path.to_path_buf(),
            flag: choice.flag,
        });
    }
    if is_cassette(&text) {
        let run: Run =
            amberfork_record::normalize_str(&text).map_err(|source| LoadError::Cassette {
                path: path.to_path_buf(),
                source,
            })?;
        // A cassette guarantees full content, so normalization raises no diagnostics today; the
        // `cassette_version`-mismatch warning is a follow-up (no non-current version exists yet).
        return Ok(Ingested {
            run,
            warnings: Vec::new(),
        });
    }
    // A Claude Code transcript is the one foreign shape sniffed here: it is already on the
    // user's disk, and without this it dead-ends in the generic JSON-Lines error.
    if amberfork_ingest::claudecode::looks_like_session(&text) {
        return amberfork_ingest::claudecode::from_jsonl_str(&text)
            .map_err(|err| LoadError::Ingest(err.with_path(path)));
    }
    // ATIF shares the `schema_version` key with the canonical trace, so it is told apart by the
    // value (`ATIF-…`) and must be routed before the canonical loader rejects it.
    if amberfork_ingest::atif::looks_like_trajectory(&text) {
        return amberfork_ingest::atif::from_json_str(&text)
            .map_err(|err| LoadError::Ingest(err.with_path(path)));
    }
    amberfork_ingest::from_json_str(&text).map_err(|err| LoadError::Ingest(err.with_path(path)))
}

/// Load a file as a recorded [`Cassette`], the input `diff --verify` re-executes. Unlike
/// [`load_run`], a canonical trace is a hard error here ([`LoadError::NotACassette`]): re-execution
/// replays recorded exchanges, and a passive trace has none. The same version sniff routes the
/// file, so a cassette that fails to parse still fails as a *cassette*, never as a not-a-trace.
///
/// # Errors
/// Returns [`LoadError`] if the file cannot be read, is a canonical trace, or is not a well-formed
/// cassette.
pub fn load_cassette(path: &Path) -> Result<Cassette, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !is_cassette(&text) {
        return Err(LoadError::NotACassette {
            path: path.to_path_buf(),
        });
    }
    serde_json::from_str(&text).map_err(|source| LoadError::Cassette {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cassette_is_detected_by_its_version_key() {
        assert!(is_cassette(
            r#"{"cassette_version": "0.1", "id": "x", "exchanges": []}"#
        ));
    }

    #[test]
    fn a_canonical_trace_is_not_a_cassette() {
        // The passive artifact carries `schema_version`, never `cassette_version` — the two
        // contracts version independently, which is exactly what makes the sniff unambiguous.
        assert!(!is_cassette(
            r#"{"schema_version": "0.1", "id": "x", "steps": []}"#
        ));
    }

    #[test]
    fn a_cassette_with_a_wrong_typed_version_still_routes_to_the_cassette_path() {
        // Presence, not validity: this belongs on the cassette path so it fails as a cassette,
        // not as a canonical trace.
        assert!(is_cassette(r#"{"cassette_version": 5, "id": "x"}"#));
    }

    #[test]
    fn malformed_json_is_not_claimed_as_a_cassette() {
        // The probe fails, so routing falls through to the canonical loader and its richer
        // JSON-Lines / not-a-trace errors, rather than mislabeling a broken file a cassette.
        assert!(!is_cassette("{not json"));
        assert!(!is_cassette("{\"a\":1}\n{\"b\":2}"));
    }

    fn write_temp(contents: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(file.path(), contents).expect("write temp");
        file
    }

    #[test]
    fn load_cassette_reads_a_recorded_cassette() {
        let file = write_temp(r#"{"cassette_version": "0.1", "id": "run-7", "exchanges": []}"#);
        let cassette = load_cassette(file.path()).expect("a well-formed cassette loads");
        assert_eq!(cassette.id, "run-7");
    }

    #[test]
    fn load_cassette_rejects_a_canonical_trace_as_not_a_cassette() {
        // A passive trace has no recorded exchanges to re-execute, so --verify cannot use it — and
        // it fails as a not-a-cassette, never as the misleading "not a canonical trace".
        let file = write_temp(r#"{"schema_version": "0.1", "id": "x", "steps": []}"#);
        let err = load_cassette(file.path()).expect_err("a canonical trace is not a cassette");
        assert!(matches!(err, LoadError::NotACassette { .. }));
    }

    #[test]
    fn load_cassette_reports_a_cassette_shaped_but_broken_file_as_a_cassette_error() {
        // Sniffed onto the cassette path by its version key, but the body is not a valid cassette:
        // it fails as a cassette (owning that error), not routed back to the trace loader.
        let file = write_temp(r#"{"cassette_version": "0.1", "id": 5}"#);
        let err = load_cassette(file.path()).expect_err("a broken cassette body is an error");
        assert!(matches!(err, LoadError::Cassette { .. }));
    }
}
