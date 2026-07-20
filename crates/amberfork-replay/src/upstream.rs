//! The live-relay seam.
//!
//! Replay serves recorded responses until the re-execution branches off the tape; past that
//! point a request has no recorded answer and must go to a real provider. [`Upstream`] is that
//! one operation, behind a trait so the test suite can substitute an in-process script and keep
//! `cargo test --workspace` fully offline — the same discipline that quarantines `tokio` to the
//! I/O-edge crates.

use amberfork_record::{Body, CapturedRequest, CapturedResponse, retain_response_headers};
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::{Mutex, PoisonError};

/// Relay one request the recording could not answer to a live provider.
///
/// A single method, mirroring `CaptureProxy`'s forward step but in the replay direction. It is
/// async and `Send` because the production implementation performs network I/O inside the
/// listener's task; the trait is written in terms of the cassette's own `CapturedRequest` /
/// `CapturedResponse` so a relayed exchange drops straight back onto the re-executed tape.
///
/// Expressed as a native async trait (return-position `impl Future`, not the `async-trait`
/// crate): a call site picks its implementation at compile time — [`ScriptedUpstream`] in tests,
/// a live `reqwest` client in production — so no `dyn`, and no per-relay allocation, is needed.
pub trait Upstream: Send + Sync {
    /// Send `request` upstream and return the response.
    ///
    /// # Errors
    ///
    /// Returns [`UpstreamError`] when a response cannot be produced — for the scripted stub,
    /// when it has no more responses queued; for the live relay (later slice), a transport
    /// failure.
    fn send(
        &self,
        request: &CapturedRequest,
    ) -> impl Future<Output = Result<CapturedResponse, UpstreamError>> + Send;
}

/// Why a live relay could not produce a response.
#[derive(Debug)]
#[non_exhaustive]
pub enum UpstreamError {
    /// The in-process scripted stub ran out of queued responses — a test drove more cache-miss
    /// relays than it scripted.
    Exhausted,
    /// The live relay to the real provider failed at the transport level — the origin was
    /// unreachable, the connection dropped, or the response body could not be read.
    Transport(reqwest::Error),
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => f.write_str("scripted upstream has no more responses"),
            Self::Transport(source) => write!(f, "live relay to the provider failed: {source}"),
        }
    }
}

impl std::error::Error for UpstreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Exhausted => None,
            Self::Transport(source) => Some(source),
        }
    }
}

/// The live provider relay: forward a cache-miss to the real API and return its answer.
///
/// The production [`Upstream`], the counterpart to [`ScriptedUpstream`]. Once a re-execution
/// branches off the tape — which a counterfactual re-run does the moment it passes the patched step
/// — the request has no recorded answer and must reach a real provider. `LiveUpstream` forwards it
/// verbatim to the configured origin and captures the round trip into a [`CapturedResponse`], the
/// same shape the tape holds, so the relayed turn drops straight onto the re-executed cassette.
///
/// The client is injected rather than built here so a `verify` can share one connection pool across
/// all N re-runs; [`reqwest::Client`] is internally reference-counted, so cloning it per run is
/// cheap.
#[derive(Debug, Clone)]
pub struct LiveUpstream {
    client: reqwest::Client,
    origin: String,
}

impl LiveUpstream {
    /// A relay that forwards misses to `origin` (e.g. `https://api.openai.com`) over `client`.
    /// A trailing slash on the origin is trimmed so `origin + path` never doubles it.
    #[must_use]
    pub fn new(client: reqwest::Client, origin: impl Into<String>) -> Self {
        Self {
            client,
            origin: origin.into().trim_end_matches('/').to_string(),
        }
    }
}

impl Upstream for LiveUpstream {
    fn send(
        &self,
        request: &CapturedRequest,
    ) -> impl Future<Output = Result<CapturedResponse, UpstreamError>> + Send {
        // Build the outbound request synchronously, then move it into the future — so the future
        // borrows neither `self` nor `request` and is straightforwardly `Send`.
        let method =
            reqwest::Method::from_bytes(request.method.as_bytes()).unwrap_or(reqwest::Method::POST);
        let mut outbound = self
            .client
            .request(method, format!("{}{}", self.origin, request.path));
        for (name, value) in &request.headers {
            // `host` names the loopback proxy's own origin; forwarding it would point the provider
            // back at us. reqwest sets the correct host for the outbound origin itself.
            if name.eq_ignore_ascii_case("host") {
                continue;
            }
            outbound = outbound.header(name, value);
        }
        let body = match &request.body {
            // Re-serialize compactly; `to_vec` on a parsed body is infallible in practice.
            Body::Json(value) => serde_json::to_vec(value).unwrap_or_default(),
            Body::Text(text) => text.clone().into_bytes(),
        };
        let outbound = outbound.body(body);

        async move {
            let response = outbound.send().await.map_err(UpstreamError::Transport)?;
            // Status and body are forwarded verbatim: a 429 or a 500 is a genuine divergence signal,
            // not a relay failure. Only a transport-level fault (unreachable, dropped) is an error.
            let status = response.status().as_u16();
            let headers = retain_response_headers(
                response
                    .headers()
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_bytes())),
            );
            let body = response.bytes().await.map_err(UpstreamError::Transport)?;
            Ok(CapturedResponse {
                status,
                headers,
                body: Body::from_bytes(&body),
            })
        }
    }
}

/// An in-process [`Upstream`] that serves a fixed script of responses in order.
///
/// This is the seam that keeps replay tests offline: instead of a network provider, a re-run's
/// cache-miss relays are answered from a queue the test set up. It hands out responses in FIFO
/// order and reports [`UpstreamError::Exhausted`] once the script runs dry, so a test that
/// under-scripts fails loudly rather than serving a stale answer.
#[derive(Debug)]
pub struct ScriptedUpstream {
    responses: Mutex<VecDeque<CapturedResponse>>,
}

impl ScriptedUpstream {
    /// A stub that will serve `responses` in order on successive cache misses.
    #[must_use]
    pub fn new(responses: impl IntoIterator<Item = CapturedResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
        }
    }
}

impl Upstream for ScriptedUpstream {
    fn send(
        &self,
        _request: &CapturedRequest,
    ) -> impl Future<Output = Result<CapturedResponse, UpstreamError>> + Send {
        // The next scripted response is resolved synchronously and moved into the returned
        // future; the async wrapper exists only to satisfy the trait's I/O-shaped signature, so
        // it borrows neither `self` nor `request`.
        let next = self
            .responses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .ok_or(UpstreamError::Exhausted);
        async move { next }
    }
}
