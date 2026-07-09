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

## 004 · 2026-07-08 · Issue #3 done: Rust engine meets fixture parity

The `amberfork-align` port (four reviewed slices: `CostModel`/`LexicalCost`, Gotoh affine-gap
NW, resync-k fork rule, public `diff()`) hits the issue's pre-stated bar, measured on the
actual Rust binary via `cargo test --test chimera_parity -- --ignored`:

- **Chimera seed-42 n=20 noise: 15/20 exact (0.75)** vs the spike bar 14/20 (0.70) — and equal
  to the Python token-RO validation in 003, i.e. no fidelity lost in the port.
- Smoke fixture: fork localized at gold step 6 through `diff()` (the CLI seam); both fixture
  runs converge against themselves.
- Property guard: proptest self-align invariant (any run vs itself ⇒ all-sync, no fork, even
  at τ=0) in CI.

The chimera test stays `#[ignore]`d in CI: the pairs derive from Who&When logs whose questions
originate in gated GAIA (redistribution unresolved — notebook 001 / T30), so they are not
committed; the test regenerates locally via `spike/make_pairs.py`. Scoring uses the spike's
failing-side prediction rule (b-side index of the fork move, or consumed-count for model-only
forks) — that logic lives in the test for now and moves into `amberfork-bench` with issue #6.

## 005 · 2026-07-09 · Fork confidence is informative (designed formula validated)

**Question.** The engine's fork confidence — `(evidence − τ)/(1 − τ)`, evidence = fork move's
sync cost or a gap move's distance-to-closest-counterpart — was designed for explainability,
not measured. Before any surface renders it as a trust meter: do high-confidence forks hit the
gold step more often?

**Method.** `spike/confidence_check.py`: replicates the Rust pipeline (token gestalt, τ=0.3,
k=2 — parity per 004) over the robustness protocol's 9 cells (seeds 42/43/44 × noise
low/base/high, N=20 each). 180 forked pairs, 0 no-fork. Metric: confidence vs exact-hit.

**Results.** Overall exact 87/180 (0.48, matching 002/003 means). Mean confidence of hits
**0.476** vs misses **0.141**; point-biserial r(confidence, hit) = **0.563**. Terciles:
low [0.00..0.00] → **0/60**; mid (0.00..0.44] → 41/60 (0.68); high (0.44..0.98] → 46/60
(**0.77**). The zero bucket is exactly the designed "weak call" case (evidence ≤ τ: marginal
sync or a gapped step with a sync-grade twin) — and it is never exactly right on these pairs.

