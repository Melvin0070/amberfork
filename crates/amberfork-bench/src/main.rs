//! `amberfork-bench` — the offline benchmark harness (issue #6, BENCHMARK.md's
//! pre-registered protocol).
//!
//! `run --pairs <dir>` scores every protocol arm ([`arms::ALL`] — the factorial ladder from
//! the random floor to the shipped engine) on a local chimera pair set and emits the markdown
//! results table (stdout) plus an optional results JSON (`--json-out`). Wilson 95% intervals
//! on every rate; abstentions reported, never dropped. Rules 1 and 4 live here too: every
//! pair carries its dev/test assignment (stable hash of the task key — `--split` selects
//! which side is scored), and the coverage line above the table counts every excluded case
//! with its reason. Rule 2 as well: parameters come ONLY from a frozen file (`--params`,
//! default `bench/params.toml`), and the published artifact names that file's sha256 — no
//! code-default fallback exists. Still to land, slice by slice: the calibration curve and
//! the committed-results `report` mode.
//!
//! Real pair sets are NOT committed: chimera pairs derive from Who&When logs whose questions
//! originate in GAIA (gated upstream — notebook 001/T30). Regenerate locally with
//! `python3 spike/make_pairs.py`. The committed sets under `tests/fixtures/` are
//! hand-authored fiction, kept so CI can lock the harness itself.
//!
//! A harness, not the product CLI: exit 0 = ran, 2 = trouble. stdout carries only the
//! published artifact (coverage line + table, paste-ready); diagnostics and context go to
//! stderr.

mod arms;
mod hash;
mod pairs;
mod params;
mod score;
mod split;

use clap::{Args, Parser, Subcommand, ValueEnum};
use pairs::{Pair, load_pairs};
use score::{ArmScore, Rate};
use serde::Serialize;
use split::Split;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

const EXIT_OK: u8 = 0;
const EXIT_TROUBLE: u8 = 2;

#[derive(Parser)]
#[command(name = "amberfork-bench", version, about)]
/// Run amberfork's pre-registered offline benchmark protocol (BENCHMARK.md).
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Score the protocol arms on a local directory of chimera pairs.
    Run(RunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Directory of pair_*.json manifests (the spike/make_pairs.py format).
    #[arg(long, value_name = "DIR")]
    pairs: PathBuf,

    /// Which protocol split to score: dev while tuning, test only with frozen params.
    #[arg(long, value_enum, default_value_t = SplitSelection::All)]
    split: SplitSelection,

    /// Frozen engine parameters (protocol rule 2). The file's sha256 publishes with the
    /// table; there is no code-default fallback. The default resolves from the repo root.
    #[arg(long, value_name = "FILE", default_value = "bench/params.toml")]
    params: PathBuf,

    /// Also write the full results document as JSON.
    #[arg(long, value_name = "FILE")]
    json_out: Option<PathBuf>,
}

/// The `--split` choices — the two protocol sides plus `all` (the whole evaluated set, the
/// walking-skeleton default; published tables come from `test`).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SplitSelection {
    All,
    Dev,
    Test,
}

impl SplitSelection {
    fn admits(self, split: Split) -> bool {
        match self {
            Self::All => true,
            Self::Dev => split == Split::Dev,
            Self::Test => split == Split::Test,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Dev => "dev",
            Self::Test => "test",
        }
    }
}

/// The results document `--json-out` writes. Versioned independently of the trace schema so
/// a committed copy stays readable as later slices extend it. 0.2: added `split` (the
/// selection scored), `coverage` (rule 4), and `pairs` (the rule-1 split manifest);
/// `n_pairs` narrowed from "pairs loaded" to "pairs scored". 0.3: `params` gained its
/// identity — `source` (the file as named on the command line) and `sha256` of its exact
/// bytes (rule 2).
#[derive(Serialize)]
struct BenchResults {
    bench_schema_version: &'static str,
    /// The evaluation protocol: `chimera` = controlled injection on real logs (BENCHMARK.md).
    protocol: &'static str,
    /// Which split selection produced the arm scores.
    split: &'static str,
    coverage: Coverage,
    /// Pairs actually scored: evaluated ∩ selected split.
    n_pairs: usize,
    params: ParamsUsed,
    /// The split manifest: every evaluated pair with its task key and assignment, whatever
    /// the selection — committed alongside results so the split is auditable (rule 1).
    pairs: Vec<PairRecord>,
    arms: Vec<ArmResult>,
}

