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
//! code-default fallback exists. Rule 7: the aligner arms publish their reliability curve
//! (fork confidence binned vs exact-hit rate) under the main table.
//!
//! `report` re-renders a committed results document — no pairs, no engine, no fetch — through
//! the same renderer `run` prints with ([`results::render`]), so the published table
//! reproduces offline, byte for byte, from the repo alone (BENCHMARK.md's definition of
//! done). The canonical committed document lives under `bench/results/`.
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
mod calibration;
mod hash;
mod pairs;
mod params;
mod results;
mod score;
mod split;

use arms::Prediction;
use clap::{Args, Parser, Subcommand, ValueEnum};
use pairs::{Pair, load_pairs};
use results::{ArmResult, BenchResults, Coverage, ExclusionRecord, PairRecord, ParamsUsed};
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
    /// Re-render a committed results document — offline, zero fetch.
    Report(ReportArgs),
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

#[derive(Args)]
struct ReportArgs {
    /// A results document (what `run --json-out` writes). The default resolves from the
    /// repo root to the canonical committed artifact.
    #[arg(
        long,
        value_name = "FILE",
        default_value = "bench/results/chimera_noise_seed42_dev.json"
    )]
    results: PathBuf,
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Run(args) => run(&args),
        Command::Report(args) => report(&args),
    };
    outcome.unwrap_or_else(|err| {
        eprintln!("amberfork-bench: {err}");
        ExitCode::from(EXIT_TROUBLE)
    })
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

    // The set's cross-system character is a fact of its pairs, not an operator flag: a scored
    // pair is Mode A′ iff its manifest declared it. A set carrying any such pair is labeled
    // `mode-a-prime` and gets the table's cross-system disclosure (issue #7).
    let cross_system = scored.iter().filter(|pair| pair.cross_system).count();
    let protocol = if cross_system > 0 {
        "mode-a-prime"
    } else {
        "chimera"
    };

    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for exclusion in &set.exclusions {
        *reasons
            .entry(exclusion.reason.kind().to_string())
            .or_default() += 1;
    }

    let results = BenchResults {
        bench_schema_version: results::SCHEMA_VERSION.to_string(),
        protocol: protocol.to_string(),
        split: args.split.as_str().to_string(),
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
                    reason: exclusion.reason.kind().to_string(),
                    file: exclusion.reason.file().display().to_string(),
                })
                .collect(),
        },
        n_pairs: scored.len(),
        cross_system,
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
                split: pair.split.as_str().to_string(),
            })
            .collect(),
        arms: arms::ALL
            .iter()
            .map(|arm| {
                let preds: Vec<Option<Prediction>> = scored
                    .iter()
                    .map(|pair| arm.predict(pair, &params))
                    .collect();
                let steps: Vec<Option<usize>> = preds
                    .iter()
                    .map(|pred| pred.map(|prediction| prediction.step))
                    .collect();
                ArmResult {
                    arm: arm.name().to_string(),
                    score: score::score(&steps, &golds),
                    calibration: arm
                        .emits_confidence()
                        .then(|| calibration::calibrate(&preds, &golds)),
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
    println!("{}", results::render(&results));
    Ok(ExitCode::from(EXIT_OK))
}

fn report(args: &ReportArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let results = results::load(&args.results)?;
    eprintln!(
        "rendering {} · {} protocol · split={} · {} scored of {} evaluated",
        args.results.display(),
        results.protocol,
        results.split,
        results.n_pairs,
        results.coverage.evaluated,
    );
    println!("{}", results::render(&results));
    Ok(ExitCode::from(EXIT_OK))
}
