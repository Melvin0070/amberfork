//! Constructing a same-agent natural pair set from TRAIL failing traces + HAL passing references
//! (issue #41 S4c) — the `build.rs` analogue for the second natural-pair source.
//!
//! A pair joins a **failing** TRAIL trace (a smolagents Open Deep Research run TRAIL's annotators
//! found ≥1 decisive error in) to a **reference** HAL run that solved the *same* GAIA task with the
//! *same* agent scaffolding, differing only in backing model (notebook 046/047). Unlike Mode A′
//! ([`crate::build`]), both sides share the ODR tool loop — that is what makes this pair natural
//! rather than cross-system.
//!
//! Two boundaries, the same "exclusions are data" ethos as [`crate::build`] and [`crate::pairs`]:
//! - **A TRAIL trace earns failing status; it is not assumed to have it.** Only a trace whose gold
//!   resolves to at least one real step ([`amberfork_ingest::trail::TrailGold::resolve`]) can anchor
//!   a pair — the gold step is the *earliest* resolved step (matches the single-`gold_step` contract
//!   [`crate::pairs::load_pairs`]/[`crate::score`] already score against; notebook 044 named this the
//!   leading pre-registered candidate). A trace with no task id, or whose gold resolves to nothing,
//!   is a counted [`FailingDrop`], never a silent skip.
//! - **A HAL run earns reference status only when it passed.** [`amberfork_ingest::hal::HalMeta::passed`]
//!   gates it, exactly as [`amberfork_ingest::tape::TapeMeta::is_success`] gates a Mode A′ tape.
//!
//! **One reference per failing trace.** A task can have several passing HAL models; picking all of
//! them would reuse the same failing trace across multiple pairs, breaking the independence the
//! Wilson interval in [`crate::score`] assumes. So when more than one model passed, the **lowest
//! model name** wins — a deterministic tie-break with no other meaning, mirroring
//! [`crate::build::match_pairs`]'s lowest-stem rule. Per-model risk (cross-model gold quality, the
//! honest caveat notebook 046/047 flags) is a later optional arm, not this slice's job.

use amberfork_ingest::hal::HalMeta;
use amberfork_ingest::trail::TrailMeta;
use amberfork_ingest::{IngestError, hal, trail};
use amberfork_model::Run;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// A reference-side candidate: one GAIA task from one decrypted HAL model dump.
pub struct Reference {
    /// Source identifier (`<dump-stem>/<gaia_task_id>`) — the pair's provenance.
    pub stem: String,
    pub run: Run,
    pub task_id: String,
    pub model: Option<String>,
    pub passed: bool,
}

/// A failing-side candidate: a converted TRAIL trace with its earliest resolved gold step.
pub struct Failing {
    /// Source identifier (the TRAIL `trace_id`) — unique across the trace set.
    pub stem: String,
    pub run: Run,
    pub task_id: String,
    /// The earliest resolved gold step (see module docs for why "earliest").
    pub gold_step: usize,
}

/// One constructed natural pair: a failing trace, its chosen HAL reference, and the gold step.
pub struct BuiltPair {
    pub index: usize,
    pub task_id: String,
    pub failing_stem: String,
    pub reference_stem: String,
    pub reference_model: Option<String>,
    pub failing: Run,
    pub reference: Run,
    pub gold_step: usize,
}

/// Why a failing candidate did not become a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailingDropReason {
    /// No HAL reference (any model) passed this trace's GAIA task.
    NoPassingReference,
}

impl fmt::Display for FailingDropReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no passing HAL reference shares this trace's task_id")
    }
}

/// A dropped failing candidate: the trace stem and why it was not paired.
pub struct FailingDrop {
    pub stem: String,
    pub reason: FailingDropReason,
}

/// The result of matching: the pairs built, and every failing candidate left unpaired.
pub struct BuildOutcome {
    pub pairs: Vec<BuiltPair>,
    pub drops: Vec<FailingDrop>,
}

