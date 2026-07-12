//! Startup contract of `amberfork serve <bad> --against <good>` (issue #25, slice 2).
//!
//! `serve` is long-running, so what an integration test can pin is the STARTUP order, and
//! these two tests do it as a pair: the same invocation fails with ingest's typed error when
//! a trace is unreadable, and with the bundle-missing message when the traces are fine —
//! proof that ingest runs before the bundle check, which runs before any port is bound
//! (nothing ever reaches stdout in either case). The serving behaviors themselves are
//! covered at the lib layer in `amberfork-server`; the happy-path e2e over a real bundle is
//! #28's release-smoke acceptance.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

const EXIT_TROUBLE: i32 = 2;

fn amberfork() -> Command {
    Command::cargo_bin("amberfork").expect("amberfork binary builds")
}

/// The committed demo pair doubles as a valid on-disk trace pair for startup tests.
fn demo_pair() -> (PathBuf, PathBuf) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/demo");
    (dir.join("bad.json"), dir.join("good.json"))
}

#[test]
fn unreadable_trace_fails_fast_with_the_typed_ingest_error() {
    let (_, good) = demo_pair();
    amberfork()
        .arg("serve")
        .arg("definitely/not/a/trace.json")
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("definitely/not/a/trace.json"));
}

#[test]
fn valid_pair_without_a_ui_bundle_refuses_before_binding() {
    // A dev build has an empty ui-dist/ by design; reaching THIS error on a valid pair
    // proves ingest and the engine already ran (the pair above proves ingest runs first).
    let (bad, good) = demo_pair();
    amberfork()
        .arg("serve")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("web UI bundle missing"));
}
