//! End-to-end contract of `amberfork replay <cassette> -- <cmd>` in strict (offline) mode.
//!
//! `replay` serves a recorded run back to the agent: every request the agent issues that is on
//! the tape gets its recorded response, with no provider, no network and no API key. The first
//! request that is *not* on the tape is where the agent's behaviour changed — which makes replay
//! the gate two independent live runs cannot be (notebook 084: those fork 97% of the time even
//! when nothing is wrong).
//!
//! The exit contract is `diff`'s, not `record`'s: 0 the agent followed the tape, 1 it diverged,
//! 2 trouble. The agent's own exit code is reported, never propagated — a recording of a failing
//! run replays to the same failure, and that is "followed the tape".
//!
//! Hermetic: the agent is a `python3` script (already a hard dependency of the verify gate) and
//! nothing listens but replay's own loopback port.

use assert_cmd::Command;
use predicates::str::contains;
use std::path::{Path, PathBuf};

const EXIT_FOLLOWED: i32 = 0;
const EXIT_DIVERGED: i32 = 1;
const EXIT_TROUBLE: i32 = 2;

/// A two-turn recording: the second request carries the first response, as an agent loop does.
fn write_cassette(dir: &Path) -> PathBuf {
    let cassette = serde_json::json!({
        "cassette_version": "0.1",
        "id": "good",
        "exchanges": [
            {"idx": 0,
             "request": {"method": "POST", "path": "/v1/messages", "headers": [],
                         "body": {"messages": [{"role": "user", "content": "plan"}]}},
             "response": {"status": 200, "headers": [], "body": {"text": "step one"}}},
            {"idx": 1,
             "request": {"method": "POST", "path": "/v1/messages", "headers": [],
                         "body": {"messages": [{"role": "user", "content": "plan"},
                                               {"role": "assistant", "content": "step one"},
                                               {"role": "user", "content": "continue"}]}},
             "response": {"status": 200, "headers": [], "body": {"text": "done"}}}
        ]
    });
    let path = dir.join("good.cassette.json");
    std::fs::write(&path, cassette.to_string()).unwrap();
    path
}

/// A stand-in agent: turn one asks for a plan; turn two (unless `turns` is 1) sends the history
/// back with `followup` appended. `followup = "continue"` reproduces the recording exactly.
fn agent_src(turns: u8, followup: &str) -> String {
    format!(
        r#"import os, json, urllib.request
base = os.environ["AMBERFORK_TEST_BASE_URL"]
def ask(messages):
    data = json.dumps({{"messages": messages}}).encode()
    req = urllib.request.Request(base + "/v1/messages", data=data, headers={{"content-type": "application/json"}})
    return json.loads(urllib.request.urlopen(req).read())["text"]
history = [{{"role": "user", "content": "plan"}}]
first = ask(history)
if {turns} > 1:
    history += [{{"role": "assistant", "content": first}}, {{"role": "user", "content": "{followup}"}}]
    ask(history)
"#
    )
}

fn replay(cassette: &Path, agent: &str) -> Command {
    replay_with(cassette, agent, &[])
}

/// `replay` with extra options, which must precede the `--` that starts the agent command.
fn replay_with(cassette: &Path, agent: &str, options: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("amberfork").expect("amberfork binary builds");
    cmd.arg("replay")
        .arg(cassette)
        .args(["--base-url-env", "AMBERFORK_TEST_BASE_URL"])
        .args(options)
        .args(["--", "python3", "-c", agent]);
    cmd
}

#[test]
fn an_unchanged_agent_follows_the_tape_and_exits_0_offline() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());

    replay(&cassette, &agent_src(2, "continue"))
        .assert()
        .code(EXIT_FOLLOWED)
        .stderr(contains("followed the tape"))
        .stderr(contains("2 of 2"));
}

#[test]
fn a_changed_request_exits_1_and_names_the_exchange_where_it_diverged() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());

    replay(&cassette, &agent_src(2, "try a different approach"))
        .assert()
        .code(EXIT_DIVERGED)
        .stderr(contains("diverged at exchange 1"))
        .stderr(contains("POST /v1/messages"));
}

#[test]
fn an_agent_that_stops_early_exits_1_even_though_every_request_hit() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());

    replay(&cassette, &agent_src(1, "unused"))
        .assert()
        .code(EXIT_DIVERGED)
        .stderr(contains("1 of 2"));
}

#[test]
fn a_file_that_is_not_a_cassette_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("trace.json");
    std::fs::write(&trace, r#"{"schema_version":"0.1","id":"x","steps":[]}"#).unwrap();

    replay(&trace, &agent_src(1, "unused"))
        .assert()
        .code(EXIT_TROUBLE)
        .stderr(contains("cassette"));
}

#[test]
fn an_agent_that_cannot_be_started_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());

    Command::cargo_bin("amberfork")
        .unwrap()
        .arg("replay")
        .arg(&cassette)
        .args([
            "--base-url-env",
            "X",
            "--",
            "amberfork-no-such-agent-binary",
        ])
        .assert()
        .code(EXIT_TROUBLE)
        .stderr(contains("cannot run agent"));
}

