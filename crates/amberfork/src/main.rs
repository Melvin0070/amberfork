//! The `amberfork` command — the terminal surface over the diff pipeline.
//!
//! `amberfork diff <bad> --against <good>` wires the crates end-to-end:
//! `amberfork-ingest` loads both traces, `amberfork_align::diff` aligns and locates the fork,
//! and this crate renders the result. Side convention (the `DiffResult` contract): `<good>` is
//! the reference (side `a`), `<bad>` is the observed/failing run (side `b`).
//!
//! `amberfork demo` is the same pipeline on a pair embedded in the binary at compile time —
//! the zero-setup first contact: no files, no network, no particular working directory.
//!
//! `amberfork serve <bad> --against <good>` runs the same engine, then hands the fork to the
//! browser: a loopback-only server (amberfork-server) over the layout `Document`. It is a
//! sibling of `diff`, not a flag on it — a long-running server has different lifecycle
//! semantics than diff's exit-code contract. `amberfork serve --demo` is the zero-setup form:
//! the embedded pair `demo` renders in the terminal, handed to the browser instead of files.
//!
//! Exit codes follow the `diff(1)` precedent: **0** converged, **1** forked, **2** trouble
//! (unreadable input, bad usage — clap's own usage errors also exit 2). Errors go to stderr;
//! stdout carries only the result. `demo` keeps the same exit semantics: its pair forks by
//! design, so it exits 1. `serve` exits 2 on any startup failure (before the port binds) and
//! otherwise runs until interrupted.

use amberfork_align::{AlignParams, DiffParams, LexicalCost, diff};
use amberfork_ingest::{IngestError, Ingested};
use amberfork_layout::{Document, ViewModel};
use amberfork_model::{DiffResult, Warning};
use amberfork_server::Server;
use clap::{Args, Parser, Subcommand};
use render::{RenderOpts, resolve_color_mode};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

mod render;

const EXIT_CONVERGED: u8 = 0;
const EXIT_FORKED: u8 = 1;
const EXIT_TROUBLE: u8 = 2;

/// The bundled demo pair (issue #5): a synthetic, hand-authored refund-triage divergence,
/// embedded so `demo` works before any trace of the user's own exists. Parse success and the
/// fork's location are locked by `tests/demo_cli.rs` against these same committed files.
const DEMO_GOOD: &str = include_str!("../assets/demo/good.json");
const DEMO_BAD: &str = include_str!("../assets/demo/bad.json");

/// The demo's hand-off line: its last job is teaching the real command.
const DEMO_HINT: &str =
    "  bundled sample pair · try your own runs:  amberfork diff <bad> --against <good>";

/// The `serve --demo` analog of `DEMO_HINT`: same teaching handoff, pointed at `serve`.
const DEMO_SERVE_HINT: &str =
    "  bundled sample pair · serve your own runs:  amberfork serve <bad> --against <good>";

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

    /// Diff a bundled sample pair — see the fork with zero setup, no files needed.
    Demo(DemoArgs),

    /// Serve the fork in a local web view (127.0.0.1 only) — Ctrl-C to stop.
    Serve(ServeArgs),
}

#[derive(Args)]
struct DiffArgs {
    /// The failing/observed run trace (side `b` of the result).
    #[arg(value_name = "BAD")]
    bad: PathBuf,

    /// The known-good reference run trace (side `a` of the result).
    #[arg(long, value_name = "GOOD")]
    against: PathBuf,

    /// Refuse runs longer than this many steps — alignment memory and time grow with steps²,
    /// so bigger traces are a choice, not a surprise. Raise it to align them anyway.
    #[arg(long, value_name = "N", default_value_t = AlignParams::DEFAULT_MAX_STEPS)]
    max_steps: usize,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Args)]
