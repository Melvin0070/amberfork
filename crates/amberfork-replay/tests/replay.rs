//! The replay server end-to-end: a stub agent re-drives a recorded cassette over real HTTP.
//!
//! The mirror image of the record crate's `capture.rs`, and offline the same way. There, the
//! "provider" was a fake axum server and the capture proxy sat in front of it; here the
//! *cassette* is the provider, [`ReplayServer`] serves it back, and the "agent" is a `reqwest`
//! client pointed at the loopback base URL exactly as an SDK pointed at `OPENAI_BASE_URL` would
//! be. The contract under test is what crosses the boundary, so the test drives real HTTP through
//! the real listener — faking at the client seam would test the mock. On the replayed path no
//! provider is ever reached; the scripted upstream stands in only once a request branches off the
//! tape.

use amberfork_record::{Body, CapturedRequest, CapturedResponse, Cassette, Exchange};
use amberfork_replay::{ReplayServer, ScriptedUpstream};
use serde_json::{Value, json};

/// A credential in the shape a real one has. The re-executed cassette is just as shareable as a
/// recorded one, so its absence there is a property worth asserting.
const SECRET: &str = "sk-secret-do-not-record";

fn request(content: &str) -> CapturedRequest {
    CapturedRequest {
        method: "POST".to_string(),
        path: "/v1/chat/completions".to_string(),
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Body::Json(json!({
            "model": "claude-sonnet-5",
            "messages": [{ "role": "user", "content": content }],
        })),
    }
}

fn response(content: &str) -> CapturedResponse {
    CapturedResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Body::Json(json!({ "choices": [{ "message": { "content": content } }] })),
    }
}

fn cassette(turns: &[(&str, &str)]) -> Cassette {
    let mut cassette = Cassette::new("recorded");
    for (idx, (question, answer)) in turns.iter().enumerate() {
        cassette.exchanges.push(Exchange {
            idx,
            request: request(question),
            response: response(answer),
        });
    }
    cassette
}

/// The request body of a recorded turn, as an SDK would re-serialize it.
fn body_of(content: &str) -> Value {
    let Body::Json(body) = request(content).body else {
        unreachable!("request bodies are JSON")
    };
    body
}

/// Fire one request at the server, exactly as an agent's SDK pointed at the loopback would —
/// credential attached, because a real SDK always sends one even to a proxy.
async fn agent_call(base: &str, body: &Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .header("authorization", format!("Bearer {SECRET}"))
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await
        .expect("call replay server")
}

#[tokio::test]
async fn stub_agent_replays_a_three_exchange_cassette_offline() {
    let turns = [("q1", "a1"), ("q2", "a2"), ("q3", "a3")];
    // An empty scripted upstream: an unmodified replay is all hits. If any turn wrongly relayed,
    // the upstream would be exhausted and the served response would be a gateway error — so this
    // also proves the recorded path never touches a provider.
    let server = ReplayServer::bind(cassette(&turns), ScriptedUpstream::new([]), "reexec")
        .await
        .expect("bind replay server");
    let base = server.base_url();

    for (question, answer) in turns {
        let served = agent_call(&base, &body_of(question)).await;
        assert_eq!(served.status(), 200);
        let got: Value = served.json().await.expect("decode replayed response");
        assert_eq!(
            got["choices"][0]["message"]["content"], answer,
            "the server must serve the recorded response back for turn {question}"
        );
    }

    // The re-execution is itself a cassette, one exchange per turn, in order — the shape
    // `amberfork_record::normalize` consumes.
    let reexecuted = server.reexecuted().await;
    let answers: Vec<Value> = reexecuted
        .exchanges
        .iter()
        .map(|exchange| {
            let Body::Json(body) = &exchange.response.body else {
                unreachable!("response bodies are JSON")
            };
            body["choices"][0]["message"]["content"].clone()
        })
        .collect();
    assert_eq!(answers, vec![json!("a1"), json!("a2"), json!("a3")]);
}

#[tokio::test]
async fn a_branched_request_relays_through_the_listener() {
    // The miss path, exercised through real HTTP rather than only the in-process proxy: a request
    // the tape cannot answer must reach `Upstream` and its live answer must reach the caller.
    let server = ReplayServer::bind(
        cassette(&[("q1", "a1")]),
        ScriptedUpstream::new([response("live answer")]),
        "reexec",
    )
    .await
    .expect("bind replay server");
    let base = server.base_url();

    let served = agent_call(&base, &body_of("off the tape")).await;
    assert_eq!(served.status(), 200);
    let got: Value = served.json().await.expect("decode relayed response");
    assert_eq!(got["choices"][0]["message"]["content"], "live answer");

    let reexecuted = server.reexecuted().await;
    assert_eq!(reexecuted.exchanges.len(), 1);
}

#[tokio::test]
async fn the_agents_credential_never_reaches_the_reexecuted_cassette() {
    // The replay-side mirror of record's central safety property. The re-driven agent still sends
    // its API key to the loopback listener, and the re-executed cassette gets committed and pasted
    // into issues exactly as a recorded one does — so the key must not survive onto it. The
    // listener reuses record's single header allowlist rather than forking one.
    let server = ReplayServer::bind(
        cassette(&[("q1", "a1")]),
        ScriptedUpstream::new([]),
        "reexec",
    )
    .await
    .expect("bind replay server");
    let base = server.base_url();

    agent_call(&base, &body_of("q1")).await; // sends `authorization: Bearer <SECRET>`

    let serialized = serde_json::to_string(&server.reexecuted().await).expect("serialize cassette");
    assert!(
        !serialized.contains(SECRET),
        "the agent's credential leaked into the re-executed cassette: {serialized}"
    );
}
