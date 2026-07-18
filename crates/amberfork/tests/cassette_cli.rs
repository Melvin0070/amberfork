//! End-to-end contract of `amberfork diff` auto-detecting a cassette (issue #33, the deferred
//! CLI-seam checkpoint).
//!
//! `normalize(&Cassette) -> Run` (issue #33 core) turns a captured cassette into the canonical
//! model, but nothing called it. This is the seam that closes that gap: `amberfork diff` sniffs
//! each input's top-level shape and, when it carries a `cassette_version`, normalizes it and
//! aligns it through exactly the same engine as a passive trace. The founder chose auto-detect
//! over an explicit `convert` step (a cassette is a first-party, self-versioning amberfork
//! artifact, not a foreign shape), so the contract under test is:
//! - a forking cassette pair diffs with zero ceremony — one command, no intermediate file —
//!   and exits 1, locating the fork at the manifest's gold step;
//! - a file that carries a `cassette_version` but is not a valid cassette fails with a
//!   cassette-specific error on stderr and exit 2, not the misleading "not a canonical trace".

use amberfork_model::DiffResult;
use assert_cmd::Command;
use std::io::Write;
use std::path::{Path, PathBuf};

const EXIT_FORKED: i32 = 1;
const EXIT_TROUBLE: i32 = 2;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cassette")
}

fn amberfork() -> Command {
    Command::cargo_bin("amberfork").expect("amberfork binary builds")
}

/// The committed manifest: which cassette is the failing side and where the fork truly is.
fn manifest() -> (PathBuf, PathBuf, usize) {
    let dir = fixture_dir();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pair.json")).unwrap()).unwrap();
    let bad = dir.join(manifest["failing"].as_str().unwrap());
    let good = dir.join(manifest["reference"].as_str().unwrap());
    let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;
    (bad, good, gold)
}

#[test]
fn forked_cassette_pair_exits_1_and_json_locates_the_fork() {
    let (bad, good, gold) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .assert()
        .code(EXIT_FORKED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult =
        serde_json::from_str(&stdout).expect("--json stdout must be a valid DiffResult");

    let fork = result
        .fork
        .expect("the cassette pair encodes a real divergence");
    assert_eq!(
        fork.b_step,
        Some(gold),
        "fork must land on observed-run step {gold} (manifest gold)"
    );
}

#[test]
fn malformed_cassette_fails_with_a_cassette_specific_error() {
    // Valid JSON that carries a `cassette_version` (so the sniff routes it to the cassette path)
    // but whose shape is broken. The error must name the cassette, not fall through to the
    // canonical loader's "not a canonical trace" — routing by the version key means owning the
    // error text for what that key selected. The temp file gets a neutral `.json` name on
    // purpose: the sniff keys on the `cassette_version` field, not the filename, so "cassette"
    // in stderr can only come from the error's own words, never from the echoed path.
    let mut file = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp cassette");
    file.write_all(br#"{"cassette_version":"0.1","id":"broken","exchanges":"not-an-array"}"#)
        .expect("write temp cassette");
    let path = file.path();
    let (_bad, good, _gold) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(path)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE);

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.to_lowercase().contains("cassette"),
        "a broken cassette must fail with a cassette-specific error, got: {stderr}"
    );
}
