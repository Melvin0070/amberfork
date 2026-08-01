//! End-to-end contract of `amberfork diff --html <path>` (issue #29): the flag writes a
//! self-contained static export alongside the normal terminal render, independent of it, and
//! fails loudly (not silently) when the path can't be written.

use assert_cmd::Command;
use std::path::{Path, PathBuf};

const EXIT_FORKED: i32 = 1;
const EXIT_TROUBLE: i32 = 2;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/fixtures/smoke")
}

fn amberfork() -> Command {
    Command::cargo_bin("amberfork").expect("amberfork binary builds")
}

fn manifest() -> (PathBuf, PathBuf) {
    let dir = fixture_dir();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pair_smoke.json")).unwrap())
            .unwrap();
    let bad = dir.join(manifest["failing"].as_str().unwrap());
    let good = dir.join(manifest["reference"].as_str().unwrap());
    (bad, good)
}

#[test]
fn html_flag_writes_a_self_contained_page_and_still_prints_the_terminal_render() {
    let (bad, good) = manifest();
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("fork.html");

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--html")
        .arg(&out)
        .assert()
        .code(EXIT_FORKED);

    // --html is a side effect, not an alternate output mode: the terminal render still happens.
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("[FORK") && stdout.contains("attribution ·"),
        "the normal terminal render is unaffected by --html: {stdout}"
    );

    let html = std::fs::read_to_string(&out).expect("the file was written");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("row--fork"), "the fork renders: {html}");
    assert!(
        !html.contains("<link") && !html.contains("<script"),
        "self-contained: no external resource, no script: {html}"
    );
}

#[test]
fn html_combines_with_json_without_suppressing_either_output() {
    let (bad, good) = manifest();
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("fork.html");

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .arg("--html")
        .arg(&out)
        .assert()
        .code(EXIT_FORKED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout is still pure JSON");
    assert!(out.exists(), "the html file is written even under --json");
}

#[test]
fn an_unwritable_html_path_is_trouble_not_silence() {
    let (bad, good) = manifest();
    // A directory that doesn't exist: `fs::write` fails, and the failure must be loud.
    let out = Path::new("/nonexistent-dir-for-amberfork-html-export-test/fork.html");

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--html")
        .arg(out)
        .assert()
        .code(EXIT_TROUBLE)
        .stderr(predicates::str::contains("--html"));
}
