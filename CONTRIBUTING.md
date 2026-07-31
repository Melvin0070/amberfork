# Contributing to amberfork

This repo is built AI-native (primarily via Claude Code) to senior-engineer standards. The rules
that keep every session consistent live in [`CLAUDE.md`](CLAUDE.md) ("Operating manual"); this
file is the human-readable summary of the same working agreement.

## The loop

1. **Pick work from the tracker.** `gh issue list` — take the lowest-numbered unblocked issue in
   the current milestone. Milestones are the cut line (v0.1 walking skeleton and v0.2 offline
   benchmark shipped; now **v0.8 the credibility pass** → **v0.9 the explain layer**).
2. **Read the governing doc.** Each issue points at the section of `docs/design/`, `BENCHMARK.md`,
   or `DESIGN.md` that specifies it. The design corpus is authoritative; where it conflicts, the
   dated "Amendment" / "Current State" blocks win.
3. **Build a vertical slice.** Keep `amberfork diff <bad> --against <good>` working end-to-end. Don't
   build a crate ahead of the need it serves.
4. **Verify before commit.** `python3 spike/test_smoke.py && cargo fmt --all --check &&
   cargo clippy --all-targets -- -D warnings && cargo test --workspace`.
   CI runs exactly these. A red CI stops the line. The fork-localization parity gate is inside
   `cargo test --workspace` — it runs on the committed, GAIA-sanitized dev set in
   `bench/fixtures/chimera_noise_seed42_dev/`, so an `amberfork-align` change that tanks parity is
   a red CI, not a silent pass (see that dir's README to audit/regenerate the fixture).
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

- `crates/` — the Rust workspace (10 crates at v0.7.0, grown by need; roster + rationale in `docs/design/`).
- `spike/` — Python, two kinds. Most is throwaway feasibility work (findings port to Rust, the
  code never ships). The exception is the **maintained benchmark data pipeline** —
  `convert_whowhen.py` → `amberfork-bench sanitize canonical` → `make_pairs.py` →
  `amberfork-bench sanitize pairs` — which regenerates and GAIA-sanitizes the committed
  fixtures. The sanitizer stages are Rust (issue #17, inside the `cargo test` gate); the
  generation scripts remain Python, re-runnable and self-verifying.
- `docs/notebook.md` — the engineering log. `docs/design/` — the locked architecture + positioning.
- `BENCHMARK.md` — the pre-registered evaluation protocol. `DESIGN.md` — the visual system.

## Collaboration mode

Chosen 2026-07-08 — "I build, you review". The founder is learning proper software dev, so an
agent works ONE vertical slice at a time and teaches through it:

1. State the contract and the test you'll write.
2. Test-first: red → green → refactor.
3. Show the diff and the WHY behind each choice.
4. WAIT for founder review/approval before committing or merging.
5. Next slice.

Keep commits small and reviewable. Never batch multiple issues silently. On a multi-part
issue, propose slice boundaries (or the reason to merge them) BEFORE building and let the
founder decide; never collapse review checkpoints unilaterally. Don't run a whole milestone in
one session even with per-slice approval — checkpoints minutes apart become "click approve",
not teaching. This is a learning collaboration, not autonomous delivery.

Commit straight to `main`: no branches, no PRs. `(#N)` in a commit message is a GitHub *issue*
reference.

## Engineering standards

- **Optimize for the artifact, not the effort.** Give essentially no weight to development
  cost when weighing a technical decision. Weigh quality, simplicity, modern practice,
  robustness and long-term maintainability. "It's just a solo project" is not a valid reason
  to pick the lesser option. This governs HOW each slice is built, not WHICH slices exist —
  it is not licence to gold-plate or break vertical-slice discipline. Simplicity is itself a
  quality goal: the highest-quality *simplest* implementation of the thing needed now.
- **Vertical slices, not horizontal layers.** Keep `amberfork diff <bad> --against <good>`
  working end-to-end at every commit; thicken the slice. Never build a crate ahead of need.
- **Contracts first.** `DiffResult`/trace-format is the seam every consumer reads. Change it
  deliberately, version it (`schema_version`), never fork it per consumer.
- **Types over stringly-typed.** Enums/newtypes and `Result` over panics on the library path.
- **Tests are part of done.** New behaviour ships with a test (unit / `proptest` / `insta`).
  The self-align invariant — a run vs itself has no fork — is the canonical guard.
- **Honesty in artifacts.** Report the number measured, its caveat, its coverage. A flake is a
  failure, not a retry. Correct a flattering number when a fuller run contradicts it.
- **No scope creep.** New thinking goes to `docs/notebook.md` or an issue, never a new
  root-level doc.
