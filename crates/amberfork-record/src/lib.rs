//! The record path: wrap an agent, capture what crossed the provider boundary.
//!
//! amberfork has two ingestion paths sharing one canonical model (design doc, "Counterfactual
//! attribution is impossible against passive OTLP alone"). The PASSIVE path aligns OTel traces
//! you already have — framework-agnostic, but content is opt-in and often absent, and you
//! cannot re-run a telemetry photo. This crate is the RECORD path: it runs the agent behind a
//! local capture proxy, so content is guaranteed and the run stays re-executable.
//!
//! Two things follow from that, and they are the reason this crate exists rather than being a
//! flag on `ingest`:
//! - **Full content.** The passive path's `full content guaranteed` cell is "no"; this path's
//!   is "yes". Full *inputs* are worth +76% relative step-level accuracy over output-only logs
//!   ("Replay fidelity ceiling"), which improves the ordinary diff before any re-execution
//!   exists.
//! - **Re-executability.** A boundary recording can be replayed and mutated, which is what
//!   counterfactual attribution needs. This crate captures; re-execution is its consumer.
//!
//! `tokio` lives here because this is an I/O edge, alongside `amberfork-server`. Engine crates
//! stay sync and pure.

mod cassette;
mod normalize;
mod proxy;

pub use cassette::{
    Body, CapturedRequest, CapturedResponse, Cassette, CassetteVersion, Exchange,
    retain_request_headers,
};
pub use normalize::{normalize, normalize_str};
pub use proxy::{CaptureProxy, RecordError};
