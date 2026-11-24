//! The Terminal-Bench 2.0 natural-pair corpus (pre-registered in docs/notebook.md 083): turning
//! published trials into same-agent pass×fail and pass×pass pairs.
//!
//! The corpus is one trial per record — `task_name`, `agent`, `model`, `reward`, `trial_id` and a
//! JSON-encoded `steps` list of `{src, msg, tools: [{fn, cmd}], obs}`. It is a third party's
//! scrape and is **benchmark-use only**: this module reads it from `bench/data/` (gitignored) and
//! writes pairs back under `bench/data/`. Nothing it produces is committed.
//!
//! Everything here is fixed by notebook 083 and none of it looks at an engine output:
//! - a *group* is `(task, agent, model)`; it is *eligible* with at least one passing and one
//!   failing trial that both carry steps;
//! - each eligible group yields one pass×fail pair, and one pass×pass pair if it has two passing
//!   trials — always the lexicographically smallest `trial_id`s, so nothing is chosen after
//!   scoring and no group contributes twice to a class;
//! - the split is keyed on the *task* ([`Split::of`], the protocol's one split rule);
//! - every case that drops out is counted with its reason (rule 4).

use crate::split::Split;
use amberfork_model::{Payload, Run, SchemaVersion, Step, StepKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// `amberfork diff`'s default `--max-steps`. Notebook 083 registers that the guard is not
/// raised: a pair with a longer run is an exclusion, exactly as a user would hit it.
pub const MAX_STEPS: usize = 2000;

/// One trial as the corpus publishes it. `steps` stays the JSON string it ships as; it is null
/// for the trials the scrape has no trajectory for.
#[derive(Debug, Clone, Deserialize)]
pub struct Trial {
    pub task_name: String,
    pub agent: String,
    pub model: String,
    pub reward: i64,
    /// The trial's name — unique within the corpus, and the identity selection keys on.
    pub trial_name: String,
    /// A UUID on a minority of records and empty on the rest (notebook 083a), so never a key.
    #[serde(default)]
    pub trial_id: String,
    #[serde(default)]
    pub steps: Option<String>,
}

/// Which registered class a pair belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairClass {
    /// Reference passed, observed failed — a real behavioural difference.
    PassFail,
    /// Both passed — ordinary run-to-run variance and nothing else.
    PassPass,
}

/// One selected pair: side `a` is the reference (always a passing trial), side `b` the observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairSpec {
    pub class: PairClass,
    pub task: String,
    pub agent: String,
    pub model: String,
    pub split: String,
    pub a_trial: String,
    pub b_trial: String,
    /// Zero-based record index of each side in the input — the corpus lists some trials twice,
    /// so the name alone does not say which record was used.
    pub a_row: usize,
    pub b_row: usize,
}

/// What selection did to the corpus — every count a coverage figure needs (rule 4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionStats {
    pub trials: usize,
    /// Trials with a null, empty or unparseable `steps` — no trajectory to pair.
    pub trials_without_steps: usize,
    /// Second records of a trial the corpus lists twice (same group and `trial_name`).
    pub duplicate_records: usize,
    /// Trials whose trajectory is byte-identical to an earlier-named one in the same group.
    pub identical_trajectories: usize,
    pub groups_with_steps: usize,
    pub eligible_groups: usize,
    /// Groups with steps but only one outcome among them.
    pub single_outcome_groups: usize,
}