/// Rule 4's accounting: every manifest found is either evaluated (and split-assigned) or
/// excluded for a tabulated reason. `evaluated / total` is the coverage the table reports.
#[derive(Serialize)]
struct Coverage {
    total: usize,
    evaluated: usize,
    /// Evaluated pairs on each side of the split.
    dev: usize,
    test: usize,
    /// Exclusion counts by reason kind (empty when nothing was excluded).
    reasons: BTreeMap<&'static str, usize>,
    /// Per-case records, in manifest order.
    exclusions: Vec<ExclusionRecord>,
}

/// One excluded case in the results document: dir-relative file, kebab-case reason. The
/// prose diagnostics stay on stderr — they may carry absolute paths and OS error text, which
/// have no business in a committed artifact.
#[derive(Serialize)]
struct ExclusionRecord {
    name: String,
    reason: &'static str,
    file: String,
}

/// One line of the split manifest.
#[derive(Serialize)]
struct PairRecord {
    name: String,
    task_key: String,
    split: &'static str,
}

/// The engine parameters every arm ran with, carrying their identity (protocol rule 2):
/// which file they came from and the sha256 of its exact bytes. The values are echoed too,
/// so a results document is readable without chasing the file.
#[derive(Serialize)]
struct ParamsUsed {
    source: String,
    sha256: String,
    tau: f64,
    resync_k: usize,
    gap_open: f64,
    gap_ext: f64,
}

#[derive(Serialize)]
struct ArmResult {
    arm: &'static str,
    #[serde(flatten)]
    score: ArmScore,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => run(&args).unwrap_or_else(|err| {
            eprintln!("amberfork-bench: {err}");
            ExitCode::from(EXIT_TROUBLE)
        }),
    }
}

