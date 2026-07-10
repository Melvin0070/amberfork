# Changelog

## [Unreleased]

- `amberfork-align`: static attribution — `diff()` now populates `DiffResult.attribution`
  on forked diffs (`mode: static`): origin = the fork's observed step (the canonical
  `fork_step_observed` rule), propagation = the observed steps downstream, confidence = the
  fork's own; `counterfactual` stays `null` until re-execution exists. Additive `--json`
  change — the field was already in the schema, previously always omitted; no schema bump.
  The human render closes forked diffs with a one-line attribution footer (issue #12).

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
