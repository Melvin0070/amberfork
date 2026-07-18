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
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Ingest(err) => Some(err),
            Self::Cassette { source, .. } => Some(source),
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

/// Load a run from a file, routing by artifact shape. Reads the bytes once, sniffs, and hands
/// them to the matching loader; the cassette path normalizes into the same [`Run`] the passive
/// path produces, so everything downstream is identical.
///
/// # Errors
/// Returns [`LoadError`] if the file cannot be read, is not a valid cassette (when routed
/// there), or is not a canonical/foreign trace (when routed there).
pub fn load_run(path: &Path) -> Result<Ingested, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
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
    amberfork_ingest::from_json_str(&text).map_err(|err| LoadError::Ingest(err.with_path(path)))
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
}
