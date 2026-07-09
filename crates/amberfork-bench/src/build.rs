//! Constructing a cross-system Mode A′ pair set from raw upstream data (issue #7).
//!
//! The Rust successor to `spike/make_realpairs.py`, made honest. A Mode A′ pair joins a
//! *failing* run (a Who&When log — every one is a failure by construction) to a *reference*
//! run of a **different agent system** (a ServiceNow TapeAgents tape that actually solved the
//! task), on the same GAIA task. This module reads both through the already-landed
//! [`amberfork_ingest`] source adapters, matches them on the shared GAIA `task_id`, and writes
//! the `pair_*.json` + `a_*`/`b_*` triples the slice-1 seam ([`crate::pairs::load_pairs`])
//! already reads — a converted pair now has an in-tree path from raw data to the honest table.
//!
//! Two boundaries the port makes explicit — the same "exclusions are data" ethos as the loader:
//! - **A tape earns reference status; it is not assumed to have it.** Only a tape that actually
//!   solved its task ([`amberfork_ingest::tape::TapeMeta::is_success`]) and names a `task_id` can
//!   anchor a pair. The spike hardcoded `pass` and filtered late; here a tape that fails either
//!   test is a counted [`Drop`], never a silent skip.
//! - **A failing log must offer a usable fork step.** Only a Who&When log whose gold resolves to
//!   [`amberfork_ingest::whowhen::GoldStep::Valid`] can serve as the failing side — a fork the
//!   benchmark scores against must be a real index into the trajectory. Gold-less logs are
//!   counted ([`BuildStats::logs_without_gold`]), not paired.
//!
//! Reads are strict about their own inputs (a malformed source file is a hard [`BuildError`], not
//! a forgiven value) — unlike [`crate::pairs::load_pairs`], whose per-pair tolerance is about the
//! *reader* surviving a bad committed set. Here the operator is building the set from raw upstream
//! data on their own disk; a source file that will not parse is theirs to fix, loudly.

use amberfork_ingest::IngestError;
use amberfork_ingest::whowhen::{GoldStep, Split};
use amberfork_ingest::{tape, whowhen};
use amberfork_model::Run;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// A reference-side candidate: a converted TapeAgents tape plus the two facts that decide
/// whether it may anchor a Mode A′ pair.
pub struct Reference {
    /// Source identifier (the tape file stem) — the pair's determinism key and provenance.
    pub stem: String,
    /// The canonical reference trajectory.
    pub run: Run,
    /// The GAIA task the tape answers, or `None` when it carried no structured task block.
    pub task_id: Option<String>,
    /// Whether the tape actually solved its task (produced answer == gold).
    pub is_success: bool,
}

/// A failing-side candidate: a converted Who&When log with a usable gold fork step and the GAIA
/// task id it shares with a reference. Only logs offering a [`GoldStep::Valid`] reach this type.
pub struct Failing {
    /// Source identifier (the converted run id, e.g. `whowhen_hand_4`) — unique across splits.
    pub stem: String,
    /// The canonical failing trajectory.
    pub run: Run,
    /// The GAIA task id (Who&When `question_ID`) — the key a pair is matched on.
    pub task_id: String,
    /// The annotated fork step, already resolved in range against the trajectory.
    pub gold_step: usize,
}

/// One constructed cross-system pair: a failing run, a different-system reference run, and the
/// gold fork step, numbered by construction order.
pub struct BuiltPair {
    /// Sequential pair number — the `NN` in `pair_NN.json`.
    pub index: usize,
    /// The GAIA task id both sides share.
    pub task_id: String,
    /// Source stem of the failing run (provenance).
    pub failing_stem: String,
    /// Source stem of the reference run (provenance).
    pub reference_stem: String,
    pub failing: Run,
    pub reference: Run,
    pub gold_step: usize,
}

