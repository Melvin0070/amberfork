//! Loading a chimera pair set from disk: `pair_*.json` manifests in one directory, each
//! naming a failing/reference run file (relative to that directory) and the gold fork step.
//! The manifest format is what `spike/make_pairs.py` writes — the de-facto pair contract the
//! align crate's parity test already reads. Runs load through `amberfork-ingest` (the one
//! trace boundary), and its warnings ride along for the harness to surface.
//!
//! Per-pair failures are NOT errors — protocol rule 4 (exclusions are data) says every case
//! that cannot be evaluated is counted and tabulated with a reason, because a rate over a
//! silently-shrunk denominator is a lie. So a bad manifest, an unloadable or empty run, or a
//! gold step outside the failing run becomes an [`Exclusion`] in the returned [`PairSet`],
//! never a skip and never an abort. Only directory-level problems — the pairs dir missing,
//! or holding no manifests at all — remain hard [`LoadError`]s: there the *set* is absent,
//! and there is nothing to tabulate.

use crate::split::Split;
use amberfork_ingest::IngestError;
use amberfork_model::{Run, Warning};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

/// One failing↔reference pair with its gold fork step (a failing-run index), its protocol
/// split assignment, and any ingest warnings, each labeled with the run file it came from.
pub struct Pair {
    /// Manifest file stem (`pair_00`, …) — the pair's name in diagnostics.
    pub name: String,
    /// The task/question identity behind the pair: the reference run's `id`. The reference
    /// is the unmodified source log, so its id names the underlying question (protocol
    /// rule 1 splits on it). Deliberately NOT the run's `task` field — that carries the
    /// question text, which for GAIA-derived sets must never reach a committed manifest
    /// (notebook 001/T30).
    pub task_key: String,
    /// Where the task key hashes in the dev/test split.
    pub split: Split,
    pub reference: Run,
    pub failing: Run,
    pub gold_step: usize,
    pub warnings: Vec<(PathBuf, Warning)>,
}

/// A loaded pair set: the evaluable pairs plus every excluded case, counted with its reason.
pub struct PairSet {
    pub pairs: Vec<Pair>,
    pub exclusions: Vec<Exclusion>,
}

impl PairSet {
    /// Total manifests found — the coverage denominator (rule 4).
    #[must_use]
    pub fn total(&self) -> usize {
        self.pairs.len() + self.exclusions.len()
    }
}

/// One excluded case: which manifest, and why it could not be evaluated.
pub struct Exclusion {
    /// Manifest file stem, same namespace as [`Pair::name`].
    pub name: String,
    pub reason: ExclusionReason,
}

/// Why a case was excluded. Every variant names the offending file *relative to the pairs
/// dir*, so tabulated exclusions stay meaningful in a committed results document.
pub enum ExclusionReason {
    /// The manifest file could not be read.
    ManifestUnreadable {
        file: PathBuf,
        source: std::io::Error,
    },
    /// The manifest file is not valid manifest JSON.
    ManifestInvalid {
        file: PathBuf,
        source: serde_json::Error,
    },
    /// A run file the manifest points at failed to ingest.
    RunUnloadable { file: PathBuf, source: IngestError },
    /// A run loaded but has zero steps — nothing to align or localize.
    EmptyRun { file: PathBuf },
    /// The gold step is not an index into the failing run.
    GoldOutOfRange {
        file: PathBuf,
        gold: usize,
        n_steps: usize,
    },
}

impl ExclusionReason {
    /// The reason's kind — the kebab-case bucket exclusions are tabulated under.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ManifestUnreadable { .. } => "manifest-unreadable",
            Self::ManifestInvalid { .. } => "manifest-invalid",
            Self::RunUnloadable { .. } => "run-unloadable",
            Self::EmptyRun { .. } => "empty-run",
            Self::GoldOutOfRange { .. } => "gold-out-of-range",
        }
    }

    /// The offending file, relative to the pairs dir.
    #[must_use]
    pub fn file(&self) -> &Path {
        match self {
            Self::ManifestUnreadable { file, .. }
            | Self::ManifestInvalid { file, .. }
            | Self::RunUnloadable { file, .. }
            | Self::EmptyRun { file }
            | Self::GoldOutOfRange { file, .. } => file,
        }
    }
}

