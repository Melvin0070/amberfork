# amberfork: Architecture and Design (authoritative, reconciled 2026-07-05)

> Single source of truth for building amberfork. This document accreted across five review
> passes and preserves the full decision trail below. **Read the Current State block first;
> where any later section conflicts with it, the Current State block wins.** Full ambition is
> kept: nothing is cut for scope, and the "phases" are build ORDER, not a smaller product.

## Current State — Source of Truth (reconciled 2026-07-05)

Passes folded in: office-hours (2026-06-27), eng-review (2026-06-30), strategic reframe v2
(2026-07-02), completeness pass (2026-07-03), and DX + design re-reviews (2026-07-05).
Everything below this block is the decision trail. **Where an earlier section conflicts with
this block, this block is authoritative.**

**What it is.** amberfork: a local, all-Rust tool that takes two AI-agent run trajectories,
aligns them with an explainable move-typed affine-gap aligner, ignites the fork point in amber,
and attributes the regression. Hybrid execution: a PASSIVE path (align any two existing OTel
traces) and a RECORD path (`amberfork record` wraps an agent under a capture proxy for full-content
cassettes and counterfactual re-execution).

**Goal.** Earn strong-engineer respect. Organic usage is a welcome byproduct, not the bar.
Primary persona is the skeptical senior engineer judging the repo/benchmark/writeup in 2 to 5
minutes, not the person installing it (do not degrade the install path either).

**Success = two equal, independent belief pillars** (neither alone can sink or carry the project):
1. **Credibility.** `cargo run -p amberfork-bench` reproduces the scoring table offline,
   deterministically, no API key. The headline claim is the defensible ASYMMETRY ("localizes the
   decisive error step as well as an LLM judge, but locally, explainably, deterministically, and
   reproducibly without a network or key"), stated WITH the honest privileged-reference caveat (a
   two-run aligner is handed a known-good reference; the single-trajectory baselines were not). It
   is NOT "beats 14.2% SOTA."
2. **Craft.** The clever, legible move-typed alignment engine, the explainable amber-fork UI, and
   the honest local eval. The `<90s` amber-fork GIF is this pillar's hero. Stands alone even if
   semantic alignment only ties shallow positional.

**Rendering: Leptos DOM + animated SVG only. wgpu/WebGPU is dropped** (2026-06-30, Issue 7:
debugger content is prompt/arg/error text that must be selectable, copyable, and accessible). Any
"wgpu" in the sections below is stale and superseded by this line.

**Workspace: phased — 5 crates shipped at v0.4.0, grown one slice at a time** (amended
2026-07-11; the original "14 crates + `ui/`" roster is a *target-state capability map*, not the
build plan — see Amendment 2026-07-11 for the shipped roster and where each planned crate went).
Full feature set kept: move-typed alignment, field-level diff, counterfactual-causal attribution,
cluster-to-consensus (gated on a corpus), replay/record cassettes, and a factorized,
local-capable judge (semantic naming only, never localization).

**Build order (full scope, strictly phased):**
- **Phase 1 (the number + the magic):** model+schema+store (T1/T12/T24) → ingest (T2) → embed (T3)
  → align (T4, moat) → field-diff → CLI (T15/T27) → bench (T22/T26) → replay/record tail
  (T5/T13/T17) → demo + `<90s` GIF (T14). Plus seams T23 (Layout) and the T25 ONNX spike.
- **Phase 2 (depth, after the number):** counterfactual attribution (T6) → layout/server + UI
  (T8/T9, DOM/SVG) → CI regression gate (T18/T29) → factorized judge (T7). Design states T34-T40.
- **Phase 3 (breadth + distribution):** cluster/consensus (gated) → more framework normalizers →
  dist matrix (T10) → community (T20) → FTO (T11).

**Execution gates on the headline claim:** T4 (semantic alignment beats shallow positional on real
fixtures) — **VALIDATED 2026-07-08**: alignment + resync rule ~0.50 exact vs 0.00 positional
(spike 002; see Amendment 2026-07-08.A). T25 (ort/ONNX cross-platform link) is **no longer a
gate** — per Amendment 2026-07-08.B the v1 cost model is lexical/tf-idf, so "offline single
binary" holds without ONNX; embeddings (and T25) are optional, gated on beating lexical on dev
fixtures.

**Visual system:** DESIGN.md is the locked source of truth for all UI. North star: "sameness
recedes, divergence glows." Amber `#FF7A1A` is the only divergence accent; red/green only inside
the content-diff pane; the fork carries a redundant non-color cue; a designed converged/empty state
exists.

**Relationship to the craft-first `~/.gstack` design doc (Approach C, 2026-07-02):** that doc
proposed *deferring* counterfactual, cluster/consensus, and the judge to lead with craft. Per the
2026-07-05 decision to keep full ambition, those capabilities are IN scope here (phased, not cut).
This architecture doc is authoritative for scope.

## Amendment 2026-07-08 — empirical (spikes 001–002; supersedes conflicting lines below)

