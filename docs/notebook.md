# amberfork — engineering notebook

Chronological working log: questions, measurements, dead ends, decisions. The benchmark's
pre-registered protocol (`BENCHMARK.md`) requires test-set runs to be logged here. Nothing in
this file is marketing; when an experiment fails, it says so.

---

## 001 · 2026-07-07 · Feasibility spike: Mode-A pairs + semantic-vs-positional

**Questions (pre-stated, before touching data).**
1. Can failing↔passing reference pairs (BENCHMARK.md "Mode A") actually be constructed from the
   published Who&When data — or does the benchmark require generating references ourselves?
2. Does DP alignment (move-typed, over step similarity) beat a shallow positional first-mismatch
   baseline — and exact-match-cost alignment (structure-only) — at localizing the annotated
   decisive error step?
3. What do real logs look like (step counts, content shape, annotation quality), and what does
   that imply for the cost model?

**Method.** Throwaway Python in `spike/` (explicitly not product code). Real Who&When logs
converted to the canonical JSON (`docs/trace-format.md`). Arms: random, positional
first-mismatch, NW + exact-match costs, NW + lexical costs, NW + TF-IDF costs (embeddings only
if feasible offline). Metrics: step exact-match, ±1, ±3 against the annotated mistake step.
Tiny N — findings are directional, not benchmark results; the real bench is governed by the
pre-registered protocol in BENCHMARK.md.

**Results.**

*Q1 — Mode-A pairs are NOT constructible from published data.* Verified against the real
dataset (ag2ai/Agents_Failure_Attribution, MIT via GitHub; HF mirror `Kevin355/Who_and_When`
carries no license tag): 184 failure logs (58 hand-crafted Magnetic-One runs, 126
algorithm-generated CaptainAgent teams), `is_correct`/`is_corrected` false in every file. The
paper's "decisive error" definition is counterfactual *in wording only* — it was operationalized
by three human annotators, never by execution — and no successful/original trajectory is
included or referenced anywhere. The only same-task structure is 45 question_IDs failed by BOTH
systems (two failures, no passes). Consequences: run-vs-reference on Who&When requires
*generating* references (every log carries the question + ground-truth answer, so
success-checking is possible, but it means re-running agent stacks at API cost), or finding
public success traces of the same GAIA/AssistantBench questions from other systems (research
pending). Upstream caveat for fixture redistribution: questions/answers originate from GAIA
(gated upstream) and AssistantBench — resolve before vendoring any derived fixture (T30).

*Q2 — mechanics on real content (chimera protocol, n=20 pairs/condition, seed 42, hand-crafted
split, 12–60 steps).* Failing run = real log X's prefix + real log Y's tail spliced at a known
gold step; benign-noise condition adds one duplicated "retry" step + token-dropout rewording to
the shared prefix. Full grids in `spike/out/{noise,clean}/results.md`. Headline (τ=0.2–0.4):

| arm (noise condition) | exact | ±1 | ±3 |
|---|---|---|---|
| random | .04 | .12 | .28 |
| positional first-mismatch (lexical) | **.00** | .15 | .30 |
| NW-aligned, naive first-divergence rule | .00 | .15 | .30 |
| NW-structural (names only) + resync | .00 | .00 | .10 |
| **NW-lexical + resync rule** | **.70** | **.90** | **1.00** |
| NW-tfidf + resync | .65 | .80 | .95 |
| NW-embed (potion-8M) + resync | .50 | .85 | .95 |

Control (no noise): positional-lexical is nearly perfect (.85 exact) — with byte-identical
prefixes you don't need alignment. **The aligner's entire value is tolerance to benign
non-determinism**, which is exactly the agent reality (47–76% non-reproducibility, Ouyang
TOSEM'24) and exactly what the noise condition simulates.

Three secondary findings: (a) the naive "first non-sync move" fork rule scores ~0 even with
perfect alignment — first divergence ≠ decisive divergence, empirically; the blip-tolerant
"sustained divergence" (resync) rule is what works; (b) structure-only costs are blind on these
logs (agent names cycle; the fork lives in content); (c) generic static embeddings LOST to
plain lexical similarity — "semantic" did not earn its keep here.

*Q3 — data shape.* Hand-crafted: 5–130 steps, median 32.5 — the value case for alignment.
Algorithm-generated: capped at 10 steps (median 10) — short enough to eyeball; use as smoke
fixtures, not headline fixtures. Annotation quirks: `mistake_step` is a STRING 0-indexed int;
~5 files where the annotated agent doesn't match the step's speaker; GH↔HF drift
(`ground_truth` vs `groundtruth`, `WebSurfer` casing) — normalize at load.

**Caveats (do not over-read).** Chimera forks are injected, not natural: sustained divergence
at gold is true *by construction*, which favors alignment arms; noise parameters are
author-chosen (though positional's collapse is structural — any single insertion breaks index
alignment); n=20, one seed; τ swept without a dev/test split. Directional evidence only — the
pre-registered protocol in BENCHMARK.md governs anything published.

**Decisions.**
1. **Fork criterion amended:** "fork = first non-sync move" is empirically wrong; the spec
   becomes "first non-sync block the alignment does not recover from" (resync-k). Carry into
   `amberfork-align`'s design (architecture doc needs a dated amendment — flagged, not yet edited).
2. **Embedding bet demoted to a hypothesis:** the v1 cost model starts lexical/tf-idf
   (dependency-free, deterministic, no 30–45MB model, no ort/ONNX linking risk). fastembed/ONNX
   stays behind the cost-model trait as an *experiment* that must beat lexical on dev fixtures
   to ship — if it loses, T25 falls off the critical path entirely.
3. **Benchmark reframe:** Who&When as published cannot support run-vs-reference. The offline
   table's primary protocol becomes controlled-injection localization on real logs (fully
   reproducible, honestly labeled) + self-generated references as the stretch goal (Mode A′),
   pending the reference-trace research.