impl fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestUnreadable { file, source } => {
                write!(f, "manifest {} unreadable: {source}", file.display())
            }
            Self::ManifestInvalid { file, source } => {
                write!(
                    f,
                    "manifest {} is not valid pair JSON: {source}",
                    file.display()
                )
            }
            Self::RunUnloadable { file, source } => {
                write!(f, "run {} failed to load: {source}", file.display())
            }
            Self::EmptyRun { file } => write!(f, "run {} has no steps", file.display()),
            Self::GoldOutOfRange {
                file,
                gold,
                n_steps,
            } => write!(
                f,
                "gold_step {gold} is outside the failing run {} ({n_steps} steps)",
                file.display()
            ),
        }
    }
}

/// A pair manifest on disk. Extra fields (`meta`, …) are provenance for humans and ignored
/// here.
#[derive(Deserialize)]
struct Manifest {
    failing: PathBuf,
    reference: PathBuf,
    gold_step: usize,
}

/// Why a pair set failed to load *as a whole*. Per-pair problems are [`Exclusion`]s, not
/// errors; these two are the cases where there is no set to report on at all.
#[derive(Debug)]
pub enum LoadError {
    /// The pairs directory itself could not be read.
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    /// The directory exists but holds no `pair_*.json` manifests.
    Empty { dir: PathBuf },
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
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dir { source, .. } => Some(source),
            Self::Empty { .. } => None,
        }
    }
}

/// Load every `pair_*.json` manifest in `dir`, sorted by file name so pair order — and with
/// it every downstream number — is deterministic regardless of directory iteration order.
/// Cases that cannot be evaluated come back as exclusions in the same set, in that same
/// order.
///
/// # Errors
/// Only [`LoadError::Dir`] and [`LoadError::Empty`] — a per-pair failure is data, not an
/// error.
pub fn load_pairs(dir: &Path) -> Result<PairSet, LoadError> {
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

    let mut set = PairSet {
        pairs: Vec::new(),
        exclusions: Vec::new(),
    };
    for path in &manifest_paths {
        match load_pair(dir, path) {
            Ok(pair) => set.pairs.push(pair),
            Err(exclusion) => set.exclusions.push(exclusion),
        }
    }
    Ok(set)
}

fn load_pair(dir: &Path, manifest_path: &Path) -> Result<Pair, Exclusion> {
    let name = manifest_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    evaluate_manifest(dir, manifest_path, &name).map_err(|reason| Exclusion { name, reason })
}

fn evaluate_manifest(
    dir: &Path,
    manifest_path: &Path,
    name: &str,
) -> Result<Pair, ExclusionReason> {
    let manifest_file = PathBuf::from(format!("{name}.json"));
    let text = std::fs::read_to_string(manifest_path).map_err(|source| {
        ExclusionReason::ManifestUnreadable {
            file: manifest_file.clone(),
            source,
        }
    })?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|source| ExclusionReason::ManifestInvalid {
            file: manifest_file,
            source,
        })?;

    let mut warnings = Vec::new();
    let mut load_run = |rel: &Path| -> Result<Run, ExclusionReason> {
        let path = dir.join(rel);
        let ingested = amberfork_ingest::load_file(&path).map_err(|source| {
            ExclusionReason::RunUnloadable {
                file: rel.to_path_buf(),
                source,
            }
        })?;
        if ingested.run.steps.is_empty() {
            return Err(ExclusionReason::EmptyRun {
                file: rel.to_path_buf(),
            });
        }
        warnings.extend(ingested.warnings.into_iter().map(|w| (path.clone(), w)));
        Ok(ingested.run)
    };
    let reference = load_run(&manifest.reference)?;
    let failing = load_run(&manifest.failing)?;

    if manifest.gold_step >= failing.steps.len() {
        return Err(ExclusionReason::GoldOutOfRange {
            file: manifest.failing,
            gold: manifest.gold_step,
            n_steps: failing.steps.len(),
        });
    }

    let task_key = reference.id.clone();
    Ok(Pair {
        name: name.to_string(),
        split: Split::of(&task_key),
        task_key,
        reference,
        failing,
        gold_step: manifest.gold_step,
        warnings,
    })
}