struct DemoArgs {
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Args)]
struct ServeArgs {
    /// The failing/observed run trace (side `b` of the result). Omit with `--demo`.
    #[arg(
        value_name = "BAD",
        required_unless_present = "demo",
        conflicts_with = "demo"
    )]
    bad: Option<PathBuf>,

    /// The known-good reference run trace (side `a` of the result). Omit with `--demo`.
    #[arg(
        long,
        value_name = "GOOD",
        required_unless_present = "demo",
        conflicts_with = "demo"
    )]
    against: Option<PathBuf>,

    /// Serve the bundled sample pair — the fork in the browser with zero setup, no files
    /// needed (the same embedded pair `demo` renders in the terminal).
    #[arg(long)]
    demo: bool,

    /// Refuse runs longer than this many steps — same escape hatch as `diff` (alignment
    /// memory and time grow with steps²).
    #[arg(long, value_name = "N", default_value_t = AlignParams::DEFAULT_MAX_STEPS)]
    max_steps: usize,

    /// Pin the port. Default is an OS-assigned free port; a busy pinned port is an error,
    /// not a hunt for the next one.
    #[arg(long, value_name = "PORT")]
    port: Option<u16>,

    /// Open the browser once the server is up.
    #[arg(long)]
    open: bool,
}

/// Output flags shared by every result-emitting subcommand, so `--json`/`--no-color` mean the
/// same thing everywhere.
#[derive(Args)]
struct OutputArgs {
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
        Command::Demo(args) => run_demo(&args),
        Command::Serve(args) => run_serve(&args).unwrap_or_else(|err| {
            eprintln!("amberfork: {err}");
            ExitCode::from(EXIT_TROUBLE)
        }),
    }
}

fn run_diff(args: &DiffArgs) -> Result<ExitCode, IngestError> {
    let good = amberfork_ingest::load_file(&args.against)?;
    let bad = amberfork_ingest::load_file(&args.bad)?;
    Ok(diff_and_report(
        good,
        bad,
        args.max_steps,
        &args.output,
        None,
    ))
}

fn run_demo(args: &DemoArgs) -> ExitCode {
    let (good, bad) = demo_pair();
    diff_and_report(
        good,
        bad,
        AlignParams::DEFAULT_MAX_STEPS,
        &args.output,
        Some(DEMO_HINT),
    )
}

/// The single embed site for the bundled pair (design doc D7): `demo` and `serve --demo` both
/// source their traces here, so there is one copy that cannot drift, not two. Infallible by
/// construction — the files are committed next to this crate and `tests/demo_cli.rs` runs them
/// end-to-end, so a parse failure cannot survive CI.
fn demo_pair() -> (Ingested, Ingested) {
    let good = amberfork_ingest::from_json_str(DEMO_GOOD)
        .expect("embedded demo trace good.json parses (locked by demo_cli tests)");
    let bad = amberfork_ingest::from_json_str(DEMO_BAD)
        .expect("embedded demo trace bad.json parses (locked by demo_cli tests)");
    (good, bad)
}

/// The startup order is the contract (issue #25): ingest fails first with its typed errors,
/// then the engine, then the server's own checks (bundle, port) — all in the terminal,
/// before any port is bound. Only a running server produces stdout.
fn run_serve(args: &ServeArgs) -> Result<ExitCode, IngestError> {
    let (good, bad) = if args.demo {
        demo_pair()
    } else {
        // clap guarantees both are present unless `--demo` (required_unless_present), so these
        // `expect`s encode a parser invariant, not a runtime hope.
        let good = amberfork_ingest::load_file(
            args.against
                .as_ref()
                .expect("clap requires --against unless --demo"),
        )?;
        let bad = amberfork_ingest::load_file(
            args.bad.as_ref().expect("clap requires BAD unless --demo"),
        )?;
        (good, bad)
    };
    let mut result = match run_engine(&good, &bad, args.max_steps) {
        Ok(result) => result,
        Err(code) => return Ok(code),
    };
    result.warnings = merged_warnings(good.warnings, bad.warnings);
    let document = Document::new(ViewModel::compute(&result, &good.run, &bad.run));
    Ok(serve_document(&document, args))
}

/// The human label for what a `serve` invocation is showing: the bundled pair for `--demo`,
/// or the two trace paths otherwise (clap guarantees both are present in that branch).
fn serve_source(args: &ServeArgs) -> String {
    if args.demo {
        return "the bundled demo pair".to_string();
    }
    let bad = args.bad.as_ref().expect("clap requires BAD unless --demo");
    let against = args
        .against
        .as_ref()
        .expect("clap requires --against unless --demo");
    format!("{} vs {}", bad.display(), against.display())
}

