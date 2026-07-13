//! Startup contract of `amberfork serve --demo` (issue #28, slice A).
//!
//! `serve --demo` is the zero-setup browser hero: the same embedded pair `demo` renders in the
//! terminal, handed to the local web view instead — no files, no cwd, no network. Like
//! `serve_cli.rs`, what an integration test can pin on a dev build (empty `ui-dist/`) is the
//! STARTUP order: on a clean pair the run reaches the bundle-missing check before any port
//! binds, and reaching it here — with NO file arguments — proves the embedded pair loaded and
//! the engine ran off it. The happy-path boot over a real bundle is #28's release-smoke
//! acceptance (slice B). The two arg-validation tests pin the mode contract: exactly one of
//! `--demo` or `<bad> --against <good>`, enforced by clap (exit 2, nothing on stdout).

use assert_cmd::Command;
use predicates::prelude::*;

const EXIT_TROUBLE: i32 = 2;

/// Runs `serve` from an unrelated cwd, so a passing `--demo` test proves independence from the
/// working directory exactly as `demo` does — the embedded pair is the only input.
fn amberfork_serve() -> Command {
    let mut cmd = Command::cargo_bin("amberfork").expect("amberfork binary builds");
    cmd.current_dir(std::env::temp_dir()).arg("serve");
    cmd
}

#[test]
fn serve_demo_needs_no_files_and_reaches_the_bundle_check() {
    // No BAD, no --against, an unrelated cwd: the only way past ingest+engine to the
    // bundle-missing error is the embedded pair. (A dev build's ui-dist/ is empty by design;
    // the happy path over a real bundle is slice B's release smoke.)
    amberfork_serve()
        .arg("--demo")
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("web UI bundle missing"));
}

#[test]
fn serve_demo_conflicts_with_explicit_traces() {
    // --demo carries its own pair; passing traces too is an ambiguous request, not a merge.
    amberfork_serve()
        .arg("--demo")
        .arg("bad.json")
        .arg("--against")
        .arg("good.json")
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn serve_without_demo_or_traces_is_a_usage_error() {
    // Neither mode chosen: clap refuses with the required-argument error, before any I/O.
    amberfork_serve()
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("required"));
}
