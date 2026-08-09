//! Integration tests over a real bound listener — every assertion is on-the-wire truth.
//!
//! The client is a raw `TcpStream` writing HTTP/1.1 by hand: the security tests need full
//! control of the `Host` header (the DNS-rebinding guard is exactly the thing a polite HTTP
//! client library refuses to let you fake), and the same helper then serves the happy paths
//! for free.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_layout::{
    DOCUMENT_VERSION, Document, SLOT_TEXT_LIMIT, Side, SlotAddress, SlotKind, ViewModel,
};
use amberfork_model::test_support::{run, step};
use amberfork_server::{DOCUMENT_ROUTE, PAYLOAD_ROUTE, ServeError, Server};
use rust_embed::Embed;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};

/// A committed two-file web bundle, embedded through the same derive the real `ui-dist/`
/// uses — the tests exercise rust-embed's actual disk/embed duality, not a hand-rolled fake.
#[derive(Embed)]
#[folder = "tests/fixture_bundle"]
struct FixtureBundle;

/// No `index.html` at all: the deterministic stand-in for a dev checkout without a built
/// UI. Deliberately NOT tested through `Server::bind`'s real bundle — the day someone
/// builds the UI locally into ui-dist/, a test asserting "the real bundle is missing"
/// would start lying.
#[derive(Embed)]
#[folder = "tests/empty_bundle"]
struct EmptyBundle;

/// A tiny forked pair pushed through the real engine pipeline (align → layout → view-model),
/// so this suite breaks if the wire contract drifts, not only if the server does. `document()`
/// is the served (truncated) form built from it — kept separate so tests can ask for either.
fn full_view() -> ViewModel {
    let reference = run(
        "good",
        vec![
            step(0, "plan").text_output("read the issue").build(),
            step(1, "search")
                .text_output("rg the-right-pattern")
                .build(),
            step(2, "answer").text_output("42").build(),
        ],
    )
    .build();
    let observed = run(
        "bad",
        vec![
            step(0, "plan").text_output("read the issue").build(),
            step(1, "search").text_output("cat the-wrong-file").build(),
            step(2, "answer").text_output("41").build(),
        ],
    )
    .build();
    let result = diff(&reference, &observed, &LexicalCost, &DiffParams::default())
        .expect("three-step fixture is far under the size guard");
    ViewModel::compute(&result, &reference, &observed)
}

fn document() -> Document {
    Document::new(full_view())
}

/// A view whose one step is oversized on both sides, so the served document truncates its
/// summary and stamps an address the payload endpoint (#30) can be asked to resolve. Returns
/// the full text alongside so a test can assert the endpoint hands back exactly that.
fn full_view_with_a_truncated_slot() -> (ViewModel, String) {
    let huge = "x".repeat(SLOT_TEXT_LIMIT + 100);
    let reference = run("good", vec![step(0, "fetch").text_output(&huge).build()]).build();
    let observed = run("bad", vec![step(0, "fetch").text_output(&huge).build()]).build();
    let result = diff(&reference, &observed, &LexicalCost, &DiffParams::default())
        .expect("one-step fixture is far under the size guard");
    (ViewModel::compute(&result, &reference, &observed), huge)
}

/// Bind on an OS-assigned port, run the accept loop on a background runtime thread, and
/// return where it landed. The thread outlives the test and dies with the process. Takes the
/// pre-envelope view, mirroring what the real CLI does: clone it into the served `Document`,
/// keep the original around for `Server::bind`'s expand-on-demand lookups.
fn spawn(view: ViewModel) -> SocketAddr {
    let document = Document::new(view.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("current-thread runtime builds");
        runtime.block_on(async move {
            let server = Server::bind_with_assets::<FixtureBundle>(&document, &view, 0)
                .await
                .expect("bind 127.0.0.1:0");
            tx.send(server.local_addr())
                .expect("test is waiting on the bound address");
            server.serve().await.expect("accept loop outlives the test");
        });
    });
    rx.recv().expect("server thread reports its bound address")
}

struct RawResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl RawResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == &name.to_ascii_lowercase())
            .map(|(_, v)| v.as_str())
    }
}

/// One GET over a fresh connection. `host: None` sends an HTTP/1.0 request with no `Host`
/// line at all — under HTTP/1.1 hyper rejects a missing `Host` itself with a 400, and the
/// test wants to prove OUR guard refuses, not the parser.
fn get(addr: SocketAddr, path: &str, host: Option<&str>, extra: &[(&str, &str)]) -> RawResponse {
    send(addr, "GET", path, host, extra, "")
}