/// Convert one trial's steps into a canonical run.
///
/// Returns `None` when the trial has no usable trajectory (null, empty, or not a JSON list).
/// The mapping is notebook 083's, and matches `amberfork_ingest::atif`: an agent `msg` → an
/// `llm` step named `assistant`; each `tools` entry → a `tool` step named by `fn` with `cmd` as
/// its input; the step's single `obs` goes to its *last* tool call (the source does not say
/// which call produced it), or to an `observation` step when there was no call.
#[must_use]
pub fn trial_to_run(trial: &Trial) -> Option<Run> {
    let source: Vec<RawStep> = serde_json::from_str(trial.steps.as_deref()?).ok()?;
    if source.is_empty() {
        return None;
    }
    let mut steps: Vec<Step> = Vec::new();
    for raw in source {
        let msg = raw.msg.unwrap_or_default();
        if raw.src == "agent" {
            if !msg.trim().is_empty() {
                push(&mut steps, StepKind::Llm, "assistant").outputs = Some(Payload::Text(msg));
            }
            let tools = raw.tools.unwrap_or_default();
            let had_tools = !tools.is_empty();
            for tool in tools {
                let mut input = Map::new();
                input.insert("cmd".to_owned(), tool.cmd);
                push(&mut steps, StepKind::Tool, &tool.name).inputs = Some(Payload::Object(input));
            }
            if let Some(obs) = raw.obs.filter(|obs| !obs.trim().is_empty()) {
                if had_tools {
                    let last = steps.len() - 1;
                    steps[last].outputs = Some(Payload::Text(obs));
                } else {
                    push(&mut steps, StepKind::Other, "observation").outputs =
                        Some(Payload::Text(obs));
                }
            }
        } else if !msg.trim().is_empty() {
            let name = if raw.src == "system" {
                "system"
            } else {
                "user"
            };
            push(&mut steps, StepKind::Other, name).outputs = Some(Payload::Text(msg));
        }
    }
    Some(Run {
        schema_version: SchemaVersion::current(),
        id: format!("tbench_{}", trial.trial_name),
        task: None,
        outcome: None,
        steps,
        edges: None,
    })
}

fn push<'a>(steps: &'a mut Vec<Step>, kind: StepKind, name: &str) -> &'a mut Step {
    steps.push(Step {
        idx: steps.len(),
        kind,
        name: name.to_owned(),
        inputs: None,
        outputs: None,
        attrs: Map::new(),
        t_start: None,
        t_end: None,
        parent_idx: None,
    });
    steps.last_mut().expect("just pushed")
}

/// The identity selection needs from a trial — everything but the trajectory itself, so a
/// 900 MB corpus can be selected over without holding it in memory.
#[derive(Debug, Clone)]
pub struct TrialHead {
    /// Zero-based record index in the input.
    pub row: usize,
    pub task_name: String,
    pub agent: String,
    pub model: String,
    pub passed: bool,
    pub trial_name: String,
    pub has_id: bool,
    /// sha256 of the `steps` string; `None` when the trial carries no trajectory.
    pub trajectory: Option<[u8; 32]>,
}

impl TrialHead {
    #[must_use]
    pub fn of(trial: &Trial, row: usize) -> Self {
        let trajectory = trial.steps.as_deref().filter(|steps| {
            serde_json::from_str::<Vec<serde::de::IgnoredAny>>(steps).is_ok_and(|s| !s.is_empty())
        });
        Self {
            row,
            task_name: trial.task_name.clone(),
            agent: trial.agent.clone(),
            model: trial.model.clone(),
            passed: trial.reward == 1,
            trial_name: trial.trial_name.clone(),
            has_id: !trial.trial_id.is_empty(),
            trajectory: trajectory.map(|steps| Sha256::digest(steps.as_bytes()).into()),
        }
    }
}

/// `(task, agent, model)` — the unit trials are compared within.
type GroupKey<'a> = (&'a str, &'a str, &'a str);

