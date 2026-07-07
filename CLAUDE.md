# agentdiff

Local, all-Rust developer tool that diffs two AI-agent run trajectories, finds the fork
point, and attributes the regression. Architecture is locked in
`docs/design/design-run-diff-debugger.md` (hybrid passive+record execution model, 14-crate
workspace, explainable semantic move-typed alignment + counterfactual-causal attribution,
embedded Leptos SVG/DOM web UI). Positioning/personas: `docs/design/POSITIONING.md`.
Benchmark protocol: `BENCHMARK.md`. Engineering notebook (spikes, measurements, dead ends):
`docs/notebook.md`. Canonical plain-JSON trace input: `docs/trace-format.md`.

## Operating manual (AI-native workflow)

**State:** pre-v1. Tracker = GitHub issues + milestones (`gh issue list`). Decisions and
measurements live in `docs/notebook.md` (append-only; every experiment gets an entry).
Benchmark numbers are governed by BENCHMARK.md's pre-registered protocol — never publish a
number outside it.

**Verify before commit:** `python3 spike/test_smoke.py` (offline, <10s). Once the Rust
workspace exists: `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings &&
cargo test --workspace`. CI runs exactly these; a red CI is a stop-the-line event.

**Working rules:**
- Walking-skeleton discipline: keep `adiff diff <bad> --against <good>` working end-to-end at
  every commit; thicken vertical slices, never build horizontal layers ahead of need.
- Planning freeze: no new root-level planning docs until v0.1 ships; new thinking goes to
  `docs/notebook.md` or an issue.
- `spike/` is throwaway Python — port its findings to Rust, never import its code.
- Fork criterion (spike 001, empirical): "first non-sync block the alignment does not recover
  from" (resync rule) — NOT "first non-sync move". Cost model starts lexical/tf-idf; embeddings
  must beat lexical on dev fixtures to earn a place.
- Commit style: short conventional one-liners (`feat:`, `fix:`, `bench:`, `docs:`, `chore:`).

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
