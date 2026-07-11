# Changelog

## [Unreleased]

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
