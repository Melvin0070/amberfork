//! End-to-end contract of `amberfork record -- <cmd>` (issue #34): the record path's capture
//! verb, the last unbuilt half of v0.6.
//!
//! `record` binds the loopback capture proxy, runs the agent as a subprocess with a base-URL env
//! var pointed at the proxy, waits for it to exit, and writes the captured cassette — the file
//! `amberfork diff` now auto-detects (#33), so the record path closes end-to-end.
//!
//! The test is hermetic: no real provider, no API key. A stub *upstream* (a hand-rolled loopback
//! HTTP server returning canned JSON) stands in for the provider, and a `python3` one-liner is the
//! *agent* — python3 is already a hard dependency of this repo's verify gate (`spike/test_smoke.py`),
//! so it is guaranteed present without a new crate or a reliance on `curl`. The contract:
//! - a run that makes one provider call yields a cassette with exactly that exchange, full-content
//!   on both sides, at the current cassette version;
//! - the agent's exit code propagates, and the cassette is written **even when the agent fails** —
//!   a failed run is precisely the one worth recording.

use assert_cmd::Command;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

/// The canned provider response the stub upstream returns for every request.
const UPSTREAM_BODY: &str = r#"{"role":"assistant","content":[{"type":"text","text":"stub upstream ok"}],"stop_reason":"end_turn"}"#;

/// A `python3` stand-in agent: read the proxy's base URL from the env var `record` set, POST one
/// provider-shaped request through it, then exit with `exit_code`. The exit lets one source cover
/// both the happy path and the "agent failed, cassette still written" path.
fn agent_src(exit_code: u8) -> String {
    format!(
        r#"import os, sys, json, urllib.request
base = os.environ["AMBERFORK_TEST_BASE_URL"]
data = json.dumps({{"model": "claude-sonnet-5", "messages": [{{"role": "user", "content": "ping"}}]}}).encode()
req = urllib.request.Request(base + "/v1/messages", data=data, headers={{"content-type": "application/json"}})
urllib.request.urlopen(req).read()
sys.exit({exit_code})
"#
    )
}

/// Bind a canned-response HTTP server on loopback and serve it on a detached thread; return its
/// address for `--upstream`. Reads each request fully (headers + declared body) before replying,
/// so the proxy's forwarding client never sees a reset, then closes the connection.
fn spawn_stub_upstream() -> SocketAddr {
    let listener =
        TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).expect("stub upstream binds");
    let addr = listener.local_addr().expect("stub upstream addr");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            serve_one(stream);
        }
    });
    addr
}

/// Handle a single request/response on one connection: drain the request, answer with the canned
/// JSON, close.
fn serve_one(mut stream: TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    // Read until the header terminator, then the Content-Length body — enough that the forwarding
    // client's write completes before we answer.
    loop {
        let Ok(n) = stream.read(&mut chunk) else {
            return;
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(header_end) = find(&buf, b"\r\n\r\n") {
            let content_len = content_length(&buf[..header_end]);
            if buf.len() >= header_end + 4 + content_len {
                break;
            }
        }
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{UPSTREAM_BODY}",
        UPSTREAM_BODY.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse `Content-Length` from a request head, case-insensitively; absent → 0 (a bodyless request).
fn content_length(head: &[u8]) -> usize {
    let head = String::from_utf8_lossy(head);
    head.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn amberfork_record(upstream: SocketAddr, out: &std::path::Path, agent: &str) -> Command {
    let mut cmd = Command::cargo_bin("amberfork").expect("amberfork binary builds");
    cmd.arg("record")
        .arg("--upstream")
        .arg(format!("http://{upstream}"))
        .arg("--base-url-env")
        .arg("AMBERFORK_TEST_BASE_URL")
        .arg("--out")
        .arg(out)
        .arg("--id")
        .arg("test-recording")
        .arg("--")
        .arg("python3")
        .arg("-c")
        .arg(agent);
    cmd
}

#[test]
fn records_the_provider_exchange_into_a_cassette() {
    let upstream = spawn_stub_upstream();
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.cassette.json");

    amberfork_record(upstream, &out, &agent_src(0))
        .assert()
        .code(0);

    let cassette: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("cassette written"))
            .expect("cassette is valid JSON");

    assert_eq!(cassette["cassette_version"], "0.1");
    let exchanges = cassette["exchanges"].as_array().expect("exchanges array");
    assert_eq!(exchanges.len(), 1, "one provider call → one exchange");
    // Full content on both sides — the record path's whole point.
    assert_eq!(exchanges[0]["request"]["body"]["model"], "claude-sonnet-5");
    assert_eq!(exchanges[0]["response"]["status"], 200);
    assert_eq!(
        exchanges[0]["response"]["body"]["content"][0]["text"],
        "stub upstream ok"
    );
}

#[test]
fn writes_the_cassette_and_propagates_a_failing_agent_exit() {
    // The agent makes its call, then exits non-zero. `record` must surface that exit code AND still
    // write the cassette — a failed run is the one you most want recorded.
    let upstream = spawn_stub_upstream();
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.cassette.json");

    amberfork_record(upstream, &out, &agent_src(7))
        .assert()
        .code(7);

    let cassette: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out).expect("cassette written even on agent failure"),
    )
    .expect("cassette is valid JSON");
    assert_eq!(
        cassette["exchanges"].as_array().expect("exchanges").len(),
        1
    );
}