/// Apply the selection rule of notebook 083 as amended by 083a. Pairs come out ordered by group
/// key then class, so the output is a pure function of the corpus.
///
/// Within a group a trial is identified by its `trial_name`. The corpus lists some trials twice
/// (one record with a `trial_id`, one without); the record with the id is kept. A trial whose
/// trajectory is byte-identical to an earlier-named one in its group is dropped too — pairing a
/// run with a copy of itself would manufacture agreement.
#[must_use]
pub fn select_pairs(heads: &[TrialHead]) -> (Vec<PairSpec>, SelectionStats) {
    let mut stats = SelectionStats {
        trials: heads.len(),
        ..SelectionStats::default()
    };
    let mut groups: BTreeMap<GroupKey<'_>, BTreeMap<&str, &TrialHead>> = BTreeMap::new();
    for head in heads {
        if head.trajectory.is_none() {
            stats.trials_without_steps += 1;
            continue;
        }
        let group = groups
            .entry((&head.task_name, &head.agent, &head.model))
            .or_default();
        match group.get(head.trial_name.as_str()) {
            Some(kept) => {
                stats.duplicate_records += 1;
                if head.has_id && !kept.has_id {
                    group.insert(&head.trial_name, head);
                }
            }
            None => {
                group.insert(&head.trial_name, head);
            }
        }
    }
    stats.groups_with_steps = groups.len();

    let mut pairs = Vec::new();
    for ((task, agent, model), trials) in groups {
        let mut seen = BTreeSet::new();
        let (mut passed, mut failed) = (Vec::new(), Vec::new());
        // Ascending `trial_name`, so "smallest" below is simply "first".
        for head in trials.into_values() {
            if !seen.insert(head.trajectory) {
                stats.identical_trajectories += 1;
            } else if head.passed {
                passed.push(head);
            } else {
                failed.push(head);
            }
        }
        if passed.is_empty() || failed.is_empty() {
            stats.single_outcome_groups += 1;
            continue;
        }
        stats.eligible_groups += 1;
        let spec = |class, a: &TrialHead, b: &TrialHead| PairSpec {
            class,
            task: task.to_owned(),
            agent: agent.to_owned(),
            model: model.to_owned(),
            split: Split::of(task).as_str().to_owned(),
            a_trial: a.trial_name.clone(),
            b_trial: b.trial_name.clone(),
            a_row: a.row,
            b_row: b.row,
        };
        pairs.push(spec(PairClass::PassFail, passed[0], failed[0]));
        if passed.len() >= 2 {
            pairs.push(spec(PairClass::PassPass, passed[0], passed[1]));
        }
    }
    (pairs, stats)
}

/// One buildable pair as written to `pairs.json`: the selection plus where its two runs live
/// (relative to the output directory) and how long they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltPair {
    #[serde(flatten)]
    pub spec: PairSpec,
    pub a: String,
    pub b: String,
    pub a_steps: usize,
    pub b_steps: usize,
}

/// A selected pair that could not be built, with its reason — counted, never dropped (rule 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedPair {
    #[serde(flatten)]
    pub spec: PairSpec,
    pub reason: String,
}

/// The whole build: what selection saw, what was written, and what dropped out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildReport {
    pub selection: SelectionStats,
    pub pairs: Vec<BuiltPair>,
    pub excluded: Vec<ExcludedPair>,
}