**Decision.** Confidence may be displayed as a meter (CLI #4, UI later) — it separates hits
from misses. Render `confidence ≈ 0` as an explicit "marginal call / weak fork" state, not a
small bar: on dev pairs it means "do not trust the exact step". Directional caveat: dev
chimera pairs only, one cost model, τ fixed at 0.3; recalibrate before any benchmark claim.

## 006 · 2026-07-09 · Protocol rules 1+4 live: dev/test split + exclusions-as-data (issue #6 slice 3)

**What changed.** `amberfork-bench` now assigns every pair its dev/test side (stable FNV-1a
hash of the task key; dev iff bucket < 30 of 100 — committed code constants, deliberately NOT
`bench/params.toml` material: a re-tunable split is no split) and treats unevaluable cases as
counted exclusions tabulated by reason (manifest-unreadable/-invalid, run-unloadable,
empty-run, gold-out-of-range) instead of hard load errors. The coverage line publishes with
the table; the results JSON (bench_schema 0.2) carries coverage + the per-pair split manifest,
so committing results (slice 6) commits the manifest. Task key = the **reference run's `id`**
(`whowhen_hand_14`-style; one Who&When log = one question) — never the `task` field, which
carries gated GAIA question text (001/T30). Pairs of one task co-locate by construction (the
leakage guard). Caveat recorded in `split.rs`: the chimera tail's source log Y is not
split-keyed; the split protects the prefix task X, where the gold step lives.

**Local seed-42 noise set (n=20):** 8 dev / 12 test across 13 unique tasks; `whowhen_hand_49`'s
three pairs all land dev — the guard visibly working. Dev tuning baseline (the number every
future cost/τ/k change is judged against): **nw-lexical/resync 0.75 exact [0.41, 0.93], 0.88 ±1,
1.00 ±3, n=8**; random / pos-lexical / nw-structural all 0.00 exact on dev. Consistent with the
full-set 0.75 (003/004), so the dev draw is not a skewed subset.

**Discipline from this commit.** Tuning runs `--split dev` only. The test side runs with frozen
params once per release tag (arrives with slice 4). Honesty note: 003/004/005 measured on all
20 pairs before the split existed, so this *local* set's test side is not pristine — acceptable
for dev-stage mechanics, but published-table fixture sets get generated and split under the
frozen protocol from birth, and any post-test change reports old-alongside-new (rule 3).

## 007 · 2026-07-09 · Protocol rule 2 live: parameter freeze (issue #6 slice 4)

**What changed.** `bench/params.toml` (repo root, where BENCHMARK.md pre-registered it) is now
the ONLY parameter source `amberfork-bench run` accepts — the `DiffParams::default()` fallback
is gone. `--params <FILE>` defaults to `bench/params.toml` resolved from the working directory
(the repo root in the publishing workflow); a missing or invalid file is exit-2 trouble, never
a silent fall back, because a freeze with a fallback is decorative. Loading is strict: deny
unknown keys, require every key, then the engine's own `DiffParams::validated()` — a typo
cannot half-apply. The published artifact names its config: a `params:` line above the table
carries file + sha256 prefix (`8ebd95ce8f3d` for the initial freeze), and the results JSON
(bench_schema 0.3) carries `params.source` + the full digest.

**Design choices worth remembering.** (1) The hash is sha256 of the exact file *bytes*, not of
parsed values — a comment or changelog edit is a new config revision too, and any reviewer
verifies with plain `shasum -a 256 bench/params.toml` (the unit test's known-answer vector was
computed with coreutils, not the sha2 crate, so the check isn't circular). (2) The file's
schema mirrors the engine's params tree via bench-local structs rather than deserializing into
the engine types, so deny-unknown-fields stays a bench policy and a new engine knob forces a
conscious schema change. (3) A unit test pins frozen file == `DiffParams::default()`: the
table must describe the product people actually run; a deliberate retune touches both, plus
the file's changelog and a notebook entry (rules 2+3). New deps: `toml` (the pre-registered
format; comments carry the changelog) and `sha2` (standard hash = independently verifiable;
in-crate fnv1a64 stays for split/stream seeding only, where the requirement is stability, not
audit).

**Check.** The 006 dev tuning baseline reproduces bit-for-bit under the frozen file:
nw-lexical/resync **0.75 exact [0.41, 0.93], 0.88 ±1, 1.00 ±3, n=8**, config `8ebd95ce8f3d`.
Initial freeze = the dev-calibrated engine defaults (001 grid; 003/004 parity): gap 0.6+0.3,
τ 0.3, resync_k 2. Remaining for issue #6: calibration curve (rule 7), committed-results
`report` mode.

## 008 · 2026-07-09 · Protocol rule 7 live: calibration curve (issue #6 slice 5)

**What changed.** `amberfork-bench run` now publishes the reliability curve under the main
table: fork confidence binned vs empirical exact-hit rate, for exactly the confidence-bearing
arms. `Arm::predict` returns `Prediction { step, confidence }` — the aligner arms carry the
engine's `Fork::confidence` (the 005 formula); the baselines carry none, and none is invented
for them: a fabricated confidence on a baseline would put a decorative number in a published
table. Bins are five fixed-width intervals over [0, 1] (last closed), committed code
constants like the ±1/±3 windows — deliberately NOT the 005 spike's equal-count terciles
(data-derived edges shift with every fixture set, so curves stop comparing across runs, and
re-tunable edges hand a cherry-picker a knob) and NOT `bench/params.toml` material (reporting
shape, not an engine tunable). Empty bins publish as `—` / `rate: null`, never vanish (the
rule-4 ethos applied to bins); occupied bins carry hits/n with the Wilson interval;
abstentions carry no confidence and stay outside the curve — they are already the `no_pred`
rate on the same denominator. Results JSON = bench_schema 0.4.

**Check.** The dev tuning baseline reproduces bit-for-bit under the frozen config
(`8ebd95ce8f3d`): nw-lexical/resync **0.75 exact [0.41, 0.93], 0.88 ±1, 1.00 ±3, n=8**. The
dev-side curve (n=8 — read directionally, no claims): nw-lexical 1/2 in [0.0,0.2), 2/2 in
[0.2,0.4), 2/3 in [0.4,0.6), 1/1 in [0.8,1.0] — consistent with 005's hits-carry-higher-
confidence. The unplanned observation: **nw-structural is confidently wrong** — 0/4 exact in
the top bin, because its 0/1 cost turns any (kind, name) mismatch into a confidence-1.0 fork.
The factorial ladder now shows content earns not just accuracy but *calibration*: the
product's confidence separates hits from misses; the structure-only arm's does not separate
at all. Published-curve numbers come from the test split under the frozen protocol, not from
these dev n's. Remaining for issue #6: committed-results `report` mode.

## 009 · 2026-07-09 · Offline reproduction closed: committed results + `report` (issue #6 slice 6)

**What changed.** The last open loop in BENCHMARK.md's definition of done — "reproduces the
results table, offline" — is now a committed artifact plus a renderer, not a promise.
`bench/results/chimera_noise_seed42_dev.json` is the dev-split run on the real seed-42 noise
set under the frozen config (`8ebd95ce8f3d`), and `amberfork-bench report` re-renders it to
the published table with zero pair loading, zero engine work, zero fetch. Committing the
document also finally lands rule 1's "the split manifest is committed" in the repo: the doc
carries every pair's task key and dev/test assignment (opaque `whowhen_hand_*` ids only —
never GAIA text, audited before commit). The committed side is **dev, deliberately**: rule 2
seals the test split until a release tag, and a pre-release repo publishing test numbers
would be spending the test set to decorate a README.