fn run(args: &RunArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    // Config before data: a run that cannot establish its frozen parameters (rule 2) has
    // nothing meaningful to say about any pair set.
    let frozen = params::load(&args.params)?;
    let set = load_pairs(&args.pairs)?;
    for exclusion in &set.exclusions {
        eprintln!(
            "amberfork-bench: excluded {}: {}",
            exclusion.name, exclusion.reason
        );
    }
    for pair in &set.pairs {
        for (file, warning) in &pair.warnings {
            eprintln!(
                "amberfork-bench: {}: {}: {}",
                pair.name,
                file.display(),
                warning.msg
            );
        }
    }

    let dev = set
        .pairs
        .iter()
        .filter(|pair| pair.split == Split::Dev)
        .count();
    let test = set.pairs.len() - dev;
    let scored: Vec<&Pair> = set
        .pairs
        .iter()
        .filter(|pair| args.split.admits(pair.split))
        .collect();
    if scored.is_empty() {
        return Err(format!(
            "no pairs to score in split {} (evaluated: dev {dev}, test {test})",
            args.split.as_str()
        )
        .into());
    }

    let params = frozen.params;
    let golds: Vec<usize> = scored.iter().map(|pair| pair.gold_step).collect();

    let mut reasons: BTreeMap<&'static str, usize> = BTreeMap::new();
    for exclusion in &set.exclusions {
        *reasons.entry(exclusion.reason.kind()).or_default() += 1;
    }

    let results = BenchResults {
        bench_schema_version: "0.3",
        protocol: "chimera",
        split: args.split.as_str(),
        coverage: Coverage {
            total: set.total(),
            evaluated: set.pairs.len(),
            dev,
            test,
            reasons,
            exclusions: set
                .exclusions
                .iter()
                .map(|exclusion| ExclusionRecord {
                    name: exclusion.name.clone(),
                    reason: exclusion.reason.kind(),
                    file: exclusion.reason.file().display().to_string(),
                })
                .collect(),
        },
        n_pairs: scored.len(),
        params: ParamsUsed {
            source: frozen.source,
            sha256: frozen.sha256,
            tau: params.fork.tau,
            resync_k: params.fork.resync_k,
            gap_open: params.align.gap_open,
            gap_ext: params.align.gap_ext,
        },
        pairs: set
            .pairs
            .iter()
            .map(|pair| PairRecord {
                name: pair.name.clone(),
                task_key: pair.task_key.clone(),
                split: pair.split.as_str(),
            })
            .collect(),
        arms: arms::ALL
            .iter()
            .map(|arm| {
                let preds: Vec<Option<usize>> = scored
                    .iter()
                    .map(|pair| arm.predict(pair, &params))
                    .collect();
                ArmResult {
                    arm: arm.name(),
                    score: score::score(&preds, &golds),
                }
            })
            .collect(),
    };

    if let Some(path) = &args.json_out {
        let json = serde_json::to_string_pretty(&results)?;
        std::fs::write(path, json)
            .map_err(|err| format!("write results {}: {err}", path.display()))?;
    }

    eprintln!(
        "chimera protocol · split={} · {} scored of {} evaluated",
        results.split, results.n_pairs, results.coverage.evaluated,
    );
    println!("{}", coverage_line(&results));
    println!("{}\n", params_line(&results));
    println!("{}", markdown_table(&results));
    Ok(ExitCode::from(EXIT_OK))
}

/// The coverage line the table is published under (rule 4: a rate without its denominator's
/// history is a lie). Exclusion reasons appear inline, alphabetically, only when present.
fn coverage_line(results: &BenchResults) -> String {
    let coverage = &results.coverage;
    let excluded = if coverage.reasons.is_empty() {
        String::new()
    } else {
        let reasons: Vec<String> = coverage
            .reasons
            .iter()
            .map(|(kind, count)| format!("{kind} {count}"))
            .collect();
        format!(" (excluded: {})", reasons.join(", "))
    };
    format!(
        "coverage: {}/{} pairs evaluated{excluded} · split={} (dev {}, test {}) · scored {}",
        coverage.evaluated,
        coverage.total,
        results.split,
        coverage.dev,
        coverage.test,
        results.n_pairs
    )
}

/// The config-identity line (rule 2: every published table names the config hash that
/// produced it). The 12-hex prefix reads like a git short hash; the results JSON carries
/// the full digest, and `shasum -a 256 <source>` verifies it.
fn params_line(results: &BenchResults) -> String {
    let params = &results.params;
    format!(
        "params: {} sha256:{} · tau {} · resync_k {} · gap {}+{}",
        params.source,
        &params.sha256[..12],
        params.tau,
        params.resync_k,
        params.gap_open,
        params.gap_ext
    )
}

/// The results as a markdown table (the shape BENCHMARK.md's published table takes):
/// `rate [ci_lo, ci_hi]` per windowed metric, two decimals, one row per arm.
fn markdown_table(results: &BenchResults) -> String {
    let mut lines = vec![
        "| arm | exact | ±1 | ±3 | no-pred | n |".to_string(),
        "|---|---|---|---|---|---|".to_string(),
    ];
    for arm in &results.arms {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2} | {} |",
            arm.arm,
            cell(arm.score.exact),
            cell(arm.score.w1),
            cell(arm.score.w3),
            arm.score.no_pred.rate,
            arm.score.exact.n,
        ));
    }
    lines.join("\n")
}

fn cell(rate: Rate) -> String {
    format!(
        "{:.2} [{:.2}, {:.2}]",
        rate.rate, rate.ci95_lo, rate.ci95_hi
    )
}