/// One POST of a JSON body over a fresh connection — the payload endpoint's shape, mirroring
/// `get`'s HTTP/1.0-with-no-`Host` trick for the same reason (proving OUR guard refuses a
/// missing `Host`, not hyper's parser).
fn post(addr: SocketAddr, path: &str, host: Option<&str>, body: &str) -> RawResponse {
    send(
        addr,
        "POST",
        path,
        host,
        &[("Content-Type", "application/json")],
        body,
    )
}

fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    host: Option<&str>,
    extra: &[(&str, &str)],
    body: &str,
) -> RawResponse {
    let mut request = match host {
        Some(host) => format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n"),
        None => format!("{method} {path} HTTP/1.0\r\n"),
    };
    for (name, value) in extra {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    request.push_str(body);

    let mut stream = TcpStream::connect(addr).expect("connect to the bound listener");
    stream
        .write_all(request.as_bytes())
        .expect("write the request");
    let mut raw = String::new();
    stream
        .read_to_string(&mut raw)
        .expect("read the response to EOF");

    let (head, body) = raw
        .split_once("\r\n\r\n")
        .expect("response has a header/body split");
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|status_line| status_line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status line carries a numeric code");
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(':').expect("header line has a colon");
            (name.trim().to_ascii_lowercase(), value.trim().to_string())
        })
        .collect();
    RawResponse {
        status,
        headers,
        body: body.to_string(),
    }
}

#[test]
fn document_endpoint_round_trips_the_versioned_document() {
    let addr = spawn(full_view());
    let response = get(addr, DOCUMENT_ROUTE, Some("127.0.0.1"), &[]);

    assert_eq!(response.status, 200);
    let content_type = response.header("content-type").unwrap_or_default();
    assert!(
        content_type.starts_with("application/json"),
        "content endpoint is JSON, got {content_type:?}"
    );
    let served: Document =
        serde_json::from_str(&response.body).expect("body deserializes as the view-model");
    assert_eq!(served.schema_version, DOCUMENT_VERSION);
    assert_eq!(served, document(), "the wire copy is the document, exactly");
}

#[test]
fn foreign_and_missing_host_headers_are_forbidden() {
    let addr = spawn(full_view());
    // The vite/Jupyter CVE class: a hostile page resolves its own name to 127.0.0.1 and
    // reads traces cross-origin. The guard must reject names, lookalike prefixes, and the
    // absent header alike — and on EVERY route, not just the content endpoint.
    for host in [
        Some("evil.example"),
        Some("localhost.evil.example"),
        Some("127.0.0.1.evil.example"),
        None,
    ] {
        let response = get(addr, DOCUMENT_ROUTE, host, &[]);
        assert_eq!(response.status, 403, "Host {host:?} must be refused");
    }
    let response = get(addr, "/no/such/route", Some("evil.example"), &[]);
    assert_eq!(response.status, 403, "the guard wraps unknown routes too");
}

#[test]
fn local_host_forms_are_allowed() {
    let addr = spawn(full_view());
    let port_form = format!("127.0.0.1:{}", addr.port());
    for host in ["localhost", "LocalHost:7777", &port_form, "[::1]:8080"] {
        let response = get(addr, DOCUMENT_ROUTE, Some(host), &[]);
        assert_eq!(response.status, 200, "Host {host:?} is a localhost form");
    }
}

