//! The `amberfork` command — the terminal surface over the diff pipeline.
//!
//! `amberfork diff <bad> --against <good>` wires the crates end-to-end:
//! `amberfork-ingest` loads both traces, `amberfork_align::diff` aligns and locates the fork,
//! and this crate renders the result. Side convention (the `DiffResult` contract): `<good>` is
//! the reference (side `a`), `<bad>` is the observed/failing run (side `b`).
//!
//! Exit codes follow the `diff(1)` precedent: **0** converged, **1** forked, **2** trouble
//! (unreadable input, bad usage — clap's own usage errors also exit 2). Errors go to stderr;
//! stdout carries only the result.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_ingest::IngestError;
use amberfork_model::{DiffResult, Warning};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

const EXIT_CONVERGED: u8 = 0;
const EXIT_FORKED: u8 = 1;
const EXIT_TROUBLE: u8 = 2;

#[derive(Parser)]
#[command(name = "amberfork", version, about)]
/// Diff two AI-agent run trajectories and find where they fork.
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Diff a failing run against a known-good reference and locate the fork.
    Diff(DiffArgs),
}

#[derive(Args)]
struct DiffArgs {
    /// The failing/observed run trace (side `b` of the result).
    #[arg(value_name = "BAD")]
    bad: PathBuf,

    /// The known-good reference run trace (side `a` of the result).
    #[arg(long, value_name = "GOOD")]
    against: PathBuf,

    /// Emit the DiffResult as JSON on stdout — the machine contract.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Diff(args) => run_diff(&args).unwrap_or_else(|err| {
            eprintln!("amberfork: {err}");
            ExitCode::from(EXIT_TROUBLE)
        }),
    }
}

fn run_diff(args: &DiffArgs) -> Result<ExitCode, IngestError> {
    let good = amberfork_ingest::load_file(&args.against)?;
    let bad = amberfork_ingest::load_file(&args.bad)?;

    let mut result = diff(&good.run, &bad.run, &LexicalCost, &DiffParams::default());
    result.warnings = merged_warnings(good.warnings, bad.warnings);

    if args.json {
        let json = serde_json::to_string_pretty(&result)
            .expect("DiffResult serialization is infallible (no non-string map keys)");
        println!("{json}");
    } else {
        print_summary(&result);
    }

    let code = if result.fork.is_some() {
        EXIT_FORKED
    } else {
        EXIT_CONVERGED
    };
    Ok(ExitCode::from(code))
}

/// Merge per-run ingest warnings into the result's flat list, each message prefixed with its
/// side (`a` = reference, `b` = observed) so the flat list stays attributable to a run.
fn merged_warnings(reference: Vec<Warning>, observed: Vec<Warning>) -> Vec<Warning> {
    fn tag(side: char, warnings: Vec<Warning>) -> impl Iterator<Item = Warning> {
        warnings.into_iter().map(move |w| Warning {
            code: w.code,
            msg: format!("run {side}: {}", w.msg),
        })
    }
    tag('a', reference).chain(tag('b', observed)).collect()
}

/// Placeholder human summary — one honest line. Slice 2 of issue #4 replaces this with the
/// DESIGN.md terminal render (sync spine, `⑂ FORK` gutter, amber, field diff).
fn print_summary(result: &DiffResult) {
    match result.fork {
        Some(fork) => {
            let side = |label: char, step: Option<usize>| match step {
                Some(idx) => format!("{label} step {idx}"),
                None => format!("no {label}-side step"),
            };
            println!(
                "fork: {} / {} (confidence {:.2})",
                side('b', fork.b_step),
                side('a', fork.a_step),
                fork.confidence
            );
        }
        None => println!(
            "converged: no fork across {} aligned moves",
            result.alignment.len()
        ),
    }
}
