//! The live relay against a fake provider.
//!
//! Offline and keyless by construction, exactly like the record crate's `capture.rs`: the
//! "provider" is an axum server in this test, and the contract under test is *what crosses the
//! boundary*, so it drives real HTTP through the real [`LiveUpstream`]. Faking at the client seam
//! would test the mock. The credential the relay must forward is asserted the same way record
//! asserts it: the fake provider records what it saw.

use amberfork_record::{Body, CapturedRequest};
use amberfork_replay::{LiveUpstream, Upstream, UpstreamError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Router, http::StatusCode};
use serde_json::{Value, json};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// A credential in the shape a real one has. A live relay must carry it to the provider (that is
/// how the re-driven agent authenticates past the branch), so the fake provider records what it saw.
const SECRET: &str = "sk-secret-live-relay";

#[derive(Default)]
struct Provider {
    seen_auth: Mutex<Vec<String>>,
}

async fn messages(
    State(state): State<Arc<Provider>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        state.seen_auth.lock().unwrap().push(auth.to_string());
    }
    // Echo the prompt so the test can prove the request body reached the provider intact.
    let content = body["messages"][0]["content"].clone();
    Json(json!({ "choices": [{ "message": { "content": content } }] }))
}

/// A route that always rate-limits — a 429 is a real divergence signal and must be forwarded
/// faithfully, not turned into a relay error.
async fn rate_limited() -> (StatusCode, Json<Value>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({ "error": "slow down" })),
    )
}

/// Stand up the fake provider; returns its origin and the record of what it saw.
async fn fake_provider() -> (String, Arc<Provider>) {
    let state = Arc::new(Provider::default());
    let app = Router::new()
        .route("/v1/messages", post(messages))
        .route("/rate-limited", post(rate_limited))
        .with_state(Arc::clone(&state));
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .expect("bind fake provider");
    let addr = listener.local_addr().expect("provider addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), state)
}

/// A miss the re-driven agent issued, with the credential its SDK always attaches — the exact
/// shape `ReplayProxy` hands `Upstream::send` on a cache miss.
fn miss(path: &str, prompt: &str) -> CapturedRequest {
    CapturedRequest {
        method: "POST".to_string(),
        path: path.to_string(),
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("authorization".to_string(), format!("Bearer {SECRET}")),
        ],
        body: Body::Json(json!({
            "model": "claude-sonnet-5",
            "messages": [{ "role": "user", "content": prompt }],
        })),
    }
}

#[tokio::test]
async fn a_miss_is_forwarded_and_the_providers_answer_returned() {
    let (origin, provider) = fake_provider().await;
    let upstream = LiveUpstream::new(reqwest::Client::new(), origin);

    let response = upstream
        .send(&miss("/v1/messages", "resume the task off the tape"))
        .await
        .expect("the relay reaches the provider");

    assert_eq!(response.status, 200);
    let Body::Json(body) = &response.body else {
        panic!("provider answered JSON, got {:?}", response.body)
    };
    assert_eq!(
        body["choices"][0]["message"]["content"], "resume the task off the tape",
        "the request body must reach the provider intact and its answer come back"
    );
    assert_eq!(
        *provider.seen_auth.lock().unwrap(),
        vec![format!("Bearer {SECRET}")],
        "the relay must carry the agent's credential upstream — that is how it authenticates"
    );
}

#[tokio::test]
async fn a_non_2xx_status_is_forwarded_faithfully_not_turned_into_an_error() {
    let (origin, _) = fake_provider().await;
    let upstream = LiveUpstream::new(reqwest::Client::new(), origin);

    let response = upstream
        .send(&miss("/rate-limited", "anything"))
        .await
        .expect("a 429 is a response, not a transport failure");

    assert_eq!(
        response.status, 429,
        "a rate-limit is a real divergence signal"
    );
}

#[tokio::test]
async fn an_unreachable_origin_surfaces_as_transport_not_a_panic() {
    // Port 0 in a client request never connects; the relay must report it, not unwind.
    let upstream = LiveUpstream::new(reqwest::Client::new(), "http://127.0.0.1:0");

    let err = upstream
        .send(&miss("/v1/messages", "anything"))
        .await
        .expect_err("an unreachable origin is a transport error");

    assert!(matches!(err, UpstreamError::Transport(_)), "got {err:?}");
}