/// Why a reference candidate did not become a pair. Reported, never dropped silently: the tape
/// set is the scarce resource (a handful of passing tapes), so every unused one is accounted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The tape did not solve its task — its produced answer differs from the gold answer.
    Unsuccessful,
    /// The tape carried no GAIA task id, so there is no key to match a failing log on.
    MissingTaskId,
    /// The tape solved its task and names one, but no failing log shares its task id.
    NoFailingMatch,
}

impl fmt::Display for DropReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Unsuccessful => "tape did not solve its task (produced answer != gold)",
            Self::MissingTaskId => "tape carries no GAIA task_id to match on",
            Self::NoFailingMatch => "no failing log shares this tape's task_id",
        };
        f.write_str(text)
    }
}

/// A dropped reference candidate: the tape stem and why it was not paired.
pub struct Drop {
    pub stem: String,
    pub reason: DropReason,
}

/// The result of matching: the pairs built, and every reference candidate that was not paired.
pub struct BuildOutcome {
    pub pairs: Vec<BuiltPair>,
    pub drops: Vec<Drop>,
}

/// Match reference candidates to failing candidates on their shared GAIA `task_id`.
///
/// Deterministic regardless of input order: both sides are sorted by stem, task-id collisions on
/// the failing side resolve to the lowest stem, references are visited in stem order, and pairs
/// are numbered sequentially. Every reference that is not paired appears in `drops` with a reason.
#[must_use]
pub fn match_pairs(mut references: Vec<Reference>, mut failings: Vec<Failing>) -> BuildOutcome {
    references.sort_by(|a, b| a.stem.cmp(&b.stem));
    failings.sort_by(|a, b| a.stem.cmp(&b.stem));

    // First (lowest-stem) failing per task id wins, so a task with several failing logs pairs
    // reproducibly against one anchor rather than an iteration-order-dependent choice.
    let mut by_task: BTreeMap<&str, &Failing> = BTreeMap::new();
    for failing in &failings {
        by_task.entry(failing.task_id.as_str()).or_insert(failing);
    }

    let mut pairs = Vec::new();
    let mut drops = Vec::new();
    for reference in &references {
        if !reference.is_success {
            drops.push(Drop {
                stem: reference.stem.clone(),
                reason: DropReason::Unsuccessful,
            });
            continue;
        }
        let Some(task_id) = reference.task_id.as_deref() else {
            drops.push(Drop {
                stem: reference.stem.clone(),
                reason: DropReason::MissingTaskId,
            });
            continue;
        };
        let Some(failing) = by_task.get(task_id) else {
            drops.push(Drop {
                stem: reference.stem.clone(),
                reason: DropReason::NoFailingMatch,
            });
            continue;
        };
        pairs.push(BuiltPair {
            index: pairs.len(),
            task_id: task_id.to_string(),
            failing_stem: failing.stem.clone(),
            reference_stem: reference.stem.clone(),
            failing: failing.run.clone(),
            reference: reference.run.clone(),
            gold_step: reference_gold(failing),
        });
    }
    BuildOutcome { pairs, drops }
}

/// The gold fork step for a pair — the failing log's annotated step. Named for readability at the
/// call site (the gold belongs to the *failing* run, not the reference it is paired against).
const fn reference_gold(failing: &Failing) -> usize {
    failing.gold_step
}

/// What `build-pairs` did: how many pairs it wrote and the coverage around them. Printed to the
/// operator so a thin overlap (0 pairs from real data is a legitimate outcome, not a tool failure)
/// is loud rather than silent.
pub struct BuildStats {
    pub pairs: usize,
    pub drops: Vec<Drop>,
    pub tapes_read: usize,
    pub logs_read: usize,
    /// Who&When logs skipped because their gold did not resolve to a usable fork step.
    pub logs_without_gold: usize,
}