**Design.** One document, one renderer: the results types moved to a `results` module, gained
`Deserialize` (schema 0.4 unchanged — same JSON bytes, `&'static str` fields became `String`),
and both `run` and `report` print through the same `render()`. The round-trip test makes the
guarantee explicit — `run`'s stdout and `report`'s stdout on the same document are asserted
byte-identical — and an insta snapshot locks the committed artifact's rendering in CI, so
either the document or the renderer drifting is a red test, not a stale table. A document
`report` cannot vouch for (missing, or a foreign `bench_schema_version` — checked before
shape, so the error names the actual problem) is trouble (exit 2), never a partial render.

**Check.** `cargo run -q -p amberfork-bench -- report` from a clean checkout prints the dev
baseline exactly as notebook 006/008 recorded it: nw-lexical/resync **0.75 exact
[0.41, 0.93], 0.88 ±1, 1.00 ±3, n=8** over baselines at 0.00 exact; README now carries the
table (verified byte-identical against the live render) with the dev-split caveat and the
one claim n=8 supports — the product's exact interval [0.41, 0.93] clears every baseline's
[0.00, 0.32]. Issue #6's slice plan is complete; next in the milestone is #7 (Mode A′) and
the #11 decision on a CI-visible parity pair set.

## 010 · 2026-07-09 · Mode A′ opens: the cross-system disclosure seam (issue #7 slice 1)