4. Hand-crafted split = headline fixtures; algorithm-generated = smoke fixtures.

**Addendum (2026-07-07, later) — reference sources + licensing resolved.**
- **Mode A′ is constructible.** Who&When algo logs carry genuine GAIA UUIDs; public known-good
  runs on the same tasks exist: HAL (`agent-evals/hal_traces`, 37 full GAIA runs, many models;
  license unspecified — benchmark-use ok, don't redistribute), TapeAgents (Apache-2.0, 8 full
  GAIA tapes, 4 successful — redistributable), `gaia-benchmark/submissions_public` (gated;
  includes Magnetic-One's own passing rows; coarse traces). Natural-failure run-vs-reference
  returns as *cross-system* alignment, honestly disclosed.
- **TRAIL corrected:** 148 traces / 1,987 spans (117 GAIA + 31 SWE-Bench), MIT via GitHub (HF
  copy gated no-reshare — always source GitHub). Real OpenInference span trees — validates the
  `amberfork-ingest` plan directly. 4 zero-error traces; span-located error annotations usable as
  localization gold. No same-task duplicate runs within TRAIL.
- **Licensing rules of the road:** Who&When + TRAIL = MIT via their GitHub repos (attribution +
  notice when vendoring fixtures); never vendor from gated HF copies; strip/hash GAIA
  ground-truth answers in anything redistributed.
- **Prior-art note for the writeup:** ServiceNow TapeAgents ships a `tape_diff.py` utility —
  inspect and cite it in the prior-art/novelty section before claiming the niche.
- **Decision 3 amended:** benchmark = controlled-injection (primary, fully reproducible) +
  Mode A′ cross-system natural pairs (co-primary target via HAL/TapeAgents) + self-generated
  references demoted to optional stretch.

---

## 002 · 2026-07-08 · Decision-grade evidence for the issue-#8 amendments

Purpose: the founder delegated issue #8 ("adopt spike-001 amendments into the locked
architecture doc") pending stronger evidence. This entry hardens or overturns each amendment
before the doc is touched. Questions (pre-stated):
1. **Fork rule robustness.** Does resync > first-divergence hold across seeds (42/43/44) and
   noise levels (reword 0.2/0.4/0.6, retries 1/1/2)? How sensitive to the resync-k parameter
   (k=1/2/3)?
2. **The fair embeddings test.** Spike 001 used a weak static model. Does **BGE-small-en-v1.5
   via fastembed** — the exact model+runtime the design doc specs — beat lexical on the same
   pairs?
3. **Mode A′ reality.** Build actual cross-system pairs (TapeAgents passing tape ↔ Who&When
   failing log, same GAIA task) and measure. How many pairs are constructible from public
   sources (HAL count via research agent)?
4. **External legitimacy.** Do published definitions (agent failure-attribution 2025–26,
   process-mining conformance, bioinformatics) support first-divergence or sustained-divergence?

Method: `spike/robustness.py` (3×3 sweep, n=20/config, best-τ oracle reporting — method
ceilings, labeled as such); `spike/make_realpairs.py` (4 real pairs found: all 8 published
TapeAgents GAIA tapes match Who&When tasks; 4 are successes); two web-research agents (HAL
per-task results; prior-art definitions).

**Results.**

*Q1 — the fork rule holds, decisively, and spike 001's headline gets an honest correction.*
Across all 9 configs (3 seeds × 3 noise levels), best-τ **oracle** results (`spike/out/robustness/`):
positional first-mismatch and NW+first-divergence score **0.00 exact in every single config** —
even with the threshold chosen oracle-optimally. NW+resync-k2: **0.47–0.50 mean exact,
0.72–0.85 mean ±1**, stable across noise levels. Honest correction: spike 001's "70% exact" was
the seed-42 draw; the across-seed mean is **~0.50 exact / ~0.75 ±1** (lexical spread 0.25–0.70
by seed). The effect that matters — resync vs first/positional — is **~0.5 vs 0.0** everywhere.
k-sensitivity: k=1 collapses (0.05–0.07; one sync step forgives the true fork too), k=2 best,
k=3 slightly worse (0.28–0.42). The recovery window is a real tunable: default k=2, calibrate
on dev fixtures.

*Q2 — the fair embeddings test changes the amendment's shape.* BGE-small-en-v1.5 via fastembed
(the exact specced model+runtime), same-system base noise: **0.53 exact (0.50–0.55)** — a
statistical TIE with lexical 0.50 (0.25–0.70) and tf-idf 0.53 (0.35–0.65), though notably more
seed-stable. On the **real cross-system pairs** (Q3, n=4): embeddings are the only arms
reaching 100% ±3 (lexical/tf-idf ~50% ±3; random 74% ±3 on these short runs — n=4, granularity
0.25, treat as directional). Net: embeddings do NOT earn their dependency cost (ONNX runtime +
30–45MB model) for same-system alignment, but show a real niche for cross-system alignment
where surface vocabulary differs.

*Q3 — Mode A′ pairs are real.* All 8 published TapeAgents GAIA tapes match Who&When tasks; the
4 successful tapes became the first real failing↔passing pairs (`spike/make_realpairs.py`).
Two validity caveats discovered by building them: (a) cross-system "gold" is murky — a
reference from a different agent system legitimately diverges from step 0 (different rosters,
different plan shapes), so the annotated mistake_step is a weaker target than in same-system
pairs; (b) Who&When algo logs are short (7–10 steps), so ±3 windows cover most of the run.
Mode A′ needs deliberate gold/metric design (e.g., longer hand-crafted logs vs Magnetic-One
references) before it can headline. HAL-scale pair counts: research agent pending.

*Q4 — external prior-art supports the direction and sharpens the novelty claim.*
- **Fork rule.** Only ONE published work defines a two-run fork as first-divergence — WebStep
  "bifurcation = last shared state before divergence" (arXiv 2606.15673) — and it works only on
  clean discrete semantic states and *explicitly disclaims* recovery/sustained divergence. On
  noisy free-text traces the dominant ground-truth standard is **counterfactual recoverability**:
  Who&When ("earliest step whose correction alone makes the task succeed", 2505.00212),
  AgenTracer (2509.03312), CausalFlow (2605.25338), CHIEF (2602.23701) all define the decisive
  step by "the error is not recovered from" — the same intuition as our resync rule, computed by
  re-simulation instead of alignment geometry. Process-mining independently moved from per-move
  flags to segment/pattern-level deviations (BPM 2024, "Beyond Log and Model Moves"); "cut on
  sustained score-drop, tolerate transient mismatch" is textbook **X-drop** (BLAST). So our
  combination — two-run semantic alignment + sustained-divergence/resync on noisy agent text —
  is novel and unclaimed, and the naive first-divergence rule is contraindicated by the field,
  matching our ~0% measurement.
- **Lexical vs embeddings.** BEIR (NeurIPS'21): BM25 is a strong zero-shot baseline dense
  retrievers fail to beat out-of-domain; log-representation literature keeps token/template
  methods competitive. BGE-small has a 512-token cap and known MTEB-rank-doesn't-transfer
  behavior. Literature's remedy is *hybrid* (+2–5%), so "lexical beat generic embeddings here"
  is well-supported but "embeddings never help" is not — our CLAUDE.md bar ("must beat lexical
  on dev fixtures to earn a place") is exactly right.
- **Tooling foil.** ServiceNow TapeAgents `tape_diff.py` compares two runs **positionally**
  (index-wise `zip_longest` + word highlight); no alignment, no fork detection — desyncs on any
  insertion. Cite as the motivation for alignment-based diffing.

*HAL-scale pair count (Q3 continued) — Mode A′ is constructible AT SCALE, cheaply.* The HAL
leaderboard page (`hal.cs.princeton.edu/gaia`) embeds a per-task success matrix (165 GAIA tasks
× 32 configs) as inline JSON — no big download needed to know who passed what. Result:
**126 of 128 Who&When GAIA failure logs (algo 96/98, hand 30/30; 106/108 unique tasks)** have
≥1 public passing HAL run. Trajectory cost: HAL zips are Fernet-encrypted with the public
password `hal1234` (PBKDF2-HMAC-SHA256, 480k iters — replicated in ~15 lines); one 48.8MB
o3-mini zip alone yields full step-by-step passing trajectories (grouped by GAIA task id) for
54 tasks; **~450MB gets 90% coverage, ~2.9GB gets all 106.** Two tasks were solved by nobody
(`whowhen_algo_9`, `whowhen_algo_63`). So the pair-*count* worry is gone; the *gold-quality*
worry (cross-system references diverge from step 0; algo logs are short) is what keeps Mode A′
from headlining, not scarcity.

**Decisions.**
1. **Amendment A — fork criterion: ADOPT.** Empirically robust (resync ~0.5 vs first/positional
   0.0 across all 9 configs) and externally supported. Spec: "fork = first non-sync block the
   alignment does not recover from within k sync moves (default k=2, dev-calibrated)." Correct
   spike 001's 70%→~50% across-seed exact in all docs.
2. **Amendment B — cost model: ADOPT, REFINED.** v1 ships lexical/tf-idf as the default
   (dependency-free, deterministic, seed-stable, ties BGE same-system). Keep embeddings behind
   the cost-model trait as a first-class experiment — they showed a real cross-system edge — with
   the "beat lexical on dev fixtures to earn default status" bar. ONNX/ort therefore leaves the
   *critical path* (T25 downgraded from gate to optional) but is NOT deleted.
3. **Amendment C — benchmark protocol: ADOPT with a scope flag.** Controlled-injection is the
   reproducible primary. Mode A′ is proven constructible **at scale** (126/128 GAIA failure logs
   pairable with public HAL passing runs; ~450MB for 90%), so scarcity is not the blocker — but
   building the first pairs surfaced that cross-system gold is murky (references legitimately
   diverge from step 0) and algo logs are short. So Mode A′ is a real **co-primary target for
   v0.2** contingent on gold/metric design (prefer long hand-crafted Who&When logs vs Magnetic-One
   HAL references; report windowed metrics), NOT a v1 headline. Do not overclaim step-exact on it.

## 003 · 2026-07-08 · Cost-model port (issue #3): token-level gestalt replaces char-level difflib

**Question (pre-stated).** The spike's `sim_lexical` is Python `difflib.SequenceMatcher.ratio()`
over 600-char-capped step text. `difflib` silently applies *autojunk* (elements above 1%
frequency are junked whenever the second sequence is ≥200 chars) — a stdlib quirk we do not want
to re-implement in Rust. Does a cleanly-portable variant match or beat the spike numbers on the
dev fixtures, per issue #3's bar ("must match or beat … 70% exact @ n=20 noise")?

**Method.** Re-scored two candidates through the existing spike harness (no Rust yet):
(a) char-level Ratcliff–Obershelp with autojunk OFF (the naive faithful port), and
(b) token-level Ratcliff–Obershelp over `[a-z0-9]+` lowercase tokens (same tokenizer as the
tf-idf arm). Fixtures: committed smoke pair; committed seed-42 n=20 noise chimera pairs
(resync-k2); then the full spike-002 robustness protocol (seeds 42/43/44 × noise low/base/high,
N=20, best-τ oracle) for (b) vs the recorded char-difflib arm.

**Results.**
- Smoke: fork=6 preserved by both candidates; positional control still misled. Token-RO holds
  fork=6 across τ=0.2–0.4.
- Committed n=20 noise: char-RO-nojunk **0.65** exact — *below* the recorded 0.70, i.e. the
  autojunk quirk was load-bearing for the naive port. Token-RO **0.75** exact, ±1 0.90, flat
  across τ=0.2–0.4.
- Robustness (exact mean over seeds, best-τ): token-RO vs char-difflib — low **0.48/0.47**,
  base **0.52/0.50**, high **0.52/0.48**; token-RO's worst seed ≥ char's at every level
  (high: min 0.45 vs 0.40). Best τ mostly 0.2.

**Decision.** v1 `LexicalCost` (crate `amberfork-align`) = token-level gestalt ratio over
lowercase ASCII-alphanumeric token sequences of 600-char-capped `"name: outputs"` text.
Equal-or-better on every dev-fixture condition, no stdlib quirks to port, ~36× fewer DP cells
than char-level, and the tokenizer is shared with a future tf-idf model. Bit-parity with Python
is explicitly a non-goal; the committed fixtures + these numbers are the contract.

**Caveats.** Dev-fixture scale only (chimera pairs, N=20 per cell); best-τ numbers are method
ceilings as in 002; benchmark claims remain governed by BENCHMARK.md. The chimera pairs are NOT
committed (`spike/data/` untracked) — the Rust crate can regression-test against
`spike/fixtures/smoke` only, so whether to commit a regenerated dev-pair set for the ≥0.70
parity check is an open decision for a later slice of #3.
