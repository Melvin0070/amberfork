//! End-to-end contract of the human render (issue #4, slice 2), on the real binary and the
//! committed smoke fixtures.
//!
//! The snapshot locks the terminal layout as an artifact: any change to the render is a
//! deliberate, reviewed snapshot update, exactly like a schema change. `--no-color` keeps the
//! snapshot byte-stable, and a piped (non-TTY) stdout must never carry ANSI even when the
//! environment advertises truecolor.

use assert_cmd::Command;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/fixtures/smoke")
}

/// Fixture paths from the committed manifest: (failing/bad, reference/good).
fn pair() -> (PathBuf, PathBuf) {
    let dir = fixture_dir();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pair_smoke.json")).unwrap())
            .unwrap();
    (
        dir.join(manifest["failing"].as_str().unwrap()),
        dir.join(manifest["reference"].as_str().unwrap()),
    )
}

#[test]
fn smoke_pair_render_matches_snapshot() {
    let (bad, good) = pair();

    let assert = Command::cargo_bin("amberfork")
        .unwrap()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--no-color")
        .assert()
        .code(1);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    insta::assert_snapshot!("smoke_pair_no_color", stdout);
}

#[test]
fn piped_stdout_never_carries_ansi_even_under_truecolor_env() {
    let (bad, good) = pair();

    let assert = Command::cargo_bin("amberfork")
        .unwrap()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .assert()
        .code(1);

    let output = assert.get_output();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(
        !stdout.contains('\x1b'),
        "non-TTY stdout must drop ANSI (structure, not color, is the contract)"
    );
    // Structure survives: the fork glyph and tag are still there.
    assert!(stdout.contains('⑂') && stdout.contains("[FORK"));
}
