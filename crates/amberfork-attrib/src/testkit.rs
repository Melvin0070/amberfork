//! Shared test fixtures for counterfactual attribution: the scripted HTTP agent and the cassette
//! builders both the driver and verify tests re-execute against.
//!
//! One site to change when the recorded-exchange shape does — the same reason the model crate
//! centralized its `Step`/`Run` builders (issue #22). Gated behind `#[cfg(test)]`, so none of it
//! ships in a normal build.

use crate::{AgentDriver, AgentError};
use amberfork_record::{Body, CapturedRequest, CapturedResponse, Cassette, Exchange};
use serde_json::{Value, json};
use std::future::Future;

// The canonical three-turn refund scenario. The good run answers cleanly; the bad run diverges at
// turn 1 (`B1` for `G1`) and stays off the rails at turn 2 (`B2` for `G2`). Divergent answers are
// token-disjoint from their good counterparts, so the lexical cost model scores them a full 1.0 —
// an unambiguous fork, not a marginal one.
pub(crate) const G0: &str = "acknowledged, looking up order 8841";
pub(crate) const G1: &str = "order 8841 found, refund eligible";
pub(crate) const G2: &str = "refund of 42 dollars issued";
pub(crate) const B1: &str = "request denied, account flagged for review";
pub(crate) const B2: &str = "escalation ticket opened pending manual handling";

/// An in-process agent that POSTs a fixed script of request bodies at the loopback listener,
/// exactly as an SDK pointed at the base URL would. It reacts to nothing it is served: the
/// re-executed tape is fully determined by the script and what the server answers, which is what
/// makes each scenario's recovered/not-recovered outcome deterministic.
pub(crate) struct ScriptedAgent {
    pub(crate) requests: Vec<Value>,
}

impl AgentDriver for ScriptedAgent {
    fn drive(&self, base_url: &str) -> impl Future<Output = Result<(), AgentError>> + Send {
        let url = format!("{base_url}/v1/chat/completions");
        let bodies = self.requests.clone();
        async move {
            let client = reqwest::Client::new();
            for body in bodies {
                client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .expect("the scripted agent reaches the replay listener");
            }
            Ok(())
        }
    }
}

pub(crate) fn request(content: &str) -> CapturedRequest {
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

pub(crate) fn response(content: &str) -> CapturedResponse {
    CapturedResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Body::Json(json!({ "choices": [{ "message": { "content": content } }] })),
    }
}

/// The request body of a turn, as an SDK would re-serialize it — what the scripted agent sends.
pub(crate) fn body_of(content: &str) -> Value {
    let Body::Json(body) = request(content).body else {
        unreachable!("request bodies are JSON")
    };
    body
}

pub(crate) fn cassette(id: &str, turns: &[(&str, &str)]) -> Cassette {
    let mut cassette = Cassette::new(id);
    for (idx, (question, answer)) in turns.iter().enumerate() {
        cassette.exchanges.push(Exchange {
            idx,
            request: request(question),
            response: response(answer),
        });
    }
    cassette
}

/// The good reference recording: three turns answered cleanly.
pub(crate) fn good_cassette() -> Cassette {
    cassette("good", &[("q0", G0), ("q1", G1), ("q2", G2)])
}

/// The failing recording: diverges at turn 1 and asks a different turn-2 question (`q2_bad`), so a
/// re-run that follows the good path cache-misses there and must relay.
pub(crate) fn bad_cassette() -> Cassette {
    cassette("bad", &[("q0", G0), ("q1", B1), ("q2_bad", B2)])
}