/// The ONE async edge in this crate: a current-thread runtime wrapping the server's
/// lifetime, so the engine path above it stays sync (design doc: tokio is quarantined to
/// I/O edges).
fn serve_document(document: &Document, args: &ServeArgs) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("amberfork: cannot start the async runtime: {err}");
            return ExitCode::from(EXIT_TROUBLE);
        }
    };
    runtime.block_on(async {
        let server = match Server::bind(document, args.port.unwrap_or(0)).await {
            Ok(server) => server,
            Err(err) => {
                eprintln!("amberfork: {err}");
                return ExitCode::from(EXIT_TROUBLE);
            }
        };
        // The pinned handoff (#25 amendment): the verdict lands in the terminal BEFORE any
        // browser opens — the web view elaborates the answer, it never gates it.
        let url = format!("http://{}", server.local_addr());
        println!("{}", document.view.headline());
        println!("serving {} → {url}  (Ctrl-C to stop)", serve_source(args));
        if args.demo {
            // The demo's last job is teaching the real command — the serve analog of DEMO_HINT.
            println!("{DEMO_SERVE_HINT}");
        }
        for warning in &document.view.warnings {
            eprintln!("amberfork: warning: {}", warning.msg);
        }
        if args.open
            && let Err(err) = webbrowser::open(&url)
        {
            // The browser is convenience; the server it points at must outlive its absence.
            eprintln!("amberfork: warning: cannot open a browser: {err}");
        }
        match server.serve().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("amberfork: {err}");
                ExitCode::from(EXIT_TROUBLE)
            }
        }
    })
}

/// Params + engine, shared by every subcommand. The single decision point for engine params:
/// anything user-supplied (--max-steps today, future --tau style flags) routes through
/// validated() here, and both param and engine refusals (today: the size guard) map to exit
/// 2 with the same stderr shape as an unreadable input — the engine itself never asserts.
fn run_engine(good: &Ingested, bad: &Ingested, max_steps: usize) -> Result<DiffResult, ExitCode> {
    let params = DiffParams {
        align: AlignParams {
            max_steps,
            ..AlignParams::default()
        },
        ..DiffParams::default()
    };
    let params = params.validated().map_err(|err| {
        eprintln!("amberfork: {err}");
        ExitCode::from(EXIT_TROUBLE)
    })?;
    diff(&good.run, &bad.run, &LexicalCost, &params).map_err(|err| {
        eprintln!("amberfork: {err}");
        ExitCode::from(EXIT_TROUBLE)
    })
}

/// The shared back half of `diff`/`demo`: run the engine on a loaded pair, emit the result
/// (render or `--json`), and map it to the `diff(1)` exit code. `footer` is an optional
/// trailing line for the human render only — `--json` stays the pure machine contract.
fn diff_and_report(
    good: Ingested,
    bad: Ingested,
    max_steps: usize,
    output: &OutputArgs,
    footer: Option<&str>,
) -> ExitCode {
    let mut result = match run_engine(&good, &bad, max_steps) {
        Ok(result) => result,
        Err(code) => return code,
    };
    result.warnings = merged_warnings(good.warnings, bad.warnings);

    if output.json {
        let json = serde_json::to_string_pretty(&result)
            .expect("DiffResult serialization is infallible (no non-string map keys)");
        println!("{json}");
    } else {
        let color = resolve_color_mode(
            output.no_color,
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
        // The seam in one line: semantics from the layout crate, styling from the painter.
        let view = ViewModel::compute(&result, &good.run, &bad.run);
        print!("{}", render::render(&view, &opts));
        if let Some(footer) = footer {
            // Chrome, not result: dim so it never competes with the amber fork.
            println!("{}", color.dim(footer));
        }
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
    ExitCode::from(code)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The D7 identity anchor: `demo` and `serve --demo` both source their pair from this one
    /// `demo_pair()` — there is a single embed site, not two that can drift. This locks the
    /// infallibility both call sites `.expect()` on, and proves the shared bytes are the real
    /// authored divergence (a forking pair), not an empty or degenerate one.
    #[test]
    fn demo_pair_is_the_single_forking_source() {
        let (good, bad) = demo_pair();
        let params = DiffParams::default()
            .validated()
            .expect("default params validate");
        let result =
            diff(&good.run, &bad.run, &LexicalCost, &params).expect("the demo pair aligns");
        assert!(
            result.fork.is_some(),
            "the embedded demo pair must encode a real divergence"
        );
    }
}
