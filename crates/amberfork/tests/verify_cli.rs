//! Real-provider end-to-end coverage for `diff --verify` (issue #44) — the acknowledged testing
//! boundary from notebook 038: no test has ever driven `--verify` through a real subprocess *and*
//! a real network provider. Every layer of the counterfactual mechanism is covered offline with
//! in-process stubs (`amberfork-attrib`'s own suite); this is the one test proving the wiring
//! (subprocess spawn, loopback replay listener, live relay past the branch) actually works against
//! real infrastructure — not a claim about *what* a re-execution should decide.
//!
//! Opt-in and network-gated (`#[ignore]`), the same discipline `amberfork-bench`'s network tests
//! use (CLAUDE.md). Needs a local Ollama server with `smollm2:135m` pulled:
//! ```text
//! brew install ollama
//! ollama serve &
//! ollama pull smollm2:135m
//! cargo test -p amberfork --test verify_cli -- --ignored
//! ```
//! `cargo test --workspace` never runs this by default, so the offline/deterministic gate is
//! unaffected.

use amberfork_model::{AttributionMode, DiffResult};
use assert_cmd::Command;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const OLLAMA_URL: &str = "http://127.0.0.1:11434";
const OLLAMA_ADDR: &str = "127.0.0.1:11434";
const BASE_URL_ENV: &str = "AMBERFORK_VERIFY_BASE_URL";
/// Bounded retries recording `bad`: temperature-driven sampling usually diverges from `good`
/// within a try or two, but real nondeterminism means "usually" — an honest test allows for that
/// rather than asserting exact LLM behavior, mirroring the tri-state `Unverified` philosophy
/// elsewhere in this crate: real nondeterminism is a fact to work with, not paper over.
const MAX_FORK_ATTEMPTS: u32 = 6;

fn agent_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/verify_agent.py")
}

fn amberfork() -> Command {
    Command::cargo_bin("amberfork").expect("amberfork binary builds")
}

fn ollama_reachable() -> bool {
    TcpStream::connect_timeout(&OLLAMA_ADDR.parse().unwrap(), Duration::from_millis(500)).is_ok()
}

fn amberfork_record(out: &Path, id: &str) -> Command {
    let mut cmd = amberfork();
    cmd.arg("record")
        .arg("--upstream")
        .arg(OLLAMA_URL)
        .arg("--base-url-env")
        .arg(BASE_URL_ENV)
        .arg("--out")
        .arg(out)
        .arg("--id")
        .arg(id)
        .arg("--")
        .arg("python3")
        .arg(agent_script());
    cmd
}

fn diff_result(stdout: &[u8]) -> DiffResult {
    serde_json::from_slice(stdout).expect("--json stdout is a valid DiffResult")
}

#[test]
#[ignore = "network: drives a real subprocess against a local Ollama server, see this file's doc comment for the recipe"]
fn verify_reports_an_honest_recovery_verdict_against_a_real_local_provider() {
    assert!(
        ollama_reachable(),
        "Ollama is not reachable at {OLLAMA_URL} — start it with `ollama serve` and \
         `ollama pull smollm2:135m` first (see this file's doc comment)"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let good = dir.path().join("good.cassette.json");
    let bad = dir.path().join("bad.cassette.json");

    amberfork_record(&good, "good").assert().code(0);

    // The counterfactual patch only ever swaps a *response*, never a request (patch.rs) — so the
    // fork this test needs has to come from real sampling variance between two recordings of the
    // identical prompts, not from the script asking a different question. Retry recording `bad`
    // until that happens.
    let mut forked = false;
    for attempt in 1..=MAX_FORK_ATTEMPTS {
        amberfork_record(&bad, "bad").assert().code(0);
        let probe = amberfork()
            .arg("diff")
            .arg(&bad)
            .arg("--against")
            .arg(&good)
            .arg("--json")
            .output()
            .expect("diff runs");
        if diff_result(&probe.stdout).fork.is_some() {
            forked = true;
            break;
        }
        eprintln!("attempt {attempt}/{MAX_FORK_ATTEMPTS}: good/bad recordings converged, retrying");
    }
    assert!(
        forked,
        "the real model answered identically across {MAX_FORK_ATTEMPTS} independent recordings — \
         rerun the test; temperature-driven sampling should diverge within a few tries"
    );

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--verify")
        .arg("--json")
        .arg("--upstream")
        .arg(OLLAMA_URL)
        .arg("--base-url-env")
        .arg(BASE_URL_ENV)
        .arg("--runs")
        .arg("3")
        .arg("--")
        .arg("python3")
        .arg(agent_script())
        .assert()
        .code(1); // still `diff(1)`-forked — verify upgrades attribution, not the exit code

    let result = diff_result(&assert.get_output().stdout);
    let attribution = result
        .attribution
        .expect("a two-sided fork with --verify always upgrades to an attribution");
    assert_eq!(attribution.mode, AttributionMode::Counterfactual);
    let counterfactual = attribution
        .counterfactual
        .expect("counterfactual mode always carries counterfactual evidence");
    assert_eq!(counterfactual.runs, 3);
    // The recovery verdict itself (Recovered/NotRecovered/Unverified) is real nondeterministic LLM
    // behavior — asserting a specific one would be asserting a fact about the model, not the
    // pipeline. This test proves the real subprocess + real network path produces *some* honest
    // tri-state verdict, closing notebook 038's coverage gap.
    eprintln!("counterfactual verdict: {:?}", counterfactual.recovered);
}