/// Match failing candidates to reference candidates on their shared GAIA `task_id`, one reference
/// per failing trace (lowest model name wins a multi-model tie — see module docs).
///
/// Deterministic regardless of input order: both sides are sorted by stem before matching,
/// failing traces are visited in stem order, and pairs are numbered sequentially. Every failing
/// trace with no passing reference appears in `drops`.
#[must_use]
pub fn match_pairs(mut failings: Vec<Failing>, mut references: Vec<Reference>) -> BuildOutcome {
    failings.sort_by(|a, b| a.stem.cmp(&b.stem));
    references.sort_by(|a, b| a.stem.cmp(&b.stem));

    // Lowest model name (falling back to stem when a dump names no model) wins a multi-model
    // collision on one task id — deterministic, no other meaning.
    let mut by_task: BTreeMap<&str, &Reference> = BTreeMap::new();
    for reference in &references {
        if !reference.passed {
            continue;
        }
        by_task
            .entry(reference.task_id.as_str())
            .and_modify(|current| {
                if tie_break_key(reference) < tie_break_key(current) {
                    *current = reference;
                }
            })
            .or_insert(reference);
    }

    let mut pairs = Vec::new();
    let mut drops = Vec::new();
    for failing in &failings {
        let Some(reference) = by_task.get(failing.task_id.as_str()) else {
            drops.push(FailingDrop {
                stem: failing.stem.clone(),
                reason: FailingDropReason::NoPassingReference,
            });
            continue;
        };
        pairs.push(BuiltPair {
            index: pairs.len(),
            task_id: failing.task_id.clone(),
            failing_stem: failing.stem.clone(),
            reference_stem: reference.stem.clone(),
            reference_model: reference.model.clone(),
            failing: failing.run.clone(),
            reference: reference.run.clone(),
            gold_step: failing.gold_step,
        });
    }
    BuildOutcome { pairs, drops }
}

/// The tie-break key for a multi-model collision: the model name when present, else the stem —
/// so a dump that names no model still resolves deterministically rather than panicking.
fn tie_break_key(reference: &Reference) -> &str {
    reference
        .model
        .as_deref()
        .unwrap_or(reference.stem.as_str())
}

/// What `build-trail-pairs` did — printed to the operator so a thin overlap is loud, not silent.
pub struct BuildStats {
    pub pairs: usize,
    pub drops: Vec<FailingDrop>,
    pub traces_read: usize,
    pub traces_without_gold: usize,
    pub hal_dumps_read: usize,
    pub hal_runs_read: usize,
}

/// Build a natural pair set from a directory of raw TRAIL traces + gold and a directory of
/// decrypted HAL dumps (one file per backing model, from `hal-fetch` → `hal-decrypt`), writing
/// the triples into `out_dir`.
///
/// `gold_dir` is matched to `traces_dir` by shared `<trace-id>.json` basename (045's pinned
/// layout); a trace with no matching gold file is not a `BuildError` — TRAIL ships 3 clean
/// (0-error) traces with no annotation file, which read the same as an unresolvable gold: a
/// counted [`FailingDrop`]... except that never happens, because a trace with no gold file can
/// never anchor a pair either way, so it is filtered before the drop tally, same as a trace whose
/// gold resolves to nothing.
///
/// # Errors
/// [`BuildError`] if a source directory cannot be read, a source file will not parse, or an
/// output file cannot be encoded or written. Building zero pairs is not an error.
pub fn build_pairs(
    traces_dir: &Path,
    gold_dir: &Path,
    hal_dumps_dir: &Path,
    out_dir: &Path,
) -> Result<BuildStats, BuildError> {
    let (failings, traces_read, traces_without_gold) = read_failings(traces_dir, gold_dir)?;
    let (references, hal_dumps_read, hal_runs_read) = read_references(hal_dumps_dir)?;

    let outcome = match_pairs(failings, references);
    write_set(out_dir, &outcome)?;

    Ok(BuildStats {
        pairs: outcome.pairs.len(),
        drops: outcome.drops,
        traces_read,
        traces_without_gold,
        hal_dumps_read,
        hal_runs_read,
    })
}