**What changed.** The harness can now honestly render a cross-system pair set. A pair manifest
may declare `cross_system: true` (promoted from the spike's throwaway `meta.cross_system` to a
first-class field, because it changes *which metrics are the headline*); the harness carries
that fact through to the results document (`bench_schema_version` 0.4 → 0.5, new `cross_system`
count) and, when any scored pair is cross-system, prints a disclosure banner and labels the
protocol `mode-a-prime` instead of `chimera`. The banner states the honest reading BENCHMARK.md
line 62-64 and notebook 002's decision C require: *cross-system references diverge from step 0,
so ±1/±3 are the metric of record and step-exact is not claimed.* This is the contract seam the
rest of #7 (converters, pair construction, `bench/fetch`) lands in — a converted or fetched
Mode A′ pair now has an honest home in the table before any of that machinery exists.

**Design.** The disclosure is *derived from the data, not asserted by the operator*: there is no
`--protocol` flag. A pair is Mode A′ iff its manifest says so, and the set's label follows the
count of such pairs among the scored split — a set cannot be mislabeled at scoring time. The
banner renders only when the count is non-zero, so a same-system chimera table is byte-identical
to what it was before the seam existed (the committed `chimera_noise_seed42_dev.json` regenerated
at 0.5 differs from its 0.4 self by exactly two lines: the version and `cross_system: 0` — every
arm score, CI, and calibration bin unchanged; the `report` snapshot never moved).

**Check (the number that justifies the disclosure).** On a hand-authored synthetic Mode A′ set
(two pairs: a CaptainAgent-style failing team vs a smolagents-style passing reference, rosters
diverging from step 0), the shipped aligner scores **0.00 exact but 1.00 ±3** — the cross-system
step-0 divergence collapses step-exact while the windowed metric holds, exactly the phenomenon
the banner discloses and notebook 002 predicted for cross-system gold. Same-system chimera is the
control (`cross_system: 0`, no banner). Full gate green (fmt/clippy/`cargo test --workspace`,
40 bench tests incl. the offline-reproduction snapshot). Next #7 slice: port the Who&When and
TapeAgents converters from spike Python to Rust so real cross-system pairs can be constructed
in-tree and fed through this seam.

## 011 · 2026-07-09 · TapeAgents reference adapter ported to Rust (issue #7 slice 2)

**What changed.** The *reference* side of a Mode A′ pair now has an in-tree home:
`amberfork_ingest::tape` converts a ServiceNow TapeAgents tape (Apache-2.0) into a canonical
[`Run`] plus a `TapeMeta` (GAIA `task_id`, gold `Final answer`, produced `result`), mirroring the
already-landed `whowhen` failing-side adapter. Ported from `spike/make_realpairs.py::convert_tape`.
The Who&When half was ported long ago (`amberfork_ingest::whowhen`), so this closes the "port both
converters" task notebook 010 left open — the two source adapters now exist side by side, and a
later slice can match a tape to a failing log by `task_id` and emit a `cross_system: true` manifest
that flows through the slice-1 disclosure seam.

**Two deliberate corrections to the spike, not a literal port.** (1) *Structured outputs, not a
blob.* Each tape node's body (everything past `kind`/`metadata`, peeled off with
`#[serde(flatten)]`) becomes a field-diffable `Payload::Object`, not the spike's
`json.dumps(body)` string — the canonical model has a typed payload the Python didn't, and the diff
engine field-diffs objects. (2) *Honest outcome.* The spike stamped `outcome: "pass"` on every tape
and filtered non-passers downstream; here `outcome = Pass` iff the produced `result` matches the
gold `Final answer` (trimmed/case-folded, GAIA's grading), else `Fail`, with one `normalize` helper
as the single source of truth and `TapeMeta::is_success()` the pairing filter. A run never claims a
success it didn't achieve. A non-object `task` block degrades to no task (an `object_or_none`
deserializer) instead of failing the parse — the crate's forgiving-loader ethos.

**Check.** 6 unit tests (`crates/amberfork-ingest/tests/tape.rs`); full gate green
(fmt/clippy/`cargo test --workspace`). The canonical round-trip guard earned its keep: a
contentless bookkeeping node round-trips with a *correct* `ContentAbsent` advisory from the loader,
so `PASS_TAPE` was made realistic (every node carries content, as real tapes do) and the empty-body
→ `None` case got its own focused test — the guard stays as strong as `whowhen`'s (identical run,
zero warnings). No committed benchmark number moved: this is an adapter + tests, no pipeline wiring
yet. Next #7 slice: pair construction — match tape ↔ Who&When log by `task_id`, filter on
`is_success()`, write the cross-system manifest the seam already reads.