/// Build a cross-system pair set from a directory of raw TapeAgents tapes and a directory of raw
/// Who&When logs, writing the triples into `out_dir`.
///
/// `logs_dir` is expected to hold `Hand-Crafted/` and/or `Algorithm-Generated/` subdirectories of
/// raw logs (the upstream Who&When layout); an absent split subdir is skipped, not an error.
///
/// # Errors
/// [`BuildError`] if a source directory cannot be read, a source file will not parse, or an output
/// file cannot be encoded or written. Building zero pairs is *not* an error — the counts say so.
pub fn build_pairs(
    tapes_dir: &Path,
    logs_dir: &Path,
    out_dir: &Path,
) -> Result<BuildStats, BuildError> {
    let references = read_references(tapes_dir)?;
    let tapes_read = references.len();
    let failings = read_failings(logs_dir)?;
    let logs_read = failings.total;
    let logs_without_gold = failings.without_gold;

    let outcome = match_pairs(references, failings.eligible);
    write_set(out_dir, &outcome)?;

    Ok(BuildStats {
        pairs: outcome.pairs.len(),
        drops: outcome.drops,
        tapes_read,
        logs_read,
        logs_without_gold,
    })
}

/// Read and convert every tape in `dir` into a [`Reference`] candidate.
fn read_references(dir: &Path) -> Result<Vec<Reference>, BuildError> {
    let mut references = Vec::new();
    for path in json_files(dir)? {
        let converted = tape::convert_file(&path).map_err(|source| BuildError::Convert {
            path: path.clone(),
            source,
        })?;
        references.push(Reference {
            stem: stem_of(&path),
            is_success: converted.meta.is_success(),
            task_id: converted.meta.task_id,
            run: converted.run,
        });
    }
    Ok(references)
}

/// The failing candidates from a Who&When log tree, plus the coverage counts around them.
struct FailingScan {
    eligible: Vec<Failing>,
    total: usize,
    without_gold: usize,
}

/// Read and convert every Who&When log under `dir`'s split subdirectories into [`Failing`]
/// candidates, keeping only those that offer a usable gold fork step.
fn read_failings(dir: &Path) -> Result<FailingScan, BuildError> {
    const SPLITS: [(&str, Split); 2] = [
        ("Hand-Crafted", Split::HandCrafted),
        ("Algorithm-Generated", Split::AlgorithmGenerated),
    ];

    let mut scan = FailingScan {
        eligible: Vec::new(),
        total: 0,
        without_gold: 0,
    };
    for (subdir, split) in SPLITS {
        let split_dir = dir.join(subdir);
        if !split_dir.is_dir() {
            continue;
        }
        for path in json_files(&split_dir)? {
            let converted =
                whowhen::convert_file(&path, split).map_err(|source| BuildError::Convert {
                    path: path.clone(),
                    source,
                })?;
            scan.total += 1;
            // A pair's fork step is the failing log's annotated gold; a log without a usable one
            // cannot anchor a Mode A′ pair, so it is counted as coverage, not paired.
            match converted.gold.gold_step {
                GoldStep::Valid(idx) if !converted.gold.question_id.is_empty() => {
                    scan.eligible.push(Failing {
                        stem: converted.run.id.clone(),
                        run: converted.run,
                        task_id: converted.gold.question_id,
                        gold_step: idx,
                    });
                }
                _ => scan.without_gold += 1,
            }
        }
    }
    Ok(scan)
}

