//! The capture proxy: the boundary the record path records at.
//!
//! The agent is pointed at this proxy with a base-URL environment variable
//! (`OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, …) and needs no code change — the zero-code
//! capture the design calls for. Every request is forwarded upstream verbatim and the round
//! trip is appended to a [`Cassette`].
//!
//! It forwards over plain HTTP inbound and TLS outbound on purpose: the agent's SDK talks
//! `http://127.0.0.1:PORT`, so there is no certificate to install and no TLS interception
//! anywhere. This crate is an I/O edge — `tokio` lives here, never in the engine crates.
//!
//! **Known gap: streamed responses are buffered.** The relay reads the upstream body to
//! completion before answering, so an agent using `stream: true` still receives every byte and
//! still parses the SSE stream correctly, but receives it in one piece at the end instead of
//! incrementally. Content is faithful; arrival timing is not. That is a real (if usually
//! benign) change to the run being recorded, and it is stated here rather than left for
//! someone to discover: an agent whose control flow keys on partial output — an early-abort on
//! first token, a UI that streams — is not yet faithfully recordable. Passing the body through
//! as a stream while teeing it to the tape is the fix, and it is a follow-up, not a rewrite.

use crate::cassette::{
    Body, CapturedRequest, CapturedResponse, Cassette, Exchange, retain_request_headers,
    retain_response_headers,
};
use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// Something that went wrong standing up or running a recording session.
#[derive(Debug)]
pub enum RecordError {
    /// Binding the loopback listener failed — in practice: the port is already taken.
    Bind { addr: SocketAddr, source: io::Error },
    /// The accept loop died after a successful bind.
    Serve(io::Error),
    /// The upstream base URL is not a usable origin.
    Upstream { url: String, source: reqwest::Error },
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind { addr, source } => write!(f, "cannot bind {addr}: {source}"),
            Self::Serve(source) => write!(f, "capture proxy stopped: {source}"),
            Self::Upstream { url, source } => {
                write!(f, "unusable upstream base URL {url}: {source}")
            }
        }
    }
}

impl std::error::Error for RecordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind { source, .. } | Self::Serve(source) => Some(source),
            Self::Upstream { source, .. } => Some(source),
        }
    }
}

/// The tape being written. Shared with every in-flight request handler.
type Tape = Arc<Mutex<Vec<Exchange>>>;

struct ProxyState {
    /// Upstream origin, trailing slash trimmed, e.g. `https://api.openai.com`.
    upstream: String,
    client: reqwest::Client,
    tape: Tape,
}

/// A bound, running capture proxy.
pub struct CaptureProxy {
    addr: SocketAddr,
    tape: Tape,
    id: String,
}

impl CaptureProxy {
    /// Bind a capture proxy on loopback, forwarding to `upstream`.
    ///
    /// Loopback-only, like the serving edge: a proxy that relays an agent's traffic — and
    /// holds the credential to do it — has no business being reachable off the machine.
    /// Port 0 lets the OS pick, which is what keeps concurrent recordings from colliding.
    pub async fn bind(id: impl Into<String>, upstream: &str) -> Result<Self, RecordError> {
        let client =
            reqwest::Client::builder()
                .build()
                .map_err(|source| RecordError::Upstream {
                    url: upstream.to_string(),
                    source,
                })?;

        let tape: Tape = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new(ProxyState {
            upstream: upstream.trim_end_matches('/').to_string(),
            client,
            tape: Arc::clone(&tape),
        });

        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|source| RecordError::Bind { addr, source })?;
        let addr = listener.local_addr().map_err(RecordError::Serve)?;

        // `fallback` rather than routes: the proxy is path-agnostic by design. It must relay
        // whatever the SDK asks for — today `/v1/chat/completions`, tomorrow an endpoint that
        // did not exist when this shipped — so enumerating provider paths here would just be
        // a list to keep stale.
        let app = Router::new().fallback(capture).with_state(state);

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(Self {
            addr,
            tape,
            id: id.into(),
        })
    }

    /// The base URL to hand the agent, e.g. via `OPENAI_BASE_URL`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The address the proxy is bound to.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Snapshot the tape as a cassette. Takes `&self` so a recording can be inspected while
    /// the session is still open.
    #[must_use]
    pub fn cassette(&self) -> Cassette {
        let exchanges = self
            .tape
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        Cassette {
            exchanges,
            ..Cassette::new(self.id.clone())
        }
    }
}

/// Relay one request upstream and record the round trip.
async fn capture(State(state): State<Arc<ProxyState>>, req: Request) -> Response {
    let method = req.method().clone();
    let path = req
        .uri()
        .path_and_query()
        .map_or_else(|| req.uri().path().to_string(), ToString::to_string);

    let (parts, body) = req.into_parts();
    let body = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("cannot read request body: {err}"),
            )
                .into_response();
        }
    };

    // Forward every inbound header, credentials included: the proxy is a relay, and the
    // agent's key is how upstream authenticates it. The allowlist governs what reaches the
    // *cassette*, which is a different question from what reaches the provider.
    let url = format!("{}{}", state.upstream, path);
    let mut outbound = state.client.request(method.clone(), &url);
    for (name, value) in &parts.headers {
        // `host` belongs to the proxy's own origin; forwarding it would point upstream at
        // 127.0.0.1 and vhost-routed providers would reject the request.
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        outbound = outbound.header(name, value);
    }

    let upstream_response = match outbound.body(body.clone()).send().await {
        Ok(response) => response,
        Err(err) => {
            // Relay failure is reported to the agent as a gateway error rather than a panic:
            // the agent under recording should see a normal HTTP failure it can handle.
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream request failed: {err}"),
            )
                .into_response();
        }
    };

    let status = upstream_response.status();
    let response_headers = upstream_response.headers().clone();
    let response_body = match upstream_response.bytes().await {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream response failed: {err}"),
            )
                .into_response();
        }
    };

    push(
        &state.tape,
        CapturedRequest {
            method: method.to_string(),
            path,
            headers: retain_request_headers(
                parts
                    .headers
                    .iter()
                    .map(|(n, v)| (n.as_str(), v.as_bytes())),
            ),
            body: Body::from_bytes(&body),
        },
        CapturedResponse {
            status: status.as_u16(),
            headers: retain_response_headers(
                response_headers
                    .iter()
                    .map(|(n, v)| (n.as_str(), v.as_bytes())),
            ),
            body: Body::from_bytes(&response_body),
        },
    );

    // The caller gets the upstream's answer, not ours. A recording that changes the run it
    // records is not a recording.
    let mut relayed = Response::builder().status(status);
    for (name, value) in &response_headers {
        // Hop-by-hop framing is the relay's own concern: we buffered the body, so upstream's
        // transfer-encoding/content-length no longer describe what we are about to send.
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        relayed = relayed.header(name, value);
    }
    relayed
        .body(axum::body::Body::from(response_body))
        .map_or_else(
            |err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("cannot relay response: {err}"),
                )
                    .into_response()
            },
            IntoResponse::into_response,
        )
}

/// Headers that describe a single hop's framing and must not be copied across a relay.
fn is_hop_by_hop(name: &str) -> bool {
    const HOP_BY_HOP: &[&str] = &[
        "connection",
        "content-length",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ];
    HOP_BY_HOP.iter().any(|h| name.eq_ignore_ascii_case(h))
}

/// Append one exchange to the tape, stamping its capture order.
fn push(tape: &Tape, request: CapturedRequest, response: CapturedResponse) {
    let mut tape = tape
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let idx = tape.len();
    tape.push(Exchange {
        idx,
        request,
        response,
    });
}
