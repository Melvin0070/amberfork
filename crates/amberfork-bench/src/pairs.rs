//! Loading a chimera pair set from disk: `pair_*.json` manifests in one directory, each
//! naming a failing/reference run file (relative to that directory) and the gold fork step.
//! The manifest format is what `spike/make_pairs.py` writes — the de-facto pair contract the
//! align crate's parity test already reads. Runs load through `amberfork-ingest` (the one
//! trace boundary), and its warnings ride along for the harness to surface.
//!
//! Errors are hard by design: a malformed manifest or unreadable run fails the whole load.
//! Protocol rule 4 (exclusions are data) means a dropped case must be counted and explained,
//! and that accounting machinery arrives with the dev/test-split slice — until then nothing
//! is skipped silently.

use amberfork_ingest::IngestError;
use amberfork_model::{Run, Warning};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

/// One failing↔reference pair with its gold fork step (a failing-run index) and any ingest
/// warnings, each labeled with the run file it came from.
pub struct Pair {
    /// Manifest file stem (`pair_00`, …) — the pair's name in diagnostics.
    pub name: String,
    pub reference: Run,
    pub failing: Run,
    pub gold_step: usize,
    pub warnings: Vec<(PathBuf, Warning)>,
}

/// A pair manifest on disk. Extra fields (`meta`, …) are provenance for humans and ignored
/// here.
#[derive(Deserialize)]
struct Manifest {
    failing: PathBuf,
    reference: PathBuf,
    gold_step: usize,
}

/// Why a pair set failed to load. Every variant names the path it failed on — the fix is
/// always a file on the user's disk.
#[derive(Debug)]
pub enum LoadError {
    /// The pairs directory itself could not be read.
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    /// The directory exists but holds no `pair_*.json` manifests.
    Empty { dir: PathBuf },
    /// A manifest file could not be read.
    ManifestRead {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A manifest file is not valid manifest JSON.
    ManifestParse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// A run file a manifest points at failed to ingest.
    RunLoad { path: PathBuf, source: IngestError },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir { dir, source } => write!(f, "read pairs dir {}: {source}", dir.display()),
            Self::Empty { dir } => write!(
                f,
                "no pair manifests (pair_*.json) in {} — generate a set with \
                 `python3 spike/make_pairs.py`",
                dir.display()
            ),
            Self::ManifestRead { path, source } => {
                write!(f, "read manifest {}: {source}", path.display())
            }
            Self::ManifestParse { path, source } => {
                write!(f, "parse manifest {}: {source}", path.display())
            }
            Self::RunLoad { path, source } => write!(f, "load run {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dir { source, .. } | Self::ManifestRead { source, .. } => Some(source),
            Self::ManifestParse { source, .. } => Some(source),
            Self::RunLoad { source, .. } => Some(source),
            Self::Empty { .. } => None,
        }
    }
}

/// Load every `pair_*.json` manifest in `dir`, sorted by file name so pair order — and with
/// it every downstream number — is deterministic regardless of directory iteration order.
///
/// # Errors
/// The first [`LoadError`] encountered; a partial pair set is never returned.
pub fn load_pairs(dir: &Path) -> Result<Vec<Pair>, LoadError> {
    let mut manifest_paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| LoadError::Dir {
            dir: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("pair_") && name.ends_with(".json"))
        })
        .collect();
    manifest_paths.sort();
    if manifest_paths.is_empty() {
        return Err(LoadError::Empty {
            dir: dir.to_path_buf(),
        });
    }
    manifest_paths
        .iter()
        .map(|path| load_pair(dir, path))
        .collect()
}

fn load_pair(dir: &Path, manifest_path: &Path) -> Result<Pair, LoadError> {
    let text =
        std::fs::read_to_string(manifest_path).map_err(|source| LoadError::ManifestRead {
            path: manifest_path.to_path_buf(),
            source,
        })?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|source| LoadError::ManifestParse {
            path: manifest_path.to_path_buf(),
            source,
        })?;

    let mut warnings = Vec::new();
    let mut load_run = |rel: &Path| -> Result<Run, LoadError> {
        let path = dir.join(rel);
        let ingested = amberfork_ingest::load_file(&path).map_err(|source| LoadError::RunLoad {
            path: path.clone(),
            source,
        })?;
        warnings.extend(ingested.warnings.into_iter().map(|w| (path.clone(), w)));
        Ok(ingested.run)
    };
    let reference = load_run(&manifest.reference)?;
    let failing = load_run(&manifest.failing)?;

    let name = manifest_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Pair {
        name,
        reference,
        failing,
        gold_step: manifest.gold_step,
        warnings,
    })
}
