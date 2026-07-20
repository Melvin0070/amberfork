//! The loopback listener: the boundary the re-driven agent talks to.
//!
//! [`ReplayProxy`] is the in-process driver — hand it a request, it answers from the tape or
//! relays. `ReplayServer` puts that driver behind an HTTP listener so a real agent, pointed at
//! `http://127.0.0.1:PORT` with a base-URL environment variable (`OPENAI_BASE_URL`,
//! `ANTHROPIC_BASE_URL`, …), re-executes against the recording with no code change — the exact
//! mirror of how `amberfork-record`'s [`CaptureProxy`](amberfork_record::CaptureProxy) sits in
//! front of a live provider. This crate is an I/O edge: `tokio`/`axum` live here, never in the
//! engine crates.
//!
//! ## Requests are serialized through one async mutex
//!
//! The driver is shared as `Arc<Mutex<ReplayProxy<U>>>` and each request locks it for the whole
//! [`ReplayProxy::answer`] call — including the live relay on a cache miss. That is deliberate,
//! not a lock held by accident: an agent loop is inherently sequential (each turn's request
//! carries the full accumulated message history, so turn N+1 cannot be built until response N
//! arrives), and the re-executed tape's `idx` order *is* answer order, which only a serialized
//! path can keep well-defined. A `tokio::sync::Mutex` (not `std`) is used precisely because the
//! guard is held across that await.

use crate::{ReplayProxy, Upstream};
use amberfork_record::{Body, CapturedRequest, CapturedResponse, Cassette, retain_request_headers};
use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// The driver, shared with every in-flight request handler. Mirrors record's `Tape`, but the
/// whole proxy is shared here (not just the tape) because answering a request reads the cassette,
/// consults the upstream, and appends the turn as one indivisible step.
type Shared<U> = Arc<Mutex<ReplayProxy<U>>>;

/// Something that went wrong standing up the replay listener.
///
/// Narrower than record's `RecordError` on purpose: replay does not construct an HTTP client to a
/// provider URL (the [`Upstream`] seam is passed in already built), so there is no upstream-origin
/// variant here. A live-relay failure surfaces as [`crate::UpstreamError`] from `answer`, not from
/// binding the listener. `#[non_exhaustive]` leaves room for a variant the live relay may add.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReplayError {
    /// Binding the loopback listener failed — in practice: the port is already taken.
    Bind {
        /// The address the bind was attempted on.
        addr: SocketAddr,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// The listener bound but its local address could not be read back.
    Serve(io::Error),
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind { addr, source } => write!(f, "cannot bind {addr}: {source}"),
            Self::Serve(source) => write!(f, "replay listener failed: {source}"),
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind { source, .. } | Self::Serve(source) => Some(source),
        }
    }
}

/// A bound, running replay listener serving a cassette back over HTTP.
pub struct ReplayServer<U> {
    addr: SocketAddr,
    proxy: Shared<U>,
}

impl<U: Upstream + 'static> ReplayServer<U> {
    /// Bind a replay listener on loopback that answers from `cassette`, relays misses through
    /// `upstream`, and records the re-execution onto a fresh cassette stamped `reexecuted_id`.
    ///
    /// Loopback-only, like record's capture proxy and the serving edge: a proxy the agent hands
    /// its credential to has no business being reachable off the machine. Port 0 lets the OS pick,
    /// which keeps concurrent replays from colliding.
    ///
    /// # Errors
    ///
    /// [`ReplayError::Bind`] if the loopback listener cannot bind; [`ReplayError::Serve`] if its
    /// bound address cannot be read back.
    pub async fn bind(
        cassette: Cassette,
        upstream: U,
        reexecuted_id: impl Into<String>,
    ) -> Result<Self, ReplayError> {
        let proxy: Shared<U> = Arc::new(Mutex::new(ReplayProxy::new(
            cassette,
            upstream,
            reexecuted_id,
        )));

        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|source| ReplayError::Bind { addr, source })?;
        let addr = listener.local_addr().map_err(ReplayError::Serve)?;

        // `fallback`, not routes: like the capture proxy, the listener is path-agnostic. It answers
        // whatever endpoint the SDK asks for — matching is on the request, never on a route table
        // that would go stale the day a provider adds a path.
        let app = Router::new()
            .fallback(answer::<U>)
            .with_state(Arc::clone(&proxy));

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Ok(Self { addr, proxy })
    }

    /// The base URL to hand the agent, e.g. via `OPENAI_BASE_URL`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The address the listener is bound to.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Snapshot the re-executed run so far — every turn this replay took, in order.
    ///
    /// Async, and takes `&self`, so a re-execution can be inspected while the listener is still
    /// serving: it briefly locks the driver and clones its growing tape. The result is a complete
    /// cassette, the shape [`amberfork_record::normalize`] consumes.
    pub async fn reexecuted(&self) -> Cassette {
        self.proxy.lock().await.reexecuted().clone()
    }
}

/// Answer one request the re-driven agent issued: parse it into a [`CapturedRequest`], drive it
/// through the shared [`ReplayProxy`], and serve the [`CapturedResponse`] back over HTTP.
async fn answer<U: Upstream + 'static>(
    State(proxy): State<Shared<U>>,
    request: Request,
) -> Response {
    let method = request.method().to_string();
    // Path and query as the agent sent them; the upstream origin is the session's, not the
    // exchange's, and never appears in a cassette request.
    let path = request
        .uri()
        .path_and_query()
        .map_or_else(|| request.uri().path().to_string(), ToString::to_string);

    let (parts, body) = request.into_parts();
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

    let captured = CapturedRequest {
        method,
        path,
        // Reuse record's single allowlist: the re-executed cassette is shareable, so the agent's
        // credential must be dropped here just as it is on capture. Headers are not load-bearing
        // for matching either, so this only governs what reaches the re-executed tape.
        headers: retain_request_headers(
            parts
                .headers
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_bytes())),
        ),
        body: Body::from_bytes(&body),
    };

    // Scope the guard to the answer itself: the body was read above without the lock, and the
    // response is built below from the returned value, so the driver is held only across `answer`.
    let answered = { proxy.lock().await.answer(captured).await };

    match answered {
        Ok(response) => into_http(response),
        Err(err) => (StatusCode::BAD_GATEWAY, format!("live relay failed: {err}")).into_response(),
    }
}

/// Render a recorded (or relayed) [`CapturedResponse`] as the HTTP response the agent receives.
///
/// The recorded response's own headers are replayed verbatim — record's allowlist already reduced
/// them to `content-type`. When a response carries none (a scripted or synthesized one might), a
/// content type is defaulted from the body shape so the agent's SDK still parses it.
fn into_http(response: CapturedResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let (bytes, default_content_type) = match response.body {
        // `to_vec` on a `Value` is infallible in practice (string keys, no non-finite floats from a
        // parsed body); the empty fallback keeps this total rather than panicking on the impossible.
        Body::Json(value) => (
            serde_json::to_vec(&value).unwrap_or_default(),
            "application/json",
        ),
        Body::Text(text) => (text.into_bytes(), "text/plain; charset=utf-8"),
    };

    let mut builder = Response::builder().status(status);
    let mut has_content_type = false;
    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        builder = builder.header(name, value);
    }
    if !has_content_type {
        builder = builder.header("content-type", default_content_type);
    }

    builder
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
