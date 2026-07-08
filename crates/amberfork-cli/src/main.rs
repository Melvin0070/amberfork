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
use amberfork_model::Warning;
use clap::{Args, Parser, Subcommand};
use render::{RenderOpts, resolve_color_mode};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

mod render;

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

    /// Disable ANSI styling (also honored: a non-empty NO_COLOR, piped stdout, TERM=dumb).
    #[arg(long)]
    no_color: bool,
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

    // The single decision point for engine params: anything user-supplied (future --tau
    // style flags) routes through validated() here and maps ParamError to exit 2 — the
    // engine itself never asserts. Defaults are valid by unit test in amberfork-align.
    let params = DiffParams::default()
        .validated()
        .expect("dev-calibrated defaults satisfy their own invariants");
    let mut result = diff(&good.run, &bad.run, &LexicalCost, &params);
    result.warnings = merged_warnings(good.warnings, bad.warnings);

    if args.json {
        let json = serde_json::to_string_pretty(&result)
            .expect("DiffResult serialization is infallible (no non-string map keys)");
        println!("{json}");
    } else {
        let color = resolve_color_mode(
            args.no_color,
            std::io::stdout().is_terminal(),
            std::env::var("NO_COLOR").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
            std::env::var("COLORTERM").ok().as_deref(),
        );
        let width = terminal_size::terminal_size().map_or(100, |(w, _)| usize::from(w.0));
        let opts = RenderOpts {
            color,
            width: width.max(60),
        };
        print!("{}", render::render(&result, &good.run, &bad.run, &opts));
        // Diagnostics stay off stdout: stdout is the result, stderr is the channel for
        // everything about producing it.
        for warning in &result.warnings {
            eprintln!("amberfork: warning: {}", warning.msg);
        }
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
