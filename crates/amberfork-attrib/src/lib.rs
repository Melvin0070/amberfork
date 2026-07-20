//! Counterfactual attribution: verify a fork by re-running the agent, not just observing it.
//!
//! `amberfork diff` localizes the fork and labels attribution `Static` — a structural claim
//! ("they diverge here, and it propagates downstream") that no re-execution ever checked. This
//! crate is the consumer the record/replay path was built for (epic #35): it patches the fork
//! step with the good run's behaviour, re-drives the agent through [`amberfork_replay`] against
//! the patched cassette, and reports whether the run *recovered* — the difference between "they
//! differ here" and "this is what broke it".
//!
//! It is the one crate that turns [`AttributionMode::Counterfactual`](amberfork_model::AttributionMode)
//! — defined in the frozen model but produced nowhere — into real output.
//!
//! ## Offline by default, opt-in to re-execute
//!
//! Nothing here runs unless the CLI is invoked with `--verify`: default `amberfork diff` stays
//! 100% offline and structural. Re-execution needs a live provider seam and a re-runnable agent
//! command, both injected by the caller; the whole test suite substitutes an in-process stub for
//! each, so `cargo test --workspace` stays offline and deterministic.
//!
//! ## Shape of the pipeline
//!
//! ```text
//! DiffResult + bad/good cassettes
//!   └─ patch_cassette        (pure) → the bad cassette with the fork step's response swapped
//!        └─ re-execute × N    (I/O) → each re-run's own cassette, via ReplayServer + the agent
//!             └─ recovery oracle (pure) → recovered / not recovered / inconclusive, per run
//!                  └─ consensus over N (pure) → Recovery tri-state → upgraded Attribution
//! ```
//!
//! This slice provides the first stage; the rest land in the following slices of issue #37.

mod driver;
mod oracle;
mod patch;

pub use driver::{AgentDriver, AgentError, ReexecError};
pub use patch::patch_cassette;
