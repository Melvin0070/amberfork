//! Constructing pairs for #49's real-agent perturbation protocol (notebook 074) — the
//! `build_trail.rs` analogue for the third natural-pair source, simpler than either of the first
//! two: pairing is not *matched* on a shared key, it is already explicit in the recording
//! session manifest `spike/record_perturbation_sessions.py` writes. Each task contributes one
//! reference (a clean recording) and up to five perturbed recordings; every perturbed session
//! pairs with its own task's reference.
//!
//! Gold is read, never recomputed: `perturbation_agent.py` writes `gold_step` into each session's
//! summary at record time (the exchange index whose request first carries the perturbed tool
//! result — notebook 074's gold rule), so this builder's only job is to read cassette + summary
//! pairs off disk and emit the same `a_NN`/`b_NN`/`pair_NN` triple every other protocol in this
//! repo produces, through the existing, unmodified [`amberfork_record::normalize_str`].

use amberfork_model::Run;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// One session's summary, written by `perturbation_agent.py`.
#[derive(Deserialize)]
struct SessionSummary {
    perturbation_type: Option<String>,
    designated_tool: String,
    gold_step: Option<usize>,
}

/// The recording orchestrator's manifest: which sessions exist per task, and which were
/// excluded after exhausting the retry budget (notebook 074's retry rule — a mechanical
/// precondition, never an outcome filter).
#[derive(Deserialize)]
struct SessionManifest {
    tasks: BTreeMap<String, TaskSessions>,
}

#[derive(Deserialize)]
struct TaskSessions {
    reference: Option<String>,
    perturbed: Vec<String>,
    excluded: Vec<String>,
}

/// One constructed pair: a perturbed recording against its task's clean reference.
pub struct BuiltPair {
    pub index: usize,
    pub task_id: String,
    pub session_id: String,
    pub perturbation_type: String,
    pub designated_tool: String,
    pub reference: Run,
    pub failing: Run,
    pub gold_step: usize,
}

/// What `build-perturbation-pairs` did — printed to the operator so a thin set is loud, not
/// silent (the same "exclusions are data" ethos as [`crate::build`] and [`crate::build_trail`]).
pub struct BuildStats {
    pub pairs: usize,
    pub tasks_without_reference: Vec<String>,
    pub excluded_sessions: usize,
}

/// Build the perturbation pair set from a directory of recorded sessions (see
/// `spike/record_perturbation_sessions.py`'s layout), writing the triples into `out_dir`.
///
/// # Errors
/// [`BuildError`] if the manifest or a referenced file cannot be read or parsed, or the output
/// cannot be written. A task with no reference is a counted exclusion, not a hard error — the
/// rest of the set still builds.
pub fn build_pairs(sessions_dir: &Path, out_dir: &Path) -> Result<BuildStats, BuildError> {
    let manifest_path = sessions_dir.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path).map_err(|source| BuildError::Read {
        path: manifest_path.clone(),
        source,
    })?;
    let manifest: SessionManifest =
        serde_json::from_str(&text).map_err(|source| BuildError::Parse {
            path: manifest_path.clone(),
            source,
        })?;

    let mut pairs = Vec::new();
    let mut tasks_without_reference = Vec::new();
    let mut excluded_sessions = 0;

    for (task_id, sessions) in &manifest.tasks {
        excluded_sessions += sessions.excluded.len();
        let Some(reference_id) = &sessions.reference else {
            tasks_without_reference.push(task_id.clone());
            continue;
        };
        let task_dir = sessions_dir.join(task_id);
        let reference_run = load_cassette(&task_dir, reference_id)?;

        for session_id in &sessions.perturbed {
            let failing_run = load_cassette(&task_dir, session_id)?;
            let summary = load_summary(&task_dir, session_id)?;
            let Some(gold_step) = summary.gold_step else {
                return Err(BuildError::MissingGold {
                    task: task_id.clone(),
                    session: session_id.clone(),
                });
            };
            pairs.push(BuiltPair {
                index: pairs.len(),
                task_id: task_id.clone(),
                session_id: session_id.clone(),
                perturbation_type: summary
                    .perturbation_type
                    .unwrap_or_else(|| "unknown".to_string()),
                designated_tool: summary.designated_tool,
                reference: reference_run.clone(),
                failing: failing_run,
                gold_step,
            });
        }
    }

    write_set(out_dir, &pairs)?;

    Ok(BuildStats {
        pairs: pairs.len(),
        tasks_without_reference,
        excluded_sessions,
    })
}