/// Write every built pair as the `a_NN`/`b_NN`/`pair_NN` triple the seam reads. Runs serialize as
/// canonical trace JSON (they round-trip through [`amberfork_ingest::load_file`]); the manifest
/// carries `cross_system: true` and a provenance block naming the two sources and the shared task.
fn write_set(out_dir: &Path, outcome: &BuildOutcome) -> Result<(), BuildError> {
    std::fs::create_dir_all(out_dir).map_err(|source| BuildError::Dir {
        dir: out_dir.to_path_buf(),
        source,
    })?;
    for pair in &outcome.pairs {
        let failing_file = format!("a_{:02}.json", pair.index);
        let reference_file = format!("b_{:02}.json", pair.index);
        let manifest_file = format!("pair_{:02}.json", pair.index);

        write_json(out_dir, &failing_file, &pair.failing)?;
        write_json(out_dir, &reference_file, &pair.reference)?;
        let manifest = Manifest {
            failing: &failing_file,
            reference: &reference_file,
            gold_step: pair.gold_step,
            cross_system: true,
            meta: ManifestMeta {
                task_id: &pair.task_id,
                tape: &pair.reference_stem,
                whowhen: &pair.failing_stem,
                provenance: PROVENANCE,
            },
        };
        write_json(out_dir, &manifest_file, &manifest)?;
    }
    Ok(())
}

/// Serialize `value` as pretty JSON (a trailing newline for git-friendliness) into `dir/name`.
fn write_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<(), BuildError> {
    let path = dir.join(name);
    let mut json = serde_json::to_string_pretty(value).map_err(|source| BuildError::Encode {
        path: path.clone(),
        source,
    })?;
    json.push('\n');
    std::fs::write(&path, json).map_err(|source| BuildError::Write { path, source })
}

/// The disclosure carried in every generated manifest's `meta` block — the honest statement of
/// what the pair is, so a generated set is self-describing on disk.
const PROVENANCE: &str =
    "cross-system Mode A′ pair: TapeAgents reference vs Who&When failing, matched on GAIA task_id";

/// The on-disk pair manifest, in the shape [`crate::pairs::load_pairs`] reads. This is the
/// *writer's* view of the pair contract (the reader has its own, meta-agnostic mirror); the
/// end-to-end build test bridges the two, so a field-name drift is a red test.
#[derive(Serialize)]
struct Manifest<'a> {
    failing: &'a str,
    reference: &'a str,
    gold_step: usize,
    cross_system: bool,
    meta: ManifestMeta<'a>,
}

/// Human-facing provenance for a generated pair. The reader ignores it; it exists so an
/// uncommitted generated set can be traced back to the tape and log it was built from.
#[derive(Serialize)]
struct ManifestMeta<'a> {
    task_id: &'a str,
    tape: &'a str,
    whowhen: &'a str,
    provenance: &'static str,
}

/// The `*.json` files in `dir`, sorted so a build is deterministic regardless of directory
/// iteration order.
fn json_files(dir: &Path) -> Result<Vec<PathBuf>, BuildError> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| BuildError::Dir {
            dir: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    Ok(paths)
}

