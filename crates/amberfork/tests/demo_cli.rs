//! End-to-end contract of `amberfork demo` (issue #5, slice 1).
//!
//! `demo` is the zero-setup first contact (DX journey "hello world"): the bundled divergent
//! pair is embedded in the binary at compile time, so the command needs no files, no network,
//! and no particular working directory — install → run → see the amber fork. The contract
//! under test:
//! - `demo` runs from an unrelated cwd with no arguments and exits 1, because the bundled
//!   pair encodes a real divergence (same `diff(1)` exit semantics as `diff`);
//! - `--json` emits a deserializable [`amberfork_model::DiffResult`] locating the fork at the
//!   authored gold step (the demo manifest mirrors the smoke-fixture manifest shape);
//! - the bundled pair is clean: a demo that prints ingest warnings looks broken, so stderr
//!   must be empty;
//! - the human render ends with the hand-off hint to `amberfork diff`, and `--no-color` output
//!   is snapshot-locked exactly like the smoke render (the demo IS the README GIF script).

use amberfork_model::DiffResult;
use assert_cmd::Command;
use std::path::{Path, PathBuf};

const EXIT_FORKED: i32 = 1;

/// The committed demo assets — the same files `main.rs` embeds via `include_str!`.
fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/demo")
}

/// `demo` must not depend on the working directory, so every test runs it from a cwd that
/// contains none of the repo's files.
fn amberfork_demo() -> Command {
    let mut cmd = Command::cargo_bin("amberfork").expect("amberfork binary builds");
    cmd.current_dir(std::env::temp_dir()).arg("demo");
    cmd
}

/// The authored fork location, committed next to the traces (manifest shape shared with the
/// smoke fixtures): the observed-run step index the fork must land on.
fn gold_step() -> usize {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(assets_dir().join("pair_demo.json")).unwrap(),
    )
    .unwrap();
    manifest["gold_step"].as_u64().expect("gold_step") as usize
}

#[test]
fn demo_runs_without_files_and_json_locates_the_authored_fork() {
    let assert = amberfork_demo().arg("--json").assert().code(EXIT_FORKED);

    let output = assert.get_output();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let result: DiffResult =
        serde_json::from_str(&stdout).expect("--json stdout must be a valid DiffResult");

    let fork = result
        .fork
        .expect("the bundled pair encodes a real divergence");
    assert_eq!(
        fork.b_step,
        Some(gold_step()),
        "fork must land on observed-run step {} (demo manifest gold)",
        gold_step()
    );
    assert!(
        result.warnings.is_empty(),
        "the bundled pair must ingest warning-free, got: {:?}",
        result.warnings
    );
}

#[test]
fn demo_render_hands_off_to_real_usage_with_clean_stderr() {
    let assert = amberfork_demo()
        .arg("--no-color")
        .assert()
        .code(EXIT_FORKED);

    let output = assert.get_output();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(
        stdout.contains("amberfork diff <bad> --against <good>"),
        "the demo's last job is teaching the real command, got:\n{stdout}"
    );
    assert!(
        output.stderr.is_empty(),
        "a demo that prints warnings looks broken: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn demo_render_matches_snapshot() {
    let assert = amberfork_demo()
        .arg("--no-color")
        .assert()
        .code(EXIT_FORKED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    insta::assert_snapshot!("demo_no_color", stdout);
}