fn load_cassette(task_dir: &Path, session_id: &str) -> Result<Run, BuildError> {
    let path = task_dir.join(format!("{session_id}.cassette.json"));
    let text = std::fs::read_to_string(&path).map_err(|source| BuildError::Read {
        path: path.clone(),
        source,
    })?;
    amberfork_record::normalize_str(&text).map_err(|source| BuildError::Normalize { path, source })
}

fn load_summary(task_dir: &Path, session_id: &str) -> Result<SessionSummary, BuildError> {
    let path = task_dir.join(format!("{session_id}.summary.json"));
    let text = std::fs::read_to_string(&path).map_err(|source| BuildError::Read {
        path: path.clone(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| BuildError::Parse { path, source })
}

/// Write every built pair as the `a_NN`/`b_NN`/`pair_NN` triple [`crate::pairs::load_pairs`]
/// reads. `cross_system: false` — both sides are the identical agent harness (notebook 074).
fn write_set(out_dir: &Path, pairs: &[BuiltPair]) -> Result<(), BuildError> {
    std::fs::create_dir_all(out_dir).map_err(|source| BuildError::Dir {
        dir: out_dir.to_path_buf(),
        source,
    })?;
    for pair in pairs {
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
                session_id: &pair.session_id,
                perturbation_type: &pair.perturbation_type,
                designated_tool: &pair.designated_tool,
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

const PROVENANCE: &str = "real-agent perturbation pair: a recorded tool-using agent session \
    (qwen3:8b via `amberfork record`) against its task's clean reference recording, gold from the \
    harness's own perturbation bookkeeping — notebook 074";

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
    session_id: &'a str,
    perturbation_type: &'a str,
    designated_tool: &'a str,
    provenance: &'static str,
}

/// Everything that can go wrong building a pair set. Each stops the build: the operator's raw
/// inputs or output location need fixing.
#[derive(Debug)]
pub enum BuildError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Normalize {
        path: PathBuf,
        source: serde_json::Error,
    },
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    Encode {
        path: PathBuf,
        source: serde_json::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A perturbed session's summary carries no `gold_step` — should never happen for a
    /// successful (`exit_code: 0`) perturbed recording; see `perturbation_agent.py`'s contract.
    MissingGold { task: String, session: String },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "read {}: {source}", path.display()),
            Self::Parse { path, source } => write!(f, "parse {}: {source}", path.display()),
            Self::Normalize { path, source } => {
                write!(f, "normalize cassette {}: {source}", path.display())
            }
            Self::Dir { dir, source } => write!(f, "directory {}: {source}", dir.display()),
            Self::Encode { path, source } => write!(f, "encode {}: {source}", path.display()),
            Self::Write { path, source } => write!(f, "write {}: {source}", path.display()),
            Self::MissingGold { task, session } => write!(
                f,
                "session {session} (task {task}) has no gold_step in its summary"
            ),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Dir { source, .. } | Self::Write { source, .. } => {
                Some(source)
            }
            Self::Parse { source, .. } | Self::Encode { source, .. } => Some(source),
            Self::Normalize { source, .. } => Some(source),
            Self::MissingGold { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn cassette_json(id: &str, exchanges: usize) -> String {
        let exchanges_json: Vec<String> = (0..exchanges)
            .map(|i| {
                format!(
                    r#"{{"idx":{i},"request":{{"method":"POST","path":"/api/chat",
                    "body":{{"model":"qwen3:8b","messages":[]}}}},
                    "response":{{"status":200,"body":{{"message":{{"content":"turn {i}"}}}}}}}}"#
                )
            })
            .collect();
        format!(
            r#"{{"cassette_version":"0.1","id":"{id}","exchanges":[{}]}}"#,
            exchanges_json.join(",")
        )
    }

    fn write_session(dir: &Path, session_id: &str, exchanges: usize, summary: Option<&str>) {
        fs::write(
            dir.join(format!("{session_id}.cassette.json")),
            cassette_json(session_id, exchanges),
        )
        .unwrap();
        if let Some(summary) = summary {
            fs::write(dir.join(format!("{session_id}.summary.json")), summary).unwrap();
        }
    }

    #[test]
    fn builds_one_pair_per_perturbed_session_against_its_tasks_reference() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        let task_dir = sessions_dir.join("refund_window");
        fs::create_dir_all(&task_dir).unwrap();

        write_session(&task_dir, "reference", 2, None);
        write_session(
            &task_dir,
            "perturbed_00",
            2,
            Some(
                r#"{"exit_code":0,"perturbed":true,"perturbation_type":"swapped_doc",
                "designated_tool":"search_docs","gold_step":1}"#,
            ),
        );

        fs::write(
            sessions_dir.join("manifest.json"),
            r#"{"tasks":{"refund_window":{"reference":"reference",
            "perturbed":["perturbed_00"],"excluded":[]}}}"#,
        )
        .unwrap();

        let out_dir = tmp.path().join("out");
        let stats = build_pairs(&sessions_dir, &out_dir).expect("build succeeds");

        assert_eq!(stats.pairs, 1);
        assert!(stats.tasks_without_reference.is_empty());
        assert_eq!(stats.excluded_sessions, 0);

        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out_dir.join("pair_00.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["gold_step"], 1);
        assert_eq!(manifest["cross_system"], false);
        assert_eq!(manifest["meta"]["perturbation_type"], "swapped_doc");

        let failing: Run =
            serde_json::from_str(&fs::read_to_string(out_dir.join("a_00.json")).unwrap()).unwrap();
        assert_eq!(failing.steps.len(), 2);
        let reference: Run =
            serde_json::from_str(&fs::read_to_string(out_dir.join("b_00.json")).unwrap()).unwrap();
        assert_eq!(reference.steps.len(), 2);
    }

    #[test]
    fn a_task_with_no_reference_is_a_counted_exclusion_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::write(
            sessions_dir.join("manifest.json"),
            r#"{"tasks":{"shipping_cutoff":{"reference":null,"perturbed":[],
            "excluded":["reference","perturbed_00","perturbed_01","perturbed_02",
            "perturbed_03","perturbed_04"]}}}"#,
        )
        .unwrap();

        let out_dir = tmp.path().join("out");
        let stats = build_pairs(&sessions_dir, &out_dir).expect("build succeeds");

        assert_eq!(stats.pairs, 0);
        assert_eq!(stats.tasks_without_reference, vec!["shipping_cutoff"]);
        assert_eq!(stats.excluded_sessions, 6);
    }

    #[test]
    fn pair_index_is_stable_and_zero_based_across_tasks() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        for task in ["order_status", "refund_window"] {
            let task_dir = sessions_dir.join(task);
            fs::create_dir_all(&task_dir).unwrap();
            write_session(&task_dir, "reference", 1, None);
            write_session(
                &task_dir,
                "perturbed_00",
                2,
                Some(
                    r#"{"exit_code":0,"perturbed":true,"perturbation_type":"wrong_status",
                    "designated_tool":"check_status","gold_step":1}"#,
                ),
            );
        }
        fs::write(
            sessions_dir.join("manifest.json"),
            r#"{"tasks":{
                "order_status":{"reference":"reference","perturbed":["perturbed_00"],"excluded":[]},
                "refund_window":{"reference":"reference","perturbed":["perturbed_00"],"excluded":[]}
            }}"#,
        )
        .unwrap();

        let out_dir = tmp.path().join("out");
        let stats = build_pairs(&sessions_dir, &out_dir).expect("build succeeds");
        assert_eq!(stats.pairs, 2);
        // BTreeMap iterates tasks in key order: order_status before refund_window.
        let pair0: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out_dir.join("pair_00.json")).unwrap())
                .unwrap();
        assert_eq!(pair0["meta"]["task_id"], "order_status");
    }
}