/// Read every TRAIL trace in `traces_dir`, resolve its gold against the matching file in
/// `gold_dir` (same basename), and keep only those with a usable earliest gold step and a task
/// id. Returns the eligible failing candidates plus (traces considered, traces dropped for lack
/// of a usable gold step or task id).
fn read_failings(
    traces_dir: &Path,
    gold_dir: &Path,
) -> Result<(Vec<Failing>, usize, usize), BuildError> {
    let mut eligible = Vec::new();
    let mut total = 0;
    let mut without_gold = 0;
    for path in json_files(traces_dir)? {
        let text = std::fs::read_to_string(&path).map_err(|source| BuildError::Read {
            path: path.clone(),
            source,
        })?;
        let converted = trail::convert_str(&text).map_err(|source| BuildError::Convert {
            path: path.clone(),
            source,
        })?;
        total += 1;

        let TrailMeta {
            gaia_task_id: Some(task_id),
        } = converted.meta
        else {
            without_gold += 1;
            continue;
        };

        let gold_path = gold_dir.join(
            path.file_name()
                .expect("json_files yields files, always named"),
        );
        let Ok(gold_text) = std::fs::read_to_string(&gold_path) else {
            without_gold += 1;
            continue;
        };
        // A gold file that exists but fails to parse (TRAIL ships one upstream file with a
        // trailing comma even Python's own `json.load` rejects) reads the same as an
        // unresolvable gold: a counted exclusion, never a crash (BENCHMARK.md's
        // exclusions-as-data rule; the fetch integrity test documents this exact file).
        let Ok(gold) = trail::annotations_from_json_str(&gold_text) else {
            without_gold += 1;
            continue;
        };
        // TRAIL's Patronus SDK faithfully logs the smolagents harness's own orchestration spans
        // (`main`, `get_examples_to_answer`, `create_agent_hierarchy`, …) ahead of the first real
        // model/tool content; HAL's Weave export never captures that layer at all, so the
        // reference side has no counterpart for it (measured: 69/69 real traces, S5). Trimmed here
        // — not in the general `trail` adapter, which stays a faithful full-tree export for every
        // other consumer — mirroring the boundary `hal`'s own adapter already draws for its
        // `litellm.completion` wrapper: drop the content-free bookkeeping prefix, keep the
        // content-bearing steps. Gold is resolved AFTER trimming so `GoldStep::step` already reads
        // in the trimmed run's index space; no manual offset arithmetic.
        let run = trim_leading_content_free(converted.run);
        let earliest = gold.resolve(&run).into_iter().filter_map(|g| g.step).min();
        let Some(gold_step) = earliest else {
            without_gold += 1;
            continue;
        };

        eligible.push(Failing {
            stem: run.id.clone(),
            run,
            task_id,
            gold_step,
        });
    }
    Ok((eligible, total, without_gold))
}

/// Drop a TRAIL run's leading content-free steps (harness/orchestration spans with neither input
/// nor output captured — see [`read_failings`]'s call site for why), re-indexing what remains so
/// `idx` stays a contiguous `0..len` and `parent_idx` stays internally consistent: a kept step
/// whose parent was trimmed becomes a root (`None`); a kept step whose parent survived is
/// remapped to the parent's new index. A run with no leading content-free steps is returned
/// unchanged (the common case for a task that starts directly with model content).
fn trim_leading_content_free(mut run: Run) -> Run {
    let n = run
        .steps
        .iter()
        .take_while(|step| step.inputs.is_none() && step.outputs.is_none())
        .count();
    if n == 0 {
        return run;
    }
    run.steps.drain(..n);
    for step in &mut run.steps {
        step.idx -= n;
        step.parent_idx = step.parent_idx.filter(|&p| p >= n).map(|p| p - n);
    }
    run
}

/// Read every decrypted HAL dump in `dir` and convert each into its per-task [`Reference`]
/// candidates. Returns the candidates plus (dumps read, total runs across all dumps).
fn read_references(dir: &Path) -> Result<(Vec<Reference>, usize, usize), BuildError> {
    let mut references = Vec::new();
    let mut runs_read = 0;
    let paths = json_files(dir)?;
    for path in &paths {
        let text = std::fs::read_to_string(path).map_err(|source| BuildError::Read {
            path: path.clone(),
            source,
        })?;
        let converted = hal::convert_str(&text).map_err(|source| BuildError::Convert {
            path: path.clone(),
            source,
        })?;
        let dump_stem = stem_of(path);
        for c in converted {
            runs_read += 1;
            let HalMeta {
                gaia_task_id,
                model,
                passed,
            } = c.meta;
            references.push(Reference {
                stem: format!("{dump_stem}/{gaia_task_id}"),
                run: c.run,
                task_id: gaia_task_id,
                model,
                passed,
            });
        }
    }
    Ok((references, paths.len(), runs_read))
}

/// Write every built pair as the `a_NN`/`b_NN`/`pair_NN` triple [`crate::pairs::load_pairs`]
/// reads. `cross_system: false` — both sides share the ODR scaffolding (notebook 046/047), unlike
/// Mode A′.
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
            cross_system: false,
            meta: ManifestMeta {
                task_id: &pair.task_id,
                trail_trace: &pair.failing_stem,
                hal_reference: &pair.reference_stem,
                hal_model: pair.reference_model.as_deref(),
                provenance: PROVENANCE,
            },
        };
        write_json(out_dir, &manifest_file, &manifest)?;
    }
    Ok(())
}

fn write_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<(), BuildError> {
    let path = dir.join(name);
    let mut json = serde_json::to_string_pretty(value).map_err(|source| BuildError::Encode {
        path: path.clone(),
        source,
    })?;
    json.push('\n');
    std::fs::write(&path, json).map_err(|source| BuildError::Write { path, source })
}

