//! `amberfork-bench` — the offline benchmark harness (issue #6, BENCHMARK.md's
//! pre-registered protocol).
//!
//! `run --pairs <dir>` scores every protocol arm ([`arms::ALL`] — the factorial ladder from
//! the random floor to the shipped engine) on a local chimera pair set and emits the markdown
//! results table (stdout) plus an optional results JSON (`--json-out`). Wilson 95% intervals
//! on every rate; abstentions reported, never dropped. Still to land, slice by slice: the
//! dev/test split manifest with exclusions-as-data, frozen params (`bench/params.toml` +
//! config hash), the calibration curve, and the committed-results `report` mode.
//!
//! Real pair sets are NOT committed: chimera pairs derive from Who&When logs whose questions
//! originate in GAIA (gated upstream — notebook 001/T30). Regenerate locally with
//! `python3 spike/make_pairs.py`. The committed set under `tests/fixtures/` is hand-authored
//! fiction, kept so CI can lock the harness itself.
//!
//! A harness, not the product CLI: exit 0 = ran, 2 = trouble. stdout carries only the table
//! (paste-ready); diagnostics and context go to stderr.

mod arms;
mod pairs;
mod score;

use amberfork_align::DiffParams;
use clap::{Args, Parser, Subcommand};
use pairs::load_pairs;
use score::{ArmScore, Rate};
use serde::Serialize;
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

    /// Also write the full results document as JSON.
    #[arg(long, value_name = "FILE")]
    json_out: Option<PathBuf>,
}

/// The results document `--json-out` writes. Versioned independently of the trace schema so
/// a committed copy stays readable as later slices extend it.
#[derive(Serialize)]
struct BenchResults {
    bench_schema_version: &'static str,
    /// The evaluation protocol: `chimera` = controlled injection on real logs (BENCHMARK.md).
    protocol: &'static str,
    n_pairs: usize,
    params: ParamsUsed,
    arms: Vec<ArmResult>,
}

/// The engine parameters every arm ran with, echoed for provenance. The frozen-config hash
/// (protocol rule 2) arrives with the params-freeze slice.
#[derive(Serialize)]
struct ParamsUsed {
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
    let pairs = load_pairs(&args.pairs)?;
    for pair in &pairs {
        for (file, warning) in &pair.warnings {
            eprintln!(
                "amberfork-bench: {}: {}: {}",
                pair.name,
                file.display(),
                warning.msg
            );
        }
    }

    let params = DiffParams::default();
    let golds: Vec<usize> = pairs.iter().map(|pair| pair.gold_step).collect();

    let results = BenchResults {
        bench_schema_version: "0.1",
        protocol: "chimera",
        n_pairs: pairs.len(),
        params: ParamsUsed {
            tau: params.fork.tau,
            resync_k: params.fork.resync_k,
            gap_open: params.align.gap_open,
            gap_ext: params.align.gap_ext,
        },
        arms: arms::ALL
            .iter()
            .map(|arm| {
                let preds: Vec<Option<usize>> = pairs
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
        "chimera protocol · {} pairs · tau={} resync_k={} gap={}+{}",
        results.n_pairs,
        results.params.tau,
        results.params.resync_k,
        results.params.gap_open,
        results.params.gap_ext
    );
    println!("{}", markdown_table(&results));
    Ok(ExitCode::from(EXIT_OK))
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
