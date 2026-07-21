# Changelog

## [Unreleased]

## [0.7.0] — 2026-07-21

v0.7 — counterfactual attribution (milestone issues #35–#38): the REPLAY + re-execution half the
record path was built for. `amberfork diff --verify` no longer just localizes the fork and labels it
`Static` — it re-executes the cassette with the fork patched to the good run's behaviour and reports
whether the run *recovered*: the difference between "they differ here" and "this is what broke it".
`AttributionMode::Counterfactual`, defined in the frozen model since it was authored and produced
nowhere, is now real output.

- **`amberfork-replay`** (9th crate): VCR re-execution. A cassette matcher (tool-call-ID normalized)
  behind an `Upstream` seam, a `ReplayProxy` that relays-on-miss and records every turn, and a
  loopback `ReplayServer` the re-driven agent talks to — serving recorded responses for the recorded
  path and going live once the run branches past the patch. `LiveUpstream` forwards a cache-miss to
  the real provider (issues #36, #37).
- **`amberfork-attrib`** (10th crate): the counterfactual harness — the design's named moat.
  `patch_cassette` swaps the fork step's response for the good run's; the re-execution driver stands
  up the replay server and re-drives the agent (an injected `AgentDriver` seam); the recovery oracle
  aligns the re-run against good under the same resync-k fork rule; and consensus over N folds the
  per-run verdicts into a tri-state `Recovery` (`recovered` / `not_recovered` / `unverified`),
  degrading to `Unverified` rather than asserting a result nondeterministic re-runs did not agree on
  (issues #36, #37).
- **`amberfork diff --verify … -- <cmd>`**: the opt-in verb. Reuses `record`'s trio (`--upstream`,
  `--base-url-env`, `-- <cmd>`) to re-drive the agent; a trailing segment on the attribution line
  reports the verdict (`… · recovered · 3 runs`). `--verify` requires both inputs to be cassettes (a
  passive trace has nothing to re-run) and is validated in one unit-testable `resolve()` (issue #37).
- **ddmin minimal-cause + origination/propagation labeling**: instead of patching only the fork,
  hand-rolled Zeller–Hildebrandt ddmin reduces the candidate region to the *minimal subset whose
  patch still recovers*, then splits it into verified **origination** (the minimal cause) and
  **propagation** (what recovers for free once the cause is patched) — pulling an independent
  downstream fault out of the tail that static analysis would have mislabeled. `origin_step` tightens
  to the minimal cause; `confidence` reflects oracle stability across the ddmin re-runs (issue #38).

Offline invariant held throughout: default `amberfork diff` (terminal + `--json`) is byte-identical
to pre-epic, and the whole suite substitutes in-process stubs for the agent and provider, so `cargo
test --workspace` stays offline and deterministic. Semantic cause naming (`cause_label`) stays the
judge's job (issue #10), never localization's.

## [0.6.0] — 2026-07-19

v0.6 — the record path (milestone issues #32–#34): the RECORD half of the hybrid
passive+record architecture. `Source::Record` is now reachable from the CLI, and a run
amberfork captured itself diffs through the same engine as a passively-ingested trace.

- **`amberfork-record`** (8th crate): the capture proxy + the cassette contract. A
  loopback-only HTTP proxy (`docs/cassette-format.md`) relays an agent's provider traffic and
  records every request/response round trip full-content, with a fail-closed header allowlist
  so a shareable cassette never carries a credential (issue #32).
- **Cassette → `Run` normalization**: `normalize(&Cassette)` maps each captured exchange to
  one canonical LLM step (full request/response bodies as inputs/outputs), so a recorded run
  reads through exactly the aligner the passive path uses — no per-consumer fork of the trace
  contract (issue #33).
- **`amberfork diff` auto-detects a cassette**: a file carrying `cassette_version` is
  normalized and aligned in place — one command, no convert step, since a cassette is a
  first-party self-versioning artifact. The sniff lives at the CLI, so `amberfork-ingest`
  stays canonical-only and the tokio quarantine holds (issue #33).
- **`amberfork record -- <cmd>`**: the zero-code capture verb. Binds the proxy, runs the agent
  as an async child with a base-URL env var (`--base-url-env`) pointed at it, and writes the
  cassette — even when the agent fails, because a failed run is the one worth recording. The
  agent's exit code propagates (transparent wrapper) (issue #34).

Scope: this completes the record path's *capture* side. Replay — serving recorded responses
back for re-execution — rides into v0.7 with counterfactual attribution, its only consumer.

## [0.5.0] — 2026-07-15

v0.5 — the fork in the browser (milestone issues #21–#28); also carries the untagged
post-v0.4.0 work (perf, CI, and two CLI fixes, issues #16/#18–#20).

- **`amberfork serve <bad> --against <good>`**: the fork in a local web view. New
  `amberfork-server` crate (7th) — a loopback-only Axum API (`/api/document`, ETag/304),
  Host-header allowlisted, the release bundle embedded via `rust-embed` so a released binary
  is a complete single-file app. `--demo` gives the zero-setup browser entry (the same
  embedded pair `demo` renders in the terminal); `--port`/`--open` round it out (issue #25).
- **`amberfork-ui`**: the Leptos/WebAssembly frontend — a shared-spine alignment canvas
  (SVG spine + DOM rows, text stays selectable), an attribution pane, a content-diff pane
  (the one surface that spends red/green), keyboard-navigable selection with roving
  tabindex, a disconnect/re-poll banner when the local server stops, a copy affordance that
  puts the selected pair's evidence + a re-runnable repro command on the clipboard, and the
  one expressive beat — amber igniting at the fork and flowing down the divergent path,
  gated behind `prefers-reduced-motion` (issue #26, #27).
- **`amberfork-layout`** (6th crate): the serializable view-model + payload envelope
  extracted from the terminal painter into its own seam, so the CLI and the web UI render
  the exact same `Document` (issue #21, #24). `field_diffs` moved from the fork row onto
  every synced row, so the content-diff pane can show evidence for *any* selected pair, not
  just the fork — `DOCUMENT_VERSION` 0.1 → 0.2 for the wire-shape change (issue #27).
  `align()` gained a typed size-guard error and the CLI's `--max-steps` escape hatch
  (issue #23).
- **CI**: the release workflow now builds the web UI (`trunk build --release`) and stages
  it into the server crate's embed folder *before* `cargo build`, so a released `serve`
  actually ships a UI; the release smoke test boots `serve --demo` over the real embedded
  bundle and checks both routes answer (issue #28).
- **Docs**: `docs/run-on-your-own-agent.md` gained a browser-reading section and documents
  the payload envelope where each truncation mechanism actually applies (terminal
  width-abbreviation vs. the browser's 4 KiB wire cap); the README leads with the web-fork
  hero GIF, `serve --demo` joins the 30-second try, and the terminal hero follows as a peer
  surface (issue #28).
- **Benchmark**: the v0.5.0 reveal (protocol rule 2) reproduces the sealed test-split
  numbers identically on every arm and metric — despite real scoring-path changes since
  v0.4.0 (the #16 tokenization cache). Committed alongside the originals as
  `bench/results/chimera_noise_seed*_test_v0.5.0.json` +
  `chimera_noise_multiseed_test_v0.5.0.json` (rule 3; notebook 037).
- **Fixes riding along untagged**: `amberfork-align`'s `LexicalCost` tokenization cached at
  a prepare-once seam, O(n·m) → ~33% faster per cell without changing any cost or alignment
  (issue #16); CI bumped off deprecated Node 20 actions (issue #18); the converged summary
  no longer claims "identical" when divergences were absorbed (issue #19); a non-canonical
  or raw-JSONL input now points at `docs/trace-format.md` instead of a dead-end parse error
  (issue #20).

## [0.4.0] — 2026-07-11

v0.4 — distribution + run-it-yourself (milestone issue #15); also carries the untagged
v0.3 work (attribution + field diff, issues #12–#14) and the Rust sanitizer port (#17).

- Distribution: the CLI crate is now `amberfork` — `cargo install amberfork`; libraries stay
  `amberfork-*` namespaced — with crates.io metadata across the workspace (bench stays
  unpublished) verified by a workspace publish dry-run. A tag-triggered release workflow
  builds, smoke-tests, and attaches macOS-arm64 + Linux-x86_64 binaries with sha256
  checksums; its dispatch dry run was verified green before this tag existed (issue #15).
- `docs/run-on-your-own-agent.md`: the end-user guide — install, convert your own agent's
  logs (real, tested Claude Code transcript example), diff, read the fork, `--json`/exit
  codes for CI, troubleshooting the failure modes a real sanity pass actually hit (issue #15).
- Sanity pass on messy real-world traces (notebook 020): the self-align invariant, fork
  localization, `--json` contract, and exit codes all hold; rough edges filed as #19
  (converged line overclaims) and #20 (dead-end parse error); the O(n·m) cost curve measured
  onto #16.
- Benchmark: the rule-2 test-split reveal at this tag **reproduces the sealed v0.2.0 numbers
  identically** on every arm and metric (per-seed documents differ only by the results-schema
  version, 0.5→0.6) — the attribution/field-diff/canonicalization changes since v0.2.0 are
  scoring-invariant. Committed alongside the sealed originals as
  `bench/results/*_test_v0.4.0.json` (rule 3; notebook 021).
- CI: actions bumped off deprecated Node 20 (issue #18).

- `amberfork-align`: static attribution — `diff()` now populates `DiffResult.attribution`
  on forked diffs (`mode: static`): origin = the fork's observed step (the canonical
  `fork_step_observed` rule), propagation = the observed steps downstream, confidence = the
  fork's own; `counterfactual` stays `null` until re-execution exists. Additive `--json`
  change — the field was already in the schema, previously always omitted; no schema bump.
  The human render closes forked diffs with a one-line attribution footer (issue #12).
- `amberfork-align`: field-diff producer — `diff()` now populates `DiffResult.field_diffs`
  for every sync pair (payload slots compared on the wire representation, object keys
  recursed with dotted paths in sorted order), so the content-diff (red/green) pane draws
  from real data; the converged self-diff invariant guards the pass (issue #13).
- `amberfork-bench aggregate`: pool results documents into one exact aggregate — hits and n
  summed per metric per arm (calibration bins too), Wilson intervals recomputed at the
  pooled n; refuses mismatched protocol/split/params, duplicates, and nested aggregates.
  The README's cross-seed headline (test, n=35 across seeds 42/43/44) is now a committed,
  `report`-reproducible document (`bench/results/chimera_noise_multiseed_test.json`) that
  names its three source documents by sha256 and rebuilds from them byte-for-byte in CI.
  Results schema 0.6 (adds `sources` + per-record `source` provenance); 0.5 documents still
  load — the sealed v0.2.0 artifacts keep their exact reveal bytes (issue #14).

## [0.2.0] — 2026-07-10

v0.2 — offline benchmark (milestone issues #6, #7, #11).

- `amberfork-bench`: the pre-registered chimera protocol (BENCHMARK.md) as a Rust harness —
  four arms (random, pos-lexical, nw-structural/resync, nw-lexical/resync) scored on identical
  fixtures with coverage accounting, Wilson 95% intervals, and a calibration curve; parameter
  freeze via `bench/params.toml` (every table names the config sha256); committed results
  documents + offline `report` mode that re-renders them with zero fetch.
- Mode A′ cross-system pairs, end-to-end in one binary: `fetch` (pinned upstream commits,
  licensing notices, provenance record) → TapeAgents reference adapter → `build-pairs` →
  scored disclosure. The honest result on the 4 constructible real pairs is a **null**
  (engine 0.50 ±3 vs random 0.75 — short runs, murky cross-system gold; both pre-registered
  threats) — shipped as a disclosed limit, not a headline (notebook 016).
- CI-visible fork-localization gate: GAIA-sanitized dev fixtures for seeds 42/43/44 committed
  under `bench/fixtures/chimera_noise_seed*_dev/` with a full regeneration recipe;
  `chimera_parity` pins each seed's own baseline on every `amberfork-align` change (issue #11,
  notebooks 013–014).
- Honesty correction: the seed-42 0.75 exact headline was a favorable draw — README and the
  gate now lead with the cross-seed ±3 window and the 0.56 aggregate exact (notebook 014).
- **First sealed test-split reveal (protocol rule 2, at this tag):** dev-tuned params
  generalized — engine ±3 **0.91 [0.78, 0.97]** vs best baseline 0.49 [0.33, 0.64]
  (non-overlapping), exact 0.49 [0.33, 0.64] vs 0.00, on n=35 unseen pairs across three seeds;
  calibration monotone on unseen data; results committed with `report` snapshot tests
  (notebook 017).
- `amberfork-ingest`: warn when a trace declares a non-native schema version.
- Workspace version now single-sourced from `[workspace.package]` (0.2.0).

## [0.1.0] — 2026-07-09

v0.1 — walking skeleton (milestone issues #1–#5): `amberfork diff <bad> --against <good>`
working end-to-end, thin everywhere.

- `amberfork-model`: canonical `Run`/`Step` trajectory model + `DiffResult` (the frozen,
  `schema_version`-carrying contract every consumer reads).
- `amberfork-ingest`: plain-JSON trace loader (`docs/trace-format.md`) + Who&When converter.
- `amberfork-align`: lexical/tf-idf cost model, affine-gap Needleman–Wunsch, resync fork rule
  (first non-sync block the alignment never recovers from); self-align invariant as the
  canonical guard.
- `amberfork-cli`: `amberfork diff` with amber terminal render + `--json`; `amberfork demo`
  runs a bundled pair embedded in the binary; README hero GIF from a committed vhs tape.
- Pre-build spike (2026-07-07, throwaway Python, findings only): alignment + sustained-
  divergence localizes at 70% exact / 90% ±1 under benign noise vs 0% for positional
  first-mismatch on real Who&When content — the core bet validated before the first crate
  (notebook 001; consequences: resync fork rule, embeddings demoted to a hypothesis,
  benchmark reframed to controlled-injection + Mode A′).
- Pre-registered benchmark protocol + dataset licensing resolution (BENCHMARK.md); canonical
  plain-JSON trace format; project scaffolding (README, MIT license, CI running the exact
  local verify gate, engineering notebook).
