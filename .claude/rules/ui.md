---
paths:
  - "ui/**"
  - "crates/amberfork-layout/**"
  - "crates/amberfork-server/**"
  - "DESIGN.md"
---

# UI work

`DESIGN.md` is the brief and wins every conflict. Read it before any visual decision. North
star: "sameness recedes, divergence glows" — colour is reserved for divergence (fork +
divergent path in amber `#FF7A1A`); red/green only inside the content-diff pane. Render with
DOM/SVG, never canvas/wgpu, so text stays selectable and accessible.

Invoke the `frontend-design` skill proactively for any UI build or restyle, without being
asked. It is the anti-slop execution layer. It must never introduce a new palette, typeface
or signature element — amberfork's signature is the amber fork ignition, already chosen.

## The two things that break builds here

- `ui/` is a SEPARATE workspace (`exclude`d from the root). `cargo test --workspace` does not
  touch its 43 tests, and `ui.yml` runs on every PR regardless of paths. Verify with
  `scripts/verify.sh --full`.
- `ssr` (default) and `csr` are mutually exclusive Leptos features. Host tests render SSR
  strings; the shipped wasm build is `--no-default-features --features csr`. Clippy must pass
  on BOTH or you ship a wasm build that never compiled. `#[cfg(feature = "csr")]` helpers
  compile to no-ops under `ssr`.

## Other traps

- `amberfork serve` always fails in a dev checkout: `ui-dist/` is gitignored (`.gitkeep` only,
  because rust-embed needs the dir at compile time) → `BundleMissing`, exit 2. A test asserts
  that. To see the UI live: `cd ui && trunk serve` (127.0.0.1:8080).
- Two unrelated truncation mechanisms, easy to conflate: the browser path applies a 4 KiB
  per-slot wire envelope inside `Document::new`; `ViewModel::compute` always emits full text
  and the CLI painter does its own width-based abbreviation, never seeing a cut slot.
- Field diffs ride every aligned pair (`AlignedStep`), not just the fork row.
