# agentdiff

Local, all-Rust developer tool that diffs two AI-agent run trajectories, finds the fork
point, and attributes the regression. Architecture is locked in
`docs/design/design-run-diff-debugger.md` (hybrid passive+record execution model, 14-crate
workspace, explainable semantic move-typed alignment + counterfactual-causal attribution,
embedded Leptos SVG/DOM web UI). Positioning/personas: `docs/design/POSITIONING.md`.
Benchmark protocol: `BENCHMARK.md`. Engineering notebook (spikes, measurements, dead ends):
`docs/notebook.md`. Canonical plain-JSON trace input: `docs/trace-format.md`.

## Operating manual (AI-native workflow)

> A `SessionStart` hook (`.claude/session-context.sh`) prints live state — branch, working
> tree, open issues + milestone, latest notebook entry — into context at the start of every
> session. This section is the durable working agreement behind it.

**Start of a session (do this before writing code):** read the injected state block; if a task
isn't already named, pick the lowest-numbered unblocked issue in the current milestone
(`gh issue list`); skim the issue body for the doc section that governs it. If resuming, run the
verify command to confirm a green baseline before changing anything.

**State:** pre-v1. Tracker = GitHub issues + milestones (`gh issue list`). Milestones encode the
cut line: **v0.1 = walking skeleton** (#1 model → #2 ingest → #3 align → #4 CLI → #5 demo),
**v0.2 = offline benchmark** (#6 bench, #7 Mode A′). Decisions and measurements live in
`docs/notebook.md` (append-only; every experiment gets an entry). Benchmark numbers are governed
by BENCHMARK.md's pre-registered protocol — never publish a number outside it.

**Verify before commit (non-negotiable):** `python3 spike/test_smoke.py` (offline, <10s). Once
the Rust workspace exists: `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings
&& cargo test --workspace`. CI runs exactly these; a red CI is a stop-the-line event. Commit or
push only when the user asks.

**Engineering standards (build like a senior engineer):**
- **Vertical slices, not horizontal layers.** Keep `adiff diff <bad> --against <good>` working
  end-to-end at every commit; thicken the slice. Never build a crate ahead of the need it serves.
- **Contracts first.** The `DiffResult`/trace-format schema is the seam every consumer reads;
  change it deliberately, version it (`schema_version`), never fork it per-consumer.
- **Types over stringly-typed.** Prefer enums/newtypes and `Result` over panics on the library
  path; `tokio` stays quarantined to I/O edges (ingest/serve), engine crates stay sync + pure.
- **Tests are part of done.** New behavior ships with a test (unit / `proptest` invariant /
  `insta` snapshot). The self-align invariant (a run vs itself = no fork) is the canonical guard.
- **Honesty in artifacts.** Report the number you measured, the caveat, the coverage. A flake is
  a failure, not a retry. Correct a flattering number when a fuller run contradicts it (see the
  70%→~50% correction in notebook 002).
- **Small, conventional commits** (`feat:`/`fix:`/`bench:`/`docs:`/`chore:`), one logical change
  each, message says why.
- **No scope creep.** Planning freeze until v0.1 ships: new thinking goes to `docs/notebook.md`
  or an issue, not a new root-level doc. `spike/` is throwaway Python — port findings to Rust,
  never import the code.

**Empirical decisions already locked (don't relitigate; see `docs/design/…` Amendment 2026-07-08):**
- Fork criterion = "first non-sync BLOCK the alignment does not recover from" (resync-k, default
  k=2) — NOT "first non-sync move" (that measured 0%).
- Cost model starts lexical/tf-idf (deterministic, no ONNX); embeddings must beat lexical on dev
  fixtures to earn default status. ONNX/T25 is optional, off the critical path.
- Benchmark = controlled-injection (primary, reproducible) + Mode A′ cross-system pairs (v0.2
  co-primary, windowed metrics only — cross-system gold is murky).

## Design System
Always read `DESIGN.md` before making any visual or UI decisions.
All font choices, colors, spacing, layout, and aesthetic direction are defined there.
The north star is "sameness recedes, divergence glows": color is reserved for divergence
(the fork + divergent path in amber `#FF7A1A`); red/green only inside the content-diff pane.
Render with DOM/SVG (never canvas/wgpu) so text stays selectable and accessible.
Do not deviate without explicit user approval. In QA mode, flag any code that doesn't match DESIGN.md.

## Skill routing

When the user's request matches an available skill, invoke it via the Skill tool. When in doubt, invoke the skill.

Key routing rules:
- Product ideas/brainstorming → invoke /office-hours
- Strategy/scope → invoke /plan-ceo-review
- Architecture → invoke /plan-eng-review
- Design system/plan review → invoke /design-consultation or /plan-design-review
- Full review pipeline → invoke /autoplan
- Bugs/errors → invoke /investigate
- QA/testing site behavior → invoke /qa or /qa-only
- Code review/diff check → invoke /review
- Visual polish → invoke /design-review
- Ship/deploy/PR → invoke /ship or /land-and-deploy
- Save progress → invoke /context-save
- Resume context → invoke /context-restore
- Author a backlog-ready spec/issue → invoke /spec