#[test]
fn listener_binds_loopback_only() {
    let addr = spawn(full_view());
    assert_eq!(
        addr.ip(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "D6: 127.0.0.1 only, no widen flag exists"
    );
}

#[test]
fn repoll_with_matching_etag_is_not_modified() {
    let addr = spawn(full_view());
    let first = get(addr, DOCUMENT_ROUTE, Some("127.0.0.1"), &[]);
    let etag = first
        .header("etag")
        .expect("document response carries an ETag")
        .to_string();

    let repoll = get(
        addr,
        DOCUMENT_ROUTE,
        Some("127.0.0.1"),
        &[("If-None-Match", &etag)],
    );
    assert_eq!(repoll.status, 304, "matching ETag re-poll is cheap");
    assert_eq!(repoll.body, "", "304 carries no body");
    assert_eq!(
        repoll.header("etag"),
        Some(etag.as_str()),
        "304 restates the ETag"
    );
}

#[test]
fn root_serves_index_html() {
    let addr = spawn(full_view());
    let response = get(addr, "/", Some("127.0.0.1"), &[]);
    assert_eq!(response.status, 200);
    let content_type = response.header("content-type").unwrap_or_default();
    assert!(
        content_type.starts_with("text/html"),
        "index is HTML, got {content_type:?}"
    );
    assert!(response.body.contains("fixture-bundle index"));
}

#[test]
fn exact_assets_are_served_with_their_mime() {
    let addr = spawn(full_view());
    let response = get(addr, "/app.js", Some("127.0.0.1"), &[]);
    assert_eq!(response.status, 200);
    let content_type = response.header("content-type").unwrap_or_default();
    assert!(
        content_type.contains("javascript"),
        "scripts need a script MIME or the browser refuses modules, got {content_type:?}"
    );
    assert_eq!(
        response.body,
        "export const marker = \"fixture-bundle app\";\n"
    );
}

#[test]
fn unknown_routes_fall_back_to_index_html() {
    // The SPA contract: client-side routes (and `#step-N` anchors, which never reach the
    // server at all) must land on the app, not a 404.
    let addr = spawn(full_view());
    let response = get(addr, "/no/such/route", Some("127.0.0.1"), &[]);
    assert_eq!(response.status, 200);
    assert!(
        response.body.contains("fixture-bundle index"),
        "unknown routes serve the app shell"
    );
}

#[test]
fn unknown_api_routes_are_404_not_html() {
    // A typo'd endpoint must fail loud: handing the UI's fetch() an HTML page to parse is
    // the silent version of this bug.
    let addr = spawn(full_view());
    let response = get(addr, "/api/no-such-endpoint", Some("127.0.0.1"), &[]);
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn missing_bundle_is_refused_with_a_clear_message() {
    let view = full_view();
    let doc = Document::new(view.clone());
    let err = Server::bind_with_assets::<EmptyBundle>(&doc, &view, 0)
        .await
        .expect_err("a bundle without index.html cannot serve");
    assert!(matches!(err, ServeError::BundleMissing));
    assert!(
        err.to_string().contains("ui-dist"),
        "the message says where the bundle goes: {err}"
    );
}

#[tokio::test]
async fn bind_on_a_taken_port_is_a_typed_error() {
    let view = full_view();
    let doc = Document::new(view.clone());
    let first = Server::bind_with_assets::<FixtureBundle>(&doc, &view, 0)
        .await
        .expect("first bind on port 0");
    let port = first.local_addr().port();

    let err = Server::bind_with_assets::<FixtureBundle>(&doc, &view, port)
        .await
        .expect_err("second bind on an occupied port");
    assert!(matches!(err, ServeError::Bind { .. }));
    assert!(
        err.to_string().contains(&port.to_string()),
        "the error names the port so the CLI message can: {err}"
    );
}

#[test]
fn payload_endpoint_resolves_a_truncated_slots_full_text() {
    // The real workflow end to end: read a truncated slot's address off the actual served
    // document (not a hand-built one), POST it back, and get the exact untruncated text.
    let (view, huge) = full_view_with_a_truncated_slot();
    let addr = spawn(view);

    let doc_response = get(addr, DOCUMENT_ROUTE, Some("127.0.0.1"), &[]);
    let served: Document = serde_json::from_str(&doc_response.body).expect("document parses");
    let address = served
        .view
        .rows
        .iter()
        .find_map(|row| {
            row.step()
                .a
                .as_ref()
                .and_then(|s| s.summary.address.clone())
        })
        .expect("the oversized step's a-side summary was truncated and addressed");

    let request_body = serde_json::to_string(&address).expect("SlotAddress serializes");
    let response = post(addr, PAYLOAD_ROUTE, Some("127.0.0.1"), &request_body);

    assert_eq!(response.status, 200);
    let content_type = response.header("content-type").unwrap_or_default();
    assert!(
        content_type.starts_with("application/json"),
        "payload endpoint is JSON, got {content_type:?}"
    );
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("payload body is JSON");
    assert_eq!(
        payload["text"], huge,
        "the full, untruncated text comes back"
    );
}

#[test]
fn payload_endpoint_404s_for_an_address_that_does_not_resolve() {
    let addr = spawn(full_view());
    // Well-formed, but names a row this three-step fixture does not have.
    let address = SlotAddress {
        row: 99,
        kind: SlotKind::StepSummary { side: Side::A },
    };
    let request_body = serde_json::to_string(&address).expect("SlotAddress serializes");
    let response = post(addr, PAYLOAD_ROUTE, Some("127.0.0.1"), &request_body);
    assert_eq!(response.status, 404);
}

#[test]
fn payload_endpoint_is_covered_by_the_host_guard() {
    // The payload endpoint exists to hand back content the document endpoint deliberately
    // withheld — it must never be reachable by a route around the same DNS-rebinding guard.
    let (view, _) = full_view_with_a_truncated_slot();
    let addr = spawn(view);
    let response = post(addr, PAYLOAD_ROUTE, Some("evil.example"), "{}");
    assert_eq!(response.status, 403);
}
