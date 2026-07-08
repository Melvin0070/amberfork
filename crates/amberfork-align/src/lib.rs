//! The alignment engine — the moat crate.
//!
//! Takes two [`amberfork_model::Run`]s and produces the move-typed alignment, the fork, and
//! ultimately the [`amberfork_model::DiffResult`] the CLI and UI render. Three parts, each an
//! empirically locked decision (see `docs/design/design-run-diff-debugger.md`, Amendment
//! 2026-07-08, and `docs/notebook.md` 001–003):
//!
//! - [`cost`]: the step-vs-step cost model. v1 default is lexical ([`LexicalCost`]) —
//!   deterministic and dependency-free. Embeddings live behind the same [`CostModel`] trait
//!   and must beat lexical on dev fixtures to earn default status (Amendment B).
//! - affine-gap Needleman–Wunsch over those costs, emitting [`amberfork_model::Move`]s.
//! - the resync-k fork rule: fork = first non-sync block the alignment does not recover
//!   from within k synchronous moves (Amendment A).
//!
//! Sync + pure by design: no I/O, no async, no globals. Loading runs is `amberfork-ingest`'s
//! job; this crate is a function from two runs to a diff.

mod cost;
mod diff;
mod fork;
mod nw;
mod params;

pub use cost::{CostModel, LexicalCost};
pub use diff::{DiffParams, diff};
pub use fork::{ForkParams, find_fork};
pub use nw::{AlignParams, align};
pub use params::ParamError;