Founder-approved (issue #8) after two feasibility spikes on real Who&When trajectories +
external prior-art verification (`docs/notebook.md` 001–002). These three lines are now
authoritative; where the decision trail below conflicts, THIS wins.

- **A. Fork criterion (the product's core output) — CHANGED.** The doc's "**fork = 1st non-sync
  move**" (data-flow diagram line ~435; echoed in the design-review pane at ~1397) is
  **empirically dead: 0.00 exact localization across all 9 sweep configs**, because benign
  retries/rewordings are the first non-sync move. Replace with: **fork = the first non-sync
  BLOCK the alignment does not recover from within k synchronous moves (resync-k, default k=2,
  dev-calibrated).** This scored ~0.50 exact / ~0.75 ±1 vs 0.00 for both first-divergence and
  positional. Externally supported: the counterfactual-recoverability standard of Who&When /
  AgenTracer / CausalFlow / CHIEF, process-mining deviation-*pattern* aggregation (BPM'24), and
  the X-drop heuristic (BLAST). The lone first-divergence definition (WebStep "bifurcation")
  works only on clean discrete states and explicitly disclaims recovery. Update every doc that
  cites "fork = 1st non-sync move," and correct spike 001's headline (70% was seed-42; the
  across-seed mean is ~50% exact / ~75% ±1).

- **B. Cost model + T25/ONNX gate — DEMOTED, not deleted.** On a fair test, BGE-small-en-v1.5 via
  fastembed (the exact specced model+runtime) **tied** lexical/tf-idf for same-system alignment
  (0.53 vs 0.50/0.53 exact) and did not justify its ONNX runtime + 30–45MB model. **v1 default =
  lexical/tf-idf** (dependency-free, deterministic, seed-stable). Embeddings stay behind the
  cost-model trait as a first-class experiment with a hard bar: **must beat lexical on dev
  fixtures to become default.** Consequently **T25 (ort/ONNX cross-platform link) drops from an
  execution GATE to an OPTIONAL de-risking task**; the "offline single binary" headline no longer
  depends on it. (Embeddings showed a real edge on *cross-system* pairs — kept as the reason to
  retain the trait, not to ship ONNX in v1.)

- **C. Benchmark Mode A — REALITY-CORRECTED.** Who&When ships zero passing runs, so same-task
  Mode-A pairs are not constructible from it (notebook 001). Primary reproducible protocol =
  **controlled-injection localization** on real logs. **Mode A′ (cross-system pairs)** is proven
  constructible **at scale**: 126/128 Who&When GAIA failure logs pair with a public passing HAL
  run (~450MB for 90% coverage; notebook 002). Scarcity is solved; the open problem is
  gold-quality (cross-system references diverge from step 0, algo logs are short). So Mode A′ is
  a **v0.2 co-primary target contingent on gold/metric design**, not a v1 headline — do not
  overclaim step-exact on it. (See BENCHMARK.md for the governing pre-registered protocol.)

## Amendment 2026-07-11 — workspace roster reconciled to shipped reality (v0.4.0)

The "14 crates + `ui/`" roster (Current State block above; detailed in "## Module / crate
layout" below) described the full capability map from the 2026-07-03 completeness pass. Under
the operating rule **"never build a crate ahead of the need it serves"** (CLAUDE.md), the
shipped workspace consolidated as slices landed. As of v0.4.0 it is **5 crates**:

- **amberfork-model** — canonical `Run`/`Step` + the versioned `DiffResult` contract (T1/T12).
- **amberfork-ingest** — forgiving plain-JSON loader + Who&When and TapeAgents reference
  adapters (T2; the normalizer layer grows here).
- **amberfork-align** — the moat crate: `CostModel` trait (lexical default per Amendment
  2026-07-08.B; the embed slot lives behind the trait), affine-gap NW, resync-k fork rule,
  static attribution (issue #12), field-diff (issue #13). Absorbs the planned
  `amberfork-core`, `amberfork-embed`, and the static half of `amberfork-attrib`.
- **amberfork** — the CLI, shipped under the product name so `cargo install amberfork` is the
  install line (planned as `amberfork-cli`).
- **amberfork-bench** — chimera protocol + Mode A′ pipeline + GAIA sanitizer; `publish = false`.

Nothing here cuts scope — the 2026-07-05 full-ambition decision stands; the remaining planned
crates arrive **with the phase that needs them**: `amberfork-layout` + `amberfork-server` +
`ui/` with the Phase-2 web UI (v0.5 target); `amberfork-record`/`amberfork-replay` with the
capture path; `amberfork-store` when persistence is a real need (record path);
counterfactual attribution earns its crate when re-execution exists; `amberfork-judge` is
issue #10; cluster/consensus stays gated on a corpus (Phase 3). Where the roster lines above
or "## Module / crate layout" below conflict with this list, THIS wins.

## Amendment 2026-07-12 — the layout seam shipped as the semantic view-model (issue #21)

`amberfork-layout` (the 6th crate) landed with v0.5's first slice — and its seam is NOT the
pixel-geometry `Layout` schema drawn in the 2026-07-03 completeness pass ("## Layout is a
separate schema, not part of DiffResult", now marked superseded). The shared seam between the
terminal painter and the web UI is the SEMANTIC view-model: extracting either painter-specific
form — column arithmetic or pixel geometry — gives a seam the other consumer has to fake (eng
review D3+D12, outside-voice finding 1 verified against render.rs).

```text
             DiffResult (+ the two Runs)     ── the frozen --json contract, presentation-free
                        │
                        ▼
                amberfork-layout
                        │
                        ▼
                    ViewModel        rows (spine / fork / downstream) carrying both sides of
                        │            each aligned pair · step summaries · designed wording
                        │            (confidence, verdict, absence) · DR5 attribution parts ·
                        │            field-diff evidence
            ┌───────────┴───────────┐
            ▼                       ▼
      CLI painter (render.rs)   web painter (ui/, Leptos — v0.5)
      columns · glyphs · ANSI   SVG/DOM geometry
```

What survives from the G2 decision: `DiffResult` stays presentation-free. What changed:
geometry (x/y, `fork_y`, edge paths) is each painter's own business, never shared state; the
serializable view-model document + payload envelope the server sends the web painter arrives
with issue #24 on top of this same `ViewModel`. The extraction was output-locked: it landed
with zero churn across the committed CLI snapshot net (issue #21 slices 0–1, commits
909aeea/95e8bba).

---

# [HISTORICAL] Design: Run-Diff Debugger for AI Agents (local, framework-agnostic) — Validation-First

> Superseded by the Current State block above. Kept as the original 2026-06-27 framing.

Generated by /office-hours on 2026-06-27
Branch: unknown (not a git repo yet)
Repo: none yet (greenfield)
Status: APPROVED
Mode: Builder

## Problem Statement

When an AI agent regresses — worked yesterday and breaks today, or works on one run
and fails on the next — there is no good *local, framework-agnostic* way to see **where
two runs diverged and why**. Today you either squint at raw logs and traces, or you
reach for a cloud or framework-locked observability platform. The specific blind spot:
aligning two non-deterministic, free-form agent trajectories, finding the fork point,
and attributing the regression to what actually changed.

The product, if it earns the right to exist: a single local binary, no account, no
cloud, that consumes the traces agents already emit (OpenTelemetry GenAI spans) and
renders a **diff of two runs** with the fork point surfaced.

Honest framing, carried from the session: this is a builder / learning / resume project,
not a company. Primary goal (chosen at D1, then revised and reaffirmed): a technically
deep, well-built, standout project that strong engineers respect. Real usage is a
welcome byproduct, not the bar. Conviction is currently **borrowed** — no firsthand
2am incident — so the build is explicitly gated behind a validation experiment.

## What Makes This Cool

- **The hard systems problem is real, not cosmetic.** Aligning two divergent
  non-deterministic trajectories and attributing where and why they forked is genuine
  sequence-alignment-over-fuzzy-steps work. It is not a UI over a JSON log, which is the
  trap that makes most "agent debugger" projects forgettable. Published SOTA localizes the
  failing step only ~14-30% of the time and works on a single trajectory, not two, so the
  cross-run version is open research, not plumbing.
- **Framework-agnostic by construction.** Consume OpenTelemetry GenAI semantic-convention
  spans so it works across the frameworks that emit them (today: LangChain, CrewAI,
  AutoGen; LangGraph / LlamaIndex / OpenAI Agents SDK to be verified in step 0) instead of
  locking to one. The incumbents that do diffing today are either cloud (LangSmith) or
  framework-locked. This is an architecture decision the locked-in tools structurally
  cannot cheaply copy.
- **Local, single binary, no account.** A real DX and trust story, and a clean excuse to
  go deep on a systems language.
- **A sharp one-line story for a writeup:** "Observability shows you what happened. This
  shows you what *changed*."

## Constraints

- Solo final-year student. Weekly hours not yet quantified (open question).
- Borrowed conviction: no firsthand incident yet. Must be earned via the validation
  experiment before any product code.
- Crowded adjacent market: Langfuse, Phoenix, LangSmith, MLflow, EvalView, Agent-Lens,
  Laminar, Braintrust. The generic trace view is commoditized; all differentiation must
  live in diff depth plus the framework-agnostic local design.
- Traces are non-deterministic. Alignment cannot assume reproducibility, and "replay"
  means input/response caching (VCR-style), not bit-exact re-derivation.
- IP: a structural run-diff using Needleman-Wunsch/edit-distance overlaps Microsoft US
  Patent 11,093,368; design at the semantic-step level and get a freedom-to-operate look.
- Absorption risk: data/orchestration platforms (ClickHouse+Langfuse, a16z framing) are
  pulling observability inward; the defensible position is local, dev-first, and OSS.
- OTel spans omit prompt/tool-arg content by default, so the tool must drive content
  capture or add its own layer.

## Premises

1. Primary goal is "technically impressive standout; usage is a byproduct," not raw
   adoption. (Reaffirmed after the D1 revision.)
2. The generic local trajectory viewer is commoditized (Langfuse is MIT and self-hosts
   air-gapped; Phoenix is local-first with zero external deps) and not worth building.
3. The defensible, impressive core is the run-diff / regression-fork problem —
   specifically the trajectory-alignment-and-attribution part — not a prettier display.
4. You have not earned the right to build yet. The immediate deliverable is a validation
   experiment, not code.
5. Framework-agnostic ingestion via OpenTelemetry GenAI spans is the most promising
   architectural wedge — but it is *provisional*, not pre-validated. Step 0 verifies which
   frameworks actually emit usable GenAI spans today and pins a span schema version before
   the wedge is locked.

## Landscape (verified across two deep-research passes, June 2026)

Two adversarially-verified research passes (each surviving claim checked by 3 independent
skeptics; 4 of 25 claims killed in pass 1) converge on one picture:

- **Single-run trace viewing is commoditized.** ~89% of surveyed orgs have some agent
  observability; ~62% can inspect individual steps and tool calls. Langfuse (MIT,
  self-hostable) was acquired by ClickHouse (Jan 2026); Phoenix is local-first. Seeing
  *what one run did* is solved and free. Do not build it. [LangChain State of Agent
  Engineering; InfoWorld]
- **Nobody owns run-diff.** No tool, OSS or cloud, convincingly does framework-agnostic,
  local, true run-to-run trajectory diffing with fork-attribution. The closest direct
  competitor, `clay-good/agent-replay`, advertises exactly this (run-diff, time-travel,
  fork, LLM-judge) but its capability claims were *refuted 0-3* on verification, and it is
  a 5-star, 0-fork micro-repo. The niche is named and unwon.
- **The published SOTA confirms the gap.** Microsoft's AgentRx (Feb 2026) and the ICML'25
  failure-attribution line operate on a *single* trajectory or against a fixed reference,
  not on aligning two divergent runs. Step-level failure localization is largely unsolved:
  ~14% in the founding benchmark, ~27-30% at 2026 SOTA. The cross-run problem you picked
  is genuinely open and genuinely hard.
- **OTel GenAI spans are the framework-agnostic substrate, but immature.** A dedicated
  `open-telemetry/semantic-conventions-genai` repo defines agent/tool/MCP spans, but every
  GenAI attribute is "Development" maturity (subject to breaking change), and prompt
  content plus tool args are NOT captured by default (opt-in only). Your context-state
  angle therefore can't just read OTel spans; you'd drive the content-capture opt-in or
  add your own capture layer.
- **The market frames this as a feature, not a category.** ClickHouse bought Langfuse;
  a16z frames agent reliability as enterprise "systems of coordination" / orchestration.
  Absorption risk is real. The upside: that top-down enterprise framing leaves a
  bottom-up, local, dev-first wedge nobody is racing for.
- **Honest evidence gaps.** The competitor feature-matrix is under-verified (only Langfuse,
  LangSmith, and agent-replay survived; Phoenix, MLflow, Braintrust, Laminar, AgentOps
  were not independently confirmed) — build a fresh hands-on matrix before committing. And
  the *demand* is still borrowed: no verbatim developer regression-pain quote survived
  verification except one HN line, "LLMs are probabilistic... you can have a regression
  WITHOUT even changing the code." The research found the gap and the techniques. It did
  not find the users. That remains your experiment's job.

## Prior Art to Steal & Differentiation Bets (depth pass)

The hard part is solved in neighboring fields and nobody has ported it to agents. This is
where the "impressive" lives, and it is grounded in primary papers/docs:

- **Boundary-only capture + portable replay (rr, Microsoft TTD).** Record only what crosses
  the agent's boundary (LLM calls, tool calls, external I/O), not internal token sampling.
  rr overhead ~1.2-1.4x; TTD traces are portable and shareable at a point-in-time. Port =
  a "VCR cassette" trace file that replays by input-caching (not bit-exact re-derivation).
- **Structural diff with move detection (GumTree).** Diff trajectories as trees, align on
  structure not text, model reordered steps as MOVES. Complexity constraint: optimal
  tree-diff-with-moves is NP-hard, so use an O(n^2) heuristic.
- **Sequence alignment + the non-determinism trick (bioinformatics MSA, ICPC'22).** Abstract
  runs into symbol sequences, align with Needleman-Wunsch-style gaps so divergences surface
  as mismatches. Key move for non-determinism: cluster many runs into "execution types" and
  align a run only against its most-similar normal cluster. The most concrete published
  answer to aligning legitimately-divergent runs.
- **Delta-debug the fork (Zeller's dd/ddmin).** Once two runs are aligned, bisect the
  divergence set to isolate the 1-minimal step/input difference that caused the regression.
  Turns "where they forked" into "the minimal cause." Caveat: ddmin needs a re-runnable
  oracle, which is hard under stochastic re-execution.
- **Factorized LLM-as-judge for attribution (Agent GPA).** A decomposed judge localizes
  errors 85.8% vs 49.1% for a monolithic judge. Use it to attribute the regression at the
  divergence point.

**Differentiation bets (combine, don't pick one):** (1) "VCR for agents" portable local
trace; (2) structure-aware cluster-then-align engine; (3) ddmin the fork to a minimal
cause; (4) own the cross-run niche the SOTA explicitly avoids and sit deliberately below
the noisy eval layer; (5) rr/TTD-style rewind-to-fork UX as the credibility and demo hook.

**IP flag (do not skip):** Microsoft US Patent 11,093,368 covers Needleman-Wunsch +
edit-distance structural diffing of two replayable traces. It targets CPU traces, not agent
trajectories, but a structural/hierarchical run-diff needs a freedom-to-operate look.
Mitigation: design at the semantic agent-step level, not the instruction level.

**Still unresearched (verified empty, needs its own pass):** what actually makes an OSS dev
tool break out on HN/GitHub in 2026, and the specific run-diff roadmaps of Langfuse /
LangSmith / Phoenix / Braintrust. Both need a pass before you bet the positioning.

## Approaches Considered

### Approach A: Validation-first, then framework-agnostic run-diff (RECOMMENDED)
Run the hardened experiment before building. Build a real multi-step agent, get a
working run, make one change to induce a regression, then debug it **against the best
existing diff tool (EvalView / LangSmith), not raw logs**, tallying every blind moment
on a pain (1-5) x depth (1-5) scorecard. Build only if run-diff lands high-pain /
high-depth: a local binary that ingests OTel GenAI spans, aligns two runs, and
attributes the fork.
- Effort: M (experiment), then L/XL (build). The build's L/XL is dominated by the
  trajectory-alignment algorithm, which is research-grade; treat the estimate as a floor.
- Risk: Low — the experiment makes it nearly impossible to waste months on a void.
- Reuses: existing frameworks for the test harness; OTel GenAI semconv; existing diff
  tools as the baseline to beat.

### Approach B: Build the run-diff tool now, validate by shipping
Skip the experiment, build the OTel-consuming diff tool directly, put it on GitHub, see
if it gets traction.
- Effort: L/XL
- Risk: High — you have conceded conviction is borrowed; high chance of building over a
  void or re-cloning EvalView / Agent-Lens.

### Approach C: Pivot target to context-state inspector
If the experiment shows your real blindness is single-run context-state ("what could the
model see at each step, what got dropped"), build that instead.
- Effort: M
- Risk: Medium — likely higher pain but lower technical depth (closer to "render the
  JSON nicely"), so harder to make impressive. Hold as a data-driven fallback, not the
  lead.

## Recommended Approach

**Approach A.** It is the only path that resolves the borrowed-conviction problem before
months are spent, and it serves the actual goal (impressive + real-enough) directly: it
forces you to find the specific blindness the *best existing tool* still leaves, which is
simultaneously your wedge and your writeup. Build only what survives the experiment.

## Open Questions

- Runway: hours per week, over how many weeks? Sets experiment and build scope.
- Access to agent-builders for the optional step-3 conversations (your startup? network?).
- Systems language (Rust / Go / Zig)? Affects build framing, not the experiment.
- Does trajectory-alignment stay hard at small scale, or only at 12+ step runs? This is a
  validity threat to the toy-agent test in both directions.
- Post-experiment: is fork-attribution genuinely unsolved versus EvalView/LangSmith, or
  only partially? That answer sets the wedge.
- OTel GenAI semconv is experimental and will shift mid-build. Which span version do you
  pin to, and how much churn risk does that carry for a solo dev?
- (Unresearched) What actually makes an OSS dev tool break out on HN/GitHub in 2026, and
  what do senior reviewers read as "depth"? The depth pass returned zero verified claims
  here — needs its own research pass.
- (Unresearched) Do Langfuse, LangSmith, Phoenix, or Braintrust have run-diff / regression
  diffing on their public roadmaps? Absorption risk hinges on this and is currently unknown.
- Freedom-to-operate scope of US Patent 11,093,368 for a semantic-step-level (not
  instruction-level) run-diff using NW/edit-distance alignment.
- Deterministic-replay fidelity: is input/response-cache "VCR" replay plus seed pinning
  enough for fork-finding, or is more needed?
- Ground truth: given neither rule-based eval nor any single LLM-judge is reliable, what
  signal establishes step correspondence and labels a divergence as the regression cause?

## Success Criteria

### Pre-registered scoring (write this down before running anything)

- **Blindness menu (pre-registered):** the list of concrete questions you might fail to
  answer, written before you run anything. Starter set: (1) where did the two runs first
  diverge? (2) which tool or argument changed at the fork? (3) what did the model see at
  step N that differed? (4) why did the model pick that tool? Add your own.
- A **blindness** = one such question you actually could not answer from the tool in front
  of you during step 4. Sample: "I could not tell which run first diverged on the argument
  passed to tool `search`." Vague entries ("hard to follow") do not count. The running
  record of which ones you hit and their scores is the **blindness tally** — discovered
  during step 4, not pre-registered.
- **Pain axis (1-5):** 1 = mild curiosity, moved on in seconds. 3 = cost real minutes and
  a workaround. 5 = fully stuck, no path forward with current tools.
- **Depth axis (1-5):** 1 = a render/format fix anyone ships in a weekend. 3 = a real but
  known engineering problem. 5 = an open systems problem (non-trivial alignment /
  attribution).
- **Decision rule (pre-committed):** build run-diff only if, across **at least 3 distinct
  induced regressions**, the run-diff blindness scores **mean pain >= 4 AND mean depth
  >= 4**, where each mean is taken over the pooled run-diff blindness entries across all
  three regressions. If pain is high but depth is low and the highest-depth blindness is
  single-run context-state, take Approach C (pivot). Any other outcome — including depth
  high but pain low (impressive, but nobody is actually blind to it) — is a learning
  build: name it and stop.
- **Evidence precedence:** the firsthand scorecard decides *what to build*; the 20-30
  public bug reports are a veto and a tiebreak. If the corpus independently screams a
  different X than your n=3 firsthand result, trust the corpus over the small sample and
  re-run targeted at that X.

### Build success (only if greenlit)

A stranger can find the fork point in a run they did not write in under 30 seconds,
faster and clearer than doing the same with EvalView or LangSmith on that case.

## Distribution Plan

_Post-greenlight only. Do not pull any of this work forward before the experiment clears._

- GitHub repo, MIT or Apache-2.0.
- Single static binary via GitHub Releases (cross-compiled), plus `brew` / `cargo install`
  / `go install` as the language allows.
- CI/CD: GitHub Actions builds release binaries on tag. The "it just works" install is
  part of the impressive story, not an afterthought.
- A writeup (blog or dev.to) plus a demo GIF for the "noticed" goal. Distribution is the
  difference between a repo and a project that gets seen — do not skip it.

## Next Steps (the assignment)

Time-box the whole experiment to ~2 weeks / ~20-25 hours total. If it runs long, that is
itself a signal the setup is too heavy — cut, do not extend.

0. **Verify the substrate.** Confirm which frameworks actually emit OTel GenAI spans
   today and pin one span schema version. Stand up your baseline tools and confirm you can
   run them — LangSmith is cloud and may gate behind an account or paywall; if either will
   not stand up cleanly, fall back to another *diff/replay* tool (Agent-Lens, Braintrust,
   or Laminar), not a raw trace viewer — scoring against raw logs is exactly what this
   experiment rejects. If no diff baseline at all will stand up, that is a blocker to
   resolve, not a fallback to accept.
1. Write the pre-registered blindness menu and the scoring rubric (see Success Criteria)
   before touching any code.
2. Build a real 12+ step agent — ideally something you would build anyway — that produces
   long, genuinely divergent trajectories (multi-tool, branchy), not a linear toy. If two
   runs are short enough to eyeball side by side, the experiment can't see the hard
   problem — make the task bigger. This step is the one most likely to blow the time-box:
   reuse an agent you have already built if you can, and otherwise shrink to the minimum
   branchy, multi-tool task that still can't be eyeballed rather than building something
   elaborate from scratch.
3. Get a working run; then induce **at least 3 distinct regressions** with single changes
   (model / prompt / tool / dependency). If an induced change fails identically, or fails
   for an unrelated reason, discard it and pick another — you need divergent failures, not
   any failure.
4. Debug each regression with the **best existing diff tool open** (EvalView and/or
   LangSmith), scoring every blind moment on the pain x depth rubric.
5. In parallel, mine 20-30 verbatim "I couldn't tell X" complaints from GitHub issues,
   framework Discords, and r/LocalLLaMA. Ignore every "agents are hard to debug"
   generality.
6. Read the dev.to post "I Built a Debugger for LLM Agents — Here's Why Observability
   Wasn't Enough" for prior-art framing before you fall in love with the slogan.
7. Apply the decision rule. Build run-diff / pivot to context-state / name it a learning
   build and stop pretending otherwise.

## What I noticed about how you think

- You wrote: "If I start typing a story... that's me writing fiction to pass your test,
  and we both know it." Most people manufacture the anecdote. You let the spotlight find
  nothing and did not flinch. That is rarer than the technical instinct.
- Your sharpest move was unprompted: "I've been designing a nicer display of data people
  may already have, and calling that a debugger." You found the crack in your own core
  assumption before I did.
- You pre-empted my critique ("convenient, for a guy with a systems-language itch") and
  then cleanly separated "realest pain" (context-state) from "most impressive build"
  (run-diff). That distinction is the entire game, and you drew it yourself.
- You picked "get used" at D1, then revised to "impressive standout, usage a byproduct."
  That was not flip-flopping. You used the question to find the truer answer, which is
  what the question is for.

---

# Engineering Review — Locked Build Architecture

Added by /plan-eng-review on 2026-06-30. Research-backed (4 deep-research passes:
language/runtime, OTel GenAI maturity, trajectory-alignment SOTA, attribution+replay SOTA).

## Supersession note (read first)

The builder elected a **pure build** (Issue 1 = "drop validation"). Validation, GTM,
competitor scoring, and "is the niche won" are explicitly OUT. This section supersedes the
"Validation-First" framing in the title and Approaches A/B/C above. The only goals are:
**highly advanced, complete, industry-grade architecture, cutting-edge + future-proof tech.**

## Locked decisions

| # | Decision | Choice |
|---|----------|--------|
| D2 | Review scope | Full build architecture now (validation gate removed) |
| 1 | Validation | Pure build. No experiment, no baseline-scoring. |
| 2 | Alignment core | Typed **causal DAG** + embedding semantic similarity (enriched w/ graph-affinity + optimal-transport robustness) + **process-mining move-typed alignment** + **cluster/POA consensus**. Learned/graph methods are auxiliary signal ONLY (explainability requirement disqualifies an opaque neural core). |
| 3 | Attribution | **Full counterfactual-causal**: ddmin/HDD bisection -> counterfactual re-execution -> origination-vs-propagation labeling. Factorized Agent-GPA judge for SEMANTIC naming only, NOT localization (LLM localization ~5-11%). |
| 4 | UI | Embedded local web app; engine stays **headless lib + CLI + JSON** (stable result schema is the seam). |
| 5 | Frontend | **All-Rust**: Leptos shell + ~~wgpu/WebGPU~~ → **DOM + animated SVG** DAG renderer (wgpu dropped 2026-06-30, Issue 7: text must stay selectable/accessible); DAG layout computed server-side in Rust; single binary via rust-embed + axum. Visual-design pass done (DESIGN.md). |

## Tech stack (verified June 2026)

| Layer | Pick | Durability note |
|---|---|---|
| Language | Rust | difftastic/ast-grep/ripgrep/ruff/uv lineage; ADTs for move-typed edit ops; single static binary |
| Ingest | `opentelemetry-proto` + serde | OTLP types are solid even though GenAI semconv is not |
| Embeddings | `fastembed-rs` (ONNX via `ort`) | Local, offline, **no Tokio**; BGE-small default; cache by content hash |
| Alignment | hand-rolled NW (affine-gap) over embedding cost matrix; `rust-bio` optional symbol pre-pass | rust-bio scores byte symbols, not continuous embedding costs; verify its version/maintenance before depending on it |
| Clustering | `linfa` (k-means/DBSCAN) + `hnsw_rs` | Cluster runs into "execution types" to absorb non-determinism |
| Reduction | hand-rolled ddmin/HDD (~100 lines) | No grammar for agent-step DAGs; treereduce/tree-sitter dropped (they reduce source code) |
| Replay | custom cassette (cagent pattern) | Boundary full-INPUT capture + tool-call-ID normalization |
| LLM judge | provider trait + `reqwest`+SSE | No official Anthropic Rust SDK; abstract so an unmaintained crate can't block |
| UI shell | Leptos (WASM) | All-Rust; no JS build |
| UI render | Leptos DOM + animated SVG | Selectable/accessible/copy-pasteable text (debugger content is text); 12-100 node DAGs render trivially; wgpu dropped |
| Serve | `axum` + `rust-embed` | Embeds SPA assets -> still one binary |
| Schema/test | serde + `insta` + `proptest` | Snapshot + property tests |
| Distribution | `cargo-dist` / `cargo-zigbuild` + GH Actions | Static binaries linux/darwin/windows x amd64/arm64; brew/cargo install |

## Pipeline (data flow)

```
INGEST            NORMALIZE              SEMANTIC ALIGN                 ATTRIBUTE (causal)          PRESENT
────────          ──────────             ───────────────                ──────────────────         ───────
OTLP spans   ─▶  multi-namespace    ─▶  embed each step (role+tool   ─▶ fork = 1st non-sync     ─▶ web UI
gen_ai.* +       normalizer →            +args+summary) via ONNX        move (positional)          (wgpu DAG,
openinf.*/       canonical typed         → step×step cosine matrix      │                          fork lit,
llm.*            DAG (content            → MOVE-TYPED alignment         ├─ cluster runs → POA      content
file/stdin/      may be absent;          (sync / log / model moves)     │  consensus of "normal"   diff,
OTLP recv        tolerant parser)        over the matrix                │  (absorbs benign         time-travel)
                                         (rust-bio affine/banded NW)    │  non-determinism)
                                              ▲                         ├─ ddmin/HDD bisection
                                              │                         │  around the fork
                                     REPLAY (VCR cassette) ────────────▶├─ COUNTERFACTUAL re-run
                                     boundary full-INPUT capture,       │  sub-trajectory from
                                     clock/seed/tool-state virtualized, │  candidate step (causal)
                                     tool-call-ID normalization         └─ factorized LLM-judge
                                     (response-cache CANNOT reproduce      (Agent GPA rubric) for
                                      the divergent path — fork-finding     SEMANTIC label ONLY
                                      is semantic, not byte-exact)
```

> **Completeness Pass (2026-07-03):** the diagram above shows the PASSIVE path only, and its
> `wgpu DAG` label predates the DOM+SVG decision (wgpu dropped). The authoritative **hybrid
> passive+record** data-flow — including `amberfork-store`, `amberfork-record`, `amberfork-bench`, and the
> Layout / SuccessPredicate / Counterfactual seams — is in **"## Architecture Completeness
> Pass"** at the end of this doc.
>
> **Amendment 2026-07-08:** the diagram's `fork = 1st non-sync move (positional)` and
> `embed … via ONNX` cells are BOTH superseded (see "## Amendment 2026-07-08" up top): fork =
> first non-sync BLOCK the alignment does not recover from (resync-k); step similarity is
> lexical/tf-idf by default, embeddings/ONNX optional and gated on beating lexical.

## Module / crate layout (Cargo workspace)

> **[SUPERSEDED by Amendment 2026-07-11]** — this diagram is the target-state capability
> map. The shipped workspace is **5 crates** (model / ingest / align / amberfork / bench);
> see the amendment for where each crate below went and which phase adds the rest.

```
amberfork/                      LANE tokio? PHASE  role
├── crates/
│   ├── amberfork-model     A   no    1    canonical typed-DAG (Run,Step,Edge) + DiffResult + SuccessPredicate/Verdict
│   ├── amberfork-core      A   no    1    pipeline wiring + serde result schema (result-schema owner)
│   ├── amberfork-store     A   no    1    segment OTLP→runs, persist, list, pick A=good/B=bad, pair
│   ├── amberfork-ingest    A   yes   1    OTLP parse + multi-namespace normalizer + unmapped report
│   ├── amberfork-embed     B   no    1    fastembed-rs wrapper + content-hash embedding cache
│   ├── amberfork-align     C   no    1    cost model + move-typed aligner + gated consensus       ← moat
│   ├── amberfork-bench     H   no    1    fixtures + baselines + scorer + benchmark table          ← payoff
│   ├── amberfork-cli       G   yes   1    headless CLI (demo/diff/record/ls/open; --json/--gate)
│   ├── amberfork-replay    D   yes  1(t)  VCR cassette: boundary full-input capture + re-exec
│   ├── amberfork-record    D   yes  1(t)  record-mode proxy/shim: full-content capture + predicate hook
│   ├── amberfork-attrib    F   no    2    ddmin/HDD + counterfactual orchestration                 ← moat
│   ├── amberfork-judge     E   yes   2    provider trait + reqwest/SSE + GPA rubric (naming; bench baseline)
│   ├── amberfork-layout    G   no    2    server-side DAG layout → separate Layout schema
│   └── amberfork-server    G   yes   2    axum + rust-embed (serves WASM + result API)
└── ui/                 G   —     2    Leptos WASM + SVG/DOM renderer (wgpu dropped)

# 14 crates + ui. LANE H = new bench lane. PHASE per Strategic Reframe v2; 1(t) = Phase-1 tail.
# Supersedes the pre-hybrid roster; see "## Architecture Completeness Pass" (2026-07-03) for the
# full data-flow, schema seams (Layout, SuccessPredicate, Counterfactual), and reconciled task order.
```

Canonical model: `Run { id, steps: Vec<Step>, edges }`,
`Step { idx, kind: Llm|Tool|Agent|Other, name, inputs, outputs, attrs, timing, parent_idx }`.
Arena/index DAG (never `Rc<RefCell>`). One model + one serde result schema; every consumer
(CLI, server, UI, tests) reads that schema (the central DRY anchor).

## Concurrency boundary

Engine crates (`model`/`embed`/`align`/`attrib`/`layout`/`core`) are **sync + pure**.
`tokio` is quarantined to I/O edges (`ingest` OTLP, `judge` HTTP, `replay`, `server`).

## Normalization layer (moat + churn shield)

No uniform `gen_ai.*` stream exists in 2026: OpenInference (most-deployed) emits
`openinference.*`/`llm.*`; native `gen_ai.*` emitters are rare (PydanticAI, Semantic Kernel);
content is opt-in (`OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT`) and often absent;
the semconv is "Development", unversioned, with 4+ breaking changes in 12 months. Therefore:
map every supported namespace into the canonical model at ingest, **pin to instrumentation-
library versions** (not the spec), and emit an explicit "unmapped attributes" report so a
silently dropped namespace cannot corrupt alignment.

## Replay fidelity ceiling (state openly)

Response-cache replay reproduces only the RECORDED path; once run B branches it cache-misses.
So fork-finding is **semantic/state-based, not byte-exact**, and replay's real job is
**counterfactual re-execution** to verify cause. Capture full INPUTS at boundaries (not just
outputs: ≥21% of cases are unattributable from output-only logs; full inputs give +76%
relative step-level accuracy). Virtualize clock/seed/tool-state; normalize tool-call IDs.

## Test & eval strategy (engine target: 100% branch)

```
amberfork-ingest:  golden multi-framework OTLP fixtures → insta snapshot; both namespaces map
               identically; content-absent → metadata-only + banner; malformed → warn no panic
amberfork-align:   proptest invariant (self-align = all synchronous); known divergent pairs →
               expected fork; benign reorder → consensus → NO false fork; long trace → banded+timeout
amberfork-attrib:  ddmin minimal-cause on synthetic divergence; counterfactual oracle stable via
               recorded cassette; inconclusive → "unverified cause" (never fabricate)
amberfork-judge:   [EVAL not unit] vs TRAIL / Who&When fixtures; track localize/detect rate +
               regression baseline gate
amberfork-replay:  cache-miss at fork → "cannot reproduce divergent path" (by design)
e2e:           fixture pair → JSON result snapshot; server smoke serves UI + API
ui:            component tests + headless render smoke
```

## Performance budget

- Alignment O(n·m) per pair; banded for long traces; align suspect-vs-CONSENSUS (not all pairs).
- Embeddings (CPU ONNX) are the throughput floor → batch + content-hash cache; embed each unique step once.
- Counterfactual re-exec bounded by ddmin to ~O(log n) candidate re-runs; cassette serves cached responses.
- Arena/index DAG; stream large OTLP; avoid step clones.
- DOM/SVG renders 12-100 node DAGs trivially (wgpu dropped); layout once server-side, incremental on interaction.

## Failure modes

| Codepath | Realistic failure | Test | Handling | User sees |
|---|---|---|---|---|
| ingest | malformed/partial OTLP | yes | tolerant parse, warn | banner, continues |
| ingest | content opt-in off | yes | metadata-only align | "limited: no content" banner |
| ingest | unmapped namespace dropped | yes | "unmapped attributes" report | explicit warning (NOT silent) |
| align | benign reorder → false fork | yes | consensus clustering | confidence score |
| attrib | counterfactual oracle nondeterministic | yes | multi-run + consensus | "unverified cause" if inconclusive |
| replay | cache miss at fork | yes | by-design message | "cannot reproduce divergent path" |
| judge | provider error/timeout | yes | degrade to positional | positional result, no block |

Critical gaps: none, IF the test plan is implemented. Watch item: normalizer silent-drop
(mitigated by A6 report + test).

## NOT in scope (deferred, with rationale)

- Live/real-time tracing dashboard — this is a post-hoc diff tool.
- Cloud/SaaS/multi-tenant/auth — contradicts local single-binary identity.
- Writing framework instrumentation — consume existing OTel emitters.
- Bit-exact deterministic replay — physically impossible (GPU/batch nondeterminism).
- Training custom embedding/attribution models — off-the-shelf ONNX + hosted judge.
- Validation experiment / market scoring — dropped by builder.

## Reuse vs build

- REUSE: rust-bio (DP), fastembed-rs (embeddings), linfa+hnsw_rs (cluster/ANN),
  treereduce+tree-sitter (ddmin), opentelemetry-proto (OTLP), axum+rust-embed (serve),
  Leptos DOM/SVG (UI; wgpu dropped), serde, insta+proptest, Agent-GPA rubric design, cagent cassette pattern.
- BUILD (moat): normalizer, semantic cost model + move-typed aligner, cluster→consensus
  orchestration, counterfactual harness, server-side layout, DOM/SVG renderer (wgpu dropped).

## Worktree parallelization

Lanes A (model→ingest), B (embed), D (replay), E (judge) launch in parallel. Then C (align,
needs model+embed). Then F (attrib, needs align+replay+judge). G (layout/server/UI) after the
`amberfork-core` result schema is frozen. Conflict flag: everything touches `amberfork-model` early —
**freeze the model + result schema first**, in its own short lane, before fanning out.

## Implementation Tasks

- [ ] **T1 (P1)** — amberfork-model + amberfork-core — Freeze canonical typed-DAG model + serde result schema. Files: `crates/amberfork-model`, `crates/amberfork-core`. Verify: `cargo test -p amberfork-model`. (Blocks all lanes.)
- [ ] **T2 (P1)** — amberfork-ingest — OTLP parse + multi-namespace normalizer (`gen_ai.*` + `openinference.*`/`llm.*`) + "unmapped attributes" report. Files: `crates/amberfork-ingest`. Verify: insta snapshot on golden fixtures.
- [ ] **T3 (P1)** — amberfork-embed — fastembed-rs wrapper + content-hash cache. Files: `crates/amberfork-embed`. Verify: `cargo test -p amberfork-embed`.
- [ ] **T4 (P1)** — amberfork-align — semantic cost model + move-typed aligner (rust-bio affine/banded) + cluster/POA consensus. Files: `crates/amberfork-align`. Verify: proptest self-align invariant + known-fork fixtures.
- [ ] **T5 (P1)** — amberfork-replay — boundary full-input cassette + virtualized re-execution + tool-call-ID normalization. Files: `crates/amberfork-replay`. Verify: recorded-cassette determinism test.
- [ ] **T6 (P1)** — amberfork-attrib — ddmin/HDD bisection + counterfactual orchestration + origination-vs-propagation labeling. Files: `crates/amberfork-attrib`. Verify: minimal-cause + stable-oracle tests.
- [ ] **T7 (P2)** — amberfork-judge — provider trait + reqwest/SSE + factorized Agent-GPA rubric. Files: `crates/amberfork-judge`. Verify: TRAIL/Who&When EVAL suite + baseline gate.
- [ ] **T8 (P2)** — amberfork-layout + amberfork-server + amberfork-cli — server-side DAG layout, axum+rust-embed server, headless JSON CLI. Files: `crates/amberfork-layout`, `crates/amberfork-server`, `crates/amberfork-cli`. Verify: e2e fixture → JSON snapshot + server smoke.
- [ ] **T9 (P2)** — ui/ — Leptos DOM + animated SVG DAG renderer (wgpu dropped) + time-travel scrubber. /design-consultation done (DESIGN.md). Files: `ui/`. Verify: component tests + headless render smoke.
- [ ] **T10 (P2)** — CI/dist — cargo-dist + GH Actions matrix (linux/darwin/windows × amd64/arm64), brew/cargo install, embedded assets. Files: `.github/workflows/`, `Cargo.toml`. Verify: tagged release builds all targets.
- [ ] **T11 (P3)** — FTO hygiene — dated prior-art notes (NW, process-mining alignments); confirm semantic-step domain distance from US 11,093,368 family. Files: `docs/fto.md`.

## Open / unresolved

- Embedding model choice (BGE-small vs BGE-M3 vs a code-aware model) — pick during T3 against real fixtures.
- Move-typed alignment solver: affine-gap NW (rust-bio) vs process-mining A* — prototype both in T4, keep the one that localizes forks better on fixtures.
- Optional stretch layer: GMN graph-affinity as an additional cost signal (auxiliary only) — defer until T4 baseline works.
- Visual design language for the UI — owned by /design-consultation before T9.

## Outside-Voice Reconciliation

An independent review challenged the locked architecture and found a fundamental
contradiction plus several real gaps. Resolutions (all builder-approved):

### Execution model: HYBRID (Issue 6 = C)
Counterfactual attribution is impossible against passive OTLP alone (you cannot re-run a
telemetry photo). Resolution: two ingestion paths sharing ONE canonical model + result
schema + UI.
- PASSIVE path: ingest any existing OTel traces -> align -> static minimal-diff fork
  localization + optional naming. Framework-agnostic; works on traces you already have.
- RECORD path (`amberfork-record`): a proxy/SDK shim wraps agent execution -> captures FULL
  content boundary I/O, enables sub-trajectory counterfactual re-execution, enforces a
  user-defined success predicate, and can emit many runs for cluster/consensus.

```
                          passive (any OTel trace)    record-mode (run under amberfork)
align + fork                      yes                          yes
field-level static diff           yes                          yes
full content guaranteed           no (opt-in/often absent)     yes
counterfactual attribution        no                           yes
success predicate                 user-supplied/labeled        user-supplied, enforced
cluster/consensus (n>2)           only if corpus ingested      yes (record a corpus)
```

### Success oracle (resolves OV-2)
Attribution requires a PASS/FAIL predicate, supplied by the user: (a) assertion fn /
expected-output match, (b) a rubric scored by the factorized judge, or (c) a manual label.
OTel span status (OK/ERROR) is NOT treated as task-success.

### New / changed crates (resolves OV-3, OV-4)
- ADD `amberfork-store` (lane A): segment OTLP stream into runs, persist, list, pick
  A=good / B=bad. Owns run selection + pairing.
- ADD `amberfork-record` (lane D, tokio): record-mode proxy/shim; full-content capture; produces
  re-runnable cassettes and run corpora.
- `amberfork-align` consensus (linfa/hnsw/POA) is GATED: active only when a corpus exists
  (record-mode or an ingested many-run set). For the 2-run case it is bypassed; benign
  non-determinism is handled by the cost model + a confidence score. Not on the 2-run
  critical path.

### Embedding granularity split (resolves OV-6)
Do NOT embed `role+tool+args+summary` as one vector.
- ALIGNMENT key = structural identity (role + tool name + arg SCHEMA/shape), embedded for
  fuzzy step-TYPE matching.
- ATTRIBUTION = explicit structured FIELD-LEVEL diff of arg/content VALUES (exact + typed),
  not cosine similarity. Embeddings find "same step"; structured diff finds "which field changed".

### Alignment DP (resolves OV-7)
Hand-roll Needleman-Wunsch (affine-gap) over the step x step embedding cost matrix
(~50-100 lines); map to process-mining move types (sync / log / model). rust-bio is at
most a fast symbol-level pre-pass; verify its version/maintenance before depending on it.

### Judge (resolves OV-9)
The factorized Agent-GPA judge is OPTIONAL and supports a LOCAL model (candle/ONNX or a
local server) so the offline single-binary story holds. Positional + static results never
require network.

### Reduction (resolves OV-12)
ddmin/HDD hand-rolled (~100 lines) over the divergence set. treereduce/tree-sitter dropped
(source-code reducers; no agent-step grammar). The cited single-trajectory accuracy stats
(14-46% step-level, 85.8% vs 49.1%, +76%, etc.) are DIRECTIONAL only and are not validated
for the cross-run case; do not quote them as guarantees.

### Result schema (resolves OV-10) — T1 must specify this, not just the input model

```
DiffResult {
  runs: { a: RunRef, b: RunRef },
  alignment: [ Move { kind: Sync|Log|Model, a_idx?, b_idx?, cost, confidence } ],
  fork:      { index, a_step?, b_step?, confidence },
  field_diffs: [ { step, path, before, after, kind } ],
  attribution: {
    mode: Static | Counterfactual,
    origin_step?, propagation: [step_idx],
    counterfactual?: { recovered: bool | Unverified, runs: u32 },
    cause_label?: String,            // judge, optional
    confidence
  },
  warnings: [ { code, msg } ],       // incl. unmapped-attributes, content-absent
  meta: { schema_version, source: Passive | Record }
}
```

### UI rendering (Issue 7): Leptos DOM + SVG; wgpu dropped
Selectable / accessible / copy-pasteable text for prompts, tool args, and errors; animated
SVG DAG; all-Rust preserved.

### Lanes / priorities fixed (resolves OV-11)
- `amberfork-model` + `amberfork-core` (INCLUDING the result schema) + `amberfork-store` = Lane A,
  FROZEN FIRST, before any fan-out.
- `amberfork-record` = Lane D. Priority encodes the real serial chain:
  model/core/store/schema (P1) -> ingest/embed (P1) -> align (P1) -> replay/record (P1) ->
  attrib (P1, after align+replay) -> judge (P2) -> layout/server/cli (P2) -> ui (P2) ->
  ci/dist (P2).

### Revised task additions
- [ ] **T12 (P1)** — amberfork-store — run segmentation/persistence/selection (A=good/B=bad). Lane A.
- [ ] **T13 (P1)** — amberfork-record — record-mode proxy/shim: full-content capture + re-runnable cassette + success-predicate hook. Lane D.
- T4 revised: hand-rolled affine-gap NW over embedding cost matrix; consensus gated on corpus.
- T7 revised: judge optional + local-model path.
- T1 revised: freeze the OUTPUT `DiffResult` schema, not just the Run/Step input model.

# Developer Experience Review

Added by /plan-devex-review on 2026-06-30. Mode: DX EXPANSION. Persona: AI engineer /
agent builder. Target tier: Champion (<2 min; <30s via demo). Product type: CLI + Rust
library/SDK dev tool with a local web UI.

## Persona card
```
Who:       AI engineer / agent builder at a startup
Context:   built a multi-tool LangGraph/CrewAI agent; it regressed; wants to see
           where two runs forked, now
Tolerance: ~15-30 min; abandons if hello-world needs long docs or standing up infra
Expects:   single binary (brew/cargo), point at traces they already have,
           copy-paste from README, opinionated defaults
```

## Empathy narrative (the planned first run)
I `brew install amberfork` (fast, one binary). I want the magic, so I need two traces.
But I haven't wired up OTel. Before I see anything I must instrument my agent, run it
twice, capture two OTLP files, and hope content was captured (opt-in, often absent).
Twenty minutes in I run `amberfork diff a.otlp b.otlp` and if content was missing I get a
degraded metadata-only diff, not the glowing fork I was promised. I might bounce before
the magic. **Root DX risk: the magic is gated behind having two good traces.**

## Competitive benchmark (TTHW = time to first visible fork)
| Tool | TTHW | Notable move |
|------|------|--------------|
| difftastic | instant | works on files you have, zero setup |
| LangSmith | minutes | cloud + account gate |
| Phoenix | ~3-5 min | local, but instrument first |
| pprof | instant | bundled view of data you already produced |
| **amberfork (as designed)** | ~20 min | RED FLAG — gated behind instrument + 2 runs |
| **amberfork (post-review)** | **<30s via `amberfork demo`** | **Champion — magic before any setup** |

## Magical moment spec
Vehicle: bundled `amberfork demo`. Ship a sample divergent trace pair (with content) inside
the binary; `amberfork demo` runs the full pipeline and opens the UI on the glowing fork in
<30s, zero setup, offline. The first experience is the magic, not the 20-min path.

## Developer journey map
| Stage | Developer does | Status |
|-------|----------------|--------|
| Discover | README + demo GIF | fix: GIF + one-line value prop |
| Install | `brew install` / `cargo install` | ok: single binary |
| Hello world | `amberfork demo` | FIXED: <30s magic (was: 20-min instrument) |
| Real usage (passive) | `amberfork diff a.otlp b.otlp` | fix: content-absent error guides to record |
| Real usage (record) | `amberfork record -- python agent.py` | FIXED: zero-code wrapper |
| Debug | read fork + attribution in UI | ok |
| CI | `amberfork diff --gate A B` | NEW: regression gate, non-zero exit |
| Upgrade | semver + CHANGELOG; `--json`/cassette are versioned contracts | fix: document |

## Locked DX decisions
- **Getting started:** install → `amberfork demo` → `amberfork diff a b`. Champion tier.
- **CLI surface:** `demo` · `diff <a> <b>` (no args = two most-recent runs in store) · `record -- <cmd>` · `ls` · `open <id>`. Flags: `--json`, `--judge local|off`, `--no-open`, `--gate`. Progressive disclosure: `amberfork diff a b` just works.
- **Record-mode integration (DX-1):** zero-code CLI wrapper `amberfork record -- <cmd>` via env-var base-URL proxy capture; SDK shim is the documented escape hatch for in-process tools the proxy can't see.
- **CI regression gate (DX-2):** `amberfork diff --gate A B` returns non-zero + `--json` when a regression cause is attributed; GitHub Actions snippet in docs. Depends on the success-predicate/threshold from record-mode's oracle.
- **Error standard:** 3-tier (problem + cause + fix + doc link). Canonical example (the modal failure):
  > `No prompt/tool content in run_a.otlp — aligned structure only, not arguments.`
  > Cause: OTel GenAI content capture is opt-in and was off. Fix: set
  > `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT=SPAN_AND_EVENT` and re-run, or use
  > `amberfork record` to capture it automatically. Docs: <url>.

## NOT in scope (DX, deferred)
- Default telemetry / cloud analytics — contradicts local/no-account identity (TTHW self-report + opt-in only).
- Hosted web playground — contradicts local-only; `amberfork demo` covers the zero-install magic locally.
- Editor/LSP plugins — it's a CLI, not a language.
- Multi-language SDKs — the record proxy is language-agnostic by construction; Rust crates cover library use.

## What already exists (reuse)
Greenfield — nothing yet. The bundled demo traces double as `examples/`; DESIGN.md governs
the UI; the `DiffResult` schema is the public `--json` contract.

## DX Scorecard
```
+====================================================================+
|              DX PLAN REVIEW — SCORECARD                             |
+====================================================================+
| Dimension            | Score  | Prior  | Trend  |
|----------------------|--------|--------|--------|
| Getting Started      |  9/10  |  3/10  |  +6 ↑  |
| API/CLI/SDK          |  8/10  |  2/10  |  +6 ↑  |
| Error Messages       |  9/10  |  2/10  |  +7 ↑  |
| Documentation        |  8/10  |  2/10  |  +6 ↑  |
| Upgrade Path         |  8/10  |  3/10  |  +5 ↑  |
| Dev Environment      |  9/10  |  4/10  |  +5 ↑  |
| Community            |  8/10  |  3/10  |  +5 ↑  |
| DX Measurement       |  7/10  |  1/10  |  +6 ↑  |
+--------------------------------------------------------------------+
| TTHW                 | <2 min (<30s demo) | was ~20 min           |
| Competitive Rank     | Champion (via demo)                          |
| Magical Moment       | designed via bundled `amberfork demo`            |
| Product Type         | CLI + Rust library/SDK + local web UI        |
| Mode                 | DX EXPANSION                                 |
| Overall DX           |  8/10  |  3/10  |  +5 ↑  |
+====================================================================+
| Zero Friction      | covered (demo)                                 |
| Learn by Doing     | covered (demo + recipes)                       |
| Fight Uncertainty  | covered (3-tier errors)                        |
| Opinionated + Escape Hatches | covered (defaults + SDK escape hatch)|
| Code in Context    | covered (per-framework record recipes)        |
| Magical Moments    | covered (amberfork demo)                           |
+====================================================================+
```

## Implementation Tasks (DX)
- [ ] **T14 (P1)** — demo — bundle a sample divergent trace pair (with content) + `amberfork demo` (the magical moment). Files: `crates/amberfork-cli`, `examples/`. Verify: `amberfork demo` opens UI on the fork in <30s.
- [ ] **T15 (P1)** — cli — verb surface (`demo`/`diff`/`record`/`ls`/`open`) + flags (`--json`/`--judge`/`--no-open`/`--gate`) + no-arg defaults. Files: `crates/amberfork-cli`. Verify: `amberfork --help` is guessable; `amberfork diff` with no args picks 2 most-recent.
- [ ] **T16 (P1)** — errors — 3-tier error standard + content-absent canonical message across ingest/align/attrib. Files: `crates/amberfork-*`. Verify: trigger content-absent → message shows problem+cause+fix+docs.
- [ ] **T17 (P1)** — record — zero-code `amberfork record -- <cmd>` env-var proxy capture; SDK shim escape hatch documented. Files: `crates/amberfork-record`. Verify: wrap a sample agent, get full-content cassette with no code change.
- [ ] **T18 (P2)** — ci-gate — `amberfork diff --gate` exit codes + `--json` + GH Actions snippet. Files: `crates/amberfork-cli`, `docs/ci.md`. Verify: regression → non-zero exit in CI.
- [ ] **T19 (P2)** — docs — README (quickstart + demo GIF), concepts page, per-framework record recipes, OTel content gotcha, supported-frameworks matrix. Files: `README.md`, `docs/`.
- [ ] **T20 (P2)** — community — LICENSE + CONTRIBUTING + issue templates + custom-normalizer plugin point. Files: repo root, `crates/amberfork-ingest`.
- [ ] **T21 (P3)** — measurement — `amberfork demo --timings` TTHW self-report; design for /devex-review boomerang; opt-in anonymous metrics only. Files: `crates/amberfork-cli`.

> The GSTACK REVIEW REPORT that previously sat here has moved to the end of this document
> (it must be the file's terminal section). See the bottom for the current report.

---

# Strategic Reframe — v2 (added 2026-07-02, post competitive + demand research)

## Why this section exists
Four adversarially-verified research passes (~150 subagents: competitive landscape, academic
SOTA, real developer demand, verbatim practitioner pain) run *after* the build architecture was
locked. Full findings persisted in agent memory (`amberfork-competitive-demand-research`). This
section **re-sequences and re-positions** the locked plan. It does **NOT delete** anything from the
Engineering Review — every crate and capability survives. What changes is **order** and **pitch**.

## What the research changed
1. **The pain is real and loud — but the framing isn't.** Non-determinism / silent regressions
   ("same prompt, different result Tue vs Thu"; Anthropic's own postmortem shipped-passed-evals-
   still-regressed) and "I can't tell where it broke" ("failed at step 4, reran it, passed, no idea
   why"; root cause 3 steps upstream) are among the most validated complaints in the space
   (Who&When ICML'25: 14.2% step-attribution; Ouyang TOSEM'24: 47–76% non-reproducible; AGDebugger
   CHI'25: devs read 50–100+ messages by hand). BUT "diff two runs / find the fork" is *builder*
   vocabulary — the two on-the-nose "diff two runs" Show HNs sit at 1 point / 0 comments. Sufferers
   ask for replay, time-travel, reproduction. We must **teach** the mental model, not assume demand.
2. **The lane is crowded.** ~6 tools built "git diff for agents" in Feb–Apr 2026 (clay-good/agent-
   replay, brevity1swos/agx, ContextSubstrate `ctx diff`, Binex `binex diff`, ai-agent-vcr, Lucidic).
   None has deep alignment or a benchmark; the only one with traction (Lucidic, YC W25, 116 HN pts)
   does cluster-N-runs, NOT A/B diff. → Differentiation must be **depth + a public benchmark number**,
   not the pitch.
3. **Counterfactual attribution is non-novel, and SOTA is fleeing replay on cost.** AgenTracer /
   CausalFlow / CHIEF (2025–26) publish the same mechanism; CHIEF deliberately chose "zero-replay"
   because replay costs ~6–8× the tokens. → Keep it, but off the critical path.
4. **Correction:** GitHub issue #3447, cited mid-research as run-diff demand evidence, **does not
   exist** — a subagent hallucinated it. Never cite it. Position run-diff as *our solution* to proven
   localization/reproducibility pain — NOT as "developers asked to diff two runs."

## Reframed positioning
- **Old lead:** "Diff two agent runs, find the fork, attribute the regression."
- **New lead:** *"Point at a failing run — see exactly where it diverged from a known-good run, and
  what changed."* Localization + reproduction lead; two-run alignment is the **mechanism**, not the
  headline. The impressive/defensible core is unchanged: **explainable move-typed semantic
  alignment** — now *proven by a benchmark*, not asserted.

## Phased roadmap (NOTHING dropped — this is sequencing, per builder decision to keep full ambition)
Keeping every capability on the roadmap costs nothing. Building them before the engine ships +
benchmarks costs everything (scope is the #1 killer of solo projects; the benchmark is the payoff;
"14 crates, no number" reads as over-engineering). So: keep all, phase strictly.

- **Phase 1 — Proof of skill (the standout).** `amberfork-model` + `DiffResult` schema (T1/T12) →
  ingest/normalizer (T2) → embed (T3) → **align: move-typed affine-gap NW (T4) ← the moat** →
  field-level diff → CLI + `--json` (T15) → **benchmark on Who&When/TRAIL (NEW — see `BENCHMARK.md`)**
  → record/replay reproduction (T13/T17) → `amberfork demo` + <90s GIF (T14).
  *Definition of done:* a reproducible **offline** benchmark table (explainable, **local**) that
  states the defensible-asymmetry claim against the published ~14% step / ~11% joint baseline plus
  named cheap baselines (random + shallow-positional), with the privileged-reference caveat, plus
  the demo. (Claim reframed from "beating the baseline" to the asymmetry per the 2026-07-05
  two-pillar decision; see Current State.)
- **Phase 2 — Depth layers (only after the number).** Counterfactual attribution (T6, kept), Leptos
  SVG/DOM UI (T9), CI regression gate (T18), factorized judge for semantic naming (T7).
- **Phase 3 — Breadth & distribution.** cluster→consensus (gated), more framework normalizers,
  cargo-dist matrix (T10), community (T20), FTO notes (T11).

## Anti-goals (reaffirmed by research)
- **Do NOT build another trajectory scorer** — agentevals / LangSmith / DeepEval own pass/fail-vs-
  reference (red ocean). Our value is the *readable divergence localization*, not a score.
- **Do NOT pivot modality** (voice/image/music "observability") — escaping difficulty, not
  conviction. Voice is the only viable retarget (trajectory-shaped) and only with firsthand conviction.
- **Do NOT re-impose the golden-dataset maintenance tax** — the loudest upstream pain. Support
  multi-reference / consensus baselines so a single brittle "golden" doesn't break under non-determinism.

## Success criterion (updated)

> **Superseded 2026-07-05 by the two-pillar framing** (see "## Magical moment specification —
> TWO EQUAL PILLARS" below and the Current State block at the top). Cross-model review found that
> resting the project on one self-graded benchmark number is a fragile single point of failure.
> Current success = two equal pillars: (1) a reproducible OFFLINE benchmark whose claim is the
> defensible ASYMMETRY (localizes as well as an LLM judge, but local/deterministic/no-key, stated
> with the privileged-reference caveat), NOT "beats the baseline"; (2) the explainable engine +
> amber-fork UI. The paragraph below is kept as the 2026-07-02 framing.

The single artifact that earns strong-engineer respect: a **public benchmark result** — explainable,
local, non-LLM step-localization that beats the LLM-judge baseline on Who&When/TRAIL — paired with a
<90s demo where the amber fork ignites on a real divergent pair. Organic usage remains an explicit
non-goal / nice-to-have; do not distort the build chasing it.

---

# Architecture Completeness Pass

Added by /plan-eng-review on 2026-07-03. A completeness + consistency audit found the architecture
~90% specified but with 11 gaps — mostly because `amberfork-store`, `amberfork-record`, `amberfork-bench`, and
the hybrid passive+record model were added *after* the original diagrams were drawn, leaving stale
pictures and a few unowned contracts. This section is now the **authoritative** data-flow, crate
roster, schema seams, and task order. Where it conflicts with an earlier diagram, this wins.

## Authoritative hybrid data-flow (closes G1, G5)

```
 PASSIVE (any existing OTel trace)
 OTLP ─▶ amberfork-ingest ─▶ amberfork-store ─┐
        (normalize→DAG)  (segment,     │
                          pick A/B)     ▼
                              amberfork-embed ─▶ amberfork-align ─▶ field-diff ─▶ amberfork-attrib ─▶ DiffResult ─▶ amberfork-layout ─▶ server ─▶ ui
                              (structural   (move-typed     (typed        (STATIC or      (pure,         (Layout        (axum+   (Leptos
                               identity)     NW  ←moat)      value diff)    COUNTERFACTUAL   portable       schema,        embed)   SVG/DOM)
                                                 ▲                          ←moat)          --json)        separate)
 RECORD (run under amberfork)                        │                            ▲
 amberfork-record ─▶ amberfork-replay ───────────────────┘  (corpus ─▶ gated         │
 (proxy, full     (VCR cassette)  ── counterfactual re-exec ──────────────────┘
  content)  ─▶ SuccessPredicate (assert-fn | rubric | label) ─▶ attrib + cli --gate;  amberfork-judge = optional semantic naming

 amberfork-bench (Phase-1 payoff):  Who&When/TRAIL ─▶ ingest/store ─▶ align + baselines(random | positional | [judge=P2]) ─▶ score ─▶ table
```

Every crate now has an owner, a lane, a phase, and a defined seam. Canonical roster: see the
corrected 14-crate table in "## Module / crate layout" above.

> The `DiffResult ─▶ amberfork-layout ─▶ server ─▶ ui` tail of the diagram above still holds,
> but the artifact `amberfork-layout` emits is the **semantic `ViewModel`**, not the geometry
> `Layout` schema — see Amendment 2026-07-12 up top; the G2 section directly below is
> superseded accordingly.

## Layout is a separate schema, not part of DiffResult (closes G2)

> **[SUPERSEDED 2026-07-12 by issue #21]** — the seam shipped as the SEMANTIC view-model
> (`amberfork_layout::ViewModel`: rows with spine/fork/downstream roles, both sides of each
> aligned pair, the designed wording), not this pixel-geometry schema. Geometry is each
> painter's own business (eng review D3+D12: either painter-specific form gives a seam the
> other consumer fakes). What this section got right and which still governs: `DiffResult`
> stays presentation-free. The serializable document + envelope arrive with issue #24.

`DiffResult` stays **layout-free** — it is the portable `--json` contract and must not carry
presentation geometry. `amberfork-layout` consumes `DiffResult` and emits a separate `Layout`:

```
Layout {                                    // presentation-only; server → UI; NOT in --json
  runs:  { a: [NodePos], b: [NodePos] },    // NodePos { step_idx, x, y }
  spine: [ { y, a_idx?, b_idx? } ],         // shared-timeline rows: synced steps share one y
  fork_y,                                    // where the amber ignites (DESIGN.md north star)
  edges: [ EdgePath ],
  meta:  { layout_version }
}
```

This gives the DESIGN.md "synchronized spine" a concrete home and keeps the `DiffResult` contract
clean. `Layout` versions independently (`layout_version`).

## SuccessPredicate has an owner (closes G3)

The success oracle is now a first-class type in **`amberfork-model`** (the shared vocabulary), so
`attrib`, `record`, `judge`, and `cli --gate` all speak one shape instead of four:

```
// amberfork-model
enum Verdict { Pass, Fail, Unknown }
enum SuccessPredicate {
  AssertFn(FnRef),        // expected-output match / user assertion
  Rubric(RubricRef),      // scored by amberfork-judge (optional, local-capable)
  ManualLabel(Verdict),   // human label
}
// OTel span status (OK/ERROR) is NEVER treated as task success.
```

Owned by `amberfork-model`; enforced by `amberfork-record`; evaluated by `amberfork-attrib` and `amberfork-cli
--gate`; the `Rubric` variant delegates to `amberfork-judge`.

## Counterfactual injection seam (closes G11)

The attrib↔replay interaction (the moat's key mechanism) now has a contract, frozen in T1, exercised
in Phase 2:

```
// amberfork-attrib decides the mutation; amberfork-replay executes it
Counterfactual { step_idx, field_path, new_value }
replay::reexec_from(cassette, step_idx, mutation) -> SubTrajectory
// attrib scores `recovered?` by running the SuccessPredicate over the re-run
```

## amberfork-bench placement + baseline dependency resolved (closes G4)

- **Real crate, lane H, Phase 1** (not an `xtask` subcommand): the industry-grade workspace *is*
  the artifact, and a testable bench crate reads as rigor.
- **Baseline dependency inversion fixed.** Phase-1 baselines = **random** + **shallow-positional**
  (both cheap, in-crate) measured against the **published SOTA cited as the number to beat**
  (Who&When 14.2% step / 53.5% agent; TRAIL ~11% joint). The self-run LLM-judge head-to-head moves
  to **Phase 2** with `amberfork-judge` (cassette-cached, so the published table stays reproducible
  offline). Phase 1's payoff no longer secretly depends on a Phase-2 crate.

## Distribution completeness: ONNX + embedding model (closes G9, G10)

- **`ort`/ONNX-Runtime is a native C++ dep** underneath `fastembed-rs`, and it underpins the "single
  static binary, offline, no account" promise. **Pre-build spike (T25):** confirm it links (static
  or vendored) across linux/darwin/windows × amd64/arm64 *before* betting the DX headline on it.
- **Embedding model bundling:** default = **bundle int8 BGE-small-en-v1.5 (~30–45MB) via rust-embed**
  so `amberfork demo` runs offline from one binary. If the total binary exceeds an ~80MB ceiling, fall
  back to first-run fetch into a cache dir with an explicit "first run downloads the model" caveat.
  Decide against the measured size in the spike.

## Test-strategy additions (closes G7 — consolidated block was missing 5 crates)

```
amberfork-model/core: DiffResult + Layout serde round-trip; schema_version + layout_version stability (insta)
amberfork-store:      golden OTLP stream → expected run segmentation; A/B pick + pairing; ambiguous → explicit error
amberfork-embed:      content-hash cache hit skips recompute; identical input → identical vector; model-load fail surfaced
amberfork-record:     wrap sample agent → full-content cassette; proxy-miss → SDK-shim guidance; re-run determinism
amberfork-bench:      fixture→canonical conversion snapshot; baseline scores reproducible OFFLINE; results table byte-stable
```

## Failure-mode additions (closes G8 — table was missing 4 codepaths)

| Codepath | Realistic failure | Test | Handling | User sees |
|---|---|---|---|---|
| record | in-process LLM call the proxy can't see | yes | SDK-shim escape hatch | "record: N calls not intercepted — use the SDK shim" |
| store | ambiguous run boundaries in a merged stream | yes | explicit segmentation rule + manual override | "could not auto-segment; pass `--run-boundary`" |
| embed | ONNX model load fail / batch OOM | yes | fail-fast on load; chunk batches | "embedding model failed to load" / batch degrades |
| server | port in use / asset mount fail | yes | pick a free port; clear error | "port 7777 busy — using 7891" |

## Phase / priority reconciliation (closes the P1/P2/P3-vs-Phase-1/2/3 dual system)

The per-task `(P1/P2/P3)` labels in the Implementation-Tasks lists are **superseded by the PHASE
column** in the corrected roster. One authoritative order:

- **Phase 1 (the number + the magic):** T1/T12 (model+schema+store) → T2 (ingest) → T3 (embed) →
  **T4 (align ←moat)** → field-diff → T15 (cli) → **T22 (bench ←payoff)** → T5/T13/T17 (replay/record,
  tail) → T14 (demo). New: T23 (Layout seam), T24 (SuccessPredicate+Counterfactual types), T25 (ONNX spike).
- **Phase 2 (depth, only after the number):** T6 (counterfactual attrib) → T8 (layout/server) + T9
  (UI) → T18 (CI gate) → T7 (judge + self-run LLM-judge baseline).
- **Phase 3 (breadth & distribution):** cluster→consensus (gated) → more normalizers → T10 (dist
  matrix) → T20 (community) → T11 (FTO).

## New tasks

- [ ] **T22 (P1)** — amberfork-bench — real crate (lane H): Who&When/TRAIL fixture loader + random +
  shallow-positional baselines + scorer + `insta` results table. Files: `crates/amberfork-bench`,
  `bench/fetch`. Verify: `cargo run -p amberfork-bench` reproduces the table offline.
- [x] **T23 (P1)** — Layout seam — *shipped 2026-07-12 as the semantic view-model instead (issue #21,
  Amendment 2026-07-12): `ViewModel` lives in `amberfork-layout` itself, no `Layout` schema in
  `amberfork-model`, no geometry anywhere shared.* `DiffResult` stays layout-free — that part held.
- [ ] **T24 (P1)** — model contracts — `SuccessPredicate`/`Verdict` + `Counterfactual` types frozen in
  `amberfork-model` at T1. Files: `crates/amberfork-model`. Verify: `attrib`, `record`, `cli --gate` all depend
  on the one type.
- [ ] **T25 (P1)** — dist spike — confirm `ort`/ONNX-Runtime links across all 4 targets + decide
  embedding-model bundling (bundle vs first-run fetch) against measured binary size. Files:
  `.github/workflows/`, `crates/amberfork-embed`. Verify: a static/vendored build runs the demo offline on
  each target.

## Defaults applied in this pass (veto any)

1. `amberfork-core` → **lane A** (was mislabeled F). 2. `Layout` → **separate schema**, not in
`DiffResult`. 3. `SuccessPredicate`/`Verdict` → owned by **`amberfork-model`**. 4. `amberfork-bench` → **real
crate**, Phase 1; **LLM-judge baseline deferred to Phase 2**, published SOTA cited in Phase 1. 5.
Embedding model → **bundle int8 BGE-small** pending the T25 size spike. 6. Counterfactual seam →
`Counterfactual{step,field,value}` frozen in T1. None are one-way doors; all are reversible doc edits.

## Remaining open (genuine decisions, not gaps — flagged, not closed)

- The **sequencing question** from D2 (spike-first vs build-P1-as-written vs benchmark-only-first) is
  still open — this pass made the architecture *complete*, not *sequenced*. Recommend a 1–2 day
  throwaway alignment spike on real fixtures before the crate build, to de-risk "can Mode A references
  even be constructed / does semantic beat positional." Decide when you're back.
- Embedding model choice (BGE-small vs BGE-M3 vs code-aware) — still pick during T3 on real fixtures.
- Align solver (affine-gap NW vs process-mining A*) — still prototype both in T4.

---

# Developer Experience Review — v2 (evaluator-first re-review)

Added by /plan-devex-review on 2026-07-05. Mode: DX EXPANSION, full 8-pass re-review.
This re-review supersedes the persona and magical-moment choices in the 2026-06-30
"# Developer Experience Review" section above. Nothing there is deleted; the CLI/error/
demo work still holds. What changed is **who the primary developer is** and therefore
**which moment earns the project**.

## Why this section exists (supersession note)

The 2026-06-30 DX review optimized for an **agent-builder** persona (an AI engineer with a
regressing agent who installs amberfork and runs it on their own traces) and locked
`amberfork demo` as the magical moment. The **Strategic Reframe v2 (2026-07-02)** then made the
project's north star a **public benchmark number** and stated the mental model must be
*taught, not assumed* ("diff two runs" is builder vocab that scored 1pt/0 comments on Show
HN). Combined with the builder's stated goal — **impress strong engineers; usage is a
byproduct** — the primary developer is no longer the person who installs the tool. It is the
**skeptical senior engineer evaluating the project** from the repo, an HN/dev.to writeup, and
the benchmark, in 2-5 minutes. This section re-scores DX for that persona.

## Persona card (evaluator-first)

```
Who:       Senior/staff engineer (agent-infra or Rust systems) arriving from HN,
           a dev.to writeup, or a portfolio link.
Context:   Not debugging their own agent. Judging whether this project is
           technically impressive and worth respect / a star / a share.
Tolerance: ~2-5 min on the repo. Skeptical by default — ~6 shallow "git diff for
           agents" tools already exist; they've seen the pattern.
Expects:   A GIF that explains it in one look, a benchmark number with baselines
           they can reproduce, legible code, honest limitations.
Converts:  when the number is reproducible in one clean command AND the fork GIF
           lands AND the crate structure reads as rigor not slop.
Bounces:   when the number is asserted not backed, repro is broken/gated, the GIF
           is buried, or the pitch is builder-vocab they've watched flop.
```

## Empathy narrative (evaluator first contact — confirmed accurate 2026-07-05)

> I land on the amberfork repo from an HN post. First screen: a one-liner and a GIF. If the
> GIF shows an amber fork igniting on a real divergent run in under 90 seconds, I get it
> instantly: it finds where two agent runs split. I scroll to the benchmark table: amberfork
> vs shallow-positional-diff vs LLM-judge vs random, on Who&When and TRAIL, step-level plus
> windowed. If the numbers are honest — and there's a "where it fails" paragraph — my
> skepticism drops, this is real work, not a wrapper. I want to verify, so I hunt for the
> repro command: `cargo run -p amberfork-bench`. If it prints the table offline with no API key,
> I'm sold. Then I skim `crates/`: clear names (`align`, `attrib`, `bench`), tests present.
> That reads as rigor. Five minutes in I either star and share it, or I bounce because the
> number was asserted not reproducible, the GIF made me read a wall of text first, or 14
> crates with no shipped number reads as over-engineering.

**Root evaluator-DX risk:** the credibility artifact is the *reproducible + explainable* work,
not the tool running on their traces. If repro hides a network/licensing/size step, or the
number rests on a contested protocol with no honest framing, the promise cracks exactly where
the skeptic is looking hardest.

## Competitive DX benchmark (evaluator "time-to-belief")

Reference class is standout OSS systems tools, not SaaS onboarding. TTHW = time-to-belief:
how fast a skeptic goes from landing to "this is real, I'll star it."

| Tool | Time-to-belief | Notable move |
|------|:--:|--------------|
| ruff / uv | ~20s | benchmark chart IS the hero image; "10-100x" hyperlinked to BENCHMARKS.md, never bare |
| ripgrep | ~30s | screenshot + benchmark tables + **admits bias/curation** + discloses exact hardware + "beware perf cliffs" |
| difftastic | ~30s | 4 progressive screenshots teach a novel model; ships **Non-goals + Known Issues** |
| hyperfine | ~30s | GIF shows the tool doing its real job; the harness *is* the reproduction |
| paperswithcode norm | — | results table + one-command repro; complete repos ~196★ median vs ~0 |
| **amberfork (as planned)** | unclear | GIF + benchmark planned, but README ordering unspecified + repro hid a `bench/fetch` step |
| **amberfork (post-review)** | **<60s** | reproduce-locally-no-key headline + explainable-craft co-pillar + amber-fork GIF |

## Magical moment specification (evaluator) — TWO EQUAL PILLARS

The 06-30 review's `amberfork demo` was the moment for the agent-builder. This re-review's initial
draft made the *reproducible SOTA number* the single primary moment; the outside voice + the
project's own `craft-over-benchmark-in-saturated-lane` learning showed that is a fragile,
contestable single point of failure. Resolution (builder-approved 2026-07-05): **two
independent, equal belief pillars**, so neither one failing sinks the project.

- **Pillar 1 — credibility (the reproducible local eval).** `cargo run -p amberfork-bench`
  reproduces the scoring table **offline, deterministically, no API key**. The headline claim is
  the *defensible asymmetry*, NOT "beats 14.2% SOTA": **"localizes the decisive error step as
  well as an LLM judge — but locally, explainably, deterministically, and reproducibly without a
  network or key."** Report the head-to-head honestly *with* the privileged-information caveat
  (a two-run aligner is handed a known-good reference; the single-trajectory baselines were not —
  this is a different, easier task, not a straight SOTA win). The verifiable asymmetry (offline +
  deterministic + no key) is the real flex and doubles as the refutation of "isn't this just an
  LLM judge?"
- **Pillar 2 — craft (the explainable engine + UI).** The clever, legible move-typed alignment
  engine + the amber-fork explainable UI + honest local eval. Equal billing. Stands on its own
  even if semantic alignment only ties shallow positional. The `<90s` amber-fork GIF is this
  pillar's hero.

**Contingency (builder chose proceed-and-validate over spike-first):** the number-leading half of
Pillar 1 is conditional on **T4 confirming semantic move-typed alignment beats shallow positional
diff** on real fixtures. Validate inline during T4. If it only ties, the equal-pillar framing
degrades gracefully to craft-first (Pillar 2 leads) at **no re-architecting cost** — that is the
entire point of hedging to two pillars now.

**Implementation requirements:** raw results JSON committed + a **license-clean** fixture pair so
the table reproduces with zero fetch (T26, depends on T30); bench runs in CI with **no secrets,
cross-platform** so a green badge is machine-checkable proof (T29, gated on the T25 ONNX spike).

## Developer journey map (evaluator, with resolutions)

| Stage | Evaluator does | Friction (evidence) | Resolution |
|-------|----------------|---------------------|------------|
| 1. Discover | Reads HN title / tagline | "diff two runs" = builder vocab, flopped on Show HN (1pt/0 comments) | T28 hook + T33 writeup (the actual gate) |
| 2. Read README | Scans first screen for GIF + number | ordering unspecified (T19 listed contents, not priority) | T28: above-the-fold order (hero = table + GIF) |
| 3. Reproduce number | `cargo run -p amberfork-bench` | `bench/fetch` hid a network+licensing+size step | T26: results JSON + license-clean fixture → zero-fetch table; T29: no-secrets cross-platform CI |
| 4. See the magic | Watches <90s GIF | GIF spec was "<90s", no storyboard | Pillar-2 hero (magical-moment spec) |
| 5. Judge novelty | "what's new vs NW+ddmin+process-mining?" | review didn't answer the evaluator's first question | T28/T33: lead depth story with the novel run-vs-reference protocol + explainability |
| 6. Skim code | Opens `crates/`, DESIGN.md | 14 crates read as over-scope | T31: crate map BELOW the number + visibly Phase-1 surface |
| 7. Decide | Star / share / bounce | depends on 2,3,5,6 | resolved via T26/T28/T29/T31/T33 |

## First-time evaluator confusion report (annotated)

```
T+0:00  "diff two agent runs" → "another one of these?"      → FIXED T28 hook + T33 writeup
T+0:20  hunts for GIF; buried below install → patience drops → FIXED T28 (GIF above the fold)
T+0:45  benchmark: "vs what baselines? cherry-picked?"       → FIXED T28 (asymmetry claim + where-it-loses + caveat)
T+1:30  repro: bench/fetch gated/slow → "take their word"    → FIXED T26 (zero-fetch table) + T30 (licensing)
T+2:00  "isn't this just NW + an LLM judge? what's new?"     → FIXED T28/T33 (novel protocol + explainability lead)
T+2:30  watches GIF; fork ignites → memorable moment          → Pillar 2 hero
T+3:30  14 crates → "vaporware? over-engineered?"            → FIXED T31 (crate map below number + Phase-1 surface)
T+4:30  decides: star+share if number reproduced + code legible
```

## Locked DX decisions (this re-review)

- **Primary persona:** evaluator (skeptical senior engineer judging the repo), not agent-builder.
  Note: the outside voice warns the evaluator and the eventual user are often the same person on a
  one-week delay — do not actively *degrade* the install path; the 06-30 work keeps it warm.
- **Target:** <60s to belief; reproduce the local eval in one offline command.
- **Magical moment:** two equal pillars — reproducible local eval (defensible-asymmetry claim) AND
  the explainable engine/UI. Neither is a single point of failure.
- **F1 / Repro:** commit raw results JSON + a **license-clean** fixture pair so
  `cargo run -p amberfork-bench` prints the table offline with ZERO fetch; `bench/fetch` pulls full
  Who&When/TRAIL only for deep reproduction; pin dataset versions/commit. (T26, depends on T30)
- **F2 / CLI:** asymmetric `amberfork diff <bad> --against <good>` teaches the failing-vs-known-good
  model; symmetric two-arg and no-arg (2 most-recent, newest = candidate) remain as escape
  hatches. (T27)
- **F3 / Hook + novelty:** README + writeup lead = "Point at a failing agent run, see exactly
  where it diverged from a known-good run, and what changed." Lead the *depth* story with what is
  genuinely new — the **novel run-vs-reference cross-run localization protocol + explainability** —
  not "beats SOTA." The "like git bisect, but for agent runs" line is a one-line intuition pump,
  NOT the headline (it advertises recombination-of-known-tools if over-used). (T28)
- **F4 / Verifiable proof:** run the benchmark in CI with **no secrets, cross-platform**
  (mac/linux/windows) — a green badge is machine-checkable proof it's offline + deterministic +
  no-API-key. Regression gate exits non-zero if numbers drop. Gated on the T25 ONNX spike. (T29)
- **F5 / Licensing:** resolve + document that Who&When and TRAIL licenses permit benchmarking +
  publishing derived numbers **and redistributing any vendored derived fixture**, with
  attribution, before publishing the table. Blocks T26's vendored sample. (T30)
- **F6 / Legibility + repro errors:** map the 14-crate workspace as a design TOC placed *below*
  the number (not above — showing crate sprawl first is a self-own), and make the shipped surface
  visibly the Phase-1 subset (T31); extend the 3-tier error standard to the bench/fetch + ONNX
  repro paths (T32).
- **F7 / Distribution (from outside voice):** the HN/dev.to writeup is the real gate to reaching
  evaluators and is a first-class DX surface, not an afterthought. (T33)

## Cross-model reconciliation (outside voice)

An independent second opinion (Claude subagent; Codex not installed) challenged this review's
own conclusions. Highest-value points and resolutions:

- **Reproducibility ≠ credibility; the number is self-graded and apples-to-oranges.** Accepted.
  Resolved by reframing Pillar 1's claim to the defensible asymmetry (local/explainable/
  deterministic/no-key) with an explicit privileged-reference caveat, not "beats SOTA."
- **Single point of failure on one unvalidated number.** Accepted. Resolved via the two-equal-
  pillar magical moment; craft stands alone if T4 shows semantic only ties positional.
- **The writeup/distribution is the real gate and went unreviewed.** Accepted. Added T33.
- **The evaluator's first question is novelty vs prior art (NW 1970 + ddmin 1999 + process-mining
  + patent overlap).** Accepted. T28/T33 lead the depth story with the novel protocol +
  explainability; the git-bisect anchor is demoted to a one-liner.
- **"Offline/no-key" rides on the unverified T25 ONNX spike; one-Linux-runner CI ≠ their box.**
  Accepted. T29 is a cross-platform matrix, gated on T25.
- **F1↔F5 conflict (vendoring derived data may be license-forbidden).** Accepted. T26 now depends
  on T30.
- **Spike sequencing.** Builder chose proceed-and-validate at T4 over a spike-first gate; the
  two-pillar hedge makes that safe (a tie degrades to craft-first, no re-architecting).

## NOT in scope (DX, deferred — this re-review)

- **Spike-first validation of semantic-vs-positional** — builder chose to validate inline at T4;
  the two-pillar framing absorbs a tie, so a standalone spike is optional, not required.
- **Actively optimizing the install-and-run-on-your-traces path beyond the 06-30 work** — the
  agent-builder is the secondary persona now, but do NOT degrade that path (outside-voice caution:
  evaluator→user is a one-week delay). `amberfork demo` / `amberfork record` keep it warm.
- **A hosted playground / "try in browser"** — contradicts local-only; committed results JSON +
  `amberfork demo` cover zero-install credibility. (Reaffirmed from 06-30.)
- **Multi-language SDKs / editor plugins** — irrelevant to an evaluator and to a CLI. (06-30.)
- **Chasing organic adoption funnels** — goal is respect; measure via stars/HN/writeup reception,
  not funnels. TTHW self-report stays opt-in only.

## What already exists (reuse)

- The 2026-06-30 DX section: `amberfork demo`, `amberfork record`, the 3-tier error standard, the CLI verb
  set (extended, not replaced, by T27's `--against`).
- BENCHMARK.md already contains the "evaluation crux," the "threats to validity," and the "where
  it fails" paragraph (DoD) plus the baseline list — T28's benchmark-trust block and T33's writeup
  promote that honesty into the README/post instead of burying it.
- DESIGN.md governs the amber-fork GIF (Pillar 2 hero); the `DiffResult` schema is the public
  `--json`/SDK contract; the `amberfork-bench` crate (T22) is the reproduction harness.

## DX Scorecard (evaluator lens)

```
+====================================================================+
|         DX PLAN RE-REVIEW — SCORECARD (evaluator-first)             |
+====================================================================+
| Dimension            | Score  | Initial| Trend  |
|----------------------|--------|--------|--------|
| Getting Started      |  9/10  |  6/10  |  +3 ↑  |
| API/CLI/SDK          |  9/10  |  6/10  |  +3 ↑  |
| Error Messages       |  9/10  |  8/10  |  +1 ↑  |
| Documentation        |  9/10  |  5/10  |  +4 ↑  |
| Upgrade Path         |  8/10  |  8/10  |   0 =  |
| Dev Environment      |  9/10  |  7/10  |  +2 ↑  |
| Community            |  8/10  |  7/10  |  +1 ↑  |
| DX Measurement       |  8/10  |  7/10  |  +1 ↑  |
+--------------------------------------------------------------------+
| TTHW (time-to-belief)| <60s   | ~2-5min (friction) |               |
| Competitive Rank     | Champion (ripgrep/uv tier) via reproduce-local|
| Magical Moment       | 2 equal pillars: reproducible eval + explainable craft |
| Product Type         | CLI + Rust lib/SDK + local web UI + benchmark|
| Mode                 | DX EXPANSION (full 8-pass re-review)        |
| Persona              | evaluator-first (was agent-builder)         |
| Overall DX           |  9/10  |  6/10  |  +3 ↑  |
+====================================================================+
| DX PRINCIPLE COVERAGE                                               |
| Zero Friction      | covered (zero-fetch offline eval table)        |
| Learn by Doing     | covered (repro command + GIF + example pair)   |
| Fight Uncertainty  | covered (3-tier errors incl. repro paths)      |
| Opinionated + Escape Hatches | covered (--against default + symmetric)|
| Code in Context    | covered (mechanism paragraph + crate map)      |
| Magical Moments    | covered (2 equal pillars, no single failure)   |
+====================================================================+
```

Note: "Initial" = this re-review's evaluator baseline (before F1-F7). The 2026-06-30 review
scored 3→8 for a *different* (agent-builder) persona; those numbers are not directly comparable
dimension-by-dimension. The 9/10 is a **plan** score contingent on two execution gates: **T4**
(semantic > positional) and **T25** (ONNX links cross-platform). If T4 ties, the two-pillar
framing degrades to craft-first at no re-architecting cost; if T25 fails, the "offline binary"
headline must soften to "first run downloads a 30-45MB model."

## DX Implementation Checklist

```
[x] Time-to-belief < 60s (zero-fetch offline eval table)          → T26
[x] Reproduce the number in one command, no API key                → T26/T29
[x] Magical moment = two equal pillars (eval + explainable craft)  → T26/T28 + Pillar 2
[x] Claim reframed to defensible asymmetry (not "beats SOTA")      → T28
[x] README above-the-fold order specified (hero = table + GIF)     → T28
[x] Benchmark trust: baselines + hardware + command + where-it-loses + caveat → T28
[x] Depth story leads with the novel protocol + explainability     → T28/T33
[x] Mental model taught by contrast; git-bisect anchor demoted     → T28
[x] CLI teaches failing-vs-known-good asymmetry                    → T27
[x] Bench runs in CI, no secrets, cross-platform + regression gate → T29
[x] Dataset licensing resolved (incl. redistribution) before publishing → T30
[x] Crate map placed below the number; shipped surface = Phase-1   → T31
[x] Every repro error has problem + cause + fix + docs             → T32
[x] Writeup/distribution treated as a first-class artifact         → T33
[ ] ONNX/ort links across all 4 targets (gates "offline binary")  → T25 (pre-existing spike)
[ ] Semantic alignment beats shallow positional on real fixtures   → T4 (validates the number pillar)
```

## Implementation Tasks (DX v2)

- [ ] **T26 (P1)** — bench offline repro — commit raw results JSON + a **license-clean** fixture
  pair so `cargo run -p amberfork-bench` prints the table offline with ZERO fetch; `bench/fetch` pulls
  full Who&When/TRAIL only for deep repro; pin dataset versions/commit. Files: `crates/amberfork-bench`,
  `bench/`, `README.md`. Depends on: **T30**. Verify: fresh `git clone` + airplane mode →
  `cargo run -p amberfork-bench` prints the table.
- [ ] **T27 (P1)** — asymmetric CLI — `amberfork diff <bad> --against <good>` teaches the
  candidate-vs-reference model; symmetric two-arg + no-arg (2 most-recent, newest = candidate)
  remain as escape hatches. Files: `crates/amberfork-cli`. Verify: `amberfork diff --help` teaches the
  asymmetry; no-arg picks candidate = newest.
- [ ] **T28 (P1)** — README spec — above-the-fold order (badges → localization value-prop → hero =
  benchmark table + amber-fork GIF → 3 highlights → install → how-it-works mechanism paragraph →
  Non-goals/Known-limitations) + benchmark-trust block (named baselines + published numbers +
  hardware + exact command + a "where we tie/lose" row + variance + the **defensible-asymmetry
  claim replacing "beats SOTA," with the privileged-reference caveat**); lead the depth story with
  the **novel run-vs-reference protocol + explainability**; git-bisect anchor is a one-liner, not
  the hero. Files: `README.md`, `docs/`. Verify: a cold reader states what amberfork does, what's
  new, and why the number is trustworthy in <60s.
- [ ] **T29 (P1)** — bench in CI + regression gate — run `amberfork-bench` in CI with **no secrets /
  offline across a mac + linux + windows matrix**; publish a green badge; exit non-zero if numbers
  regress. Gated on **T25**. Files: `.github/workflows/`, `crates/amberfork-bench`. Verify: CI passes
  with no secrets on all three OSes; a seeded regression turns it red.
- [ ] **T30 (P2)** — dataset licensing — confirm Who&When + TRAIL licenses permit benchmarking,
  publishing derived numbers, **and redistributing any vendored derived fixture**; add attribution
  + license notes. Files: `docs/datasets.md`, `README.md`. Verify: each dataset's license cited +
  publish/redistribute permission documented before the table ships. (Blocks T26.)
- [ ] **T31 (P2)** — crate legibility — map the 14-crate workspace in the README as a design TOC
  placed **below** the number/how-it-works (not above); make the shipped surface visibly the
  Phase-1 subset (not empty Phase-2/3 stubs shown as done). Files: `README.md`, `Cargo.toml`.
  Verify: `Cargo.toml` + the README crate map read as a table-of-contents of the design, and appear
  after the benchmark on the page.
- [ ] **T32 (P2)** — repro error paths — extend the 3-tier error standard to `bench/fetch`
  (dataset gated/unreachable, checksum mismatch) + ONNX/ort load-or-link failure. Files:
  `crates/amberfork-bench`, `crates/amberfork-embed`. Verify: trigger a gated-dataset + a checksum
  mismatch → each shows problem + cause + fix + docs link.
- [ ] **T33 (P2)** — writeup / distribution — treat the HN/dev.to post as a first-class artifact
  (for a "be seen" goal it is the product, and the on-the-nose framing is proven weak: 1pt/0
  comments). Lead with personal motivation + the novel eval protocol + explainability + the
  local/deterministic asymmetry; include the honest "where it fails" + prior-art/novelty framing;
  do NOT lead with "diff two runs" or "beats SOTA." Files: `docs/writeup.md`, `README.md`. Verify:
  a strong engineer who reads only the post can state what is new here and why the number is
  trustworthy.

# Design Review — fork-diff UI (added 2026-07-05 by /plan-design-review)

Reviewed the amberfork instrument UI as specified in this plan + DESIGN.md. Classifier: APP UI
(data-dense instrument). Passes the Design Hard Rules clean (no hard rejections; litmus all green:
unmistakable brand, one visual anchor = the amber fork, scannable, one job per pane, motion improves
hierarchy, premium without decorative shadows). The visual SYSTEM (DESIGN.md) is strong; the gaps
were undesigned states + the explainability pane + a11y. A DOM+SVG mockup of the fork-diff hero was
hand-built and approved (see Approved Mockups). Overall design completeness 6/10 → 9/10.

## Locked design decisions

- **DR1 / No-divergence "converged" state (Pass 2).** When two runs are identical or differ only
  cosmetically, the instrument does NOT show a dead screen: the spine renders fully synced/gray with
  a calm centered message ("Identical through all N steps" / "No decisive divergence — largest
  difference is cosmetic at step K") plus the alignment confidence. Empty states are features. (T34)
- **DR2 / Colorblind redundancy (Pass 6).** The "divergence glows amber" signal is NEVER color-alone.
  The fork node carries a persistent `⑂ FORK` label, a heavier/distinct stroke, and the divergent
  path a distinct line style — the signal survives grayscale. ~8% of target (male engineer) users
  have reduced red-green discrimination. Ratified in DESIGN.md. (T35)
- **DR3 / Multiple divergences (Pass 7).** The first decisive fork gets full amber + the attribution
  pane; downstream divergences (after re-convergence) show as amber markers listed in the attribution
  pane and steppable via the scrubber. Matches "fork = 1st non-sync move" without hiding the rest,
  preserves "one glowing thing wins." (T36)
- **DR4 / Uniform amber divergent path (Pass 5).** Fork and every downstream divergent step share ONE
  amber; the mockup's dimmer-propagation tint is dropped. One scarce accent. Ratified in DESIGN.md. (T35)
- **DR5 / Attribution reading order (Pass 1, approved via mockup).** Fixed: fork step → move-typed
  alignment summary (sync/model/log) → field-level diff (red/green) → confidence → counterfactual
  result → plain-English cause. Where, then what moved, then what changed, then proof, then why. (T36)
- **DR6 / Content-absent degraded pane (Pass 2).** When OTel content is opt-in-off, the pane shows the
  structural/move-typed diff only (no field VALUE diff), an amber "content: limited" banner, and a
  "run under `amberfork record` to capture arguments" nudge. Reuses the DX 3-tier error standard. (T36/T37)
- **DR7 / Loading/compute state (Pass 2).** Embedding is the throughput floor; align is banded+timeout
  on long traces. The canvas reveals the spine progressively as steps embed, with an "aligning N
  steps" indicator — the wait is oriented, not a frozen blank. (T37)
- **DR8 / A11y spec (Pass 6).** Arrow-keys step through the DAG + scrubber; ARIA landmarks (rail=nav,
  canvas=main, attribution=complementary); a focus ring on the fork; a contrast pass (faint `#55555C`
  fails 4.5:1 — reserve for non-essential labels only); prompt/arg/error text selectable+copyable
  (the reason DOM+SVG beat wgpu). (T38)
- **DR9 / Canvas IA (Pass 1).** On load, auto-center/scroll to the fork (a 60-step DAG otherwise opens
  at step 1, burying the point); spec scroll/virtualization for 50-100 step runs and how the spine +
  scrubber stay oriented. (T39)

## Interaction-state table (fork-diff view)

| State | What the user sees |
|-------|--------------------|
| Loading/compute | spine reveals progressively as steps embed; "aligning N steps" indicator; no frozen blank |
| Empty · no runs | first-launch: rail prompts "point at two OTel traces or `amberfork record`"; empty-instrument affordance |
| Empty · one run | "need a second run to diff — pick a reference (A=good)"; rail highlights pairing |
| Empty · converged | spine fully gray/synced + "Identical through N steps / no decisive divergence" + confidence (DR1) |
| Partial · content-absent | structural/move-typed diff only + amber "content: limited" banner + record nudge (DR6) |
| Error · malformed OTLP | banner "partial trace — parsed K of N spans", continues |
| Error · unmapped namespace | explicit "unmapped attributes" warning (never silent) |
| Error · counterfactual inconclusive | attribution shows "unverified cause" (never fabricated) |
| Error · cache-miss at fork | "cannot reproduce divergent path" (by design) |
| Success | the populated fork view (approved mockup) |

## NOT in scope (design, deferred)

- **Mobile/phone layout** — local desktop instrument; tablet/laptop (down to ~1024px) is in scope,
  phone is not.
- **Light-mode state coverage** — DESIGN.md defines light tokens; dark is primary and the mockup is
  dark. Light-mode states deferred until dark ships.
- **Colorblind alternate palette theme** — DR2's redundant shape+label fixes the default view; a full
  alternate palette is a later nicety, not required.

## What already exists (reuse)

DESIGN.md (the full system + north star), the server-side `Layout` schema (spine geometry, `fork_y`),
the `DiffResult` schema (the data the attribution pane renders), and the DX review's 3-tier error
standard (the content-absent path reuses it).

## Approved Mockups

| Screen | Mockup Path | Direction | Notes |
|--------|-------------|-----------|-------|
| Fork-diff hero | `docs/design/mockups/fork-diff-hero.html` (repo copy; original at `~/.gstack/projects/Melvin0070-fantastic-broccoli/designs/fork-diff-hero-20260705/`) | DOM+SVG instrument; amber-only-on-divergence; approved attribution reading order | Fold DR2 (`⑂ FORK` label) + DR4 (uniform amber) into the mockup when it becomes the build reference |

## Design Scorecard

```
Pass 1  Information Architecture     6/10 → 9/10
Pass 2  Interaction States           4/10 → 9/10
Pass 3  User Journey                 6/10 → 8/10
Pass 4  AI Slop Risk                 8/10 → 9/10
Pass 5  Design System Alignment      7/10 → 9/10
Pass 6  Responsive & Accessibility   5/10 → 8/10
Pass 7  Unresolved Decisions         6 surfaced · 6 resolved · 0 open
Overall design completeness          6/10 → 9/10
```

## Implementation Tasks (Design)

- [ ] **T34 (P1)** — converged + empty states — no-divergence "converged" view (gray spine + calm
  message + confidence) + no-runs / one-run empty states. Files: `ui/`, `crates/amberfork-layout`.
  Verify: two identical runs → converged state, not a blank/dead screen.
- [ ] **T35 (P1)** — colorblind + uniform amber — fork carries `⑂ FORK` label + distinct stroke +
  distinct divergent line style (grayscale-legible); divergent path uniform amber (drop intensity-
  grading). Files: `ui/`, `DESIGN.md`. Verify: a grayscale screenshot still shows the fork; one amber token.
- [ ] **T36 (P1)** — attribution pane spec — reading order (fork → moves → field diff → confidence →
  counterfactual → cause) + content-absent degraded variant + multiple-divergence list (primary +
  navigable secondaries). Files: `ui/`, `crates/amberfork-layout`. Verify: content-absent run → structural-
  only pane + record nudge; multi-fork run → secondaries listed and steppable.
- [ ] **T37 (P1)** — interaction-state table — implement loading/compute, empty (no-runs/one-run/
  converged), content-absent, error (malformed/unmapped/inconclusive/cache-miss), success. Files:
  `ui/`. Verify: each row of the state table renders as specified.
- [ ] **T38 (P2)** — a11y — keyboard stepping (DAG + scrubber), ARIA landmarks, fork focus ring,
  contrast pass (retire `#55555C` from essential text), selectable/copyable text. Files: `ui/`,
  `DESIGN.md`. Verify: keyboard-only step to the fork; essential text ≥ 4.5:1.
- [ ] **T39 (P2)** — canvas IA — auto-center on the fork on load + long-DAG (50-100 step) scroll/
  virtualization + spine/scrubber orientation. Files: `ui/`, `crates/amberfork-layout`. Verify: a 60-step
  run opens centered on the fork.
- [ ] **T40 (P2)** — DESIGN.md ratification — fully spec move-typed chips, the confidence meter, the
  counterfactual result row, the uniform-amber path, and the colorblind rule in the design system.
  Files: `DESIGN.md`. Verify: DESIGN.md documents each new component with tokens.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 2 | issues_found | eng: 12 findings; DX v2: 7 findings (both Claude subagent; Codex not installed) |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | clean | 24 findings, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | clean | 6/10 → 9/10, 9 decisions (DR1-DR9) + 10-row state table; mockup approved |
| DX Review | `/plan-devex-review` | Developer experience gaps | 2 | clean | v2 evaluator-first re-review: 6/10 → 9/10, TTHW <60s to belief; 7 findings (F1-F7) resolved |

- **CODEX:** Not installed. Outside voices ran as independent Claude subagents (`npm install -g @openai/codex` for cross-model). The DX v2 subagent was pointed at that review's OWN conclusions and surfaced 7 substantive points; the 3 strongest drove the two-pillar reframe.
- **CROSS-MODEL:** Convergent. The DX outside voice AND the project's own `craft-over-benchmark-in-saturated-lane` learning independently flagged the SOTA number as a fragile single pillar; resolution hedges to two equal belief pillars (reproducible eval + explainable craft). Design review adds the explainable-craft pillar's concrete UI: converged state, colorblind-safe fork, and the fixed attribution reading order.
- **VERDICT:** ENG CLEARED + DX reviewed (9/10, evaluator-first, two-pillar magical moment) + DESIGN reviewed (9/10, mockup approved). Ready to implement: hybrid passive+record, 14-crate all-Rust workspace, explainable alignment + counterfactual-causal attribution, Leptos SVG/DOM UI, DX tasks T26-T33, design tasks T34-T40. Execution gates on the DX headline: T4 (semantic > positional) and T25 (ONNX links cross-platform).

NO UNRESOLVED DECISIONS