## 012 · 2026-07-10 · Cross-system pair construction — raw data to the honest table (issue #7 slice 3)

**What changed.** The join between the two source adapters now exists: `amberfork-bench
build-pairs --tapes DIR --logs DIR --out DIR` converts each TapeAgents tape (reference side,
`amberfork_ingest::tape`) and each Who&When log (failing side, `amberfork_ingest::whowhen`),
matches a *successful* tape to a failing log on their shared GAIA `task_id`, and writes the
`pair_*.json` + `a_*`/`b_*` triples the slice-1 disclosure seam already reads. This is the Rust
successor to `spike/make_realpairs.py` and the last construction piece Mode A′ needed — a real
cross-system pair now has an in-tree path from raw upstream data all the way to the honest table,
no Python in the loop. The two adapters landed in slices 1–2 (notebook 010/011); this slice is the
seam between them.

**Design — a pure core in a thin I/O shell.** The intellectual content is
`build::match_pairs(references, failings) -> BuildOutcome`, a pure function with zero filesystem
contact: it sorts both sides by stem, indexes failing logs by `task_id` (lowest-stem wins on
collision), and emits one pair per eligible reference. Six unit tests pin the *algorithm* —
matching, the gold carried through from the failing side, determinism under shuffled input,
collision resolution — without touching disk. Dir-reading and file-writing wrap it. The build
lives in `amberfork-bench` (not `amberfork-ingest`) on purpose: it produces the manifest only
`load_pairs` reads, so keeping it here lets one end-to-end test round-trip **build → write →
`load_pairs` → score** inside a single crate — the strongest guard against the writer's manifest
shape and the reader's drifting apart. (The reader and writer keep separate serde mirrors of the
pair contract; the round-trip test bridges them, so a field-name drift is a red test.)

**Three honesty boundaries, same ethos as the loader.** (1) *A tape earns reference status.* The
spike hardcoded `pass` and filtered late; here a tape anchors a pair only if `is_success()` **and**
it names a `task_id`, else it is a counted, named drop (`unsuccessful` / `missing-task-id` /
`no-failing-match`) on stderr — never a silent skip. (2) *A failing log must offer a usable fork.*
Only a log whose gold resolves to `GoldStep::Valid` becomes a failing candidate; gold-less logs are
counted (`logs_without_gold`), not paired. (3) *Strict inputs, honest zero.* A malformed source
file is a hard `BuildError` (exit 2 — the operator's raw data on their own disk, theirs to fix
loudly), unlike `load_pairs`' tolerance for a bad *committed* set; but building zero pairs is a
legitimate outcome (raw sources may not overlap on `task_id`), so it exits 0 with a loud count, not
a failure.

**Check.** 6 pure unit tests + one end-to-end (`tests/build_cli.rs`): synthetic *raw* tape +
Who&When JSON (a 6×7 arithmetic task, hand-authored fiction under `CARGO_TARGET_TMPDIR` — nothing
benchmark-derived committed, notebook 001/T30) → `build-pairs` builds exactly one pair (the losing
tape a counted drop) → the manifest carries `cross_system: true`, `gold_step: 2` → `run` on the
generated set prints the Mode A′ banner and the results document records `protocol: mode-a-prime`,
`cross_system: 1`. Full gate green (fmt / clippy `-D warnings` / `cargo test --workspace`, 22 test
groups). No committed benchmark number moved: this is a generator + tests, and real pairs stay
uncommitted. `amberfork-align` untouched, so the quantitative parity gate does not apply. What
remains on #7 is acquisition (a `bench/fetch` step to pull the gated upstream tapes/logs the
generator consumes) and the separate #11 decision on a CI-visible sanitized parity set — the
*construction* machinery is now complete.
