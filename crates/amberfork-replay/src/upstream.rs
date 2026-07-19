//! The live-relay seam.
//!
//! Replay serves recorded responses until the re-execution branches off the tape; past that
//! point a request has no recorded answer and must go to a real provider. [`Upstream`] is that
//! one operation, behind a trait so the test suite can substitute an in-process script and keep
//! `cargo test --workspace` fully offline — the same discipline that quarantines `tokio` to the
//! I/O-edge crates.

use amberfork_record::{CapturedRequest, CapturedResponse};
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
    /// relays than it scripted. The live `reqwest` relay adds its transport-failure variant here
    /// when it lands; `#[non_exhaustive]` keeps that a non-breaking addition.
    Exhausted,
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => f.write_str("scripted upstream has no more responses"),
        }
    }
}

impl std::error::Error for UpstreamError {}

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