/// The file stem of `path` as an owned string, or `"unknown"` when it has none.
fn stem_of(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Everything that can go wrong building a pair set. Unlike a per-pair exclusion, each of these
/// stops the build: the operator's raw inputs or output location need fixing.
#[derive(Debug)]
pub enum BuildError {
    /// A source or output directory could not be read or created.
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    /// A source file could not be converted by its adapter.
    Convert { path: PathBuf, source: IngestError },
    /// A run or manifest could not be encoded as JSON.
    Encode {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// An output file could not be written.
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir { dir, source } => write!(f, "directory {}: {source}", dir.display()),
            Self::Convert { path, source } => {
                write!(f, "convert {}: {source}", path.display())
            }
            Self::Encode { path, source } => write!(f, "encode {}: {source}", path.display()),
            Self::Write { path, source } => write!(f, "write {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dir { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Convert { source, .. } => Some(source),
            Self::Encode { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Outcome, SchemaVersion};

    fn run(id: &str) -> Run {
        Run {
            schema_version: SchemaVersion::current(),
            id: id.to_string(),
            task: None,
            outcome: Some(Outcome::Fail),
            steps: Vec::new(),
            edges: None,
        }
    }

    fn reference(stem: &str, task_id: Option<&str>, is_success: bool) -> Reference {
        Reference {
            stem: stem.to_string(),
            run: run(&format!("tape_{stem}")),
            task_id: task_id.map(str::to_string),
            is_success,
        }
    }

    fn failing(stem: &str, task_id: &str, gold_step: usize) -> Failing {
        Failing {
            stem: stem.to_string(),
            run: run(stem),
            task_id: task_id.to_string(),
            gold_step,
        }
    }

    #[test]
    fn matches_on_shared_task_and_carries_the_gold() {
        let outcome = match_pairs(
            vec![reference("win", Some("gaia-1"), true)],
            vec![failing("whowhen_hand_4", "gaia-1", 3)],
        );
        assert!(outcome.drops.is_empty());
        assert_eq!(outcome.pairs.len(), 1);
        let pair = &outcome.pairs[0];
        assert_eq!(pair.index, 0);
        assert_eq!(pair.task_id, "gaia-1");
        assert_eq!(pair.gold_step, 3, "the gold is the failing log's fork step");
        assert_eq!(pair.reference_stem, "win");
        assert_eq!(pair.failing_stem, "whowhen_hand_4");
        assert_eq!(pair.reference.id, "tape_win");
    }

    #[test]
    fn an_unsuccessful_tape_never_serves_as_a_reference() {
        let outcome = match_pairs(
            vec![reference("lose", Some("gaia-1"), false)],
            vec![failing("whowhen_hand_4", "gaia-1", 3)],
        );
        assert!(outcome.pairs.is_empty());
        assert_eq!(outcome.drops.len(), 1);
        assert_eq!(outcome.drops[0].reason, DropReason::Unsuccessful);
    }

    #[test]
    fn a_tape_without_a_task_id_cannot_be_matched() {
        let outcome = match_pairs(
            vec![reference("win", None, true)],
            vec![failing("whowhen_hand_4", "gaia-1", 3)],
        );
        assert!(outcome.pairs.is_empty());
        assert_eq!(outcome.drops[0].reason, DropReason::MissingTaskId);
    }

    #[test]
    fn a_successful_tape_with_no_failing_match_is_a_counted_drop() {
        let outcome = match_pairs(
            vec![reference("win", Some("gaia-9"), true)],
            vec![failing("whowhen_hand_4", "gaia-1", 3)],
        );
        assert!(outcome.pairs.is_empty());
        assert_eq!(outcome.drops[0].reason, DropReason::NoFailingMatch);
    }

    #[test]
    fn pairing_is_deterministic_under_shuffled_input() {
        // Two matchable references in reversed input order plus an unmatched one interleaved;
        // the output must be sorted by reference stem and numbered in that order.
        let build = || {
            match_pairs(
                vec![
                    reference("beta", Some("gaia-2"), true),
                    reference("alpha", Some("gaia-1"), true),
                    reference("zeta", Some("gaia-none"), true),
                ],
                vec![
                    failing("whowhen_algo_7", "gaia-2", 5),
                    failing("whowhen_hand_1", "gaia-1", 2),
                ],
            )
        };
        let outcome = build();
        let names: Vec<_> = outcome
            .pairs
            .iter()
            .map(|p| (p.index, p.reference_stem.as_str()))
            .collect();
        assert_eq!(names, vec![(0, "alpha"), (1, "beta")]);
        assert_eq!(outcome.drops.len(), 1, "zeta has no failing match");
        assert_eq!(outcome.drops[0].stem, "zeta");
    }

    #[test]
    fn a_task_id_collision_resolves_to_the_lowest_failing_stem() {
        let outcome = match_pairs(
            vec![reference("win", Some("gaia-1"), true)],
            vec![
                failing("whowhen_hand_9", "gaia-1", 8),
                failing("whowhen_hand_2", "gaia-1", 2),
            ],
        );
        assert_eq!(outcome.pairs.len(), 1);
        assert_eq!(outcome.pairs[0].failing_stem, "whowhen_hand_2");
        assert_eq!(outcome.pairs[0].gold_step, 2);
    }
}
