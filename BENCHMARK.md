# BENCHMARK.md — agentdiff proof-of-skill benchmark plan

> Created 2026-07-02. The standout artifact for the Phase-1 build (see the "Strategic Reframe — v2"
> section of `design-run-diff-debugger.md`). A public benchmark number is what separates agentdiff
> from the ~6 shallow "agent diff" tools already in the lane. Build the engine to the point it runs
> this, then publish the table.

## Purpose
Prove — with a number, not a claim — that agentdiff's **explainable, local, non-LLM** alignment
localizes the failure/divergence step **at least as well as the LLM-judge attribution the SOTA
reports**, and better than the shallow positional diffs the competitors ship.

## Target numbers to beat (published SOTA — verify at build time)
| Benchmark | What it is | Best published result | Access |
|---|---|---|---|
| **Who&When** (ICML'25 Spotlight) | 127 multi-agent failure logs, human-annotated with responsible **agent** + decisive **error step** | **53.5% agent-level, 14.2% step-level**; some methods below chance; o1/R1 not practically usable | `ag2ai/Agents_Failure_Attribution` (GitHub / HuggingFace) |
| **TRAIL** (arXiv 2505.08638, Patronus) | **148 traces / 1,987 OTel spans** (117 GAIA + 31 SWE-Bench), OpenInference span trees (directly ingestible); span-located error annotations | best model (Gemini-2.5-Pro) ~**11% joint accuracy** | GitHub `patronus-ai/trail-benchmark` (MIT, ungated — the HF copy is gated no-reshare; always source GitHub) |
| *(optional)* **TELBench** | earliest-erroneous-span localization | first-error accuracy **7.5–34.5%** | arXiv |
| *(optional)* **TraceElephant** | full-trace vs output-only attribution | full traces improve attribution **up to +76%** over output-only | arXiv |

## The evaluation crux (state this honestly in the writeup)
These are **single-trajectory** failure-attribution benchmarks; agentdiff is a **cross-run aligner**.
That mismatch is the intellectually interesting part — turn it into the scientific claim under test:

> *Aligning a failing run against a known-good reference run localizes the decisive error step at
> least as well as single-trace LLM-judge attribution — and does so explainably, locally, offline.*

Two evaluation modes:

- **Mode A — run-vs-reference (primary).** For each failing trace, obtain/generate a **passing**
  reference trace of the same task. Align them; predict the **first meaningful divergence step**;
  score against the annotated **decisive error step**. Hypothesis: first-divergence tightly bounds
  the decisive error. Reference sourcing: (i) use benchmark-provided success/failure pairs where they
  exist; (ii) otherwise re-run the task under **record-mode** to capture success traces (non-
  deterministic — capture N, take consensus, report variance).
- **Mode B — run-vs-consensus (fallback; exercises the cluster path).** When no clean single
  reference exists, align the failing trace against a **consensus of N successful runs** — tests
  `adiff-align`'s gated cluster→consensus path.

Both modes are a **novel evaluation protocol** and a **threat to validity**. Document them; do not
overclaim. If Mode A can't be constructed for a benchmark, say so and report only what it supports.

> **Reality check (2026-07-07, spike — notebook 001):** Who&When as published contains ONLY
> failure logs (184; `is_correct` false in all; the "decisive error" was annotator-judged, never
> executed) — **Mode A pairs cannot be constructed from the published data.** Reference sourcing
> (ii) (re-run under record-mode) is therefore the *only* Who&When path and carries real cost:
> reconstructing CaptainAgent/Magnetic-One-style stacks + API spend + non-determinism. Primary
> reproducible protocol until then: **controlled-injection localization on real logs** (chimera
> pairs: real prefix + real divergent tail at a known gold step + benign prefix noise), honestly
> labeled as injected. Spike result on that protocol: alignment + sustained-divergence rule 70%
> exact / 90% ±1 vs 0% exact for shallow positional under benign noise; positional 85% exact in
> the noise-free control (the aligner's value IS the non-determinism tolerance). Also: the
> "first non-sync move" fork rule is empirically dead (0%) — use the resync/sustained rule.
>
> **Mode A′ (same day):** natural-failure run-vs-reference IS constructible as **cross-system**
> pairs: Who&When's algorithm-generated logs carry genuine GAIA task UUIDs, and public
> known-good runs on the same tasks exist (HAL traces, TapeAgents tapes, leaderboard
> submissions — see Data & licensing). The reference comes from a *different* agent system —
> disclose that; it stress-tests alignment and matches the real incident-postmortem case.

## Metrics
- **Step-level exact-match** — predicted fork step == gold decisive step (headline; vs 14.2% / ~11%).
- **Within-window (±1 step)** and **top-k** — honest, since "first divergence" and "decisive error"
  can legitimately differ by a step.
- **Agent-level** accuracy (vs 53.5%).
- **Calibration** — does the alignment confidence score correlate with correctness?
- Report all of the above for agentdiff **and every baseline**, on the same fixtures.

## Baselines (must run alongside — the number is meaningless without them)
1. **Random step** (floor).
2. **Shallow positional diff** — align by index, first mismatch. This is the agent-replay/agx
   approach and the control that demonstrates *why* semantic move-typed alignment is better.
3. **All-in-one LLM judge** and **step-by-step LLM judge** (the Who&When methods) — via the optional
   `adiff-judge` provider trait, network-gated and cassette-cached for reproducibility.

## Harness design (Rust; fits the workspace)
- New crate **`adiff-bench`** (or an `xtask bench` subcommand): fixture loader → convert to canonical
  `Run`/`Step` → run aligner + baselines → score → emit markdown table + JSON.
- **Converters:** TRAIL is OpenInference/OTel → reuse `adiff-ingest` directly. Who&When ships its own
  JSON logs → write a dedicated converter; segment into runs via `adiff-store`.
- **Determinism:** the core method is offline (local ONNX embeddings, no network). The LLM-judge
  baseline is opt-in and **cassette-cached**, so the published table is reproducible without live API.
- **Output:** `cargo run -p adiff-bench` prints the results table; snapshot with `insta`; paste into
  README.

## Data & licensing (RESOLVED 2026-07-07 — notebook 001 addendum)
- **Who&When:** MIT via GitHub `ag2ai/Agents_Failure_Attribution` (dataset ships in-repo). The HF
  mirror declares NO license — never source or cite the HF copy for redistribution.
- **TRAIL:** MIT via GitHub `patronus-ai/trail-benchmark`. The HF copy is gated with a
  contractual no-reshare clause — source GitHub only.
- Benchmarking + publishing derived numbers: permitted for both (MIT). Vendoring small derived
  fixtures: permitted WITH copyright + license notice, sourced from GitHub.
- **Upstream caveat:** both embed GAIA validation questions/answers; GAIA is gated upstream
  ("no crawlable resharing"). Conservative rule for any vendored fixture: strip or hash the
  GAIA ground-truth answers.
- **Reference-run sources for Mode A′ (cross-system pairs):** HAL traces
  (`agent-evals/hal_traces`, 37 full GAIA runs across many models; license unspecified — use for
  benchmarking, do not redistribute), TapeAgents (Apache-2.0, 8 full GAIA tapes, 4 successful —
  redistributable), `gaia-benchmark/submissions_public` (gated; per-task correctness incl.
  Magnetic-One's own passing rows; coarse free-form traces).
- Do **not** vendor large datasets into the repo — `bench/fetch` script; cache locally.

## Threats to validity (put these in the writeup — the honesty is part of the impressiveness)
1. Single-trajectory benchmark vs a two-run tool — the Mode A/B protocol is novel and arguable.
2. "First divergence" ≠ "decisive error step" — align the metric to the claim; always report windowed.
3. Reference-trace generation is non-deterministic — capture N, consensus, report variance.
4. Small N (Who&When = 127) — report confidence intervals; don't over-read a few points.
5. OTel content is opt-in — when absent, degrade to metadata-only alignment and report that split
   separately (do not silently mix content-present and content-absent cases).

## Definition of done
- `cargo run -p adiff-bench` reproduces the results table, **offline** for the core method.
- README shows: agentdiff (Mode A) vs shallow-diff vs LLM-judge vs random — step-level + windowed +
  agent-level — on Who&When and TRAIL.
- An honest **"where it fails"** paragraph.
- Paired with the `adiff demo` <90s GIF on one vivid divergent trace (the amber-fork moment).

## Pre-registered protocol (added 2026-07-07; binding before any number is published)

The table is only as credible as the discipline behind it. These rules exist so the harshest
reviewer — hunting for tuning-on-test, cherry-picking, or flaky "determinism" — finds nothing.

1. **Dev/test split.** Fixtures split ~30/70 dev/test by a stable hash of the task/question id;
   the split manifest is committed. ALL tuning — move costs, gap penalties, divergence threshold
   τ, embedding-model choice, normalization — happens on dev only.
2. **Parameter freeze.** Cost-model parameters ship as a committed config (`bench/params.toml`)
   with a changelog. Every published table names the config hash that produced it. The test split
   runs once per release tag with frozen params.
3. **No silent replacement.** If a test run motivates a change, the change is tuned on dev and the
   new test result is reported ALONGSIDE the old one in `docs/notebook.md` — never swapped in.
4. **Exclusions are data.** Every excluded case (no reference constructible, content absent,
   malformed) is counted and tabulated with a reason. The table reports coverage =
   evaluated/total per dataset. A rate over a silently-shrunk denominator is a lie.
5. **Determinism engineering.** Pinned embedding-model file (sha256 committed), fixed thread
   count, batch-order independence, quantized/integer score comparisons where possible. CI
   snapshots: EXACT match on predicted fork indices, absolute tolerance ε on floating scores.
   A flake is a failure, not a retry.
6. **Small-N honesty.** Wilson (or bootstrap) 95% CIs on every headline rate. No claimed
   difference between arms whose intervals overlap. Who&When is ~127 cases; do not over-read.
7. **Calibration.** Report a reliability curve (alignment confidence binned vs empirical
   correctness). The CI `--gate` feature depends on this being real, not decorative.
8. **Factorial baselines on identical fixtures.** Same split, same exclusions, same metrics for
   every arm — including NW alignment with exact-match costs (structure-only, no embeddings), so
   the table isolates what alignment adds over position AND what semantics adds over alignment.

Status note (2026-07-07): a throwaway feasibility spike (`spike/`) is testing Mode-A pair
constructibility and semantic-vs-positional on real fixtures before the Rust build. Findings go
to `docs/notebook.md` entry 001. This protocol governs the real bench; the spike is directional.