/// Build the pair set from one or more JSON-Lines trial files into `out_dir`: canonical runs
/// under `runs/`, plus `pairs.json` (the [`BuildReport`]).
///
/// Two passes over the input, so the corpus is never held in memory: the first keeps only each
/// trial's identity, the second converts the records selection actually chose.
///
/// # Errors
/// An unreadable input, a line that is not a trial record, or an unwritable output. A trial
/// that cannot be converted is an exclusion, not an error.
pub fn build(
    trial_files: &[PathBuf],
    out_dir: &Path,
) -> Result<BuildReport, Box<dyn std::error::Error>> {
    let mut heads = Vec::new();
    for_each_trial(trial_files, |row, trial| {
        heads.push(TrialHead::of(&trial, row));
        Ok(())
    })?;
    let (specs, selection) = select_pairs(&heads);
    drop(heads);

    let wanted: BTreeSet<usize> = specs
        .iter()
        .flat_map(|spec| [spec.a_row, spec.b_row])
        .collect();
    std::fs::create_dir_all(out_dir.join("runs"))?;
    // record index → (file relative to out_dir, canonical step count)
    let mut written: BTreeMap<usize, (String, usize)> = BTreeMap::new();
    for_each_trial(trial_files, |row, trial| {
        if !wanted.contains(&row) {
            return Ok(());
        }
        if let Some(run) = trial_to_run(&trial) {
            let file = format!("runs/{}.json", file_stem(&trial.trial_name));
            std::fs::write(out_dir.join(&file), serde_json::to_string(&run)?)?;
            written.insert(row, (file, run.steps.len()));
        }
        Ok(())
    })?;

    let mut pairs = Vec::new();
    let mut excluded = Vec::new();
    for spec in specs {
        let sides = (written.get(&spec.a_row), written.get(&spec.b_row));
        let (Some((a, a_steps)), Some((b, b_steps))) = sides else {
            excluded.push(ExcludedPair {
                spec,
                reason: "trial could not be converted".to_owned(),
            });
            continue;
        };
        if *a_steps > MAX_STEPS || *b_steps > MAX_STEPS {
            excluded.push(ExcludedPair {
                spec,
                reason: format!("run exceeds {MAX_STEPS} steps"),
            });
            continue;
        }
        pairs.push(BuiltPair {
            spec,
            a: a.clone(),
            b: b.clone(),
            a_steps: *a_steps,
            b_steps: *b_steps,
        });
    }

    let report = BuildReport {
        selection,
        pairs,
        excluded,
    };
    std::fs::write(
        out_dir.join("pairs.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}

/// Visit every trial record with its zero-based index across all files, in order.
fn for_each_trial(
    trial_files: &[PathBuf],
    mut visit: impl FnMut(usize, Trial) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut row = 0;
    for path in trial_files {
        let reader =
            BufReader::new(File::open(path).map_err(|err| format!("{}: {err}", path.display()))?);
        for (number, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let trial: Trial = serde_json::from_str(&line).map_err(|err| {
                format!(
                    "{}:{}: not a trial record: {err}",
                    path.display(),
                    number + 1
                )
            })?;
            visit(row, trial)?;
            row += 1;
        }
    }
    Ok(())
}

/// A trial name as a file name: anything outside `[A-Za-z0-9_-]` becomes `_`, so a name can
/// never address a path outside `runs/`.
fn file_stem(trial_name: &str) -> String {
    trial_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Deserialize)]
struct RawStep {
    #[serde(default)]
    src: String,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    tools: Option<Vec<RawTool>>,
    #[serde(default)]
    obs: Option<String>,
}

#[derive(Deserialize)]
struct RawTool {
    #[serde(rename = "fn", default)]
    name: String,
    #[serde(default)]
    cmd: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trial(task: &str, agent: &str, reward: i64, name: &str, steps: Option<&str>) -> Trial {
        Trial {
            task_name: task.to_owned(),
            agent: agent.to_owned(),
            model: "m@lab".to_owned(),
            reward,
            trial_name: name.to_owned(),
            trial_id: String::new(),
            steps: steps.map(str::to_owned),
        }
    }

    /// A one-step trajectory whose content is `text` — distinct text, distinct trajectory.
    fn steps(text: &str) -> String {
        format!(r#"[{{"src":"agent","msg":"{text}"}}]"#)
    }

    const STEPS: &str = r#"[
        {"src":"user","msg":"Fix the build.","tools":null,"obs":null},
        {"src":"agent","msg":"Looking.","tools":[{"fn":"Bash","cmd":"ls"},{"fn":"Read","cmd":["a.rs","b.rs"]}],"obs":"fn main() {}"},
        {"src":"agent","msg":"","tools":null,"obs":"session resumed"},
        {"src":"agent","msg":"Done.","tools":null,"obs":null}
    ]"#;

    #[test]
    fn a_trial_fans_out_like_the_atif_adapter() {
        let run = trial_to_run(&trial("t", "a", 1, "id-1", Some(STEPS))).expect("has steps");
        let shape: Vec<(StepKind, &str)> = run
            .steps
            .iter()
            .map(|s| (s.kind, s.name.as_str()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (StepKind::Other, "user"),
                (StepKind::Llm, "assistant"),
                (StepKind::Tool, "Bash"),
                (StepKind::Tool, "Read"),
                (StepKind::Other, "observation"),
                (StepKind::Llm, "assistant"),
            ]
        );
        assert_eq!(run.id, "tbench_id-1");
        assert_eq!(run.outcome, None);
        assert_eq!(
            run.steps.iter().map(|s| s.idx).collect::<Vec<_>>(),
            (0..6).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_single_observation_goes_to_the_last_tool_call_of_its_step() {
        let run = trial_to_run(&trial("t", "a", 1, "id-1", Some(STEPS))).unwrap();
        assert_eq!(run.steps[2].outputs, None, "Bash: not the last call");
        assert_eq!(
            run.steps[3].outputs,
            Some(Payload::Text("fn main() {}".to_owned()))
        );
        let Some(Payload::Object(input)) = &run.steps[3].inputs else {
            panic!("cmd is kept as a field");
        };
        assert_eq!(input["cmd"], serde_json::json!(["a.rs", "b.rs"]));
    }

    #[test]
    fn a_trial_without_a_trajectory_converts_to_nothing() {
        assert!(trial_to_run(&trial("t", "a", 1, "x", None)).is_none());
        assert!(trial_to_run(&trial("t", "a", 1, "x", Some("[]"))).is_none());
        assert!(trial_to_run(&trial("t", "a", 1, "x", Some("{truncated"))).is_none());
    }

    fn heads(trials: &[Trial]) -> Vec<TrialHead> {
        trials
            .iter()
            .enumerate()
            .map(|(row, trial)| TrialHead::of(trial, row))
            .collect()
    }

    #[test]
    fn an_eligible_group_yields_one_pair_per_class_from_its_smallest_trial_names() {
        let trials = [
            trial("task", "agent", 0, "f-2", Some(&steps("a"))),
            trial("task", "agent", 1, "p-3", Some(&steps("b"))),
            trial("task", "agent", 1, "p-1", Some(&steps("c"))),
            trial("task", "agent", 0, "f-1", Some(&steps("d"))),
            trial("task", "agent", 1, "p-2", Some(&steps("e"))),
        ];
        let (pairs, stats) = select_pairs(&heads(&trials));
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].class, PairClass::PassFail);
        assert_eq!(
            (pairs[0].a_trial.as_str(), pairs[0].b_trial.as_str()),
            ("p-1", "f-1")
        );
        assert_eq!((pairs[0].a_row, pairs[0].b_row), (2, 3));
        assert_eq!(pairs[1].class, PairClass::PassPass);
        assert_eq!(
            (pairs[1].a_trial.as_str(), pairs[1].b_trial.as_str()),
            ("p-1", "p-2")
        );
        assert_eq!(pairs[0].split, Split::of("task").as_str());
        assert_eq!(stats.eligible_groups, 1);
    }

    #[test]
    fn groups_that_cannot_form_a_pass_fail_pair_are_counted_not_paired() {
        let trials = [
            // Only passes: no failing counterpart, so no pair of either class.
            trial("all-pass", "agent", 1, "a", Some(&steps("1"))),
            trial("all-pass", "agent", 1, "b", Some(&steps("2"))),
            // The failing trial has no trajectory, so the group is single-outcome in effect.
            trial("no-steps", "agent", 1, "c", Some(&steps("3"))),
            trial("no-steps", "agent", 0, "d", Some("null")),
            // Same task, different agent: a different group, never mixed.
            trial("mixed", "agent-1", 1, "e", Some(&steps("4"))),
            trial("mixed", "agent-2", 0, "f", Some(&steps("5"))),
        ];
        let (pairs, stats) = select_pairs(&heads(&trials));
        assert!(pairs.is_empty(), "{pairs:?}");
        assert_eq!(
            stats,
            SelectionStats {
                trials: 6,
                trials_without_steps: 1,
                duplicate_records: 0,
                identical_trajectories: 0,
                groups_with_steps: 4,
                eligible_groups: 0,
                single_outcome_groups: 4,
            }
        );
    }

    #[test]
    fn one_pass_and_one_fail_yield_only_the_pass_fail_pair() {
        let trials = [
            trial("task", "agent", 1, "p", Some(&steps("a"))),
            trial("task", "agent", 0, "f", Some(&steps("b"))),
        ];
        let (pairs, _) = select_pairs(&heads(&trials));
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].class, PairClass::PassFail);
    }

    #[test]
    fn a_trial_listed_twice_is_one_trial_and_never_paired_with_itself() {
        // The corpus's real defect (notebook 083a): the same trial as two records, one with a
        // trial_id and one without, serialized slightly differently. Without the dedupe this
        // group would yield a pass-pass pair of a run against itself.
        let mut with_id = trial("task", "agent", 1, "p-1", Some(&steps("rendering one")));
        with_id.trial_id = "uuid".to_owned();
        let trials = [
            trial("task", "agent", 1, "p-1", Some(&steps("rendering two"))),
            with_id,
            trial("task", "agent", 0, "f-1", Some(&steps("x"))),
        ];
        let (pairs, stats) = select_pairs(&heads(&trials));
        assert_eq!(pairs.len(), 1, "one passing trial, so no pass-pass pair");
        assert_eq!(
            pairs[0].a_row, 1,
            "the record carrying the trial_id is the one kept"
        );
        assert_eq!(stats.duplicate_records, 1);
    }

    #[test]
    fn a_byte_identical_trajectory_under_another_name_is_dropped() {
        let same = steps("identical");
        let trials = [
            trial("task", "agent", 1, "p-1", Some(&same)),
            trial("task", "agent", 1, "p-2", Some(&same)),
            trial("task", "agent", 0, "f-1", Some(&steps("x"))),
        ];
        let (pairs, stats) = select_pairs(&heads(&trials));
        assert_eq!(
            pairs.len(),
            1,
            "p-2 is a copy of p-1, not a second passing run"
        );
        assert_eq!(stats.identical_trajectories, 1);
    }

    #[test]
    fn build_writes_runs_and_a_report_and_excludes_over_long_pairs() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("trials.jsonl");
        let short = r#"[{"src":"agent","msg":"x","tools":[{"fn":"Bash","cmd":"ls"}],"obs":"ok"}]"#;
        let long = format!(
            "[{}]",
            vec![r#"{"src":"agent","msg":"x"}"#; MAX_STEPS + 1].join(",")
        );
        let row = |task: &str, reward: i64, name: &str, steps: &str| {
            serde_json::json!({"task_name": task, "agent": "a", "model": "m", "reward": reward,
                               "trial_name": name, "trial_id": "", "steps": steps})
            .to_string()
        };
        let lines = [
            row("ok-task", 1, "p/1", short),
            row("ok-task", 0, "f1", &short.replace("ok", "no such file")),
            row("long-task", 1, "p2", short),
            row("long-task", 0, "f2", &long),
        ];
        std::fs::write(&input, lines.join("\n")).unwrap();

        let out = dir.path().join("out");
        let report = build(&[input], &out).expect("builds");

        assert_eq!(report.pairs.len(), 1);
        let pair = &report.pairs[0];
        assert_eq!(pair.spec.task, "ok-task");
        assert_eq!((pair.a_steps, pair.b_steps), (2, 2));
        assert_eq!(pair.a, "runs/p_1.json", "an id cannot escape runs/");
        let run: Run =
            serde_json::from_str(&std::fs::read_to_string(out.join(&pair.a)).unwrap()).unwrap();
        assert_eq!(run.steps.len(), 2);

        assert_eq!(report.excluded.len(), 1);
        assert_eq!(report.excluded[0].spec.task, "long-task");
        assert!(report.excluded[0].reason.contains("2000"));

        let on_disk: BuildReport =
            serde_json::from_str(&std::fs::read_to_string(out.join("pairs.json")).unwrap())
                .unwrap();
        assert_eq!(on_disk, report);
    }
}
