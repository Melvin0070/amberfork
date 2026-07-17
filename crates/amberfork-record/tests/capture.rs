//! The capture proxy against a fake upstream.
//!
//! Offline and keyless by construction: the "provider" is an axum server in this test. That is
//! not a compromise for CI's sake — the contract under test is *what crosses the boundary*, so
//! the test has to drive real HTTP through the real proxy. Faking at the client seam would
//! test the mock.

use amberfork_record::{Body, CaptureProxy};
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use serde_json::{Value, json};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// A credential in the shape a real one has. Its absence from the cassette is the assertion
/// this whole file exists for.
const SECRET: &str = "sk-secret-do-not-record";

/// What the fake provider saw. Recorded so the test can prove the proxy relays faithfully —
/// including the credential, which must reach upstream even though it must not reach disk.
#[derive(Default)]
struct Upstream {
    seen_auth: Mutex<Vec<String>>,
}

async fn chat_completions(
    State(state): State<Arc<Upstream>>,
    headers: HeaderMap,
    Json(_body): Json<Value>,
) -> Json<Value> {
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        state.seen_auth.lock().unwrap().push(auth.to_string());
    }
    Json(json!({
        "id": "chatcmpl-fake",
        "choices": [{ "message": { "role": "assistant", "content": "I'll look up the order first." } }],
    }))
}

/// Stand up the fake provider; returns its origin and the record of what it saw.
async fn fake_upstream() -> (String, Arc<Upstream>) {
    let state = Arc::new(Upstream::default());
    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(Arc::clone(&state));
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .expect("bind fake upstream");
    let addr = listener.local_addr().expect("upstream addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), state)
}

/// Send a chat-completion request through the proxy, exactly as an SDK pointed at
/// `OPENAI_BASE_URL` would.
async fn call_through(proxy: &CaptureProxy) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", proxy.base_url()))
        .header("authorization", format!("Bearer {SECRET}"))
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-5",
            "messages": [{ "role": "user", "content": "Handle refund for order 8841" }],
        }))
        .send()
        .await
        .expect("call through proxy")
}

#[tokio::test]
async fn credential_reaches_upstream_but_never_the_cassette() {
    // The record path's central safety property. A cassette gets committed as a fixture and
    // pasted into issues; a key that survives capture is a leak the user cannot take back,
    // and no later fix un-publishes it. Meanwhile the proxy is a relay — strip the credential
    // on the way *out* and the agent under recording simply stops working.
    let (upstream, seen) = fake_upstream().await;
    let proxy = CaptureProxy::bind("run-a", &upstream)
        .await
        .expect("bind proxy");

    call_through(&proxy).await;

    assert_eq!(
        seen.seen_auth.lock().unwrap().as_slice(),
        &[format!("Bearer {SECRET}")],
        "the credential must reach upstream — the proxy relays, it does not authenticate"
    );

    let serialized = serde_json::to_string(&proxy.cassette()).expect("serialize cassette");
    assert!(
        !serialized.contains(SECRET),
        "credential leaked into the cassette: {serialized}"
    );
}

#[tokio::test]
async fn cassette_captures_the_full_request_body() {
    // Full *input* capture is the record path's reason to exist: output-only logs leave ≥21%
    // of cases unattributable, and a counterfactual cannot re-ask a question that was never
    // recorded.
    let (upstream, _seen) = fake_upstream().await;
    let proxy = CaptureProxy::bind("run-a", &upstream)
        .await
        .expect("bind proxy");

    call_through(&proxy).await;

    let cassette = proxy.cassette();
    assert_eq!(cassette.exchanges.len(), 1);
    let exchange = &cassette.exchanges[0];
    assert_eq!(exchange.idx, 0);
    assert_eq!(exchange.request.method, "POST");
    assert_eq!(exchange.request.path, "/v1/chat/completions");

    let Body::Json(body) = &exchange.request.body else {
        panic!(
            "expected a JSON request body, got {:?}",
            exchange.request.body
        );
    };
    assert_eq!(body["model"], "claude-sonnet-5");
    assert_eq!(
        body["messages"][0]["content"],
        "Handle refund for order 8841"
    );
}

#[tokio::test]
async fn caller_receives_the_upstream_response_verbatim() {
    // A recording that alters the run it records is not a recording. This is the property that
    // lets someone put `amberfork record` in front of a real agent without wondering whether
    // the tool changed the outcome it is about to explain.
    let (upstream, _seen) = fake_upstream().await;
    let proxy = CaptureProxy::bind("run-a", &upstream)
        .await
        .expect("bind proxy");

    let response = call_through(&proxy).await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("decode relayed response");
    assert_eq!(body["id"], "chatcmpl-fake");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "I'll look up the order first."
    );

    let cassette = proxy.cassette();
    let Body::Json(recorded) = &cassette.exchanges[0].response.body else {
        panic!("expected a JSON response body");
    };
    assert_eq!(
        recorded, &body,
        "what the caller got and what the cassette recorded must be the same bytes"
    );
}

#[tokio::test]
async fn exchanges_are_recorded_in_capture_order() {
    let (upstream, _seen) = fake_upstream().await;
    let proxy = CaptureProxy::bind("run-a", &upstream)
        .await
        .expect("bind proxy");

    call_through(&proxy).await;
    call_through(&proxy).await;
    call_through(&proxy).await;

    let cassette = proxy.cassette();
    let indices: Vec<usize> = cassette.exchanges.iter().map(|e| e.idx).collect();
    assert_eq!(indices, vec![0, 1, 2]);
}

#[tokio::test]
async fn upstream_failure_surfaces_as_a_gateway_error_not_a_panic() {
    // Point at a closed port: the agent under recording should see an ordinary HTTP failure it
    // can handle, and the proxy should stay up.
    let dead = {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .expect("bind to find a free port");
        let addr = listener.local_addr().expect("addr");
        drop(listener);
        format!("http://{addr}")
    };
    let proxy = CaptureProxy::bind("run-a", &dead)
        .await
        .expect("bind proxy");

    let response = call_through(&proxy).await;
    assert_eq!(response.status(), 502);
    assert!(
        proxy.cassette().exchanges.is_empty(),
        "an exchange that never completed upstream is not an exchange"
    );
}
