//! The explain layer: a local, optional model that narrates a fork `amberfork-align` already
//! computed (issue #10, Phase 2 / Zone 3 of the design doc).
//!
//! The deterministic aligner stays the headline. This crate never localizes — it reads a
//! finished [`amberfork_model::DiffResult`] plus a narrow content window and only ever
//! describes or speculates. Two guardrails are enforced in the types rather than left to
//! convention:
//!
//! - [`ExplainContext`] hands a [`Judge`] the fork step (+ `k` neighbours) it is scoped to
//!   reason about, never the two full trajectories — a judge cannot hunt for a different fork
//!   because it never sees content outside the window.
//! - [`ground`] validates a judge's claimed fork index against the `DiffResult` it was asked
//!   about before the result is trusted; an explanation that contradicts the aligner is
//!   rejected, never rendered.
//!
//! This slice is the crate skeleton only: the [`Judge`] trait, [`ScriptedJudge`] (the in-process
//! test double that keeps `cargo test --workspace` offline), and the grounding guard. No CLI
//! wiring, no live provider, no network — those are later slices (`--judge local|off` and an
//! `Ollama`-backed implementation).

mod context;
mod grounding;
mod judge;
mod scripted;

pub use context::{ExplainContext, Side, StepSnapshot};
pub use grounding::{Grounded, GroundingError, ground};
pub use judge::{Explanation, Judge, JudgeError};
pub use scripted::ScriptedJudge;