// ── with --upstream / --out ──────────────────────────────────────────────────────────────────

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

/// What the stub provider answers to any request that leaves the tape.
const LIVE_BODY: &str = r#"{"text":"live answer"}"#;

/// A canned-response provider on loopback, for the off-tape requests `--upstream` relays. Reads
/// each request through its declared body before answering, so the relay never sees a reset.
fn spawn_stub_upstream() -> SocketAddr {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let declared = text[..head_end]
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if buf.len() >= head_end + 4 + declared {
                        break;
                    }
                }
            }
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{LIVE_BODY}",
                    LIVE_BODY.len()
                )
                .as_bytes(),
            );
        }
    });
    addr
}

fn exchanges(cassette: &Path) -> Vec<serde_json::Value> {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cassette).unwrap()).unwrap();
    doc["exchanges"].as_array().expect("a cassette").clone()
}

#[test]
fn an_unchanged_agent_writes_a_reexecuted_cassette_identical_to_the_tape() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());
    let out = dir.path().join("candidate.cassette.json");

    replay_with(
        &cassette,
        &agent_src(2, "continue"),
        &["--out", out.to_str().unwrap()],
    )
    .assert()
    .code(EXIT_FOLLOWED);

    let (got, tape) = (exchanges(&out), exchanges(&cassette));
    assert_eq!(got.len(), tape.len());
    for (got, tape) in got.iter().zip(&tape) {
        // Headers are the session's (record's allowlist decides what survives), not the run's.
        assert_eq!(got["request"]["body"], tape["request"]["body"]);
        assert_eq!(got["response"]["body"], tape["response"]["body"]);
    }
    // And the gate downstream agrees: the candidate converges on its reference.
    Command::cargo_bin("amberfork")
        .unwrap()
        .arg("diff")
        .arg(&out)
        .arg("--against")
        .arg(&cassette)
        .assert()
        .code(EXIT_FOLLOWED);
}

#[test]
fn with_an_upstream_a_changed_agent_runs_on_live_and_the_result_diffs_against_the_tape() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());
    let out = dir.path().join("candidate.cassette.json");
    let upstream = spawn_stub_upstream();

    let upstream = format!("http://{upstream}");
    replay_with(
        &cassette,
        &agent_src(2, "try a different approach"),
        &[
            "--upstream",
            &upstream,
            "--out",
            out.to_str().unwrap(),
            "--id",
            "candidate",
        ],
    )
    .assert()
    .code(EXIT_DIVERGED)
    .stderr(contains("diverged at exchange 1"))
    .stderr(contains("relayed"));

    let got = exchanges(&out);
    assert_eq!(got.len(), 2, "the run carried on past the divergence");
    assert_eq!(
        got[0]["response"],
        exchanges(&cassette)[0]["response"],
        "the shared prefix is answered from the tape, not the provider"
    );
    assert_eq!(got[1]["response"]["body"]["text"], "live answer");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(doc["id"], "candidate");

    // The whole workflow: record once, replay the changed agent, diff the two cassettes.
    Command::cargo_bin("amberfork")
        .unwrap()
        .arg("diff")
        .arg(&out)
        .arg("--against")
        .arg(&cassette)
        .assert()
        .code(EXIT_DIVERGED);
}

#[test]
fn a_strict_replay_still_writes_the_on_tape_prefix_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = write_cassette(dir.path());
    let out = dir.path().join("candidate.cassette.json");

    replay_with(
        &cassette,
        &agent_src(2, "try a different approach"),
        &["--out", out.to_str().unwrap()],
    )
    .assert()
    .code(EXIT_DIVERGED);

    assert_eq!(
        exchanges(&out).len(),
        1,
        "only the request the tape could answer"
    );
}

// ── the committed example docs/ci.md quotes ─────────────────────────────────────────────────

fn example(system_prompt: Option<&str>) -> Command {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/ci-gate");
    let mut cmd = Command::cargo_bin("amberfork").expect("amberfork binary builds");
    cmd.current_dir(&dir)
        .env_remove("AGENT_SYSTEM_PROMPT")
        .args([
            "replay",
            "reference.cassette.json",
            "--base-url-env",
            "AGENT_BASE_URL",
        ])
        .args(["--", "python3", "agent.py"]);
    if let Some(prompt) = system_prompt {
        cmd.env("AGENT_SYSTEM_PROMPT", prompt);
    }
    cmd
}

#[test]
fn the_documented_example_follows_its_recording_and_diverges_when_the_prompt_changes() {
    // docs/ci.md prints both transcripts; this keeps them true.
    example(None)
        .assert()
        .code(EXIT_FOLLOWED)
        .stderr(contains("followed the tape — 2 of 2"));
    example(Some("You are a refund assistant. Be strict."))
        .assert()
        .code(EXIT_DIVERGED)
        .stderr(contains("diverged at exchange 0"));
}
