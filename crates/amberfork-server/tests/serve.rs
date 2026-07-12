//! Integration tests over a real bound listener — every assertion is on-the-wire truth.
//!
//! The client is a raw `TcpStream` writing HTTP/1.1 by hand: the security tests need full
//! control of the `Host` header (the DNS-rebinding guard is exactly the thing a polite HTTP
//! client library refuses to let you fake), and the same helper then serves the happy paths
//! for free.

use amberfork_align::{DiffParams, LexicalCost, diff};
use amberfork_layout::{DOCUMENT_VERSION, Document, ViewModel};
use amberfork_model::test_support::{run, step};
use amberfork_server::{DOCUMENT_ROUTE, ServeError, Server};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};

/// A tiny forked pair pushed through the real engine pipeline (align → layout → document),
/// so this suite breaks if the wire contract drifts, not only if the server does.
fn document() -> Document {
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
    Document::new(ViewModel::compute(&result, &reference, &observed))
}

/// Bind on an OS-assigned port, run the accept loop on a background runtime thread, and
/// return where it landed. The thread outlives the test and dies with the process.
fn spawn(document: Document) -> SocketAddr {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("current-thread runtime builds");
        runtime.block_on(async move {
            let server = Server::bind(&document, 0).await.expect("bind 127.0.0.1:0");
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
    let mut request = match host {
        Some(host) => format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n"),
        None => format!("GET {path} HTTP/1.0\r\n"),
    };
    for (name, value) in extra {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");

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
    let addr = spawn(document());
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
    let addr = spawn(document());
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
    let addr = spawn(document());
    let port_form = format!("127.0.0.1:{}", addr.port());
    for host in ["localhost", "LocalHost:7777", &port_form, "[::1]:8080"] {
        let response = get(addr, DOCUMENT_ROUTE, Some(host), &[]);
        assert_eq!(response.status, 200, "Host {host:?} is a localhost form");
    }
}

#[test]
fn listener_binds_loopback_only() {
    let addr = spawn(document());
    assert_eq!(
        addr.ip(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "D6: 127.0.0.1 only, no widen flag exists"
    );
}

#[test]
fn repoll_with_matching_etag_is_not_modified() {
    let addr = spawn(document());
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
fn unknown_route_is_404_until_the_spa_fallback_lands() {
    // Slice 1 (#25) replaces this with the SPA fallback: unknown routes serve index.html.
    let addr = spawn(document());
    let response = get(addr, "/no/such/route", Some("127.0.0.1"), &[]);
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn bind_on_a_taken_port_is_a_typed_error() {
    let doc = document();
    let first = Server::bind(&doc, 0).await.expect("first bind on port 0");
    let port = first.local_addr().port();

    let err = Server::bind(&doc, port)
        .await
        .expect_err("second bind on an occupied port");
    assert!(matches!(err, ServeError::Bind { .. }));
    assert!(
        err.to_string().contains(&port.to_string()),
        "the error names the port so the CLI message can: {err}"
    );
}
