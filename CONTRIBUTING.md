# Contributing to amberfork

This repo is built AI-native (primarily via Claude Code) to senior-engineer standards. The rules
that keep every session consistent live in [`CLAUDE.md`](CLAUDE.md) ("Operating manual"); this
file is the human-readable summary of the same working agreement.

## The loop

1. **Pick work from the tracker.** `gh issue list` — take the lowest-numbered unblocked issue in
   the current milestone. Milestones are the cut line (v0.1 walking skeleton → v0.2 benchmark).
2. **Read the governing doc.** Each issue points at the section of `docs/design/`, `BENCHMARK.md`,
   or `DESIGN.md` that specifies it. The design corpus is authoritative; where it conflicts, the
   dated "Amendment" / "Current State" blocks win.
3. **Build a vertical slice.** Keep `amberfork diff <bad> --against <good>` working end-to-end. Don't
   build a crate ahead of the need it serves.
4. **Verify before commit.** `python3 spike/test_smoke.py` today; `cargo fmt --all --check &&
   cargo clippy --all-targets -- -D warnings && cargo test --workspace` once the workspace exists.
   CI runs exactly these. A red CI stops the line.
5. **Record decisions.** Every experiment/measurement gets a `docs/notebook.md` entry (append-only).
   Benchmark numbers follow `BENCHMARK.md`'s pre-registered protocol — no number outside it.
6. **Commit small.** Conventional one-liners (`feat:`/`fix:`/`bench:`/`docs:`/`chore:`), one
   logical change each.

## Standards

- **Contracts first** — the `DiffResult`/trace-format schema is the seam; version it, never fork it.
- **Engine crates are sync + pure**; `tokio` is quarantined to I/O edges (ingest, serve).
- **Tests are part of done** — unit / `proptest` invariant / `insta` snapshot. Canonical guard:
  a run aligned against itself has no fork.
- **Honesty in artifacts** — report the measured number, its caveat, and coverage. A flake is a
  failure, not a retry.

## Layout

- `crates/` — the Rust workspace (once it exists; see the crate map in `docs/design/`).
- `spike/` — throwaway Python feasibility work. Findings port to Rust; the code never ships.
- `docs/notebook.md` — the engineering log. `docs/design/` — the locked architecture + positioning.
- `BENCHMARK.md` — the pre-registered evaluation protocol. `DESIGN.md` — the visual system.
