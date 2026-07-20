//! The replay path: re-drive a recorded agent run from its cassette.
//!
//! Where `amberfork-record` writes a cassette by *capturing* what crossed the provider
//! boundary, this crate *serves* it back — the VCR to record's tape deck. It is the substrate
//! counterfactual attribution stands on (epic #35): to ask "what if step N had gone the good
//! run's way", you re-run the agent against the recording, patch one response, and watch what
//! it does next. That re-run needs something that answers the agent's requests from the tape
//! and only reaches a live provider once the run has branched off it.
//!
//! This crate is an I/O edge, alongside `amberfork-record` and `amberfork-server`: when the
//! live relay and loopback listener land (later slices of #36), `tokio`/`reqwest` live here,
//! never in the engine crates. The matching core in [`Replay`] is deliberately sync and pure —
//! the async surface is confined to the [`Upstream`] seam.
//!
//! ## Matching is content-addressed, not positional
//!
//! A re-issued request is matched to a recorded exchange on `(method, path, body)` — the same
//! key `amberfork-record`'s cassette contract names as load-bearing — and never on call order.
//! In an agent loop the request body carries the whole accumulated message history, so each
//! turn's body is distinct and content-addressing distinguishes turns for free; it is also what
//! lets a patched re-run still match the turns it did *not* change while missing on the one it
//! did.
//!
//! ## Fidelity limit (stated, not solved)
//!
//! Clock and seed virtualization are not attempted: a re-run that reads wall-clock time or a
//! fresh RNG seed can diverge for reasons unrelated to a patch. Making replay bit-exact against
//! a nondeterministic provider is physically impossible; the counterfactual oracle (epic #35)
//! tolerates this by consensus over multiple runs, degrading to `Unverified` rather than
//! asserting a false result. Keeping that limit here, in the crate that could pretend otherwise,
//! is the honest place for it.

mod proxy;
mod replay;
mod upstream;

pub use proxy::ReplayProxy;
pub use replay::Replay;
pub use upstream::{ScriptedUpstream, Upstream, UpstreamError};
