//! End-to-end contract of `diff --judge local|off` (issue #10).
//!
//! Three cases stay fully offline and always run: the default (`off`) is byte-identical to no
//! flag at all; a converged diff answers "no divergence to explain" without ever dialing
//! out (`OllamaJudge` short-circuits before building a request — see
//! `amberfork-judge/src/ollama.rs`); and `--json` never carries the AI line, regardless of
//! `--judge`, because the explain layer is not part of the machine contract. The one case that
//! needs a real model is `#[ignore]`d, mirroring `verify_cli.rs` (#44)'s local-Ollama discipline.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const EXIT_CONVERGED: i32 = 0;
const EXIT_FORKED: i32 = 1;
const OLLAMA_ADDR: &str = "127.0.0.1:11434";

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

fn ollama_reachable() -> bool {
    TcpStream::connect_timeout(&OLLAMA_ADDR.parse().unwrap(), Duration::from_millis(500)).is_ok()
}

#[test]
fn judge_off_is_the_untouched_default() {
    let (bad, good) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_FORKED)
        .stdout(predicates::str::contains("AI (").not());
}

#[test]
fn judge_local_on_a_converged_diff_needs_no_network() {
    // Self-diff always converges; `OllamaJudge` answers "no divergence to explain" before ever
    // building a request (fork is None), so this is deterministic with no Ollama server running.
    let (bad, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&bad)
        .arg("--judge")
        .arg("local")
        .assert()
        .code(EXIT_CONVERGED)
        .stdout(predicates::str::contains(
            "AI (unverified): no divergence to explain",
        ));
}

#[test]
fn judge_local_never_joins_the_json_contract() {
    let (bad, good) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .arg("--judge")
        .arg("local")
        .assert()
        .code(EXIT_FORKED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains("AI ("),
        "--json must stay the pure machine contract regardless of --judge"
    );
    serde_json::from_str::<amberfork_model::DiffResult>(&stdout)
        .expect("--json stdout is still a valid DiffResult");
}

#[test]
#[ignore = "network: drives a real local Ollama server, see verify_cli.rs's recipe (same server/model)"]
fn judge_local_narrates_a_real_fork_against_a_real_local_provider() {
    assert!(
        ollama_reachable(),
        "Ollama is not reachable at {OLLAMA_ADDR} — start it with `ollama serve` and \
         `ollama pull smollm2:135m` first"
    );
    let (bad, good) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--judge")
        .arg("local")
        .assert()
        .code(EXIT_FORKED)
        .stdout(predicates::str::contains("AI (unverified): "))
        .stdout(predicates::str::contains("AI (unverified): no divergence to explain").not());
}