const PROVENANCE: &str = "same-agent natural pair: TRAIL failing trace vs HAL passing reference, \
     matched on GAIA task_id, same Open Deep Research scaffolding, differing backing model";

#[derive(Serialize)]
struct Manifest<'a> {
    failing: &'a str,
    reference: &'a str,
    gold_step: usize,
    cross_system: bool,
    meta: ManifestMeta<'a>,
}

#[derive(Serialize)]
struct ManifestMeta<'a> {
    task_id: &'a str,
    trail_trace: &'a str,
    hal_reference: &'a str,
    hal_model: Option<&'a str>,
    provenance: &'static str,
}

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

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Everything that can go wrong building a pair set. Each stops the build: the operator's raw
/// inputs or output location need fixing.
#[derive(Debug)]
pub enum BuildError {
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Convert {
        path: PathBuf,
        source: IngestError,
    },
    Encode {
        path: PathBuf,
        source: serde_json::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir { dir, source } => write!(f, "directory {}: {source}", dir.display()),
            Self::Read { path, source } => write!(f, "read {}: {source}", path.display()),
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
            Self::Dir { source, .. } | Self::Read { source, .. } | Self::Write { source, .. } => {
                Some(source)
            }
            Self::Convert { source, .. } => Some(source),
            Self::Encode { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Outcome, Payload, SchemaVersion, Step, StepKind};

    /// A minimal step for [`trim_leading_content_free`] tests: `content` toggles whether it counts
    /// as content-bearing (`Some(text)` sets `outputs`, `None` leaves both `inputs`/`outputs` unset).
    fn step(idx: usize, content: Option<&str>, parent_idx: Option<usize>) -> Step {
        Step {
            idx,
            kind: StepKind::Other,
            name: format!("step{idx}"),
            inputs: None,
            outputs: content.map(|text| Payload::Text(text.to_string())),
            attrs: serde_json::Map::new(),
            t_start: None,
            t_end: None,
            parent_idx,
        }
    }

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

    fn failing(stem: &str, task_id: &str, gold_step: usize) -> Failing {
        Failing {
            stem: stem.to_string(),
            run: run(stem),
            task_id: task_id.to_string(),
            gold_step,
        }
    }

    fn reference(stem: &str, task_id: &str, model: Option<&str>, passed: bool) -> Reference {
        Reference {
            stem: stem.to_string(),
            run: run(&format!("hal_{stem}")),
            task_id: task_id.to_string(),
            model: model.map(str::to_string),
            passed,
        }
    }

    #[test]
    fn matches_on_shared_task_and_carries_the_earliest_gold() {
        let outcome = match_pairs(
            vec![failing("trace_a", "gaia-1", 6)],
            vec![reference("gpt41/gaia-1", "gaia-1", Some("gpt-4.1"), true)],
        );
        assert!(outcome.drops.is_empty());
        assert_eq!(outcome.pairs.len(), 1);
        let pair = &outcome.pairs[0];
        assert_eq!(pair.index, 0);
        assert_eq!(pair.task_id, "gaia-1");
        assert_eq!(pair.gold_step, 6);
        assert_eq!(pair.failing_stem, "trace_a");
        assert_eq!(pair.reference_stem, "gpt41/gaia-1");
        assert_eq!(pair.reference_model.as_deref(), Some("gpt-4.1"));
        assert_eq!(pair.reference.id, "hal_gpt41/gaia-1");
    }

    #[test]
    fn a_failing_run_never_serves_as_a_reference() {
        let outcome = match_pairs(
            vec![failing("trace_a", "gaia-1", 6)],
            vec![reference("gpt41/gaia-1", "gaia-1", Some("gpt-4.1"), false)],
        );
        assert!(outcome.pairs.is_empty());
        assert_eq!(outcome.drops.len(), 1);
        assert_eq!(outcome.drops[0].stem, "trace_a");
        assert_eq!(
            outcome.drops[0].reason,
            FailingDropReason::NoPassingReference
        );
    }

    #[test]
    fn a_failing_trace_with_no_passing_reference_is_a_counted_drop() {
        let outcome = match_pairs(
            vec![failing("trace_a", "gaia-1", 6)],
            vec![reference("gpt41/gaia-9", "gaia-9", Some("gpt-4.1"), true)],
        );
        assert!(outcome.pairs.is_empty());
        assert_eq!(
            outcome.drops[0].reason,
            FailingDropReason::NoPassingReference
        );
    }

    #[test]
    fn a_multi_model_collision_resolves_to_the_lowest_model_name() {
        let outcome = match_pairs(
            vec![failing("trace_a", "gaia-1", 6)],
            vec![
                reference("gemini/gaia-1", "gaia-1", Some("gemini-2.5"), true),
                reference("gpt41/gaia-1", "gaia-1", Some("gpt-4.1"), true),
                reference("claude/gaia-1", "gaia-1", Some("claude-3.5"), true),
            ],
        );
        assert_eq!(outcome.pairs.len(), 1);
        assert_eq!(
            outcome.pairs[0].reference_model.as_deref(),
            Some("claude-3.5"),
            "claude-3.5 sorts lowest of the three model names"
        );
    }

    #[test]
    fn a_failing_reference_never_wins_a_collision_over_a_passing_one() {
        let outcome = match_pairs(
            vec![failing("trace_a", "gaia-1", 6)],
            vec![
                reference("aaa/gaia-1", "gaia-1", Some("aaa-model"), false),
                reference("zzz/gaia-1", "gaia-1", Some("zzz-model"), true),
            ],
        );
        assert_eq!(outcome.pairs.len(), 1);
        assert_eq!(
            outcome.pairs[0].reference_model.as_deref(),
            Some("zzz-model"),
            "the only passing candidate wins even though its name sorts higher"
        );
    }

    #[test]
    fn pairing_is_deterministic_under_shuffled_input() {
        let build = || {
            match_pairs(
                vec![
                    failing("trace_z", "gaia-none", 0),
                    failing("trace_b", "gaia-2", 5),
                    failing("trace_a", "gaia-1", 2),
                ],
                vec![
                    reference("r2/gaia-2", "gaia-2", Some("m2"), true),
                    reference("r1/gaia-1", "gaia-1", Some("m1"), true),
                ],
            )
        };
        let outcome = build();
        let names: Vec<_> = outcome
            .pairs
            .iter()
            .map(|p| (p.index, p.failing_stem.as_str()))
            .collect();
        assert_eq!(names, vec![(0, "trace_a"), (1, "trace_b")]);
        assert_eq!(outcome.drops.len(), 1);
        assert_eq!(outcome.drops[0].stem, "trace_z");
    }

    #[test]
    fn a_reference_with_no_model_name_falls_back_to_its_stem_for_the_tie_break() {
        let outcome = match_pairs(
            vec![failing("trace_a", "gaia-1", 6)],
            vec![
                reference("bbb/gaia-1", "gaia-1", None, true),
                reference("aaa/gaia-1", "gaia-1", None, true),
            ],
        );
        assert_eq!(outcome.pairs.len(), 1);
        assert_eq!(outcome.pairs[0].reference_stem, "aaa/gaia-1");
    }

    #[test]
    fn trim_leading_content_free_drops_only_the_content_free_prefix() {
        let mut r = run("trace");
        // Two content-free harness steps, then real content, then a content-free step in the
        // MIDDLE (must survive — trimming is a prefix operation, not a filter).
        r.steps = vec![
            step(0, None, None),
            step(1, None, Some(0)),
            step(2, Some("real"), Some(1)),
            step(3, None, Some(2)),
        ];
        let trimmed = trim_leading_content_free(r);
        assert_eq!(trimmed.steps.len(), 2, "only the leading pair is dropped");
        assert_eq!(trimmed.steps[0].idx, 0);
        assert_eq!(trimmed.steps[0].outputs, Some(Payload::Text("real".into())));
        assert_eq!(
            trimmed.steps[0].parent_idx, None,
            "its parent (old idx 1) was trimmed, so it becomes a root"
        );
        assert_eq!(trimmed.steps[1].idx, 1);
        assert_eq!(
            trimmed.steps[1].parent_idx,
            Some(0),
            "its parent (old idx 2) survived and is remapped to the new index"
        );
    }

    #[test]
    fn trim_leading_content_free_is_a_no_op_when_the_first_step_has_content() {
        let mut r = run("trace");
        r.steps = vec![step(0, Some("real"), None), step(1, None, Some(0))];
        let trimmed = trim_leading_content_free(r);
        assert_eq!(trimmed.steps.len(), 2, "nothing to trim");
        assert_eq!(trimmed.steps[1].parent_idx, Some(0));
    }

    #[test]
    fn trim_leading_content_free_on_an_all_content_free_run_drops_everything() {
        let mut r = run("trace");
        r.steps = vec![step(0, None, None), step(1, None, Some(0))];
        let trimmed = trim_leading_content_free(r);
        assert!(trimmed.steps.is_empty());
    }
}
