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
    let mut cmd = Command::cargo_bin("amberfork").expect("amberfork binary builds");
    cmd.arg("replay").arg(cassette).args([
        "--base-url-env",
        "AMBERFORK_TEST_BASE_URL",
        "--",
        "python3",
        "-c",
        agent,
    ]);
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
