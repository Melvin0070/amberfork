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

## 013 · 2026-07-10 · The parity gate goes CI-visible: sanitize preserves the number (issue #11)

**Decision (founder, 2026-07-10).** Commit a GAIA-sanitized **dev-split** chimera set so the
fork-localization number is guarded by CI, not only by operator discipline. Chosen over the
aggressive-redaction and decline-and-document options after the deciding experiment below. The
issue itself pre-registered the experiment: "sanitize, re-run the dev baseline, and compare
before deciding."

**The number survives sanitization — exactly.** N=4 phrasing redaction (replace any run of ≥4
consecutive question tokens, wherever it appears in step content, with per-question hash
placeholders; boundary-redact the answer; hash the `task` field) leaves nw-lexical/resync
**bit-identical**: dev **6/8 = 0.75 [0.41, 0.93]**, all **15/20 = 0.75**, ±1 0.88/0.90, ±3
1.00 — matching the notebook 006/009 baseline arm-for-arm. Baselines stay at 0.00. The one
perturbation is a single pair crossing the [0.2] *calibration* bin edge (fork confidence is a
continuous function of the fork step's sync cost, which placeholders nudge); localization — what
the gate asserts — is untouched. Mechanism: the aligner localizes where the token stream
*diverges*, and deterministic substitution applied identically to both chimera sides preserves
prefix-match / tail-divergence. Confirmed invariant even under aggressive bag-of-content-word
redaction (still 0.75) — the number measures structure, not vocabulary, exactly as a
controlled-injection localization test should.

**Two stages, both load-bearing (the non-obvious part).** A single naive pass fails two ways:
- *Order.* `reword()` noise is added during `make_pairs`. Sanitizing pairs *after* generation
  redacts the failing side's noised prefix and the reference's clean prefix **differently**,
  breaking alignment symmetry: measured **0.75 → 0.55**. Fix: sanitize canonical logs *before*
  `make_pairs`, so placeholders bake into the prefix and reword drops tokens uniformly.
- *Altitude.* Canonical sanitization redacts each log against *its own* question, but a chimera
  splices log Y's tail onto log X's prefix — so X's question phrasing reappears through Y's real
  tail content (caught in the committed set: `('the','dog','genome','was')` from `hand_32`). A
  per-log sanitizer structurally cannot see this; the committed artifact is the *pair*, so it
  must be swept against *both* source questions. The sweep runs on already-canonical-sanitized
  pairs, where the prefix is clean (re-redaction is a no-op) and only post-fork tail residue is
  touched — so the number holds (0.75) and residue drops to **0**.

`spike/sanitize_gaia.py` exposes both stages (`canonical` / `pairs` subcommands); the committed
fixture reproduces **byte-for-byte** from `convert_whowhen → sanitize_gaia canonical → make_pairs
→ sanitize_gaia pairs`, seed 42.

**Residual, recorded not hidden.** No ≥4-token run of any question survives (longest = 3), and no
boundary-matched answer survives, but **~86% of a question's individual content words still appear
scattered** across the agent's own reasoning — never as a reconstructable phrase. Whether that
clears GAIA's "no crawlable resharing" is a licensing judgment; the founder accepted it for a
dev-only, provenance-noted, MIT-sourced fixture. This is why the pairs carry hash placeholders,
not natural text.

**What shipped.** `bench/fixtures/chimera_noise_seed42_dev/` (8 pairs + README with provenance,
license notice, and the re-runnable audit recipe); test side stays out (rule 2, tuning-on-test).
`chimera_parity.rs` loses `#[ignore]` and now pins the dev baseline in CI: `exact ≥ 6/8`
(= the ≥0.70 floor at n=8; measured 6). An `amberfork-align` change that tanks parity is now a
red CI, not a silent pass caught only by the local-regen discipline. `DiffParams::default()` in
the test equals the frozen bench config `8ebd95ce8f3d` (bench unit test pins that). Local gate
`cargo test -p amberfork-align --test chimera_parity` is green; the `--ignored` regen path is
retired.

## 014 · 2026-07-10 · Correction: the 0.75 headline is a lucky seed; lead with the ±3 window

**What prompted it.** Hardening the just-shipped parity gate, I widened it past seed 42 and
measured seeds 43 + 44 through the identical two-stage pipeline at the frozen τ=0.3. The
exact-hit rate is strongly **seed-sensitive**, and the committed headline (seed-42 **0.75**) is
the most favorable of the three.

**Numbers (fixed τ=0.3, frozen config `8ebd95ce8f3d`).** Dev split, per seed → aggregate:
- exact: seed42 **6/8 (0.75)**, seed43 **2/7 (0.29)**, seed44 **6/10 (0.60)** → **14/25 (0.56)**
- ±1: 0.72 aggregate · **±3: 25/25 (1.00)** aggregate
- All-split, n=60: exact **0.52**, ±1 0.72, **±3 0.95**. Baselines stay at 0.00 exact, ±3 ≤ 0.40.

This is **old-alongside-new** (rule 3): the seed-42 dev number is unchanged and reproduces
bit-for-bit; what changes is the honest *framing*. And it is not a new discovery so much as a
surfacing — notebook 002/005 already put the cross-seed exact mean at ~0.48–0.52; the fault was
that the committed README + gate led with seed 42's exact in isolation. For a repo whose whole
pitch is honesty-as-the-impressive-part, leading with the lucky seed was the one soft spot a
sharp reviewer catches first.

**Why exact wobbles but ±3 holds.** The controlled fork is a real content boundary; the aligner
reliably lands *near* it (±3 = 0.95–1.00 everywhere), but pinning the *exact* step depends on how
the benign-noise rewording/retry draw of a given seed reshapes token overlap right at the seam.
So exact is the seed-fragile metric and the window is the stable capability — which is the honest
thing to headline.

**Decisions (founder, 2026-07-10).** (1) *Publish the window.* README now leads with "localizes
within 3 steps 100% of the time (dev, n=25) vs the best baseline's 0.40", presents exact as
seed-sensitive (0.56 aggregate), and keeps seed-42's `report`-rendered table as the committed
reproducible slice. (2) *Gate on all three seeds, per-seed baselines.* `chimera_parity` now pins
seed42 ≥ 6/8, seed43 ≥ 2/7, seed44 ≥ 6/10 (25 dev pairs); the gate can no longer rest on one
lucky draw. Seeds 43/44 dev sets committed under `bench/fixtures/chimera_noise_seed{43,44}_dev/`,
same GAIA-sanitization + provenance, residue 0.

**Also corrected.** The README's Who&When source link said `mingyin1/…`; BENCHMARK.md, the
converter, and notebook 001 all standardize on the MIT source `ag2ai/Agents_Failure_Attribution`
— aligned the README to match.

## 015 · 2026-07-10 · Acquisition closes the Mode A′ pipeline: `fetch` at pinned commits (issue #7 slice 4)

**What changed.** The step notebook 012 left open now exists: `amberfork-bench fetch` pulls the
two raw sources `build-pairs` consumes — TapeAgents GAIA tapes (`ServiceNow/TapeAgents`,
Apache-2.0, 8 files) and Who&When logs (`ag2ai/Agents_Failure_Attribution`, MIT, 184 files) —
from GitHub at **pinned commits** into `bench/data/` (gitignored), in exactly the layout
`build-pairs --tapes/--logs` reads. The whole Mode A′ path — acquire → construct → disclose →
score — is now one binary, no Python and no hand-downloading in the loop. BENCHMARK.md's
"`bench/fetch` script" line updated to name the real subcommand.

**Design.** Reproducibility comes from the *pin*, not checksums: content addressed by
`(repo, commit, path)` is immutable on GitHub, and the file list itself is read from the git
tree at that same commit — so bumping a pin is a reviewed manifest edit, and a truncated tree
listing or an empty prefix match is a hard error, never a silently partial cache. The network
sits behind one blocking-`GET` seam (`ureq` — the tokio quarantine holds; a manifest, tree
filter, path mapping, and skip-vs-download logic are all pure or fake-drivable and tested
offline; CI never touches the network). Files write temp-then-rename so the skip-if-present
resume check can trust that a present file is a whole file; upstream paths are component-checked
so listing content can never write outside the cache. Licensing is in the contract: each source
carries its license + a "local benchmarking only, never commit" notice printed before any bytes
move, and the cache self-describes via `bench/data/provenance.json` (repo, commit, license,
count per source). HAL traces stay out of the manifest deliberately: no in-tree adapter consumes
them yet (encrypted-zip acquisition + an adapter are one future slice), and fetch serves only
what `build-pairs` reads today.

**Check.** 10 offline tests (manifest well-formedness, URL shapes incl. the literal `&` in
`Who&When/`, tree filtering, truncation/empty-match refusal, traversal rejection, fake-driven
download/skip/error paths, provenance record) + one `#[ignore]`d live test (fetches the 8 pinned
tapes, strict-parses each through `tape::convert_file`). Operator path exercised for real:
`fetch` landed 8 + 184 files; a re-run downloaded 0 (all cached); `build-pairs` on the fetched
cache strict-parsed **all 192 real files with zero failures** and built **4 cross-system pairs
(the 4 unsuccessful tapes are counted drops, 0 logs without a usable gold step)** — bit-matching
the spike's `make_realpairs.py` n=4 from notebook 001. No committed benchmark number moved; the
real pairs stay uncommitted. What remains on #7 is the closing slice: score the real pair set
through the disclosure seam and write the honest Mode A′ table (windowed metrics of record,
short-log caveat from notebook 002).

## 016 · 2026-07-10 · Mode A′ scored on real pairs: an honest null closes issue #7 (slice 5)

**What changed.** The slice-1 disclosure seam met real data: `run` on the 4 fetched-and-built
cross-system pairs under the frozen config, and the results document is now committed —
`bench/results/mode_a_prime_realpairs_all.json` (identifiers only: pair names + tape stems, no
GAIA content) with snapshot test `report_committed_mode_a_prime`, so the Mode A′ table
re-renders offline, banner included, exactly like the chimera one. The run also surfaced an
honesty bug, fixed test-first: `run`'s stderr diagnostic introduced *every* set as "chimera
protocol" — hardcoded, contradicting the artifact's own `mode-a-prime` label. The line now
follows the computed protocol and the end-to-end Mode A′ test pins it.

**The number (frozen `8ebd95ce8f3d`, τ=0.3, n=4, `--split all`).** The stable hash assigns
2 dev / 2 test; params have been frozen since notebook 007 and no tuning ever touched these
pairs, so scoring the test-assigned side is rule-2-legal — disclosed regardless.
- nw-lexical/resync (shipped): exact 0.00 · ±1 0.25 · **±3 0.50 [0.15, 0.85]**
- pos-lexical ±3 0.50 · nw-structural/resync ±3 0.25 · **random ±3 0.75 [0.30, 0.95]**

Read honestly: **the engine does not beat random here.** At n=4 the Wilson intervals all
overlap heavily, so no arm separates from any other; what this slice delivers is the pipeline
and the disclosed limit, not a capability claim. Calibration at n=4 is uninformative (0 exact
hits spread across bins) — do not read the bins.

**Why this is the expected result, not a surprise.** The lexical and random arms replicate the
spike arm-for-arm (notebook 002 Q3, same 4 tape tasks: lexical/tf-idf ~50% ±3, random ~74% ±3)
— the Rust engine and the throwaway Python agree on real data, which is the validation this
slice actually adds. The mechanism was pre-registered as a threat before any measurement:
(a) these runs are 7–10 steps, so a ±3 window covers most of the run and random is strong;
(b) cross-system gold is murky — the annotated `mistake_step` is a weak target when a
different system legitimately diverges from step 0 (BENCHMARK.md threats 1–2, notebook 001).

**What it means going forward.** (1) Controlled-injection chimera stays the primary protocol;
Mode A′ ships as a disclosed, reproducible limit — the demotion decision 3 pre-registered,
now with its in-tree number. README gained a Mode A′ subsection saying exactly that. (2) The
spike found embeddings were the *only* arm reaching 100% ±3 on these pairs — cross-system
alignment is the one measured niche where embeddings beat lexical; if ONNX/T25 ever earns a
slice, its dev evidence trail starts here. (3) A Mode A′ that could headline needs longer
hand-crafted logs vs same-family references (HAL adapter + deliberate gold/metric design) —
future work, not v0.2.

**Check.** Full gate green (fmt / clippy `-D warnings` / workspace tests incl. the new
snapshot and the protocol-label assertion; spike tests). Determinism: re-running `run` on the
same cache reproduces the committed document **byte-for-byte**. Chimera artifacts untouched
(their snapshot never moved). This closes #7 — seam (010), adapter (011), construction (012),
acquisition (015), scored disclosure (here) — and with it the v0.2 milestone.

## 017 · 2026-07-10 · The seal comes off: first test-split reveal, at the v0.2.0 tag

**What prompted it.** The v0.2 milestone closed with notebook 016, which makes this the first
release tag — and protocol rule 2 says the sealed test split runs **once per release tag** with
frozen params. This entry is that run. Params are the same bytes frozen since notebook 007
(`bench/params.toml`, sha256 `8ebd95ce8f3d…`); no tuning has touched them since.

**Recipe audit first.** The full n=20 sets (seeds 42/43/44) were regenerated from scratch per
the committed recipe (`convert_whowhen` canonical → `sanitize_gaia canonical` → `make_pairs` →
`sanitize_gaia pairs`), and the dev subsets of all three regenerated sets are **byte-identical**
to the committed `bench/fixtures/chimera_noise_seed*_dev/` fixtures — every a/b/pair file. The
chain from raw upstream data to the published dev numbers reproduces exactly; the test pairs
scored below come from that same validated generation.

**The numbers (frozen `8ebd95ce8f3d`, τ=0.3, `--split test`, n=35: 12+13+10 across seeds).**
- **nw-lexical/resync (the engine): exact 17/35 = 0.49 [0.33, 0.64] · ±1 0.71 [0.55, 0.84] ·
  ±3 32/35 = 0.91 [0.78, 0.97]**
- pos-lexical (best baseline): exact 0.00 [0.00, 0.10] · ±1 0.20 · ±3 0.49 [0.33, 0.64]
- nw-structural/resync: exact 0.03 · ±3 0.20 · random: exact 0.00 · ±3 0.29 [0.16, 0.45]
- Per seed, engine exact: seed42 9/12 (0.75) · seed43 3/13 (0.23) · seed44 5/10 (0.50) — the
  same seed-sensitivity shape as dev (43 hard, 42 favorable), on unseen pairs.

**Read against dev (rule 3: alongside, not instead).** Dev aggregate was exact 0.56 · ±1 0.72 ·
±3 1.00 (n=25, notebook 014). Test tracks it: exact 0.49 vs 0.56, ±1 0.71 vs 0.72, ±3 0.91 vs
1.00. No overfitting cliff — and the test exact lands almost exactly on the ~0.48–0.52
cross-seed mean notebooks 002/005 measured before any Rust existed. The one honest loss: the
±3 window gives up its perfect 1.00 — 32/35, with 2 misses on seed 43 and 1 on seed 44 (the
engine abstained on none: no-pred 0.00 in every seed). The claim that matters survives
with **non-overlapping intervals** (rule 6): engine ±3 [0.78, 0.97] vs best-baseline ±3
[0.33, 0.64], and engine exact [0.33, 0.64] vs all baselines ≤ [0.01, 0.15].

**Calibration (rule 7) is real on unseen data.** Engine reliability across confidence bins,
test aggregate: 0.08 (n=12) → 0.57 (7) → 0.60 (10) → 1.00 (4) → 1.00 (2) — monotone. A
high-confidence fork call on the test split was always an exact hit; the CI `--gate` idea
stands on something.

**Determinism.** Re-running `run --split test` reproduces each committed results document
byte-for-byte. The three documents are committed (`bench/results/chimera_noise_seed*_test.json`,
identifiers only — no GAIA content) with `report` snapshot tests, same contract as the dev and
Mode A′ tables.

**What it means.** The pre-registered protocol did its job: numbers tuned blind on dev
generalized to sealed pairs, and the published claim survives its first adversarial checkpoint —
"localizes within 3 steps" is now a **0.91 [0.78, 0.97] test-split number**, not a dev-only one.
README updated to lead with the test result and demote dev to context. Next reveal happens at
the next release tag, on a regenerated split, per rule 2.

## 018 · 2026-07-11 · The cross-seed headline becomes a committed document (issue #14)

**What prompted it.** Since the notebook-014 correction, the README *leads* with the pooled
cross-seed number (test ±3 0.91 [0.78, 0.97], n=35), but `report` could only render per-seed
slices — the aggregate lived in prose, computed by ad-hoc scripts at 014/017 time. A small
honesty seam: the headline was asserted, not reproducible.

**What changed.** `amberfork-bench aggregate --results <docs...> [--json-out]` pools results
documents into one, through the same renderer as `run` and `report`. The pooling is **exact**,
not approximate: every published rate already carries its `hits` and `n`, so the aggregate is
`sum(hits)/sum(n)` per metric per arm — the number a single run over the union would have
scored — with Wilson intervals recomputed at the pooled n (rule 6). Calibration bins pool the
same way (fixed-width edges are code constants, identical across documents). Coverage sums and
exclusions concatenate (rule 4); the split manifest concatenates with each record tagged by its
source document (rule 1 — pair names like `pair_00` repeat across seed sets, so provenance must
be explicit). The committed artifact is `bench/results/chimera_noise_multiseed_test.json`,
which names its three source documents by the sha256 of their exact bytes — the same identity
discipline `params` already had.

**What refuses to pool** (the refusals are the feature): fewer than two documents, the same
document twice (double-counting), an aggregate as input (sources-of-sources hides the real
inputs), and any mismatch in protocol, split, params sha256, or arm set — a pooled table over
mixed configurations would be exactly the dishonesty this closes.

**Design decision: pool committed documents, don't re-run.** The test pairs are not committed
(GAIA-derived), so a re-run-based aggregate could never reproduce from the repo alone; the
per-seed test documents ARE committed, and they carry every count the pool needs. Corollary
decision: results schema 0.6 adds the optional `sources`/`source` fields, and `load` accepts
{0.5, 0.6} rather than forcing a regeneration — the sealed v0.2.0 test documents were produced
once, at the tag (rule 2), and rewriting their bytes to bump a version string would betray
exactly what "sealed" means.

**Check.** The pooled table reproduces notebook 017 digit for digit (engine exact 17/35 = 0.49
[0.33, 0.64] · ±1 0.71 · ±3 32/35 = 0.91 [0.78, 0.97]; calibration 0.08 → 0.57 → 0.60 → 1.00 →
1.00, monotone). CI now rebuilds the committed aggregate from its committed sources and
byte-compares it (`aggregate_reproduces_the_committed_multiseed_document_byte_for_byte`), and a
snapshot pins its render, aggregate disclosure line first. No existing snapshot moved — the
sealed per-seed artifacts and their renders are untouched. Full gate green (fmt / clippy `-D
warnings` / workspace / spike). The dev-side n=25 aggregate stays prose-backed but is
regenerable offline too (dev fixtures for all three seeds are committed; `run` × 3 then
`aggregate`) — not committed, to keep the artifact count honest to what the README claims.

## 019 · 2026-07-11 · The sanitizer moves inside the gate (issue #17)

**What prompted it.** The 2026-07-10 audit's top finding: `spike/sanitize_gaia.py` was
provenance-critical — it certifies the redistributed `bench/fixtures/` pairs against GAIA's
no-resharing clause — yet lived in the "throwaway" spike dir, covered only by a Python test
outside `cargo test`. The least-protected code in the repo gated the most licensing-sensitive
artifact.

**What changed.** The two-stage sanitizer is now `amberfork-bench sanitize canonical|pairs`
(`crates/amberfork-bench/src/sanitize.rs`), a line-auditable port of the Python. **Byte parity
is the port's contract**, because the provenance README promises byte-identical regeneration:
that forced `pyjson.rs`, a writer byte-compatible with CPython's `json.dumps(obj, indent=1)`
(1-space indent, `ensure_ascii` escapes incl. surrogate pairs, integers only — floats are a
loud error), plus `serde_json`'s `preserve_order` feature workspace-wide so key order survives
the parse→serialize round trip. Even Python's `or ""` truthiness on `ground_truth` is ported
(and documented) rather than "fixed".

**Parity, measured.** (1) Canonical stage over the full raw set: 184 logs, Python == Rust ==
the historical on-disk artifact, byte-identical. (2) Pairs sweep on fresh `make_pairs` output,
seeds 42/43/44: 60 files each, Python == Rust byte-identical. (3) The recipe run through the
Rust stages reproduces **all 75 committed fixture files byte-for-byte**. The invariant suite
(space-count preservation, no residue, determinism, idempotence, the cross-log sweep) now runs
inside `cargo test`, alongside two new committed-artifact checks: a byte-exact parse→serialize
round trip over every fixture file, and a structural "sanitizer signature" test (valid pairs,
gold in range, task markers whose hash reappears as `q<sha8>` placeholders in step content).

**Collateral finding — two latent map-order dependencies.** `preserve_order` flushed out code
that *inherited* determinism from `serde_json`'s map being a `BTreeMap` instead of owning it:
the align cost model's object-payload serialization (an engine invariant! now canonicalizes
with explicitly sorted keys at every nesting level, nested-order test added) and ingest's
`sorted_keys` warning helper (named the promise, delegated the sort; now sorts). One accepted
render change: the CLI's one-line payload gist now shows keys in author order — the content
diff pane still compares sorted (`field_diff` always sorted explicitly).

**Retired.** `spike/sanitize_gaia.py` and `spike/test_sanitize.py` are deleted; the CI
sanitizer step is gone (covered by `cargo test`); the verify command drops to
`python3 spike/test_smoke.py` + the cargo gate. The fixtures README, CLAUDE.md, and
CONTRIBUTING.md recipes now name the Rust stages. `make_pairs.py`/`convert_whowhen.py` stay
Python: generation is spike-side, certification is not. Full gate green (fmt / clippy `-D
warnings` / workspace / spike smoke).

## 020 · 2026-07-11 · Sanity pass: the CLI meets a messy real-world trace (issue #15)

**What prompted it.** v0.4 slice 1: before handing out install links (#15), point `amberfork
diff` at a trace an external user would actually bring — not the chimera fixtures, not the
demo pair. Source: this machine's own Claude Code session transcripts (multi-MB JSONL agent
trajectories: giant tool payloads, embedded ANSI escapes, unicode, nested JSON-in-strings).
A ~50-line throwaway Python converter (scratchpad, per spike discipline) mapped two real
sessions to canonical v0.1: assistant text block → `llm` step, `tool_use` → `tool` step with
the `tool_result` paired back by id into `outputs`, user text → `other`; thinking blocks and
sidechains skipped. Conversion friction was minimal — the format's "at least one of
inputs/outputs" and forgiving extras made the mapping obvious; this becomes the slice-3 guide
example shape (a shareable one, not these private transcripts).

**What held (measured, release build, M-series darwin).** (1) Self-align on a real 133-step
run: converged, exit 0, 0.26s — the canonical invariant now confirmed on real data. (2) Real
vs real (133×123 steps, different sessions, same project): exit 1, fork at step 2 with conf
0.66 — the first genuine divergence — then honest model-moves and a later re-sync; 0.20s;
embedded ANSI in step content renders escaped, never styles the terminal. (3) `--json` is
valid JSON with `schema_version` under `meta`, alignment/fork exactly per contract. (4) Exit
codes all correct: 2 on truncated JSON / non-canonical `kind` / raw-JSONL-by-mistake, 1 on a
fork incl. the empty-run edge (0 steps vs 133 → fork at 0, conf 1.00). Serde's parse errors
are precise (`unknown variant `message`, expected one of `llm`, `tool`, `agent`, `other`` with
line/column).

**What broke (filed).** (1) **The converged summary line overclaims.** A 1000-step pair with
one perturbed step and one deletion (real content, stitched for scale) aligns correctly —
sync·cost-0.56 at the perturbation, a `model` move for the deletion, immediate re-sync, no
fork — and the per-step render shows the `[model-move]`. But the summary prints `converged —
identical through 1000 steps`: *identical* is false (and "1000" counts side A while B has
999). Converged-with-absorbed-divergence and identical are different claims; the one line
everyone reads must not flatten them (honesty-in-artifacts rule). (2) **The likeliest first
mistake gets a dead-end error.** Pointing the CLI at a raw exporter file (e.g. the `.jsonl`
transcript itself) yields `missing field `schema_version` at line 1 column 82` — correct,
but no pointer to `docs/trace-format.md` and no "this looks like JSONL" detection. The error
text is the product surface here; the guide can't fix a dead end.

**Scale datapoint (→ #16).** 133×123 real steps: 0.20s. 1000×999: 12.6s — ~60× the DP cells,
~60× the time, the documented O(n·m) tokenization cost measured in the wild. Tolerable at
1000 steps, not at 5000 (projected minutes). #16's trigger ("a real long-run trace feels
slow") isn't met yet; the curve is now on the issue so the trigger has numbers.

## 021 · 2026-07-11 · v0.4.0: the reveal that changed nothing (issue #15)

**What prompted it.** Tagging v0.4.0 (distribution + guide) triggers BENCHMARK.md rule 2's
one-test-look-per-tag, and the engine HAS changed since the v0.2.0 seal: static attribution
(#12), field-diff production (#13), and — the one that touches scoring — the notebook-019
canonicalization of object-payload serialization in the cost model. The dev parity gate
stayed green throughout, but dev is 25 pairs; the reveal is the test-side check of the same
promise.

**Provenance before scoring.** The regenerated pair sets (`spike/data/regen_noise_seed*`,
produced by the committed recipe during the notebook-019 parity work) were verified
byte-identical to the CI-pinned committed dev fixtures on every dev pair of all three seeds —
the scored test pairs come from the exact recipe the fixtures certify.

**Result: identical, to the digit.** Test split, frozen params (`bench/params.toml`
sha256:8ebd95ce8f3d), seeds 42/43/44, n=35 pooled: every arm, every metric, every calibration
bin matches the sealed v0.2.0 documents — full engine 0.49 exact / 0.71 ±1 / 0.91 ±3, best
baseline ±3 0.49, the lot. The per-seed documents differ from the sealed ones in exactly one
byte-range: `bench_schema_version` 0.5→0.6 (the #14 aggregate schema). Committed alongside
the originals as `bench/results/chimera_noise_seed*_test_v0.4.0.json` +
`chimera_noise_multiseed_test_v0.4.0.json` (rule 3: alongside, never swapped).

**Reading.** The post-v0.2.0 changes are scoring-invariant on test as well as dev — the
attribution/field-diff producers sit downstream of alignment, and the canonicalization
change reordered serialization without changing any cost. A reveal that reproduces the seal
is the protocol working: the number survives its second look untouched.

**Correction to 020 (2026-07-11, found while fixing #19).** The 1000-step scale fixture did
not have "one perturbed step": the probe script replicated the step list with shallow
copies, so the single mutation hit one shared object appearing at three indices (244, 500,
756). The honest count is 3 perturbed syncs + 1 deletion — the fixed footer's "4 absorbed
divergences across 1000⇄999 steps" is the engine counting my fixture more accurately than I
described it. No conclusion of 020 changes (the aligner absorbed correctly; the old summary
line's "identical" was still false — just false four ways instead of two).

## 022 · 2026-07-11 · Scale baseline made reproducible: the O(n·m) curve on committed data (issue #16)

**What prompted it.** #16 (cache LexicalCost tokenization) starts. Its guard clause says
benchmark before optimizing, but the trigger numbers on the issue came from notebook 020's
probe — private Claude Code transcripts through a throwaway scratchpad converter, neither
re-runnable. Slice 1 turns that one-off datapoint into a harness anyone can re-run, so slice
2 (the cache) has a pinned "before" and a permanent measuring stick.

**Method.** New criterion bench `crates/amberfork-align/benches/align_scale.rs`
(`cargo bench -p amberfork-align`). Long runs are stitched from the committed seed-42 dev
fixture: each side's runs concatenated in filename order (234 real a-steps, 188 b-steps),
cycled to the target length, steps deep-copied and re-indexed with `parent_idx` cleared
(Rust `Clone` is an owned deep copy, so the 020 shallow-copy trap can't recur). Deterministic
end to end — no randomness, no clock. Measures `diff()` (align + fork + field diffs +
attribution), release profile, 10 flat samples per size. The bench target sets `test = false`:
`cargo test` would otherwise execute it in the debug profile, where the top size takes minutes.

**Baseline (M-series darwin, release, criterion means).**

| steps per side | time | vs previous size |
|---|---|---|
| 125 | 195 ms | — |
| 250 | 769 ms | 3.9× |
| 500 | 3.05 s | 4.0× |
| 1000 | 12.18 s | 4.0× |

Each doubling costs 4.0× — the documented quadratic, now on committed data. Cross-check
against the wild numbers of 020: 12.18s here vs 12.6s CLI at 1000-scale, 195ms vs 0.20s at
~125-scale. The stitched-fixture harness reproduces the private-transcript curve, so the
fixture content is representative where it matters (payload serialization + tokenization
per cell).

**Reading.** This is the "red" for #16's slice 2: the prepare-once `CostModel` seam must
bend this curve, and by how much is now a measurement, not a claim. Caveat going in: the
cache removes per-cell payload serialization and tokenization but not the gestalt DP itself,
which is also per-cell — if gestalt dominates on 600-char-capped texts, the win will be
modest, and the after-run gets reported either way.

## 023 · 2026-07-11 · Prepare-once cost seam: the cache pays exactly its third (issue #16)

**What changed.** `CostModel` split at the per-step precomputation seam:
`prepare(step) -> Prepared` digests a step once, `cost_prepared` scores two digests, `cost`
stays as a one-off convenience with a provided default. `align()` now prepares each side once
(O(n+m)) and the O(n·m) matrix fill only scores. `LexicalCost::Prepared` is its token
sequence; the three other implementors (`StructuralCost` bench arm, `BlindCost` gate control,
`NameEq` test mock) carry trivial digests. Chosen over a memo table hidden inside
`LexicalCost` because the seam is what every future model needs anyway: tf-idf prepares a
term vector, an embedding model must embed per *step* — per cell would be absurd. The
`cost.rs` deferral note this issue was born from is deleted.

**Behavior invariance.** The full workspace suite passed untouched — chimera parity, the
self-align invariant, the hand-computed ratio pins. Same costs, same alignments, same forks.

**After (same harness as 022, criterion's own change report, p < 0.05 throughout).**

| steps per side | before (022) | after | change |
|---|---|---|---|
| 125 | 195 ms | 134 ms | −31% |
| 250 | 769 ms | 517 ms | −33% |
| 500 | 3.05 s | 2.05 s | −33% |
| 1000 | 12.18 s | 8.18 s | −33% |

**Reading — honest version.** A uniform 1.5× constant factor; the curve is still 4.0× per
doubling, quadratic as before. The issue title's O(n·m)→O(n+m) is achieved for tokenization,
but tokenization plus payload serialization was only ~a third of per-cell time — the gestalt
token DP is the other two-thirds, runs per cell, and this slice deliberately didn't touch it.
The 5000-step projection drops from ~5 min to ~3.4 min: better, not solved. If a real trace
at that scale ever shows up, the next lever is per-cell gestalt cost (intern tokens to
integer ids during `prepare`, so the inner DP compares `u32`s instead of `String`s — the seam
now exists to hold exactly that), same trigger discipline as #16 had: a real slow trace, not
a schedule.

## 024 · 2026-07-13 · The serving edge: amberfork serve, born loopback-only (issue #25)

**What changed.** `amberfork-server` (7th crate) in three slices: (0) loopback server over
the layout `Document` — one content endpoint (`/api/document`, D12), serialized+hashed once
at bind, ETag/304 for the UI's disconnect re-poll, Host-header allowlist on the whole router
(D6); (1) rust-embed bundle + SPA fallback + crates.io packaging (D7/D13); (2)
`amberfork serve <bad> --against <good> [--port] [--open]` with the pinned terminal handoff —
`ViewModel::headline()` lives in layout so serve's terminal line and #26's web header print
the same string. tokio enters exactly twice: the server crate and the CLI's one `block_on`.

**Decisions that will outlive the code.**
- *The guard wraps the router, not routes.* The Host check is a `.layer` on everything, so
  slice 1's SPA fallback was born behind it — verified by a foreign-Host-on-unknown-route
  test that survived the fallback landing unchanged.
- *`/api/*` never falls back to index.html.* A typo'd endpoint 404s loud instead of handing
  `fetch()` HTML to parse.
- *Bundle check precedes bind; ingest precedes both.* Pinned by a PAIR of CLI tests: same
  invocation, unreadable trace → typed ingest error; valid pair → bundle-missing message.
  Order proven by which error you get, with stdout empty in both.
- *The missing-bundle test uses a committed empty fixture, not the real `ui-dist/`* —
  asserting on the real one starts lying the day someone builds the UI locally.
- *Port default is `:0`* (OS-assigned, can't collide); `--port` pins one and busy is a typed
  error naming the port — reconciles the doc's "pick a free port" with "port-in-use → clear
  error" without a port hunt.

**Learned the hard way.** rust-embed 8's `Metadata` has no `mimetype()` (guessed API — the
compiler said no); MIME comes from `mime_guess` directly. Axum handlers need `A: 'static` on
the embed generic. `include = ["src/**", "ui-dist/**"]` genuinely overrides gitignore at
package time — proven with `cargo package --list`, not assumed.

**Coverage honesty.** The happy-path e2e (spawn `serve`, GET through a real bundle) does not
exist yet: a dev build has no bundle by design, so it lands with #28's release smoke against
the real artifact. Serving behavior is covered at the lib layer (11 integration tests over a
bound listener, raw-TCP client); the CLI layer pins startup order and error wording only.
Port-in-use is lib-tested, not CLI-tested (unreachable in a dev build — bundle check first).

## 025 · 2026-07-13 · The web painter lands: `ui/`, toolchain + header (issue #26 slice 0)

**What changed.** New `amberfork-ui` crate (Leptos 0.8.20) outside the cargo workspace — the
first slice of the browser hero. It fetches the real `Document` from `/api/document` and
renders the header for real: neutral `amber⑂fork` logo, pair identity with roles, the verdict
as the protagonist (`⑂ forked at step 11 · conf 0.86` / `converged — identical through N
steps`), and the step-count/schema meta. Body below is a labelled-but-empty canvas region;
the spine and the amber fork itself are slice 1. Verify path: 4 host-side SSR string-render
tests + fmt + clippy on **both** Leptos backends + `trunk build`.

**Decisions that will outlive the code.**
- *Contracts first: the UI path-deps `amberfork-layout` for the `Document`, never a mirrored
  DTO.* Confirmed layout → model → serde is pure (no tokio/net/fs), so it compiles clean to
  wasm32 — the browser deserializes into the exact server type, so a schema mismatch is
  impossible by construction. The one duplicated string is the route path `/api/document`
  (a URL, not the schema); depending on `amberfork-server` would drag tokio/axum into wasm.
- *`ssr` is the default feature, `csr` is trunk-only.* Leptos's two reactive backends are
  mutually exclusive, so the split IS the test story: `cargo test` renders components to
  strings host-side with zero flags (D16's "plain cargo test"); `index.html` pins
  `--features csr --no-default-features` for the wasm build. The render is a pure function of
  the document (lib.rs); the fetch is the one impure edge, quarantined to the `csr` binary —
  the same sync-core / IO-edge discipline the engine crates follow.
- *The header carries ZERO amber (founder-confirmed).* Reading "amber exactly twice (fork +
  path)" and "verdict is the protagonist, never faint" together: the verdict earns prominence
  through the `text` token, mono, and position adjacent to the pair — not color. Amber stays
  saved for the canvas ignition. `faint` is decorative-only here (the `vs` separator); role
  labels are `muted` (the readable-text floor, DD4).
- *Never a blank page (D20) is static, not wasm.* The loading / `<noscript>` / wasm-error
  states live in `index.html` so they exist before wasm boots; the csr entry removes the boot
  node once it's alive, and a global error handler flips loading→error if boot never runs.
  The SSR test asserts on `include_str!("../index.html")` — the shell is a file, not a view.
- *`ui/` is its own workspace root, excluded from the parent (D4).* The wasm-free verify
  ritual is preserved by construction: the root's `--workspace` commands can't reach an
  excluded crate. `ui/` gets its own CI job (fmt + clippy ssr/csr + test + trunk build).

**Learned the hard way / measured.** Leptos 0.8 SSR string render = `Owner::new().with(|| view!
{…}.to_html())` — no DOM, no browser (guessed from the reactive-owner model; the compiler
agreed first try). `trunk build` auto-fetched wasm-bindgen 0.2.126 to match the lockfile.
Bundle so far: **477 KB gzipped** (debug, no `wasm-opt`) against the ≤1 MB budget — headroom
for release `-Oz` + latin-subset woff2 fonts (both slice 4).

**Coverage honesty.** The actual browser render (wasm mount → shell → fetch → header) is NOT
yet exercised — it's the manual `/qa` step the issue scopes pre-release, and the end-to-end
`serve`-through-a-real-bundle path lands with #28. This slice is verified host-side (SSR +
clippy on the shipping wasm build) and by a real `trunk build`; the pixels are unverified.

## 026 · 2026-07-13 · The amber fork lands: shared-spine canvas (issue #26 slice 1)

**What changed.** `amberfork-ui` grows its hero — the alignment canvas. Side-by-side runs (A
reference | B observed) on one shared vertical timeline: sync rows recede (`muted`), the fork
row and every downstream row glow `amber` with the `⑂`/`✗` gutter glyphs + a dashed non-color
cue, and the `[FORK · conf 0.NN]` tag reuses the terminal painter's exact wording. Rendering is
split so text stays selectable — DOM rows in a fixed side-by-side grid over a narrow SVG spine
overlay (faint rail + amber divergent-path segment + fork node), both keyed to one `ROW_H`
constant. The header's live `#fork` anchor now lands on a real fork row. Verify: 11 new
host-side tests (SSR string render + pure geometry invariants) atop slice 0's 5, fmt + clippy on
both Leptos backends, `trunk build` (536 KB gz, < 1 MB budget), and an eyeballed static preview.

**Decisions that will outlive the code.**
- *Geometry is a pure function, tested independent of the paint.* `spine_geometry(rows)` maps
  semantic rows to y-coordinates; the invariants (y monotone + evenly spaced, `fork_y` on the
  fork index, `None` when converged) run in plain `cargo test`, no browser. The SVG and the DOM
  grid never measure each other — they share `ROW_H`, so alignment holds by construction.
- *SVG spine + DOM text, not one or the other.* Honors "DOM/SVG, text selectable" AND gives a
  real drawn timeline for the ignition beat (slice 4) to animate. The amber path is a literal
  stroke from the fork down, not a border trick.
- *An absent side renders empty — the gap IS the break.* A gap move shows one column and leaves
  the other blank; "a divergence visibly breaks the alignment" is the empty cell, not prose.
- *Fixed side columns, left-anchored (not `1fr 1fr`).* The pixels showed `1fr` stretching the
  two runs to opposite edges with a dead band — no longer a comparison. Bounded columns +
  `fit-content` rows keep A|B adjacent, hug the fork's dashed band to its content, and leave the
  right open for the attribution pane (slice 2).
- *The web UI is the first surface to see a cut slot.* The CLI reads the view directly and never
  sees envelope truncation; the canvas renders `SlotText::truncated` with the project's `…` mark.

**Contract completed.** `amberfork-layout` now `pub use`s the four model types embedded in the
`ViewModel`'s public fields (`StepKind`, `MoveKind`, `Outcome`, `Warning`). Latent gap: a
consumer depending only on layout could not name what `StepView::kind` etc. are. Source-level
re-export — no wire/schema change, `schema_version` untouched, root workspace stayed green.

**Coverage honesty.** The wasm mount → fetch → render path is still the manual `/qa` step (#28);
this slice is verified host-side + by a static preview built from the true SSR output and the
real stylesheet (colors confirmed against tokens via computed style, no console errors). The
live browser pixels — hover, scroll, real font metrics — are unverified until fonts + `/qa`.

## 027 · 2026-07-13 · The attribution pane + default fork selection (issue #26 slice 2)

**What changed.** The composition closes: header + a two-pane body (canvas flexes, a fixed 320px
attribution pane on the right). The pane renders `AttributionView` as a description list in DR5
reading order (mode → origin → propagation → confidence) — the parts the terminal flattens to one
footer line, now separate elements; when there is no attribution it still speaks (converged → "no
fork to attribute", forked-but-unlocalized → its own line), so the pane is never dead. The fork
pair is selected by default so the app opens on the answer. Verify: 9 new host-side tests (25
total), fmt + clippy on both backends, `trunk build` 555 KB gz, two-pane pixels eyeballed.

**Decisions that will outlive the code.**
- *The pane reads the answer, not the rows.* `Attribution` takes `Option<AttributionView>` +
  `Verdict`, nothing else — it never touches the canvas rows. Attribution is a statement ABOUT
  the divergence, so no amber and no red/green live in it (the red/green field-diff card is #27's
  job, confined there); an SSR test asserts the pane carries none of the canvas amber hooks.
- *Selection is a class separate from the amber role.* `row--selected` (raised surface + hairline
  via inset box-shadow, no layout shift) rides on the same `<li>` as `row--fork` for the
  default-selected fork, but keys to neutral tokens only — so "selection is never amber" (DD2)
  holds even where the selected row IS the amber fork (computed bg confirmed `raised`, not amber).
  Slice 3 makes selection signal-driven; here it is fixed to the fork index by construction.
- *Default selection = the fork.* The app opens answering DR5's reading order — no dead pane, no
  fold-hidden fork.
- *Canvas-only horizontal scroll, done right.* Dropped the forced `min-width:1024` on the track
  (once the 320px pane took its share it scrolled the canvas needlessly); the bounded rows
  left-anchor on the dotted field and the canvas alone scrolls only when content truly exceeds
  it. Side columns tightened to 300px so the two runs + the fork tag fit beside the pane, and the
  `[FORK · conf]` tag is `nowrap` so it never breaks the fork's single 30px row.

**Coverage honesty.** Same as slice 1: wasm mount → fetch → render is the manual `/qa` step (#28).
Verified host-side + a static preview from the true SSR output and the real stylesheet (selection
bg = `raised`, pane border = `hair`, values = `text` via computed style; no console errors).
Moving the selection, keyboard nav, and the disconnect banner are slice 3.

## 028 · 2026-07-13 · The canvas comes alive: selection, keyboard nav, auto-scroll (issue #26 slice 3a)

**What changed.** The canvas becomes an interactive listbox. Selection is a signal (default = the
fork); click or Enter commits it, and the highlight is the neutral raised+hairline frame — a class
kept separate from the amber role, so selection is never amber even on the fork. Roving tabindex:
exactly one row is `tabindex=0`; arrows move the focus cursor (clamped at the ends) without moving
the selection so navigating never thrashes the pane; the rows are `role="listbox"`/`option` with
`aria-selected`. focus-visible ring (a box-shadow, so it never overrides the fork's dashed cue) +
hover tint. The canvas is now the scroll container (canvas-only scroll — header + pane stay fixed),
and the fork auto-scrolls into the upper third on load. Verify: 3 new SSR scaffolding tests (28
total) + the whole interaction driven live in a real browser (`trunk serve` + a stub
`/api/document` + the browse skill). `trunk build` 623 KB gz.

**Decisions that will outlive the code.**
- *Selection follows Enter, not focus.* Arrows move a roving focus cursor; selection changes only
  on Enter/click. For a debugger where selection will drive the content-diff pane (#27),
  decoupling nav from selection avoids thrashing the pane as you arrow through — the issue's
  "Enter selects" made literal.
- *Selection is a class, proven neutral live.* `row--selected` keys to `raised`/`hair` only; a
  browser computed-style check on a selected SYNC row read `muted` text + `raised` bg — "selection
  is never amber" (DD2) holds as the selection moves, not just on the default fork.
- *The focus ring is a box-shadow, not an outline.* So it never overrides the fork's dashed amber
  `outline` — a focused fork keeps its non-color divergence cue.
- *The canvas owns the vertical scroll.* `body { height:100vh; overflow:hidden }` + the flex chain
  bounds the canvas so IT scrolls, not the page — header and pane stay fixed, and
  `scroll_into_view` + `scroll-margin-top:96px` land the fork in the upper third. The on-mount
  scroll is deferred one animation frame: an immediate scroll runs before layout settles (caught
  live — `scrollTop` stayed 0 until the RAF deferral).
- *Behaviour is verified by driving it, not by asserting scaffolding.* Host SSR tests pin the
  static contract (listbox/option, roving-tabindex initial state, `aria-selected`); the real
  click/keyboard/scroll behaviour was exercised against live wasm — the honest way to verify an
  interaction slice, closest to the `/qa` the issue defers.

**Coverage honesty.** The live drive used a stub `/api/document` + `trunk serve` (throwaway, not
committed); the shipped serve-through-a-real-bundle path is still #28. Interaction is proven in one
browser (Chromium) at a few viewports; cross-browser + real-font metrics are the pre-release `/qa`.
The disconnect re-poll banner is slice 3b (next).

## 029 · 2026-07-13 · The server-stopped state: disconnect re-poll banner (issue #26 slice 3b)

**What changed.** The browser now notices when the server that fed the view stops. After the
first load keeps the snapshot's ETag, a 5s interval re-polls the one content endpoint with a
conditional GET (`If-None-Match`); a healthy server answers with a cheap 304, and only a
*transport error* — the loopback process gone — reads as stopped. On the first dead probe a slim
`warning` banner docks at the bottom edge — `server stopped — restart: amberfork serve <bad>
--against <good>` with the REAL run ids — and the poll *latches off*. No spinner. Verify: 3 new
SSR tests (31 total, pure banner: copy + real names + `role=alert` + carries no amber hook + never
shown on the connected App), fmt + clippy on both backends, wasm compiles — and the whole thing
driven live against the real `amberfork serve` (demo refund traces): killed the server, watched
the banner appear, and proved the two behaviours only a live drive can prove.

**Decisions that will outlive the code.**
- *The pure/impure seam holds again.* `DisconnectBanner` is pure markup in `lib.rs` (SSR-tested,
  D16); the re-poll loop — the app's only ongoing I/O — lives in the `csr` binary (`main.rs`), the
  one impure edge. Same split as every prior UI slice: the thing the browser must do lives at the
  edge, the thing we can assert lives where a plain `cargo test` can render it.
- *Disconnect = a transport error, not a bad status.* ANY HTTP answer (even a 500) means the
  server is up; the probe treats only `send().await` failing as stopped. This is exactly the
  ETag/304 path `amberfork-server` was built for ("a strong ETag/304 pair is all the UI's
  disconnect detection needs") — cheap liveness, no re-download of the document each tick.
- *Latch, don't reconnect.* On loopback a dead fetch means the process is gone, and the server
  serves an *immutable* snapshot — so recovery is restart + reload, never a silent reconnect to a
  possibly-different diff. The banner stays and polling stops (proven live: 0 further polls over a
  real 15s window). A "reconnecting…" spinner would be a lie about what the state is.
- *Warning is not amber.* The banner speaks in `warning #F5A623`; amber is still spent exactly
  twice, both in the canvas. A system-status message is not a divergence. An SSR test asserts the
  banner carries none of the canvas amber hooks; the live computed style read `rgb(245,166,35)`,
  not amber's `rgb(255,122,26)`.
- *Real names over placeholders.* The restart command names the loaded runs (evidence-out rule),
  so it is paste-ready — the live drive showed `amberfork serve refund-bad --against refund-good`,
  pulled from the document, not a template.
- *Fixed, not in flow.* The strip is `position: fixed` so it annunciates without reflowing the
  canvas — no scroll jump when a terminal state arrives.

**Coverage honesty.** The live drive used the real `serve` binary over a dev bundle copied into
the (gitignored) embed folder — throwaway, restored after. The shipped serve-through-the-release
bundle + the ≤1MB gzip gate (needs `wasm-opt`, network) are the `ui/` CI job, still #28's pre-release
`/qa`. Proven in one browser (Chromium). Noted in passing: a pre-existing Leptos warning at
`canvas.rs:130` (slice 3a's auto-scroll RAF reads a `NodeRef` outside a tracking context; benign,
`get_untracked` would silence it) — a follow-up, not this slice.

## 031 · 2026-07-13 · The content-diff pane: red/green for the selected pair (issue #27 slice 1)

**What changed.** The attribution aside gains the ONE surface that spends red/green (DESIGN.md
containment): a content-diff card showing the *selected* row's field-level `-`/`+` evidence —
removed red `#FF5C5C`, added green `#46D39A`, nowhere else. Selection is lifted out of the canvas
up to `App` (one `RwSignal<Option<usize>>` the canvas commits and the pane reflects), defaulting to
the fork so the pane opens on the answer's evidence, never a dead zone. The card renders the diff,
or the pinned `no field changes for this pair — payloads identical on the wire` when the pair
matched, or nothing at all when nothing is selectable (a converged diff — the attribution
empty-state already speaks). New crate touch: none — `amberfork-ui` gains a `content_diff` module;
the enabling change is in the layout seam. Verify: `amberfork-layout` 20 host tests, `amberfork-ui`
38 SSR tests, parent `cargo test --workspace` + smoke + fmt + clippy `-D warnings` all green; the
live reactive re-render (click a different row → pane updates) is browser behaviour deferred to
`/qa` (#28), the same SSR-vs-live split as slices 3a/3b.

**Decisions that will outlive the code.**
- *Field diffs ride the aligned pair, not the fork.* The engine (`field_diff.rs`, #13) already
  diffs **every** synchronous pair; the layout was attaching the result to the `ForkRow` alone and
  silently dropping the rest. For "select any sync pair → its diff" to be honest, `field_diffs`
  moved from `ForkRow` onto `AlignedStep`, so `compute` attaches each row's own evidence and the
  pane reads `row.step().field_diffs`. This is what keeps the empty line truthful: it shows only
  when the engine genuinely found no change, never as a fork-bound fiction over a diverged
  downstream sync. The move also collapsed the envelope's fork special-case into `envelope_step`
  (one truncation path for all rows) and cost the CLI painter exactly one line
  (`fork.field_diffs` → `fork.step.field_diffs`). `DOCUMENT_VERSION` → `0.2` because the wire
  shape shifted.
- *Selection is lifted, not duplicated.* The canvas owned `selected` privately; the pane needs the
  same value, so it rose to `App` as the single source of truth. The canvas keeps its roving-focus
  cursor (a canvas-only concern) but no longer owns what is selected. No cross-pane wiring, no
  second signal to keep in sync.
- *Containment holds by construction, not by discipline.* Red/green exist only as
  `.content-diff-*` CSS classes, and those classes are emitted only by the `ContentDiff` component
  inside the aside. An App-level test splits the rendered HTML at the aside and asserts the canvas
  region carries neither class — so the "red/green only in the content-diff pane" rule is a
  compiled guard, not a convention someone must remember.
- *Color is never the only signal.* Each line carries a `-`/`+` glyph (grayscale- and
  colorblind-safe) and an `aria-label` naming the side in words ("removed …"/"added …"), so the
  meaning survives without the hue — the same redundancy rule the fork row follows (DR2/DD4).

**Coverage honesty.** The SSR host tests pin the static contract at the initial selection (the
fork by default); the `ContentDiff` unit tests preset the signal to arbitrary rows to exercise the
"any pair" logic without a browser, but the *live* re-render on click is genuine client reactivity
and goes to `/qa` (#28). The copy affordance (terminal unified format + repro command, issue #27's
evidence-out amendment) is deliberately held for slice 2 — it mixes a pure formatter with an impure
clipboard edge and earns its own review. Real-font metrics, reduced-motion visual, and the ≤1MB
gzip gate remain #28's pre-release `/qa` + `ui/` CI job, unchanged by this slice.

## 030 · 2026-07-13 · The fork ignites: the one expressive beat (issue #26 slice 3c)

**What changed.** The canvas gets its single motion (DESIGN.md §Motion): on load the amber
*ignites at the fork and flows down the divergent path*. Three coordinated sub-animations inside
a 0–380ms envelope (medium, ease-out) — the fork node pops (`fork-ignite`), the amber spine
segment draws downward from the fork (`path-flow`, a `scaleY(0→1)` transform anchored at the fork
end), and the divergent rows kindle to full amber (`row-kindle`). It is **pure CSS keyed to the
existing classes**, wholly inside `@media (prefers-reduced-motion: no-preference)`; zero Rust
change — the whole beat lives at the presentation layer. Verify: the 31 host tests are unchanged
and green (the static end-state they pin IS each keyframe's `to` state); driven live against the
real `amberfork serve` (demo refund pair, fork at step 05) — `getComputedStyle` confirmed all four
elements carry their animation, 8 animations run on the `.track` subtree, and a scrubbed 220ms
frame showed the path 65% drawn *from the fork down* with rows at 0.79 opacity. Closes #26.

**Decisions that will outlive the code.**
- *The beat is CSS, not JS — so it needed no new impure edge.* Every prior UI slice split a pure
  render (SSR-tested) from an impure browser edge (`main.rs`). This slice adds neither: the
  animation is declarative, the reduced-motion gate is a media query, and the render is
  byte-identical. So the 8 SSR tests that pin the lit end-state ARE the reduced-motion contract —
  that end-state is exactly what `reduce` shows, because each keyframe's `to` equals the default.
- *Draw the path with a transform, not a dash length.* `scaleY(0→1)` with `transform-origin` at
  the fork end grows the amber line downward without the render computing the segment's pixel
  length — geometry stays a pure Rust function ([`spine_geometry`]) and "flows down the path"
  reads literally. `transform-box: fill-box` confirmed resolving live.
- *One beat, spent once.* Node pop + path draw + row kindle are a single orchestrated moment
  (0–380ms), not scattered effects. No overshoot bounce, no blur halo — minimal-functional, so it
  doesn't read as AI-generated motion (the `frontend-design` discipline, subordinate to DESIGN.md).
  Amber is still spent exactly twice, both in the canvas; the motion introduces no new color.
- *Fires where the eye is, with no observer.* Slice 3a already auto-scrolls the fork into the
  upper third on load, so a plain on-mount CSS animation plays exactly where the fork sits — no
  IntersectionObserver needed for v0.5. When the scrubber lands (record milestone) ignition must
  re-fire *on scrub*, which will need a JS trigger; that is a future slice's seam, deferred here
  as a decision, not an omission.

**Coverage honesty.** Reduced-motion is correct by construction (the whole block is gated on
`no-preference`; every keyframe's `to` == the static default), but the live CDP media-emulation is
denied by the `browse` allowlist, so the reduced-motion *visual* confirm, real-font metrics, and
cross-browser all go to the pre-release `/qa` (#28) — same honesty as slices 3a/3b. The live drive
used the real `serve` binary over a throwaway `ui-dist` copy (restored after). The ≤1MB gzip gate
still needs `wasm-opt` (#28's `ui/` CI job).

## 032 · 2026-07-13 · Evidence-out: the copy affordance on the field-diff card (issue #27 slice 2)

**What changed.** The content-diff card gains a top-right **Copy** button. One click puts the
selected pair's field diff on the clipboard as the grayscale-safe terminal unified `-`/`+` format,
a blank line, then the re-runnable repro command (`amberfork diff <bad> --against <good>`) — the
DESIGN.md evidence-out rule (2026-07-12) made real, so a debugger pastes runnable evidence into a
bug report or PR, not a screenshot. The label confirms with `Copied ✓` for ~1.5s, then reverts.
This closes #27. New crate touch: none — `amberfork-ui` gains a `copy_text` formatter + a
`CopyButton`, plus a csr-only `web-sys` (Navigator + Clipboard) dep. Verify: `amberfork-ui` **43**
SSR tests (was 38: +3 `copy_text`, +2 button-render), the csr/wasm `cargo check` + clippy
`-D warnings` on both backends, parent `cargo test --workspace` + smoke + fmt + clippy all green;
**driven live** against the real `amberfork serve` (demo refund pair, fork at step 05) — the button
renders `Copy`, a click flips it to `Copied ✓`, and it reverts to `Copy` after the timer, all
confirmed by reading the live DOM.

**Decisions that will outlive the code.**
- *The copy text is a pure function; only the write is a browser edge.* `copy_text(diffs, bad,
  good)` is a total, SSR-unit-tested `String` builder — the exact bytes the clipboard receives are
  asserted by a plain `cargo test`, no browser. The two browser touches — `navigator.clipboard`
  and the reset `set_timeout` — are `#[cfg(feature = "csr")]` helpers that compile to no-ops under
  the `ssr` host build. Same pure-render/impure-edge split as the disconnect banner: the thing we
  can assert lives where `cargo test` renders it; the thing the browser must do lives at the edge.
  So the button's markup + label are SSR-pinned, and the copy content is pinned separately as a
  pure fn — the wiring between them is one obvious line.
- *The paste mirrors the terminal, verbatim.* The `-`/`+` lines match the CLI painter's fork block
  (`- path: value`), one-sided fields render only their present side, and a slot the envelope cut
  keeps its honest `…` — so the pasted evidence never reads a shortened payload as whole (D17). The
  repro verb is `diff`, not `serve` (it reproduces the *diff*), with the real run names in the
  observed-first / `--against`-reference order the disconnect banner already established.
- *The affordance appears only where there is evidence.* No button on the pinned empty line (an
  identical pair) or the no-selection state (a converged diff) — nothing to hand out, so nothing to
  click. An SSR test pins both absences.
- *Feedback without a new colour.* `Copied ✓` stays neutral (`--muted`→`--text`), never the diff's
  red/green and never amber: the confirmation must not spend the pane's one scarce pair of hues,
  which are reserved for the evidence itself. Buttons-at-rest are neutral by DESIGN.md; hover is a
  surface tint, focus-visible a hairline ring matching the canvas rows, and the transition is gated
  behind `prefers-reduced-motion`.

**Coverage honesty.** The live drive proved everything observable headless: the button renders, the
click flips the label reactively, and the timer reverts it — all read back from the live DOM against
the real `serve` binary (throwaway `ui-dist`, restored after). What a headless browser can *not*
prove is the clipboard's byte content: `readText` is blocked (`NotAllowedError`), and a bare
`writeText` probe is refused for lack of user activation — the click-driven write fires *with*
activation (the label flip confirms the handler ran) and will land in a real user's browser
(localhost is a secure context, a click is a gesture), but reading it back to assert the exact
bytes is a headed `/qa` step. So the clipboard-content confirm, real-font metrics, reduced-motion
visual, and the ≤1MB gzip gate all remain #28's pre-release `/qa` + `ui/` CI job — the same
SSR-vs-live honesty every prior UI slice drew. Noted again in passing: the pre-existing Leptos
warning at `canvas.rs:131` (slice 3a's auto-scroll `NodeRef` read outside a tracking context)
surfaced in the live console — benign, still a follow-up, untouched by this slice.

## 033 · 2026-07-13 · serve --demo: the zero-setup browser hero (issue #28 slice A)

**What changed.** `amberfork serve` gains a `--demo` mode: the same pair embedded in the binary
that `demo` renders in the terminal is handed to the local web view instead — no files, no cwd,
no network. `bad`/`--against` became `Option<PathBuf>` carrying `required_unless_present = "demo"`
+ `conflicts_with = "demo"`, so the parser enforces "exactly one of {`--demo`, `<bad> --against
<good>`}" and a wrong invocation is a clap usage error (exit 2) before any I/O. The terminal
hand-off carries over: `serve --demo` prints `DEMO_SERVE_HINT`, teaching the real
`serve <bad> --against <good>` — the exact analog of `demo`'s `DEMO_HINT`. First of #28's three
slices (**A** serve --demo · B the real bundle ships · C docs+hero). Verify: 3 new
`serve_demo_cli` integration tests + 1 `demo_pair` unit test, parent `cargo test --workspace` +
smoke + fmt + clippy `-D warnings` all green. No new crate.

**Decisions that will outlive the code.**
- *One embed site, made structural (design doc D7).* Extracted `demo_pair() -> (Ingested,
  Ingested)` — the single place the embedded `good.json`/`bad.json` are parsed; both `run_demo`
  and `run_serve` now source from it. The "serve --demo reads the same bytes as demo" identity is
  no longer a promise to keep — there is one copy that cannot drift, not two that might. The unit
  test asserts `demo_pair()` yields a *forking* pair, so the shared bytes are proven to be the
  real authored divergence, not an empty or degenerate one.
- *The parser owns the mode contract, not a hand-rolled `if`.* clap's `required_unless_present` +
  `conflicts_with` express "one mode or the other" declaratively; usage errors stay exit-2 and are
  phrased by clap before ingest runs. The two `.expect()`s in the file branch encode that parser
  invariant (clap already guaranteed presence) — a proof obligation discharged by the arg
  attributes, not a panic-on-bad-input.
- *serve --demo is a flag on serve; demo stays its own subcommand — deliberately.* D14 puts every
  long-running surface under `serve`, so the zero-setup entry to the *browser* is a mode of
  `serve`, while the zero-setup entry to the *terminal* stays the `demo` subcommand. Same bytes,
  two front doors, one loader.

**Coverage honesty.** A dev build has an empty `ui-dist/`, so `serve` on a clean pair reaches the
"web UI bundle missing" refusal *before* binding a port (the startup-order contract from #25).
Slice A therefore pins the *wiring*, not the boot: `serve --demo` with no file arguments, run from
an unrelated cwd, reaching that bundle check proves the embedded pair loaded and the engine ran off
it. The happy-path boot over a real bundle — serve responds, `index.html` embedded, `serve --demo`
works from the release artifact — is deliberately slice B's release-smoke acceptance, where the UI
bundle is built into `ui-dist/` before cargo build. Nothing here builds or serves a real bundle yet.

## 034 · 2026-07-13 · The real bundle ships: release builds + embeds the web UI (issue #28 slice B)

**What changed.** The release workflow now builds the web UI and stages it into the server crate's
embed folder *before* `cargo build` — closing the D13/D5 gap that made every released `serve` a dud.
`rust-embed` captures `crates/amberfork-server/ui-dist/` at compile time; that folder is `.gitignored`
(empty in a checkout) and nothing populated it in CI, so a released binary shipped an empty bundle and
`serve` refused with "web UI bundle missing". Three steps fix it: (1) `trunk build --release` in `ui/`
+ `cp -R ui/dist/. crates/amberfork-server/ui-dist/`, ahead of the existing cargo build; (2) the smoke
step boots `serve --demo` over the REAL embedded bundle and asserts `/` returns the `amberfork` index
and `/api/document` returns Document JSON — the happy-path boot slice A structurally could not reach;
(3) a `cargo package -p amberfork-server --list` check asserts the `include` override actually pulls
the built `ui-dist/` into the `.crate` tarball, so `cargo install` from crates.io gets a UI too. All
four of #28's distribution-acceptance checkboxes are now satisfied (the identity one by slice A). No
Rust changed — this is entirely `release.yml`.

**Decisions that will outlive the code.**
- *Ordering is the fix, and it lives where the artifact is born.* The bug isn't a missing file, it's a
  missing *step order*: embed-at-compile-time means the bundle must exist before the compiler runs, not
  before the tarball is cut. Staging into `ui-dist/` is placed immediately after checkout and before
  `build release binary`, and the comment says why so a future edit can't reorder it back into breakage.
- *The smoke tests the shipped artifact, not a rebuild of it.* The boot runs the just-built release
  binary (`serve --demo`, pinned `--port`, `curl` the two routes) — the same bytes that get tarred and
  attached. A green smoke is a statement about what a user downloads, which is the only statement worth
  making at release time. Proven verbatim under `bash -eo pipefail` locally (trap-kill the server,
  retry-until-up loop, `set -e`-safe greps) before it was ever written into the workflow.
- *`--list`, not `publish --dry-run --verify`.* A verifying dry-run compiles the crate against the
  registry and would fail on the unpublished workspace path-deps (`amberfork-layout` isn't on crates.io).
  The question #28 actually asks — "do the built assets travel into the package?" — is answered by the
  packaging manifest `--list` emits, no registry needed. Picked the check that answers the question over
  the ceremony that shares its name.

**Coverage honesty.** Everything here runs on GitHub Actions, which I can't green locally; what I can
and did prove is every *command* end-to-end on this machine (trunk release build → 481 KB wasm vs 5.8 MB
debug → stage → release build → `serve --demo` answers 200 on both routes → `--list` shows
`ui-dist/index.html`), plus the exact smoke shell block verbatim under `bash -eo pipefail`. The runner
environment itself (trunk install action, macOS-14 + ubuntu matrix, cold caches) only truly proves out
via a `workflow_dispatch` run — the pre-tag dry-run path that already exists for exactly this. That
dispatch is the acceptance gate for this slice; it should pass on both targets before v0.5 tags. Slice C
(README hero screenshot/GIF + run-on-your-own-agent guide) is the remaining scope on #28.

## 035 · 2026-07-13 · The guide learns the browser + the envelope (issue #28 slice C1)

**What changed.** `docs/run-on-your-own-agent.md` — the guide that teaches reading *your own*
fork — taught only the terminal, predating the v0.5 browser view entirely. Added a §4 "Read it in
the browser" (`serve <bad> --against <good>` + `serve --demo`, the loopback/no-telemetry
guarantee, `--open`/`--port`/`--max-steps`, the "verdict lands in the terminal first" contract),
renumbering the machine + troubleshooting sections to 5/6. Documented the payload envelope where
it applies (browser: 4 KiB per-slot cap with a visible truncation marker, `--json` for the whole
field, expand-on-demand is #30), and clarified that a terminal `…` is display-width abbreviation.
Prose only; no code. Splitting #28's slice C, this is C1 (docs); C2 is the README hero GIF, which
carries the README's own `serve` framing with it.

**Decisions that will outlive the code.**
- *Document each truncation where it is actually true — the honest correction.* First draft
  claimed "the payload envelope is the same in the terminal and the browser." It is not:
  `amberfork-layout` builds the envelope only in `Document::new` (the serve path), while
  `ViewModel::compute` always emits full text and the CLI painter reads it directly, doing its
  own *width-based* line truncation (lib.rs's own comment: "the CLI painter … never sees a cut
  slot"). Two different mechanisms, so two different notes: width-abbreviation in the terminal §3,
  the 4 KiB wire envelope in the browser §4. A guide that conflated them would teach a bug. Same
  honesty-in-artifacts reflex as the notebook 002 number correction, applied to prose.
- *`serve` belongs in the reading guide, not only the README.* The guide's whole job is "read the
  answer"; v0.5 added a second surface to read it on, so the omission was a correctness gap, not a
  nicety. The README's `serve` mention stays paired with the C2 hero visual (a GIF *of* serve), so
  the slice boundary is clean: C1 teaches the surface, C2 shows it.
- *Every abbreviation names its full-fidelity escape.* Both the terminal `…` note and the browser
  envelope note point at `--json` (and the envelope note at #30) — a reader who hits a `…` is never
  left wondering whether data was lost. "A shortened payload must never read as the whole payload"
  is the layout crate's own invariant (SlotText.truncated), restated for the human running the tool.

**Coverage honesty.** Prose-only change, so the workspace stays green from slice B; what "verify"
means here is that every command and flag the guide now names is real (`serve`, `--demo`, `--open`,
`--port`, `--max-steps`, `--json`, the /api/document behavior) — all exercised live in slices A/B on
this machine, and the 4 KiB figure is `SLOT_TEXT_LIMIT` read from the source, not remembered. What
remains on #28 is C2: the README web-fork hero (an animated GIF of the fork igniting, chosen over a
still) plus the README's own `serve` framing.

## 036 · 2026-07-13 · The web-fork hero ships — #28 and the v0.5 milestone close (issue #28 slice C2)

**What changed.** `docs/assets/hero-web.gif` — a 5.1s looping animation of the browser fork view,
the v0.5 headline. Sibling of the terminal `hero.gif`: `hero-web.html` is a designed, deterministic
motion piece driven by `window.__render(t)`, and `build_hero_web.sh` renders it to 2x frames in
headless Chrome → gifski (same pipeline as `build_hero.sh`, reused `render.mjs` unchanged). The one
beat (DESIGN.md §Motion): calm gray alignment → amber ignites at the fork node and flows down the
divergent path → the answer resolves (attribution + the red/green field diff). README updated: the
web fork leads the hero, `serve --demo` joins the 30-second try, the terminal fork follows ("the
same fork, in your terminal"), and the stale `diff`/`demo`-only status + crates-table row now name
`serve` + the web UI. 1.30 MB (on par with the 1.6 MB terminal hero). This closes **#28** — all four
distribution-acceptance boxes (A/B) plus docs+demo+hero — and the **v0.5 "fork in the browser"**
milestone.

**Decisions that will outlive the code.**
- *The hero is the product, choreographed — not a redraw.* Rather than hand-reconstruct the Leptos
  UI from DESIGN.md prose (which drifts), I captured the real rendered DOM and the server-computed
  SVG geometry from a running `serve --demo` (playwright), then built `hero-web.html` on the shipping
  app's actual markup + `ui/index.html`'s CSS copied verbatim + the real spine coordinates (fork
  y=183, path 183→333, 30px pitch). Fidelity is structural: when the app's CSS changes, the hero is
  rebuilt from the same tokens, not re-eyeballed. The deterministic `__render(t)` seam is the same
  contract the terminal hero already proved, so the two heroes share one renderer.
- *Every DESIGN.md rule enforced, checked frame-by-frame.* Amber spent exactly twice (fork node/row
  + divergent path); no blur halo (the saturated amber IS the glow — cold-design-read); red/green
  contained to the content-diff pane; the fork's redundant non-color cues (`⑂`, `✗`, dashed outline)
  present in gray before ignition so the signal survives grayscale; selection is raised-surface +
  hairline, never amber (DD2). Screenshot-driven: rendered the calm (t=0.7) and payoff (t=2.75)
  frames and read them before encoding, per the frontend-design skill's critique loop.
- *The glow is the message; text legibility is secondary — on purpose.* At README display width the
  row text is small, and that's the right call: DESIGN.md's north star is "you don't read the diff,
  you see where it broke." The hero optimizes for the amber reading at any size; the transcript
  detail is supporting texture, and the runnable specifics live in `demo`/`serve` one command away.
- *Both surfaces kept — terminal is a peer, not a legacy.* The web hero leads (it's the browser
  milestone), but the terminal `hero.gif` stays right below it: DESIGN.md makes the terminal render
  a first-class v1 surface (CI/SSH/GIF), so dropping it would misrepresent the tool as browser-only.

**Coverage honesty.** The GIF was rendered and inspected on this machine (frames at calm + payoff,
plus the encoded GIF's first frame extracted via ffmpeg — amber/red/green clean, no banding at
1200px). What a still can't prove is the motion's feel end-to-end (pacing of the ignite→flow→answer
arc); that reads only by watching the loop, which the founder reviewed and approved. `build_hero_web.sh`
makes it reproducible, so a future palette or layout change regenerates the hero from source rather
than freezing a stale render. v0.5 is done; the deferred follow-ups (#29 `--html` export, #30
expand-on-demand, #31 light mode) carry the browser platform into v0.6+.

## 037 · 2026-07-15 · v0.5.0: the reveal survives a real scoring-path change (issues #21–#28)

**What prompted it.** Tagging v0.5.0 ("the fork in the browser") triggers BENCHMARK.md rule 2's
one-test-look-per-tag. Unlike v0.4.0 — whose reveal (021) changed nothing scoring-relevant by
construction — the diff since v0.4.0 genuinely touches the cost path: #16's prepare-once
tokenization cache rewired `cost.rs`/`nw.rs`/`params.rs`. Notebook 023 claimed "same costs, same
alignments, same forks" from the dev gate staying green; the reveal is the test-side check of
that claim, at the only granularity that matters — predicted fork indices, not "it compiles."

**Method.** Same recipe as 021: `spike/data/regen_noise_seed{42,43,44}` (the cached regenerated
pair sets — unchanged since v0.4.0, no upstream or sanitizer work this cycle) scored via
`amberfork-bench run --split test`, same frozen `bench/params.toml`
(sha256:8ebd95ce8f3d, unchanged).

**Result: identical, to the digit.** Test split, seeds 42/43/44, n=35 pooled: every arm, every
metric, every calibration bin matches the sealed v0.2.0/v0.4.0 documents exactly — full engine
0.49 exact / 0.71 ±1 / 0.91 ±3, best baseline ±3 0.49; per-seed exact 0.75/0.23/0.50 (seeds
42/43/44). A structural diff of the new aggregate against
`chimera_noise_multiseed_test_v0.4.0.json`, excluding the `sources` list (which necessarily
names different filenames), shows zero difference anywhere else. Committed as
`bench/results/chimera_noise_seed*_test_v0.5.0.json` +
`chimera_noise_multiseed_test_v0.5.0.json` (rule 3: alongside, never swapped).

**Reading.** This is the sharpest test rule 2 has faced so far — the first reveal since the seal
where the code path that computes cost actually changed, not just code around it. It held:
prepare-once tokenization is a cache in front of the same function, and the test split confirms
that where it counts. v0.5's own scope (server, UI, `amberfork-layout` extraction) never touched
`amberfork-align`'s cost/alignment path at all, so that half of the diff was never in question —
only #16 was.

**Release.** Workspace + `ui/` bumped 0.4.0 → 0.5.0 (`Cargo.toml`'s internal path-dep versions
moved with it, per its own comment). CHANGELOG's `[0.5.0]` entry covers the milestone (#21–#28:
`amberfork-server`, `amberfork-ui`, the `amberfork-layout` extraction, release CI embedding the
web bundle, docs) plus the untagged riders since v0.4.0 (#16 perf, #18 CI, #19/#20 CLI fixes).
Tag `v0.5.0` closes the "fork in the browser" milestone.

## 038 · 2026-07-21 · Counterfactual attribution ships: diff --verify re-executes the fork (issues #35, #37)

**What prompted it.** Static attribution (`amberfork-align`, #12) answers *where* the runs diverge
and labels it `Static` — a structural claim ("they diverge here, and it propagates downstream")
that no re-execution ever checked. `AttributionMode::Counterfactual` has sat in the frozen model,
produced nowhere, since the model was authored. #35 is the epic that fills it: re-execute the
cassette with the fork patched, and report whether the run *recovered* — the difference between
"they differ here" and "this is what broke it". #37 is its payoff child; #36 (`amberfork-replay`,
the VCR matcher + loopback `ReplayServer`) was the substrate it re-drives against.

**What shipped.** `amberfork diff --verify … -- <cmd>` now emits a real `Counterfactual`
attribution. The pipeline, all in the new `amberfork-attrib` crate: `patch_cassette` (pure) swaps
the fork step's response for the good run's → `reexecute_once` stands up a `ReplayServer` over the
patched cassette and re-drives the agent (recorded turns from the tape, live relay on cache-miss
past the branch) → the recovery oracle aligns the re-executed run against good and applies the same
resync-k fork rule → consensus over N folds the per-run verdicts into a `Recovery`. The terminal
answer is a trailing segment on the attribution line: `… · recovered · 3 runs`.

**Decisions worth keeping.**
- *The whole fork step is the patch candidate.* Shrinking to a minimal sub-cause is #38 (ddmin);
  #37 patches the single fork step and asks only "does the sustained divergence at/after it go away".
- *Recovery is a tri-state, not a bool.* `recovered` / `not_recovered` / `unverified`. A live
  provider will not answer bit-identically twice, so a single run's crisp call is only evidence;
  the consensus (strict majority of *conclusive* runs) degrades to `Unverified` rather than
  asserting a result the runs did not agree on. `Unverified` is an honest verdict, never a silent
  absence — the reason the model chose a tri-state over a bool in the first place.
- *`--verify` requires both inputs to be cassettes.* Re-execution replays recorded exchanges; a
  passive trace has none. `load_cassette` makes a canonical trace a hard `NotACassette` error that
  points at `amberfork record`, rather than silently having nothing to re-run.
- *Injected seams keep the offline discipline.* The agent (`AgentDriver`) and provider (`Upstream`)
  are traits; `amberfork-attrib`'s whole suite substitutes in-process stubs, so `cargo test
  --workspace` stays offline and deterministic. The CLI supplies the two production seams — a
  subprocess `AgentDriver` and `LiveUpstream` — behind a current-thread runtime, the same tokio
  quarantine `serve`/`record` observe. `amberfork-align` is *never* called with a provider; it
  stays pure and offline, and the CLI (the composition root) swaps the upgraded attribution into
  the `DiffResult` after the fact.
- *Flag validation in `resolve()`, not clap.* The cross-flag contract (`--verify` ⟺ `--upstream` +
  `--base-url-env` + `-- <cmd>`) lives in one unit-testable function that gives one honest message
  per failure mode, instead of version-dependent clap constraint magic.

**The one deliberate testing boundary.** No subprocess-plus-network end-to-end test of the *happy*
`--verify` path. Driving a real agent binary against a real provider through the CLI cannot be
hermetic, and the workspace suite must stay offline. Coverage is layered instead: the re-execution
*mechanism* (patch → drive → oracle → consensus) is fully tested offline with stubs at the
`amberfork-attrib` layer; the counterfactual `--json` wire form (`mode: counterfactual`,
`recovered`, `runs`) is locked by `amberfork-model`'s round-trip test, which `diff --json`
serializes through unchanged; the CLI's own logic (`resolve`, `load_cassette`, and hermetic
`diff --verify` validation paths that fail before any network) adds 11 tests. Recorded here as the
honest coverage boundary, not an oversight — a fake-provider + stub-agent harness through the
binary would be slower and more brittle than the layered offline coverage for the same claim.

**Verified.** `smoke ✓ · fmt ✓ · clippy -D warnings ✓ · cargo test --workspace ✓` (333 passed).
Real-binary spot checks: `diff --help` shows the new flags, both validation paths print their
designed errors, default `diff` renders byte-identically (static attribution line unchanged).
#37 closes; epic #35 now has only #38 (ddmin minimal-cause) left in the v0.7 milestone.

## 039 · 2026-07-21 · ddmin minimal-cause: verified origination vs propagation (issue #38, epic #35 closes)

**What prompted it.** #37 patches the *fork* step and asks "did the run recover" — but the fork is
rarely the whole story. The true fault can sit a step deeper in the divergent region, and a run of
downstream steps may merely *propagate* one upstream break. Static attribution paints the whole
tail one colour (DR4's uniform divergent path) because structure alone cannot tell a carried error
from an independent one. #38 is the crate's named moat: reduce the patch-set to the **minimal
subset whose patch still recovers**, and split the region into *origination* vs *propagation*.

**What shipped.** Three pieces in `amberfork-attrib`, composed by `verify`:
- `ddmin::minimize` — hand-rolled Zeller–Hildebrandt ddmin over the index set `0..n`, re-pointed to
  preserve `Recovery::Recovered` instead of a failing property. Pure algorithm, pluggable oracle.
- `cause::fork_candidates` — the contiguous run of patchable (two-sided) alignment moves from the
  fork onward; `cause::relabel` — maps ddmin's verdict back onto `origin_step`/`propagation`.
- `verify` wires them: candidates → `minimize` (oracle = re-execute the subset-patched cassette,
  consensus over N) → `relabel`. The minimal cause becomes origination; the rest of the region,
  propagation. The attribution *wire shape is unchanged* — only `origin_step`/`propagation` tighten.

**Decisions worth keeping.**
- *Integration drove the interface — the slice-1 core was revised, not frozen.* ddmin started sync
  and pure (clean, but the oracle is real async re-execution). Rather than a `spawn_blocking` +
  nested-runtime bridge that forces `'static`/`Arc` onto the borrowed driver and cassettes, the
  oracle became `async` + fallible (`Reduction` tri-state). The algorithm logic is untouched; it
  now composes with `verify`'s borrows directly. Recorded because "the just-built component's API
  was wrong once it met its caller" is the normal shape of honest slicing, not a misstep.
- *Full ddmin, not a bisection.* The minimal cause can be *multiple independent faults* (step k and
  step k+2 both broken, neither alone sufficient) — a binary search returns one and mislabels the
  other. ddmin's complement phase finds the pair. `finds_a_multi_element_minimal_cause` guards it.
- *The O(log n) bound is real on the single-cause case, and asserted.* Single relevant element →
  each level halves the region in ≤2 oracle calls, +1 precondition; `stays_within_the_logarithmic_
  rerun_bound` asserts `calls ≤ 3·⌈log₂n⌉`. The cassette serves the unbranched prefix from the tape,
  so each re-run is cheap. Multi-fault is quadratic worst case — the honest cost of correctness.
- *Inconclusive never reduces; an unstable full set is `Unverified`, never a false cause.* Only
  `Recovered` triggers a reduction, so the set stays recovering at every step (what survives is
  1-minimal). If the *precondition* — patching the whole region — can't be established (a
  nondeterministic re-run), `minimize` returns `Inconclusive` and the labels stay static. This is
  the ddmin-level echo of #37's tri-state honesty: acceptance criterion 3, no fabricated minimum.
- *The candidate run stops at the first one-sided move.* A structural gap (a step in only one run)
  has no response to graft; re-executing *past* an unpatchable step would be serving a branch we
  cannot set. So a one-sided fork yields no candidates and falls back to static — matching what the
  single-patch path already did. Caught reviewing the first cut (`filter_map` skipped gaps and kept
  going); `map_while` is the honest shape.
- *`origin_step` is the earliest cause step; an independent downstream fault is pulled out of
  propagation.* The whole value over static: a tail step that does *not* recover once the upstream
  cause is patched is origination, not propagation, and no longer mislabeled. Under the fixed
  single-origin contract a genuinely multi-step cause surfaces only its earliest step as
  `origin_step` (the others are excluded from propagation but not separately listed) — a documented
  limitation, not a wire change (out of scope per #38; a future multi-origin field would carry it).
- *`confidence` = conclusive ÷ total ddmin oracle calls* — oracle stability across the re-runs. A
  fork verified across stable re-runs is worth more than one whose oracle kept wavering. Tallied
  with two `AtomicUsize` so the count survives `.await` without making `verify`'s future `!Send`.

**The testing boundary (unchanged discipline).** The labeling logic is tested *purely* — synthetic
`Reduction`s into `relabel`, a counting synthetic oracle into `minimize` — because the offline
`ScriptedAgent` reacts to nothing it is served, so it cannot exercise "patching subset S changes
what recovers". The happy path (align → candidates → ddmin → re-execute via stub agent+upstream →
relabel) is integration-tested end-to-end offline and asserts the refined `origin_step=1`,
`propagation=[2]`, `confidence=1.0`. `amberfork-attrib` grew 14 → 29 unit tests.

**Verified.** `smoke ✓ · fmt ✓ · clippy -D warnings ✓ · cargo test --workspace ✓` (348 passed).
#38 closes, and with it epic #35 (counterfactual attribution) and the v0.7 milestone: `diff
--verify` now re-executes the cassette to verify the cause *and* minimize it.

## 040 · 2026-07-22 · mid-project retrospective: what's strong, what's thinning, what to track

Halfway-point deep retrospect (founder-initiated). Four parallel deep-reads — the full notebook
arc, the architecture doc + amendments, a crate-by-crate code audit, and a fresh 2026 competitive
sweep — plus a re-read of POSITIONING / BENCHMARK / trace-format / DESIGN.

**Verdict: the engineering is not the weak point.** Code audit is clean — no stubs / `todo!()`, no
`unsafe`, no `#[allow]` anywhere, `-D warnings`, panics kept off the library path, tokio quarantine
honored, one frozen `DiffResult` seam defined once. The roster grew by need (14 aspirational → 10
built). What has thinned since v0.2 is **defensibility and proof**, not build quality:
- The market commoditized the adjectives: local / offline / deterministic is now table-stakes
  (Phoenix, Langfuse, Opik, Laminar, and **EvalView** — `hidai25/eval-view`, the closest *shipping*
  analog: offline trajectory diff, but stops before fork-localization + attribution). The durable
  wedge is narrower: **two-run fork localization + no-re-run counterfactual attribution**, a
  craft/DX moat, not a technology moat.
- The research frontier is co-inventing the attribution half in the open (FALAT 2606.00765,
  CausalFlow 2605.25338, Causal Agent Replay 2606.08275 — the last *requires* re-running, which is
  exactly the asymmetry to keep leading with).
- The sharpest reviewer-catch: every strong number is **true by construction** (injected chimera
  forks favor the alignment arms, notebook 001); the only natural-pair evidence (Mode A′) is a null
  (016). The README is already honest about the null and seed-sensitivity; it does not yet state the
  by-construction caveat *at* the number.

**Actions.**
- Positioning re-aim in progress (docs-only, slices B→A→C): README hero + currency refresh (the
  README undersells v0.6/v0.7 — record/replay/attrib/`--verify` go unmentioned); POSITIONING §6 adds
  EvalView + promotes the no-re-run wedge; surface the by-construction caveat.
- Filed 6 un-milestoned backlog issues: **#39** OpenInference/OTel ingest adapter (real on-ramp +
  natural TRAIL fixture), **#40** cost/token/latency deltas (UC1), **#41** natural-pair bench that
  isn't a null, **#42** held-out fixture for resync-k generalization, **#43** cassette secret/PII
  redaction, **#44** real-provider e2e for `--verify`. Milestoned deliberately after v0.8.

**Architectural watch-items (track, don't act):**
1. `amberfork-align` is widening into a god-crate (absorbed core + embed + static-attrib; owns cost
   model + aligner + fork rule + static attribution + field-diff). Natural split point: when a
   second cost model (embeddings) actually lands.
2. Linear NW vs the "typed causal DAG" ambition — the model carries `edges`/`parent_idx` but the
   aligner linearizes, so multi-agent / branchy runs aren't tree-aligned. Either reconcile the
   design doc's claim to the shipped linear reality, or scope GumTree-style tree alignment as the v2
   frontier.
3. Quadratic O(n·m) scale wall — fine to ~1000 steps, projected minutes at 5000 (022/023). The
   `prepare`/`cost_prepared` seam is designed; trigger to build it = a real slow trace, not a
   schedule.
4. ViewModel vs Document doubling (layout) — two serialization layers between engine and painters; a
   minor drift watch.

Next milestone unchanged: **v0.8 = the explain layer (#10, `amberfork-judge`)** — localize → verify
→ **name**. Honest note logged: naming is the LLM-in-the-loop, most-co-invented part
(lower-differentiation, lower-risk); #41 / #39 are arguably higher skeptic-leverage and worth
sequencing right behind it.

## 041 · 2026-07-23 · held-out generalization probe: frozen params on a different agent shape (#42)

First slice of the re-scoped **v0.8 = the credibility pass** (milestones reshaped this session:
v0.8 now = prove localization on data we did not construct — #42 → #39 → #41, plus #44; the explain
layer #10/#40 moved to v0.9). Attacks the sharpest catch from 040: every strong number is on the
Who&When-derived chimera family, so "the fork params generalize" is an *untested, load-bearing*
assumption — τ=0.3 / resync_k=2 / gaps 0.6/0.3 were calibrated on that one family (notebook 001/007)
and frozen since, never run against an unseen agent shape.

**Method.** New held-out fixture `bench/fixtures/heldout_react_v1/` — six pairs in a deliberately
different structural shape: a **single-agent ReAct tool loop** (`llm` think → `tool` act/observe →
`llm` answer), where the chimera set is all multi-agent Magentic-One orchestration (`kind: agent`
throughout). Built by the *same* mechanical injection as the chimera set (splice a reference prefix +
a different task's suffix at a known `gold_step`, one duplicated `(retry)` step + token-dropout
rewording on the shared prefix), so "gold fork" is defined by construction, not by peeking at the
aligner. Frozen `DiffParams::default()` run **once**; whatever it scored is the finding — no tuning
on this set. Test: `crates/amberfork-align/tests/heldout_generalization.rs`.

**Result — frozen params generalize to the ReAct shape:**

| metric | held-out ReAct (n=6) |
|---|---|
| exact | 5/6 |
| ±1 | 5/6 |
| ±3 | **6/6** |

Per-pair: 01/02/04/05 exact; `pair_03` (austen→currency) fires a *model-only* fork (the fork move
has no `b` side) that `fork_step_observed`'s fallback resolves to gold (exact); `pair_06`
(moon→eiffel) is the one near-miss — localizes two steps early (predicted 2, gold 4), still inside
±3. Reported, not hidden. The blind (all-identical) cost control localizes nothing, so the fixture
discriminates. Test pins ±3 at the observed 6/6 (deterministic — no seed draw, unlike the chimera
gate), mirroring `chimera_parity`'s pin-at-baseline convention: any drop out of ±3 is a red CI that
forces a notebook entry, never a silent retune (#42 acceptance).

**Honest caveat (the reason this is a first datapoint, not the answer).** The fixture is
**synthetic and hand-authored**. Because we wrote it, we cannot fully rule out having made the fork
findable — it is weaker evidence than a real third-party log, and it varies *shape*, not *provenance*.
So this narrows the "tuned to one family" critique (the frozen rule does transfer to a structurally
different trajectory without retuning) but does **not** touch the natural-data null (Mode A′, 016) —
that is #41's job, and the genuinely-external different-framework version of this same probe arrives
with the OpenInference/OTel adapter over TRAIL (#39). The probe *mechanism* built here is what #39/#41
reuse: swap the fixture, keep the test.

**Bench corpus notes.** The synthetic fixture forced two honest adjustments to amberfork-bench's
corpus-wide invariants (both had globbed *all* of `bench/fixtures/` assuming GAIA-sanitized chimera
pairs): (1) `pyjson` byte-for-byte round trip — dropped the generator's trailing newline so the
files match `json.dumps(indent=1)`; they now *extend* the serializer-parity corpus rather than
break it. (2) The `sanitize` licensing check (every `task` must be a GAIA redaction marker) — scoped
with an explicit opt-**out** allowlist (`NON_GAIA_FIXTURES`), safe by default: a new fixture is
checked unless deliberately exempted, so a real GAIA set can never slip the check by naming. The
serializer itself was correct throughout — the 1-byte divergence was a fixture artifact, not an
engine bug.

## 042 · 2026-07-23 · OpenInference OTLP ingest adapter — the real on-ramp (#39, v0.8 slice 2)

Second slice of **v0.8 = the credibility pass** (after 041's held-out probe). The sharpest
skeptic-catch closed here: the design doc positioned amberfork as "framework-agnostic, aligns any
two existing OTel traces," but the shipped `amberfork-ingest` was a forgiving canonical-JSON loader
plus two *dataset* adapters (Who&When, TapeAgents). There was **no** real OpenInference/OTel
normalizer — so the only realistic on-ramp for the "point it at traces I already have" persona
(logs coming out of LangSmith / Phoenix / Langfuse as OpenInference spans) did not exist, and the
claim over-reached what was built.

**What shipped.** `amberfork_ingest::openinference` — one genuine adapter: OTLP/JSON span export →
one canonical `Run` per `traceId`. It owns two layers that the sibling `gen_ai.*` slice will reuse
verbatim: (1) the **OTLP envelope reader** — resource/scope nesting flattened, `AnyValue` typed
values (`stringValue`/`intValue`-as-string/`boolValue`/`doubleValue`/`arrayValue`/`kvlistValue`)
decoded to plain JSON; (2) the **OpenInference vocabulary** on top. Mapping: `openinference.span.kind`
LLM/TOOL/AGENT → `kind` (CHAIN/RETRIEVER/EMBEDDING/… → `other`); `tool.name` → `name` (else span
name); `input.value`/`output.value` honoring `*.mime_type` (JSON → field-diffable `Object`, else
`Text`). Steps are re-indexed by `startTimeUnixNano` (exporters don't promise execution order),
then `parentSpanId` → `parent_idx`. A non-OpenInference attribute is preserved to `attrs` **and**
raises an `unmapped-attributes` warning (recognized vocabulary rides in `attrs` silently — the
known/foreign split is a prefix allowlist); a content-free span → metadata-only step +
`content-absent` warning.

**Architecture rule held under temptation.** OTLP spans carry a `status` (`STATUS_CODE_ERROR`); the
adapter *ignores* it — `outcome` stays `None`. A run's verdict is a user assertion, never inferred
from span status (POSITIONING §187, trace-format.md). A test pins this: an error-status span still
produces `outcome == None`.

**Slice boundaries drawn on purpose (founder-approved before building).** (a) OpenInference now,
native `gen_ai.*` next — same envelope, additive vocabulary; not folded in. (b) The TRAIL
natural-pair *benchmark* fixture stays in #41; this slice ships the adapter it will consume, not the
bench. (c) CLI auto-detection (`amberfork diff trace.otlp`) is out — like `whowhen`/`tape`, this is
a library adapter first (bench/tooling call it); wiring the CLI sniffer is a later slice. (d)
Deferred inside the adapter, with nothing lost: structured-message reconstruction from
`llm.input_messages.*` (rides in `attrs`), and RFC3339 timing (raw nanos preserved in
`attrs.otel.*_time_unix_nano` — timing is display-only, never an alignment signal, so no new time
dependency was pulled in for it).

**Fixture provenance — honest.** The test fixture is **spec-faithful but hand-authored** (an inline
OTLP/JSON export matching the OpenInference wire shape), chosen deliberately over sourcing a real
third-party trace now: parser correctness is *not* a "true by construction" risk the way
localization is (041) — the adapter either decodes the documented wire shape or it doesn't. The
genuinely-external, different-*provenance* trace (TRAIL, from real logs) arrives with #41, which
needs a real pair regardless; that is where this adapter meets data we did not construct. So this
slice narrows the "framework-agnostic over-claim" (a real OpenInference normalizer now exists and is
tested against the spec) without yet touching the natural-data question.

**Tests (7, mirroring the `whowhen`/`tape` discipline):** kind/name/idx/parent wiring under
out-of-order + multi-trace spans; mime-driven content typing; recognized-vs-foreign attribute
split + warning; content-absent advisory; the canonical round-trip guard (normalized run
re-serializes and re-loads through the plain-JSON loader unchanged); parse-error and empty-export
paths. Full gate green (smoke + fmt + clippy `-D warnings` + `cargo test --workspace`).

**Docs reconciled (acceptance item 3).** `docs/trace-format.md` Mappings now states status per
adapter (OpenInference **implemented** with its covered subset; `gen_ai.*` **planned**; Who&When /
TapeAgents **implemented**) instead of listing all as informative. The design doc's Current State
"align any two existing OTel traces" gained the honest qualifier: OpenInference/OTLP shipped
2026-07-23, native `gen_ai.*` next.

**Watch-item update (040 #1).** `amberfork-ingest` grew a fourth adapter, but stayed a *thin*
crate — each adapter is a self-contained namespace over the shared model; no god-crate pressure
here (unlike `amberfork-align`). The OTLP envelope reader is the first shared substrate between
adapters; if a third OTLP-based adapter lands, factor it out then, not now.

## 043 · 2026-07-23 · TRAIL ingest adapter — real external traces, a shared vocabulary seam (#41, v0.8 slice S1)

First slice of **#41** (the natural-pair bench that isn't a null). #41 turned out to be a
~5-slice mini-epic, not the "swap TRAIL into the Mode A′ pipeline via #39's adapter" one-liner the
backlog assumed — a feasibility spike against the real repo (`patronus-ai/trail-benchmark`, MIT,
ungated, ~28 MB) found three things that reshape the work:

1. **Format mismatch.** TRAIL traces are the Patronus SDK's **nested** JSON (`{trace_id, spans:[…
   child_spans …]}`), *not* the flat OTLP `resourceSpans→scopeSpans→spans` envelope #39 (042) built
   — so `openinference::from_otlp_json_str` does not ingest TRAIL. But the deep spans carry the
   *same* OpenInference vocabulary (`openinference.span.kind`, `input.value`/`output.value`,
   `llm.*`, `tool.*`) in `span_attributes`. It is the mirror of #39's gen_ai sibling: there, *same
   envelope, additive vocabulary*; here, *same vocabulary, different envelope*.
2. **Gold is span-located and multi-error.** `processed_annotations_gaia/<id>.json` (in-repo,
   ungated) gives `errors[].location = <span_id>` + category + impact, up to 9 per trace. Scoring
   will need span-id → step-index and a predicted-∈-gold reading — deferred to S2/S5.
3. **Single-trajectory.** No same-task duplicate run exists in TRAIL, so the *reference* problem
   that nulled Mode A′ (016) is unchanged — a reference still has to come cross-system or from a
   consensus. TRAIL improves provenance/N/length, not the reference. That is S4's hard part.

**What shipped in S1 (the adapter only).** `amberfork_ingest::trail` — a Patronus trace tree → one
canonical `Run`. The reuse was the point: the OpenInference **vocabulary** layer (kind/name/content
mapping + the known-vs-foreign attribute split + step assembly) was extracted out of
`openinference.rs` into a shared `oivocab` module, and both adapters now call it. `openinference`
behaviour is byte-identical (its 20 tests unchanged, green) — this was a pure factor-out, the
second-consumer trigger notebook 042 named (it flagged the OTLP *envelope* as the first shared
substrate; the *vocabulary* is the second, and TRAIL is what earned it). `trail` owns only the
envelope: a pre-order DFS over `child_spans` (which *is* execution order for a single-SDK tree —
no timestamp re-sort, no time dependency), `parent_idx` from the nesting, `kind` from the
`openinference.span.kind` attribute never the wire `span_kind` (always `"Internal"`), RFC3339
`timestamp` → `t_start` (native — TRAIL gives real RFC3339, unlike #39's raw nanos), and the source
`span_id` retained in `attrs["otel.span_id"]` so S2 can resolve an annotation's span-located gold to
a step.

**Architecture rule held again.** TRAIL spans carry a `status_code` (`Error`); the adapter never
reads it — `outcome` stays `None`. A run's verdict is a user assertion, not inferred from span
status (POSITIONING §187, trace-format.md). A test pins it (`status_code: "Error"` → `outcome ==
None`).

**Tests (7, mirroring the whowhen/tape/openinference discipline):** tree flatten to pre-order steps
+ parent wiring under nesting; kind from the attribute not the wire kind; `tool.name` name
override; mime-driven content typing; foreign (`pat.*`/`smolagents.*`) vs known (`llm.*`) attribute
split + warning; content-absent advisory; span_id + `t_start` provenance retention; the canonical
round-trip guard; empty-trace and parse-error paths. Full gate green (smoke + fmt + clippy
`-D warnings` + `cargo test --workspace`).

**Fixture provenance — honest, and the real-bytes check.** The committed fixture is **spec-faithful
hand-authored** (no GAIA content — real TRAIL traces embed gated GAIA questions/answers, never
committed; same call as #39/042). To prove the adapter parses genuine bytes anyway, a *throwaway*
check (deleted, not committed) ran two real fetched traces through it: an 11-step and a 46-step run
normalized cleanly — 100% span-id retention, `parent_idx` nested to depth 42, kinds distributed
`{agent, llm, tool, other}`, `t_start` on every step. Two things that matter downstream: (a) these
are **much longer** than the 7–10-step Who&When logs that nulled at ±3 in 016 — a ±3 window no
longer swamps the run, which is the pre-registered reason #41 *could* separate from random this
time; (b) the committed real-bytes parse test belongs in S3 (the `fetch` pin), where the ingest
crate's std-fs-only purity isn't compromised by a network dep — it is not in S1.

**Watch-item (for S3).** Every Patronus span carries `pat.*` platform attrs, so on real traces the
`unmapped-attributes` advisory fires on **every** step (one warning/step). It is honest (those
attrs *are* foreign to the OpenInference vocabulary) and advisory-only, but it is noise at scale;
if S3/S4 wants it quieter, add a TRAIL-scoped silent prefix rather than polluting the shared
`oivocab` vocabulary. Left as-is in S1 for consistency with #39 — not gold-plated ahead of the need.

**Watch-item update (040 #1 / 042).** `amberfork-ingest` now has five adapters but is still thin;
the new `oivocab` is the first *shared* substrate between two of them, extracted exactly when the
second consumer arrived rather than speculatively. No god-crate pressure here (unlike
`amberfork-align`).

**Next (S2):** parse `processed_annotations_gaia/<id>.json` → resolve `errors[].location` span-ids
to step indices via the retained `attrs["otel.span_id"]`, returning gold *beside* the run (the
whowhen/tape convention), never merged in.

## 044 · 2026-07-23 · TRAIL gold annotations parsed + resolved to steps (#41, v0.8 slice S2)

Second slice of #41. S1 (043) retained each span's `span_id` in `attrs["otel.span_id"]`; S2 turns
that into gold. `amberfork_ingest::trail` now parses a TRAIL error-annotation file
(`processed_annotations_gaia/<id>.json`) into a `TrailGold` — the trace's errors, each with its
taxonomy `category`, its span-located `location`, and a typed `Impact` — and `TrailGold::resolve(&
run)` maps every `location` span id to a step index via the retained provenance, returning one
`GoldStep` per error in file order. Gold lives *beside* the run, never merged (the `tape`/`whowhen`
convention); the `otel.span_id` join key stays an internal detail of the adapter, so the bench layer
(S4/S5) asks for gold by calling `resolve`, not by knowing the magic attribute.

**Schema, confirmed against the real repo before building** (117 GAIA annotation files, MIT):
`errors[]` may be empty (a trace the annotators cleared — no gold fork, a candidate *reference* not
a failing side); `location` is always a single span-id string; `impact` is the closed vocabulary
`{LOW, MEDIUM, HIGH}`. `Impact` is therefore a typed enum, with `Impact::Other(String)` preserving
any out-of-vocabulary value losslessly rather than failing the parse — the forgiving-input contract
the crate holds everywhere. `category` stays a `String` (TRAIL's taxonomy is free-ish text carried
through for per-category coverage in S5, not a control-flow value worth an enum guess). The
annotation file's `scores`/`evidence`/`description` fields are human context the benchmark does not
read and are ignored, never a parse failure.

**Tests (4, extending `tests/trail.rs` so the gold resolves against the S1 fixture's real span
ids):** typed-impact + file-order parse with a lossless `Other`; span-id → step resolution
(`agent001`→1, `tool0001`→3, `root0000`→0) with an unresolvable `ghost999`→`None` (data, rule 4);
an empty-`errors` clean trace → empty gold; malformed-JSON parse error. Full gate green (smoke +
fmt + clippy `-D warnings` + `cargo test --workspace`).

**Real-bytes join — exact.** A throwaway check (deleted, not committed) ran two real fetched
trace+annotation pairs through `load_file` + `load_annotations` + `resolve`: the 11-step trace's one
error resolved to step 6; the 46-step trace's nine errors resolved to steps 14/23/31/33/34/36/37/37/
43 — **100% resolution, no unresolvable span ids**. Two things this settles: (a) the span-id join is
real, not just fixture-shaped — annotation `location`s are genuine trace span ids; (b) the gold is
**distributed through the run**, not clustered at the end or pinned at step 0 (the murk that made the
short cross-system Mode A′ null in 016). On a 46-step trace with gold at 14–43, a ±3 window covers a
small fraction of the run — random is weak, and a real localizer has room to separate. That is the
pre-registered reason #41 *could* be non-null; S4/S5 will measure whether it is.

**Multi-error, and what S5 must decide.** A TRAIL trace carries up to 13 errors (median a few); the
9-error trace even had two at the same step. amberfork predicts *one* fork. So S5 owns a
pre-registered metric choice: score the prediction against the *earliest* resolved gold step (first
decisive divergence — matches the "first meaningful divergence" hypothesis) and/or predicted-∈-gold
(any annotated error), always windowed and reported per BENCHMARK.md. S2 deliberately returns the
full ordered `Vec<GoldStep>` and makes no such choice — the seam stays honest.

**Still ahead.** S3 = `fetch` pin for TRAIL (traces + annotations) + the committed real-bytes
network test the ingest crate's std-fs purity keeps out of S1/S2. S4 = the hard, uncertain part —
sourcing a *reference* run per failing TRAIL trace (single-trajectory means the reference is
cross-system or a consensus; the 016 wall is unchanged). S5 = scoring + committed results doc +
`report` snapshot + the honest null-or-not writeup.

## 045 · 2026-07-24 · TRAIL pinned into `fetch` + the real-bytes integrity test (#41, v0.8 slice S3)

Third slice of #41. S1/S2 built the TRAIL adapter + gold resolver in `amberfork-ingest` (std-fs
pure, no network). S3 teaches the bench `fetch` harness where the real bytes live and lands the
committed integrity test that the ingest crate's purity deliberately keeps out. Pure manifest
thickening: `SOURCES` went `[Source; 2] → [Source; 4]`, no `Source`-struct change, no `main.rs`
change (`fetch_all` iterates `SOURCES`; `--out` already defaults to the gitignored `bench/data`),
no `.gitignore` change.

**The pin.** `patronus-ai/trail-benchmark` **@ `0ffbed9d`** (main HEAD, resolved via `git
ls-remote`), MIT via GitHub — never the gated HF copy (same rule as tapes/whowhen). TRAIL is *one*
repo split across two upstream dirs, so it is **two** `Source` entries pinned to the *same* commit:
`benchmarking/data/GAIA/` → `trail-traces/` (117 traces) and
`benchmarking/processed_annotations_gaia/` → `trail-gold/` (117 annotation files). Kept the flat
single-component `dest` contract (the `manifest_pins_are_well_formed` invariant) rather than nesting
under `trail/` — the `trail-` name prefix groups them, and the important structure is that a trace
and its gold **share a `<trace-id>.json` basename** across the two dests (verified in the tree),
which is the join key S4 pairing reads. Appended last: smallest-pull-first fail-fast (8 tapes → 184
Who&When → 234 TRAIL).

**A design fact the tests surfaced.** Both TRAIL sources share repo+commit ⇒ **one** recursive
tree URL. The real fetch lists the whole tree once *per source* and each filters to its own prefix
(one extra identical GET of an immutable tree — harmless, not worth refactoring the one-source-one-
fetch architecture for). The provenance test caught this the honest way: two *partial* canned
listings under the same key collided in the fake HTTP map (last wins), so the traces source filtered
a gold-only listing to nothing → `NothingToFetch`. Fix = one combined listing under the shared key,
which is what reality returns anyway.

**The finding that mattered — one malformed upstream gold file.** The live `#[ignore]`d e2e
(`network_fetch_trail_end_to_end`) pulls all 234 real files and strict-parses each through the
S1/S2 adapters. It found that **1 of 117 gold files** (`a96c6811…json`) is **not valid JSON** — a
trailing comma before the end of an array that even Python's own `json.load` rejects (surveyed all
234 files against the pinned tarball to confirm scope: **117/117 traces** parse, **116/117 gold**;
exactly this one fails). So it is a genuine *upstream data defect*, not an amberfork adapter gap.

Decision (founder, this session): **exclude-as-data, keep the adapter strict** — the crate's
forgiveness is for *unknown fields*, not *malformed syntax*; tolerating a trailing comma would mean
adopting a lenient JSON dialect crate-wide for one upstream typo, softening a contract that should
stay strict. This *is* BENCHMARK.md's exclusions-as-data rule. The e2e therefore pins the known-bad
set `MALFORMED_GOLD = {a96c6811…}` and asserts `excluded == known` + `parsed == 116`: a strict
parser never silently drops the file, the defect is visible in code, and a future pin bump that
changes which files are malformed fails the test loudly (the "reviewed manifest edit" discipline).

**What the e2e proved on real bytes (reported, this run):** `trail: 116/117 gold parsed (1
excluded); resolve 580/580 error location(s) mapped to a step`. So across **all 116** parseable gold
files, **every one of the 580 gold error `location`s resolved to a real step** — 100% span-id
resolution, not just the 2 spot-checked pairs of 044. That settles at scale that the span-id join is
genuine (annotation `location`s really are trace span ids) and confirms 044's "gold distributed
through the run" claim over the whole set — the pre-registered reason #41 *could* separate from
random (S4/S5 measure whether it does). The resolution *rate* is `eprintln`'d, never asserted — it
is data S5 pre-registers, not a fetch invariant.

**Tests (2 added, `fetch.rs`):** `trail_manifest_pins_the_two_gaia_dirs` (pure — pins repo/commit/
prefixes/dests/MIT; red→green anchor) and the live `network_fetch_trail_end_to_end` (`#[ignore]`,
operator-run). The existing `manifest_pins_are_well_formed` covers the new rows' 40-hex/dest-shape
invariants for free; `fetch_all_writes_the_provenance_record` extended to drive all four sources.
Full gate green (smoke + fmt + clippy `-D warnings` + `cargo test --workspace`: 61 bench tests, 2
ignored network e2es kept off CI as designed).

**Still ahead.** S4 (the hard, uncertain part) — a *reference* run per failing TRAIL trace. TRAIL is
single-trajectory, so the reference is cross-system (HAL/TapeAgents → murky gold, the 016 wall) or a
Mode-B consensus; this is where #41 may still null, honest either way. Then S5 = pre-registered
metric (earliest-resolved-gold-step and/or predicted-∈-gold, windowed, Wilson CIs, the malformed
file among the exclusions-as-data) + committed results doc + `report` snapshot.

## 046 · 2026-07-25 · TRAIL's GAIA task_id join key (#41 S4a) + the HAL reference-overlap measurement

Two things this session: the S4a code slice (the join key) and the reference-feasibility
measurement that decides how #41 sources its reference side — the "hard, uncertain part" S1–S3
kept flagging. The measurement came back far better than the 016 null predicted, and reshaped the
plan from "likely null" to "a well-powered offline experiment."

**S4a — the GAIA `task_id` join key (shipped).** A TRAIL trace's `Run.id` is the opaque Patronus
`trace_id` and its `task` is `None` — the underlying GAIA identity lives only in gated content. But
inspecting the real bytes found the canonical GAIA `task_id` (a UUID) embedded in the smolagents
harness spans' structured `logs`: `get_examples_to_answer` carries the loaded dataset row under
`logs[].body["function.output"]`, and `answer_single_question` carries the answered example under
`logs[].body["function.arguments"].example`. That UUID is a *non-gated* identifier — it sits beside
the gated question/true_answer/annotator-steps but is itself just a key. So `amberfork_ingest::trail`
gained a `TrailMeta { gaia_task_id }` returned beside the run via `convert_str`/`convert_file`,
mirroring `tape::TapeMeta` exactly (identity beside the run, never inside the trajectory). The
`RawTrailSpan` parse — which had dropped `logs` — now reads them for this one purpose;
`task_id_in_log_body` lifts **only** the UUID (example first as the more precise source, then the
dataset row), never the content around it. Tests (5, extending `tests/trail.rs`): extraction from
each of the two locations, `None` when no harness span is present (data, not a failure),
normalization-parity with the content-only `from_trace_json_str`, and a **gating guard** — `SECRET-*`
markers placed only in the harness log body are asserted absent from the serialized run. Full gate
green (smoke + fmt + clippy `-D warnings` + `cargo test --workspace`).

**The failing set, at scale.** Ran the extractor over all 117 real GAIA traces (from the pinned
tarball): **117/117 carry a task_id** (the join is universal), the 1 known-malformed gold file stays
excluded-as-data (045), **113 traces have ≥1 annotated error → 112 distinct failing task_ids**, and 3
are clean (0-error) — different tasks, so not same-task references. One task_id is duplicated inside
TRAIL (116 distinct of 117), a lone possible internal pair, noted not used.

**The reference-overlap measurement (the go/no-go).** The reference must be a *passing* run of the
same GAIA task. HAL (`hal.cs.princeton.edu/gaia`) embeds a per-task success matrix inline (Plotly
`heatmap_data`: x = 165 GAIA task ids, y = 33 agent configs, z = fraction of runs solved). TRAIL's
112 failing task_ids are **all** within GAIA-165 (same UUID namespace, no mapping needed). Overlap of
TRAIL-failing with tasks each family *passed* (z>0):

| reference family | tasks passed | ∩ TRAIL failing (112) |
|---|---|---|
| **HF Open Deep Research** (smolagents, same agent) | 152 | **105** |
| HAL Generalist Agent | 158 | 108 |
| any agent (upper bound) | 160 | 109 |

**The finding that matters — same *agent*, not merely same framework.** TRAIL's GAIA traces use HF
Open Deep Research's exact signature toolset across all 117 files (`web_search`, `visit_page`,
`find_on_page_ctrl_f`, `page_down`, `inspect_file_as_text`, `SearchInformationTool` 701×,
`TextInspectorTool` 585×, `managed_agent`/`search_agent`) on **o3-mini** (`llm.model_name` = o3-mini,
1507×). TRAIL's traces *are* HF Open Deep Research runs. So the HAL "HF Open Deep Research" configs
are the **same agent implementation**, differing only in the backing model (no o3-mini ODR config on
HAL). This largely dissolves the 016 step-0 wall: both runs share the ODR scaffolding (same tool
loop, same sub-agent structure), so they stay structurally synced until the model makes a different
decision — which is where a meaningful fork lives. #41 goes from "likely null, n=4" to ~105 natural
same-agent pairs, **offline, zero API cost**.

**Honest caveats (the remaining, much-reduced risk).** (1) *Different model* — o3-mini vs the HAL ODR
configs' models is the new gold-quality threat, but it is cross-*model within one agent*, not
cross-*system*; whether it is small enough to separate from random is exactly what S5 will measure,
now with the N to detect it (gold distributed at steps ~14–43 on the long traces, so random is weak —
043/044). (2) The reference *trajectories* still have to be fetched: HAL zips are free but
Fernet-encrypted (`hal1234`, PBKDF2 480k) and sizeable, and likely need a HAL-format ingest adapter —
that is S4b. (3) z>0 = ≥1 of N runs passed; take one passing run, or a **consensus of passing ODR
runs** (Mode B) to wash out model-specific quirks and align against the shared ODR structure. The
record-mode exact-config o3-mini re-run drops to an *optional precision arm*, not the primary path.

**Next (S4b).** Source the passing HF ODR reference trajectories from HAL — download + decrypt the
ODR zips, group by GAIA task_id, ingest to canonical `Run` — then S4c (the TRAIL pairing builder, the
`build.rs` analogue joining failing-trace + resolved-gold ↔ passing same-task reference on the
task_id) and S5 (scoring, Wilson CIs, committed results, `report` snapshot). First step of S4b:
verify one HF ODR GAIA zip's format/access before scoping the adapter.

## 047 · 2026-07-25 · HAL ODR reference format resolved + the Weave→Run adapter (#41 S4b)

Two things: the S4b feasibility spike (where the reference trajectories live and what shape they
are) came back GO with a shape *different* from what 046 assumed, and the first S4b code slice —
the HAL→canonical `Run` ingest adapter — shipped test-first against it.

**Feasibility (GO), and the shape 046 got half-right.** The reference trajectories are not behind
Princeton's Fernet wall: HAL's `/gaia` page links straight to a public Hugging Face dataset
(`agent-evals/hal_traces`), one `gaia_hf_open_deep_research_<model>_…_UPLOAD.zip` per backing model
(~290–575 MB each). Each zip holds a *single* `.json.encrypted` member — so 046's "Fernet-encrypted"
intuition was right, just delivered via HF. The recipe is HAL's own published `hal-decrypt.sh` (not
guessed): the member is a JSON envelope `{salt, encrypted_data}`, and
`key = urlsafe_b64(PBKDF2-HMAC-SHA256(salt=b64d(salt), iters=480000).derive("hal1234"))`, then
`Fernet(key).decrypt(b64d(encrypted_data))` yields one traces JSON *per agent config* (all GAIA
tasks in one blob). Verified end-to-end on the gpt-4.1 ODR zip (290 MB → 228 MB decrypted JSON).
Range-read the zip tail first to learn the container without pulling 290 MB; the Fernet token is one
MAC'd blob, so the schema probe did need the full download.

**The inner schema — a W&B Weave export, and the double-log.** Top level:
`{config, results, raw_eval_results, raw_logging_results, total_usage, total_cost, git_info}`.
`results.successful_tasks`/`failed_tasks` and `raw_eval_results.<task>.score` (bool) grade each task,
all keyed by GAIA UUID — the *same namespace* TRAIL's S4a join key lifts, so the two sides join with
no mapping. `raw_logging_results` is the trajectory: a **flat** list of Weave call records, each
tagged `attributes.weave_task_id`, ordered by RFC3339 `started_at`. Not a deep span tree — a turn
stream, and **double-logged 1:1**: every model call appears as a `litellm.completion` wrapper over
the `openai.chat.completions.create` leaf it delegates to (verified: exactly paired across all 164
tasks, 0 exceptions). The openai leaf is content-complete (`inputs.messages` in, `output.choices`
out); message counts grow 1→4→6→8 across a task, so each leaf is one agent turn. gpt-4.1 numbers:
165 GAIA-165 tasks, 83 pass / 82 fail, 5726 records → **2863 turns** (leaves), median **13 turns/task**.

**S4b-ingest slice — `amberfork_ingest::hal` (shipped, test-first).** `convert_str(decrypted_json)
-> Vec<ConvertedHal>`, one canonical `Run` per GAIA task (sorted by task id), each with
`HalMeta { gaia_task_id, model, passed }` beside it — the `trail`/`tape` sidecar pattern. Design
decisions: (1) **keep only the openai leaves as turns, drop the redundant litellm wrappers** — a
task's step count is its turn count, the granularity a fork lives at (proper normalization, like
`trail` collapses transport `span_kind`). (2) **Whitelist content across the seam**: `inputs` keep
`{model, messages}`, `outputs` `{choices, usage}`; the request's `self`/`extra_headers`/`extra_body`
and the response's transport ids are dropped — fidelity *and* a safety guard, so an `extra_headers`
auth token can never ride into a committed `Run` (#43). (3) **`Run.outcome` stays `None`** — the HAL
pass/fail rides in `HalMeta.passed` (from `successful_tasks`), never asserted on the trajectory
(trace-format rule). (4) **Emit all tasks with a `passed` flag** (not passing-only): ingest
normalizes, the S4c pairing layer selects — benchmark policy stays out of the adapter. Turns form a
linear chain (`parent_idx = None`); `started_at`/`ended_at` land natively in `t_start`/`t_end`; the
source Weave call/trace ids ride in `attrs` as provenance. 8 unit tests on a synthetic Weave fixture
(no gated content: wrapper-drop, RFC3339 ordering, whitelist+secret-drop, verdict-in-meta,
content-absent advisory, config-scoped ids). Full gate green (smoke + fmt + clippy `-D warnings` +
`cargo test --workspace`, chimera_parity included). Validated out-of-band against the real decrypted
gpt-4.1 blob (throwaway, not committed): `convert_str` → 164 runs / 83 passed / 2863 turns /
median 13, every invariant holding on data we did not construct.

**The honest risk (S5, flagged not fixed).** HAL's Weave export logged **only LLM turns**, while
TRAIL's o3-mini ODR trace (OpenInference) carries the full tool/sub-agent span tree. So the two
sides of a pair sit at *different granularities* — a real threat to whether alignment separates from
random, and the sharpest thing S5 must measure. The adapter's job is a faithful HAL `Run`, which
this is; the granularity reconciliation is the pairing/scoring question, not an ingest one. This is
distinct from the (already-known) cross-*model* gold caveat: the reference is a same-agent,
different-model (gpt-4.1/o3/…) ODR run, `HalMeta.model` carries which.

**Next.** S4b-fetch (the second S4b slice): a Rust HF-pinned fetch + Fernet/PBKDF2 decrypt of the ODR
zips to plaintext JSON, tested offline with a synthetic envelope round-trip (the live pull `#[ignore]`d
like `fetch.rs`) — adds `fernet`/`pbkdf2`/`sha2` deps. Then S4c (pair TRAIL-failing + resolved-gold ↔
HAL-passing same-task run on the task_id, consensus-of-passing-runs to wash out model quirks) and S5
(scoring, Wilson CIs, committed results, `report` snapshot).

## 048 · 2026-07-26 · HAL zip decrypt → plaintext JSON (#41 S4b, slice 1 of 2)

S4b-fetch, split. Notebook 047 framed "S4b-fetch" as one slice (HF-pinned fetch + Fernet/PBKDF2
decrypt). Built as two: the **decrypt** (this slice, no network, all the crypto-correctness risk)
and the **HF fetch** (next slice, I/O plumbing mirroring `fetch.rs`). Merging them would put crypto
and HTTP orchestration in one diff — the decrypt earns its own tight red→green, and it's usable
alone today: an operator hand-downloads a zip (as the feasibility spike did) and gets ingestable JSON.

**What shipped — `amberfork_bench::hal_fetch::decrypt_traces(zip, password) -> Vec<u8>`.** Unwraps
one HAL config zip to the plaintext dump `amberfork_ingest::hal::convert_str` already reads, per HAL's
own `hal-decrypt.sh` recipe (047): the lone `.json.encrypted` member is a `{salt, encrypted_data}`
envelope; `key = urlsafe_b64(PBKDF2-HMAC-SHA256("hal1234", b64d(salt), 480_000))`, plaintext =
`Fernet(key).decrypt(b64d(encrypted_data))`. Typed `HalDecryptError` for every failure (bad zip,
wrong member count, non-envelope, bad base64, non-UTF8 token, MAC failure) — no panics on the lib path.

**The two layerings that bite, both pinned.** (1) The key is *derived*, not stored — PBKDF2 output
is urlsafe-b64 (padded), the encoding `Fernet(key)` consumes. (2) `encrypted_data` is *double*-base64:
a Fernet token is already urlsafe-b64 text, HAL b64-encodes it again; one standard-b64 decode yields
the token string `fernet` then unwraps. Getting either wrong reads as a generic decrypt failure, so
the test pins them separately.

**Correctness = cross-implementation KAT, not a self-round-trip.** Minted a vector once from Python's
`cryptography` (the library HAL uses) and pinned both layers offline: `derives_the_pinned_fernet_key`
asserts the derived key equals Python's byte-for-byte (a PBKDF2 rounds/hash/alphabet drift fails
*there*, localized); `decrypts_the_python_cryptography_known_answer` decrypts a genuine Python Fernet
token to its plaintext (end-to-end HAL-format proof). Five error-path tests: wrong password (MAC
rejects → `Decrypt`, never plausible-but-wrong bytes), 0/2 members (`Member{count}` — exactly one is
the format, never pick-the-first), non-envelope, non-base64 salt, non-zip.

**Decisions.** (a) Lives in `bench`, not `ingest`: ingest stays the lean serde-only forgiving loader;
decryption (and next slice's fetch) is data acquisition — it sits beside `fetch.rs` with the zip/crypto
deps and hands plaintext to `hal::convert_str`. (b) Crypto never hand-rolled — the `fernet` crate
(RustCrypto AES-CBC+HMAC) authenticates, so a bad password/dump fails at the MAC. (c) A thin
`hal-decrypt` subcommand is the operator seam (and the caller a binary crate's `pub fn` needs, or
`clippy -D warnings` flags dead code). Deps added: `fernet`/`pbkdf2`/`base64`/`zip` (deflate-only, no
default codecs); `sha2` was already in. Full gate green (smoke + fmt + clippy -D warnings + workspace
tests, chimera_parity included). Debug tests ~6.5s — three run the real 480k-iter PBKDF2; left honest
and un-optimized (no `[profile.dev.package]` tweak) rather than fake-fast.

**Next.** S4b-2: the pinned Hugging Face fetch (`agent-evals/hal_traces`, one zip per backing model) —
a networked seam like `fetch.rs`'s `Http`, `#[ignore]`d live pull, feeding `decrypt_traces` → `convert`.
Then S4c (pair TRAIL-failing + resolved-gold ↔ HAL-passing same-task run on the task_id) and S5
(scoring, Wilson CIs, committed results, `report` snapshot).

## 049 · 2026-07-27 · HAL reference zips: pinned Hugging Face fetch + content-verified download (#41 S4b, slice 2 of 2)

S4b-fetch's second half. Slice 1 (048) decrypted a zip already on disk; this slice *acquires* it,
so the whole reference-side path is now one reproducible command — `hal-fetch` (pull the pinned
zips) → `hal-decrypt` (048) → `hal` ingest (047). The passing-run side of every #41 natural pair no
longer depends on a hand-download.

**The manifest, pinned from a live HF lookup (not guessed).** The HAL Open Deep Research GAIA
reference set is a **public, ungated** Hugging Face dataset (`agent-evals/hal_traces`) — verified via
the datasets API (`gated: False`), so no token. Pinned to revision `e7dcedc8…c71e`. It publishes **16
GAIA ODR zips**, one per backing-model run, **106 MB (o3mini-high) → 2.36 GB (claudesonnet45)**,
~10.8 GB total. `HAL_ODR_ZIPS` carries all 16 sorted **smallest-first** (a broken network or manifest
typo fails on the cheapest pull; the live test hits the 106 MB file). One faithful entry per run —
the full pool S4c's cross-model consensus draws from; *selection* is a `--model` substring filter on
the CLI, never baked into the adapter (benchmark policy stays downstream, per the S1 rule).

**Content verification, because the commit-pin alone can't catch a bad 500 MB transfer.** `fetch.rs`
leans on strict-JSON-parse downstream as its integrity net — impossible for a binary zip, and a
truncated half-gigabyte download is a *real* failure mode. HF exposes each file's **LFS SHA-256** (the
`x-linked-etag`), so the manifest pins it and the download is checked two ways before it may land:
byte count == `bytes` (truncation → `ShortRead`) **and** streamed SHA-256 == `sha256` (corruption/wrong
body → `Sha256`), *then* the `.part` → final atomic rename. This is also what makes skip-if-present
sound: a file under its final name was verified before it got there. Cross-checked the recipe against
the server — a `HEAD` on the smallest zip's `resolve` URL 302s to the CDN and its `x-linked-etag`
equals the pinned `sha256` byte-for-byte.

**The seam — `HalHttp::get_to(url, max_bytes, &mut dyn Write)`, separate from `fetch::Http` on
purpose.** That seam buffers a whole body into a `Vec` under a 64 MB cap and speaks GitHub's API;
a HAL zip is 106 MB–2.3 GB and streams straight to disk. `HfClient` (ureq 3, blocking — tokio stays
quarantined) copies `body.as_reader().take(max_bytes)` into the sink; `as_reader()` is unlimited (the
10 MB default cap only applies to the buffered `read_*` helpers), so `.take` is what actually bounds
the stream to the pinned size — the streaming analogue of `fetch`'s response cap. A `HashingWriter`
folds bytes into the SHA-256 *as they land*, so verification is single-pass, no re-read of the file.

**Decisions.** (a) Lives in `hal_fetch` beside the decrypt (048) and `fetch` (the HTTP/zip/crypto
deps already sit there); ingest stays the lean serde-only loader. (b) Reuses `fetch::HttpError` (same
crate, same shape) rather than a parallel type. (c) A `provenance.json` lands beside the cache
(dataset, revision, GAIA-lineage notice, per-zip file+sha256) — the honesty-in-artifacts rule; the
notice prints *before* any bytes move. (d) `bench/data/hal` is already under the gitignored
`bench/data` — GAIA-lineage data, never committed. No new deps: `zip`/`sha2`/`ureq` were all present.

**Tests — 9 offline + 1 `#[ignore]`d live.** Offline (fake streaming `HalHttp`, no network):
manifest well-formed (16 entries, unique files/models, 64-hex-lowercase shas, sorted smallest-first,
40-hex revision), `resolve_url` pinned shape, download→verify→land (real content hash of a fixed KAT
payload, pinned like the decrypt vector), skip-cached (no GET), **sha mismatch** and **truncated
short-read** and **http error** all leave no `.part`, provenance records revision+shas. The live pull
(`#[ignore]`d, operator-only) fetches the 106 MB smallest zip → `decrypt_traces` → `hal::convert_str`,
asserting ≥1 GAIA run, ≥1 passing, turns present — the whole path this slice exists to enable. Full
gate green (smoke + fmt + clippy `-D warnings` + `cargo test --workspace`, chimera_parity included).

**Unchanged risk (still S5's job).** This slice makes the reference *acquirable*; it does nothing
about the 047 granularity mismatch — HAL's Weave export is LLM-turns-only while TRAIL's o3-mini ODR
trace carries the full tool/sub-agent span tree, so the two sides of a pair still sit at different
granularities. That reconciliation is the pairing/scoring question S4c/S5 must measure, not a fetch
one.

**Next.** S4c — the TRAIL pairing builder (the `build.rs` analogue): join TRAIL-failing +
resolved-gold ↔ HAL-passing same-task run on the GAIA `task_id` (S4a), consensus-of-passing across
models to wash out model quirks. Then S5 (scoring, Wilson CIs, committed results, `report` snapshot).

## 050 · 2026-07-30 · TRAIL↔HAL natural pair builder (#41 S4c)

The `build.rs` analogue for the second natural-pair source: `amberfork_bench::build_trail` joins a
TRAIL failing trace to a HAL passing reference on shared GAIA `task_id`, writing the same
`pair_*.json` + `a_*`/`b_*` triples [`pairs::load_pairs`] already reads. Same "exclusions are data"
shape as Mode A′ — a trace or dump that cannot anchor a pair is a counted drop, never a silent skip.

**Gold step = earliest resolved TRAIL error, settled by the existing contract, not a new call.**
`pairs.rs`/`score.rs` carry and score one `usize` per pair, windowed ±1/±3 — 044 already flagged
"earliest resolved gold step" as the metric that fits that shape. So `build_trail` resolves a trace's
full `Vec<GoldStep>` (043/044) and keeps the minimum step; the full multi-error list stays available
for S5's separate predicted-∈-gold reading if that metric earns its place later, but this slice does
not carry it into the manifest — the seam stays single-`gold_step`, honest about what it does today.

**One reference per failing trace — a founder call, not assumed.** A GAIA task can have several
passing HAL models; pairing all of them would reuse the same failing trace across multiple rows,
breaking the i.i.d. assumption `score::wilson95` relies on. Asked and decided: **one reference per
failing trace**, lowest model name wins a multi-model collision (a deterministic tie-break with no
other meaning, mirroring `build::match_pairs`'s lowest-stem rule) — not the fan-out alternative, which
would have maximized N at the cost of a correlated-N problem S5 isn't designed to correct for yet.
Cross-model gold-quality risk (o3-mini TRAIL vs whichever model wins the tie) stays a later optional
arm, exactly as 046/047 flagged, not folded into this slice.

**Shipped:** `Reference`/`Failing`/`BuiltPair`/`FailingDrop` (mirroring `build.rs`'s `Reference`/
`Failing`/`BuiltPair`/`Drop`), `match_pairs` (pure, 7 unit tests: shared-task match carries the
earliest gold, a failing HAL run never serves as reference, an unmatched task is a counted drop, the
lowest-model-name tie-break including a passing-beats-higher-sorting-name case and a no-model-name
fallback to stem, and determinism under shuffled input), and `build_pairs` (reads a TRAIL traces dir +
matching gold dir by shared basename per 045's pinned layout, plus a directory of already-decrypted
HAL dumps — one file per backing model, `hal-fetch`→`hal-decrypt` output — writes the triples).
`cross_system: false` in the manifest, unlike Mode A′: both sides share the ODR scaffolding
(046/047), so this is a same-agent pair, not a cross-system one. Wired as
`amberfork-bench build-trail-pairs --traces --gold --hal --out`, the same shape as `build-pairs`.
Full gate green (smoke + fmt + clippy `-D warnings` + `cargo test --workspace` + `ui/` workspace).

**Not in this slice, on purpose.** No CLI integration test yet (`build_cli.rs` covers `build-pairs`
but not `build-trail-pairs` — worth adding before this runs for real) and no live end-to-end run
against the real ~105-pair overlap (046) — that needs actual `hal-fetch`/`hal-decrypt` output on disk,
an operator step, not a unit-test fixture.

**Next.** S5 — scoring, Wilson CIs, committed results, `report` snapshot, and the pre-registered
predicted-∈-gold metric choice 044 deferred. That is also where the cross-model gold-quality caveat
gets measured rather than just flagged.

## 051 · 2026-07-31 · S5 real run: two granularity bugs found and fixed, still an honest null (#41 S5)

**The operator run.** `amberfork-bench fetch` pulled the real TRAIL GAIA traces + gold (117/117,
GitHub, MIT) and Who&When/TapeAgents (already cached). For the HAL side, picked the backing model
by actual GAIA accuracy rather than the first zip in the manifest: HAL's own leaderboard puts
**GPT-5 Medium (`gpt520250807`) at 62.8%** — best of every Open Deep Research entry, ~103 passing
tasks vs the 54 the BENCHMARK.md o3-mini example covers — for an 880 MB download (sha256-verified
against the pinned LFS hash) instead of the full ~10.8 GB set. `hal-decrypt` → `build-trail-pairs`
against the fetched traces immediately hard-crashed: `read_failings` propagated a malformed-gold
parse failure as a `BuildError` instead of counting it, so the one upstream gold file with a
trailing comma (documented in `fetch.rs`'s own integrity test, notebook 050) took the whole build
down. Fixed — malformed gold now falls into the same `without_gold` bucket a missing file or an
unresolvable gold already does (`build_trail.rs`, one `Ok(gold) else { without_gold += 1; continue }`
swap); `build_trail_cli.rs` gained a third fixture trace with a trailing-comma gold file asserting
the drop, never a crash. Real build: **69 natural pairs** from 117 TRAIL traces (4 without a usable
gold step: the 3 known clean traces + the 1 malformed file; 44 unpaired — no passing GPT-5 run on
that task).

**First score was alarming, not just null.** All three real arms measured **0.00 exact/±1/±3**
against `random`'s 0.03/0.13/0.41 — the engine was *worse than guessing*. `amberfork diff` on one
pair showed why: origin step 00, conf 0.97 — sustained divergence from the very first step, no
recovery. Two distinct, stacked causes, both root-caused before touching anything:

1. **HAL logs the request, never the reply.** Weave records what the model was *asked*, but a
   tool/environment result is never its own record — it only exists as the extra messages a later
   turn's `inputs.messages` carries beyond what the previous turn already had. TRAIL's smolagents
   export, by contrast, gives every micro-action (llm decision, tool execution, harness log) its
   own step. A HAL reference was structurally ~half the effective step count of the TRAIL trace it
   was scored against, with no tool-result content anywhere to match. Fixed in
   `amberfork-ingest::hal`: for each turn, diff its `inputs.messages` against the previous turn's,
   and synthesize a `Tool` step (name `tool_result`, generic — the specific tool name is not
   reconstructed, not this slice's job) for every newly appended *non-assistant* message (the
   assistant's own message is skipped — it is already the previous turn's own step, just echoed
   back in history). Both OpenAI content shapes handled (`content: string` and the multimodal
   `content: [{type: "text", text: …}]` array TRAIL's tool-error turns use). Module docs and all 9
   `tests/hal.rs` cases updated; new `appended_non_assistant_messages_become_tool_steps` locks the
   dedup (the echoed assistant message must never earn a second step).
2. **TRAIL logs the harness, HAL doesn't.** Measured, not assumed: **69/69** TRAIL failing traces
   start with ≥1 content-free orchestration step (`main`, `get_examples_to_answer`,
   `create_agent_hierarchy`, …) — the Patronus SDK faithfully captures the smolagents harness's own
   bookkeeping spans; Weave never logs that layer at all. Every pair was therefore guaranteed to
   start misaligned (empty vs. real content) before any semantic comparison had a chance. Fixed —
   *only* inside `build-trail-pairs`, not the general `trail` adapter (which must stay a faithful
   full-tree export for every other consumer) — by trimming a run's leading content-free steps and
   re-resolving gold against the *trimmed* run, so `GoldStep::step` already reads in the trimmed
   index space with no manual offset arithmetic. `trim_leading_content_free` re-indexes `idx`
   sequentially and remaps `parent_idx` (a kept step whose parent was trimmed becomes a root; one
   whose parent survived is remapped to its new index) — 3 unit tests cover a mixed-position
   content-free step (must survive; trimming is a prefix operation, not a filter), the no-op case,
   and the all-content-free case. Mirrors the exact boundary `hal`'s own adapter already draws for
   its `litellm.completion` wrapper: drop the bookkeeping prefix, keep the content-bearing steps.

**Final measured result (both fixes applied, real 69-pair set, frozen params):**
exact 0.00 [0.00, 0.05], ±1 0.00 [0.00, 0.05], **±3 0.35 [0.25, 0.47]** — vs. random's
0.14/0.36/0.61 (n=69, split=all: 23 dev / 46 test). Real, non-zero signal now exists at the ±3
window where there was none before either fix — but the headline metric is still a genuine null,
and the engine still trails random at every window. Going further would mean changing the cost
model itself (how step similarity is scored), which BENCHMARK.md gates behind beating lexical on
dev fixtures first — a separate, larger piece of work, deliberately out of scope here. Honesty note
on protocol discipline: the diagnosis that motivated both fixes came from an all-split run and one
inspected test-split pair (`pair_00`); both changes are data-construction/ingest-fidelity fixes, not
cost-model or engine-parameter tuning, so they sit outside rule 1's "tuning happens on dev only"
prohibition — but the provenance is recorded here in the spirit of rule 3 regardless.

**Committed:** `bench/results/trail_hal_natural_all.json` (all 69 pairs, split=all, same-agent so
`cross_system: 0` and the ordinary chimera protocol, never the Mode A′ banner) plus
`report_reproduces_the_committed_trail_hal_natural_results_offline` (insta snapshot,
`bench_cli.rs`) proving it re-renders byte-for-byte offline. Full gate green (fmt, clippy
`-D warnings`, `cargo test --workspace`; the only failures are the pre-existing sandbox
127.0.0.1-bind ones CLAUDE.md already documents as unrelated to any code path touched here).

**Not in this slice, on purpose.** The predicted-∈-gold metric 044 deferred is still deferred. The
cross-model gold-quality caveat (GPT-5 reference vs whatever model TRAIL's own o3-mini ODR run used)
is flagged, not measured — a second, later arm if the natural-pair source earns further investment.
No cost-model change was attempted (see above).

**Next.** #41's epic is at a natural close for now: a real, honestly-measured (still null) result
is committed. #39 slice B (gen_ai) is next in the tracked epic order.

## 052 · 2026-07-31 · Native OTel GenAI ingest adapter ships (#39 slice B)

**What changed.** `amberfork_ingest::genai` closes the slice boundary 042 drew before either
adapter existed ("OpenInference now, native `gen_ai.*` next — same envelope, additive
vocabulary; not folded in"). It reuses the exact same OTLP/JSON envelope as `openinference`
(resource/scope flattening, `AnyValue` decoding, start-time ordering, `parentSpanId` →
`parent_idx`, `outcome` never inferred from span status) over a different attribute vocabulary:
`gen_ai.operation.name` `chat`/`text_completion`/`generate_content` → `kind=llm`,
`execute_tool` → `kind=tool` (named by `gen_ai.tool.name`), `create_agent`/`invoke_agent` →
`kind=agent` (named by `gen_ai.agent.name`); everything else (`embeddings`, and anything the
spec adds later) folds to `other` — the same "land on the canonical set, don't invent finer
structure" rule 042 applied to OpenInference's CHAIN/RETRIEVER/EMBEDDING.

**Two founder decisions, taken before writing code (per the collaboration-mode discipline: slice
boundaries proposed and decided before building).**
1. **Extract the shared envelope now, not later.** `openinference.rs`'s `RawExport`/
   `decode_any_value`/`normalize`/`build_run` moved into a new `otlp.rs`, generic over a
   `SpanToStep` function pointer (the vocabulary layer's mapping signature — `oivocab` and the
   new `genaivocab` both match it exactly). This is the envelope's *second* consumer, the same
   circumstance notebook 043 named for factoring out the vocabulary layer when TRAIL became its
   second consumer — so the same "factor out on the second consumer, not speculatively" rule
   argues for extracting, not duplicating. `openinference.rs` shrank to a two-function shell
   (`otlp::from_otlp_json_str(s, oivocab::map_span_to_step)`); its own 7 tests stayed green
   unchanged, proving the refactor behavior-preserving before any new code was added.
2. **Wrap a `messages` array as `{"messages": [...]}`.** `gen_ai.input.messages`/
   `output.messages` are spec-typed `any` — structured arrays of `{role, parts}` objects, or (on
   an SDK without structured-attribute support) a pre-serialized JSON string carrying the same
   array. `Payload` only field-diffs `Object`; a bare array would fall to `Payload::Other` and
   diff as an opaque blob. Wrapping keeps content field-diffable like every other structured
   carrier this crate handles, at the cost of an invented `messages` key not on the wire.

**A genuinely new mapping shape: the content carrier is kind-conditional.** Unlike
OpenInference's uniform `input.value`/`output.value` on every span, `gen_ai.*` splits the
carrier by operation: LLM/agent/other spans read `input.messages`/`output.messages`, but a TOOL
span reads `tool.call.arguments`/`tool.call.result` instead — `genaivocab::map_span_to_step`
branches on the already-computed `kind` to pick the right pair before decoding. `decode_content`
handles both carriers uniformly once selected: a structured array or object decodes directly, a
string is tried as JSON first (the SDK-fallback case) and falls back to `Payload::Text` only if
that fails.

**Fixture provenance — honest, and a flagged tension.** Like 042's OpenInference fixture, the
test export is spec-faithful but hand-authored, not sourced from a real instrumentation library —
parser correctness against a documented wire shape is not a "true by construction" risk the way
localization is. Worth flagging: the design doc's normalization-layer rationale (line 671) says
"pin to instrumentation-library versions, not the spec," because the GenAI semconv is
"Development," unversioned, and had 4+ breaking changes in 12 months — exactly the churn that
made the old per-role event convention (`gen_ai.user.message`/…) obsolete before this slice even
started (confirmed live against the current spec: consolidated into span attributes,
`gen_ai.client.inference.operation.details` replacing the per-role events). This slice targets
the current spec directly rather than a specific library's emitted shape, because no widely-used
gen_ai.*-native library trace was sourced to pin against (native `gen_ai.*` emitters are still
rare — PydanticAI, Semantic Kernel — per the design doc's own note). If a real one surfaces, it
is the fixture to validate against next, the same way TRAIL later validated the OpenInference
vocabulary against real logs.

**Tests (8, mirroring the `openinference` discipline plus one for the array-wrap):** kind/name/
idx/parent wiring under out-of-order + multi-trace spans, including the `embeddings` → `other`
fold; the kind-conditional carrier switch (LLM/agent read messages, tool reads call
arguments/result) with both a structured-array and a pre-serialized-JSON-string input each
decoding the same way; a dedicated test pinning the array→`{"messages": [...]}` wrap; recognized-
vs-foreign attribute split + warning; content-absent advisory; the canonical round-trip guard;
parse-error and empty-export paths. All 8 passed on the first run. Full gate green (fmt, clippy
`-D warnings`, `cargo test --workspace` incl. `ui/`).

**Docs reconciled.** `docs/trace-format.md` Mappings: `gen_ai.*` moved from "planned" (with the
stale "opt-in content events" phrasing, superseded by the spec consolidation above) to
"implemented" with the actual kind-conditional carrier mapping. Design doc Current State: "native
`gen_ai.*` is the next slice" → "native `gen_ai.*` ingestion shipped 2026-07-31."

**Not in this slice, on purpose.** CLI auto-detection (`amberfork diff trace.otlp`) — same
deferral 042 drew for OpenInference. Structured-message reconstruction beyond the wrap (e.g.
per-message field diffing inside the array) — the wrap makes the whole array one field-diffable
unit, not per-message. RFC3339 timing — same as OpenInference, raw nanos only.

**Next.** #39 is closed: both the OpenInference and native GenAI on-ramps now exist. Remaining
v0.8 "credibility pass" items per the tracker: #44 (real-provider `--verify` e2e), #43 (cassette
redaction).

## 053 · 2026-08-01 · Real-provider `--verify` e2e closes notebook 038's coverage hole (#44)

**What prompted it.** Notebook 038 named the one deliberate testing boundary in counterfactual
attribution: no test had ever driven `diff --verify` through a real subprocess *and* a real network
provider. Every mechanism piece (patch → drive → oracle → consensus) was covered offline with
in-process stubs; the CLI's own two production seams — `SubprocessDriver` and `LiveUpstream`
(`crates/amberfork/src/verify.rs`) — had never actually run.

**What shipped.** `crates/amberfork/tests/verify_cli.rs`, one `#[ignore]`d, network-gated test
(same discipline as `amberfork-bench`'s three ignored network tests) plus a toy stdlib-only Python
agent (`tests/fixtures/verify_agent.py`) that talks to a local Ollama server's `/api/generate`.
The agent's second-turn prompt embeds the first turn's answer verbatim, so once `--verify` patches
turn 0's response, turn 1's request genuinely changes and cache-misses the replay tape — forcing
the live relay to a real upstream, the one code path the offline suite structurally cannot reach.

**Decisions worth keeping.**
- *Local Ollama, not a paid API.* The issue's own wording ("real — or realistic local — provider")
  and the project's local/offline trust story both point the same way: no API key, no cost, no
  rate limit, and `--upstream`/`--base-url-env` are already provider-agnostic by design (confirmed
  against `main.rs`'s help text) — Ollama's OpenAI-compatible-in-spirit `/v1`-adjacent surface is
  just another origin. `smollm2:135m` (270 MB) keeps the pull cheap.
- *The fork must come from real sampling variance, never a scripted difference.* `patch_cassette`
  (patch.rs) only ever swaps a *response* onto an unchanged *request* — grafting the good run's
  answer onto the bad run's identical question. So `good` and `bad` run the exact same prompts;
  divergence has to come from the model itself, which is also the honest scenario `--verify`
  exists for (a flaky agent, not a scripted one). Temperature is turned up and the `bad` recording
  retries up to 6× if the model happens to answer identically — a bounded, honest allowance for
  real nondeterminism, not a flaky test papered over.
- *The test asserts the pipeline ran, not what the model decided.* It checks `attribution.mode ==
  Counterfactual` and `counterfactual.runs == 3` — proof the real subprocess + real network path
  produced *some* tri-state verdict — but never asserts `Recovered` vs `NotRecovered`. Asserting a
  specific verdict would be asserting a fact about the model's sampling, not about the pipeline;
  the tri-state's whole point (notebook 038) is that `Unverified` is an honest value, not a bug.
- *Recipe lives in the test file's own doc comment*, matching how `record_cli.rs` and
  `driver.rs` already document their own test rigs inline rather than in a separate doc.

**Verified.** Ran twice against a real local Ollama server (`smollm2:135m`): both times a genuine
fork was found within the retry budget and the pipeline returned a real verdict (`Recovered` both
times, 3.8s and 35.7s). `cargo test --workspace` (the new test excluded by default, 0 failures) and
`scripts/verify.sh --full` both green.

**Next.** #44 closes; the last open v0.8 "credibility pass" item is #43 (cassette redaction).

## 054 · 2026-08-01 · v0.8.0 released; amberfork-judge skeleton — trait, scripted double, grounding guard (#10 slice A)

**v0.8.0 released.** All four milestone issues (#39, #41, #42, #44) were closed but unreleased —
still tagged `v0.7.0`, empty `CHANGELOG.md` `[Unreleased]` section. Cut properly: workspace version
0.7.0 → 0.8.0 (`Cargo.toml` + the 8 internal path-dep pins), a `CHANGELOG.md` entry summarizing the
milestone's actual measured results (041's 5/6 exact ReAct generalization, 042/052's OpenInference
+ native `gen_ai.*` adapters, 051's 69-pair TRAIL↔HAL result — honest null on exact/±1, real signal
at ±3 — 053's real-provider `--verify` e2e), `CLAUDE.md`/`CONTRIBUTING.md`'s "10 crates at vX.Y.0"
line bumped, tag `v0.8.0` pushed (commit `cd760c8`). `release.yml`'s tag guard + both platform
builds + `gh release create` all green; binaries attached. (053's own "last open v0.8 item is #43"
line is now read as loose phrasing, not milestone scope — #43 was filed un-milestoned backlog per
notebook 040, and the GitHub milestone shows exactly #39/#41/#42/#44, matching 041's explicit "v0.8
now = #42 → #39 → #41, plus #44" scope statement.)

**v0.9 — the explain layer, slice A.** #10 is the lower-numbered of the milestone's two open
issues (#40 is the other). Slice boundary proposed and founder-approved before building: crate
skeleton only — trait + in-process test double + grounding logic — no CLI flag, no live provider,
no network. That is slices B and C.

**What shipped.** `amberfork-judge`, the 11th crate. Three pieces:

- **`ExplainContext::windowed(result, a, b, k)`** (`context.rs`) — the *only* content a judge can
  ever see: the fork step + `k` neighbours on each side that has one (`Fork::a_step`/`b_step`),
  never the two full trajectories. A converged result windows to empty. This is guardrail #3 from
  the issue (a judge can't hunt for a different fork) enforced by what the type *can* hold, not by
  convention.
- **`Judge`** (`judge.rs`) — one method, `explain(&ExplainContext) -> Result<Explanation,
  JudgeError>`, written as a native async trait (return-position `impl Future`, not
  `async-trait`) — the exact shape `amberfork-replay::Upstream` already established for this
  workspace's "pick the implementation at compile time, no `dyn`" I/O-edge pattern. `Explanation`
  carries a claimed `fork_index`, a narrative, and a `speculative_fix` kept in its own field so a
  future CLI slice can label it separately from the grounded narrative, per the issue's guardrail
  #4.
- **`ground(result, explanation)`** (`grounding.rs`) — the actual enforcement of guardrail #1
  ("the aligner stays the headline; the AI layer never localizes"). Not a parser over free text:
  `Explanation::fork_index` is a structured claim, checked by equality against
  `DiffResult.fork.map(|f| f.index)`. Four cases, all tested: matching claim on a real fork →
  grounded; wrong index → rejected; silence on a real fork → rejected; a fabricated fork on a
  converged result → rejected. A real provider (slice C) has to emit a fork index as data, not
  prose the CLI would need to regex — cheaper and more honest than the "parse model output"
  wording in the issue's own draft suggested.
- **`ScriptedJudge`** (`scripted.rs`) — FIFO in-process double, same shape as `ScriptedUpstream`
  (`Mutex<VecDeque<...>>`, `Exhausted` on underrun), so this crate's own tests and slice B's CLI
  tests stay offline.

**Decision taken, not yet needed.** `GroundingError` started as an enum with one variant
(`Ungrounded { claimed, actual }`) to mirror `UpstreamError`'s shape, then was flattened to a plain
struct once it was clear there was only ever going to be the one failure mode — a single-variant
enum was ceremony, not a real closed set (unlike `JudgeError`, which already has two: `Exhausted`
and the future `Unreachable`).

**Tests.** 11 unit tests (`context`: converged-empty, k-neighbour coverage, bounds-clamping,
model-only-fork windows only the `a` side; `grounding`: the four cases above; `scripted`: FIFO
order, exhausted). No new dependency beyond `amberfork-model`; `tokio` is a dev-dependency only
(`.await` in tests), matching how `amberfork-attrib`/`amberfork-replay` scope their test-only
runtime. Full gate green (`fmt`, `clippy -D warnings`, `cargo test --workspace` incl. the new
crate, `ui/` workspace) — `amberfork diff` output is untouched, nothing wired to it yet.

**Next.** Slice B: `--judge local|off` on `diff`/`demo`, off by default, rendered under an `AI
(unverified):` label below the deterministic fork, using `ScriptedJudge` so CLI tests stay offline.
Slice C (later): a live provider — founder's call was Ollama (a local server), not a bundled
model, so no new mandatory dependency lands in the default (`--judge off`) path.

## 055 · 2026-08-01 · `--judge local|off` ships — Ollama explain layer on diff (#10 slice B, first slice closes)

Merged what 054 called slices B and C into one: the founder's call (asked before building) was
that landing `--judge local` without a working provider behind it would be dishonest UX — a flag
in `--help` that silently does nothing until a later slice. So this slice pairs the CLI flag with
`OllamaJudge`, the real local provider, in one commit (9e20446).

**What shipped.**

- **`OllamaJudge`** (`amberfork-judge/src/ollama.rs`) — POSTs to a local Ollama server's
  `/api/generate`, the exact endpoint/shape `verify_cli.rs` (#44) already established as this
  workspace's local-provider convention, reusing that precedent rather than inventing a second
  one. Converged results never dial out (`fork_index: None` short-circuits before a request is
  built). `Explanation::fork_index` is set from the `ExplainContext` the caller already has, never
  parsed from the model's answer — the model is never even asked to name a location, so 054's
  grounding guard is enforcing an invariant the provider can't violate by construction, not just
  catching it after the fact.
- **`prompt.rs`** — pure, offline-tested prompt builder. Caps each step's payload preview at 400
  chars (a local model's context window is small; an unbounded tool payload could both blow up the
  prompt and bury the actual divergence content) and never prints a step index in the rendered
  text, tested directly (`the_prompt_never_mentions_a_step_index`).
- **CLI** — `--judge local|off` on `diff` only (not `demo`/`serve`; keeps the vertical slice to the
  one place a real reference/observed pair exists to narrate). Default `off` is verified
  byte-identical: the full pre-existing `diff_cli.rs` suite runs unmodified and green. Printed
  under `AI (unverified):`, dimmed like the demo hand-off line — `ColorMode::dim` was already
  crate-visible for exactly this "main styles chrome" case. Skipped entirely under `--json` (the
  explain layer stays outside the machine contract, per 054's Approach A→C growth note — no
  `schema_version` bump here) and under `--verify` (an explicit stderr warning, not silent
  nothing: "`--judge` has no effect with `--verify` yet").
- **A real bug, caught by manually running the built binary against your own Ollama server before
  calling this done** (CLAUDE.md: type-check the intent, not just the types): `reqwest`'s
  `connect_timeout` schedules a timer on the tokio runtime, and the first runtime builder here only
  had `.enable_io()` — worked until the very first real HTTP call, then panicked
  (`"A Tokio 1.x context was found, but timers are disabled"`) instead of degrading. Fixed with
  `.enable_time()` + the `time` tokio feature on the CLI crate. Caught by running `amberfork diff
  --judge local` against the demo fixture by hand, not by any test — worth remembering next time a
  slice adds a new runtime/client construction, since `cargo test` alone did not catch it (the
  offline tests never reach a real connect attempt).

**Verified live, twice.** Founder's machine already had Ollama + `smollm2:135m` pulled from #44's
setup. Ran the built binary directly: a real forked diff produced a real (rambling, as expected
from a 135M model — quality isn't this slice's job) narrative under `AI (unverified):`; a self-diff
answered "no divergence to explain" with no network call; `--json --judge local` stayed pure JSON.
Then ran the new `#[ignore]`d `judge_local_narrates_a_real_fork_against_a_real_local_provider` test
for real (`cargo test -p amberfork --test judge_cli -- --ignored`) — passed.

**Tests.** 2 new unit tests in `amberfork-judge` (`ollama::tests`) targeting `http://127.0.0.1:1`
— nothing listens there without root, so "provider unreachable" is deterministic regardless of
whether the test machine happens to have a real Ollama server running (unlike port 11434, which it
does). 3 new offline CLI tests in `judge_cli.rs` (default-off, converged-needs-no-network,
json-is-a-no-op) plus the 1 ignored real-provider e2e. Full gate green (`fmt`, `clippy -D
warnings`, `cargo test --workspace`, `ui/` workspace).

**Known gap, named rather than silently covered.** The `--verify --judge local` stderr warning has
no dedicated test — wiring a full cassette-backed `--verify` scenario just to check one warning
line felt like more scaffolding than the message is worth. Verified by manual read of the code
path, not automated.

**Not in this slice, on purpose.** `demo`/`serve` don't get `--judge`. No configurable
Ollama URL/model (hardcoded to the #44 convention: `127.0.0.1:11434`, `smollm2:135m`) — a one-line
addition later if wanted, deliberately not built ahead of a real request for it. `speculative_fix`
stays unpopulated (`OllamaJudge` never asks for one; "one call, one paragraph" per the issue's own
Approach A scoping).

**#10's first slice (issue's own "Approach A — minimal") is now fully closed**: crate, trait,
grounding guard, CLI flag, real local provider, all edge cases from the issue (converged,
unreachable, `--json`) handled. The issue's own growth path (A→C: `narrative` as a schema field;
C→B: `amberfork chat`) stays documented, not built — next milestone work is #40 (cost/token/latency
deltas), the other open v0.9 issue.

## 056 · 2026-08-01 · Latency/token deltas land on `DiffResult` (#40 slice A of 3)

Issue #40 asked for cost/token/latency deltas "already carried in `Step.attrs`" — that framing
turned out wrong on inspection. No adapter in this workspace attaches a dollar figure to
anything; token usage lives in `outputs.usage` (HAL/OpenAI-shaped adapters), not `attrs`; latency
is real via `Step.t_start`/`t_end` but most adapters (`whowhen`, `tape`, `build_trail`, `record`)
leave those `None`, and `otlp.rs` deliberately declines to fabricate RFC3339 from raw nanos.
Proposed a 3-slice split before building (compute+schema / terminal / web UI) with cost deferred
to its own follow-up — no pricing-table design exists anywhere yet, and inventing one wasn't this
slice's job. Founder picked that split over folding in a hardcoded price table or cutting to
latency-only.

**What shipped (slice A).** `DiffResult.deltas: Option<ResourceDeltas>` — additive, no
`schema_version` bump, same pattern as `Attribution.counterfactual`. Two granularities, both
`b − a` (observed minus reference, matching `FieldDiff`'s before/after convention): `total`
(whole-run wall clock + summed tokens) and `at_fork` (the single diverging step pair, only when
the fork lands on a synchronous move — a log/model-only fork has no counterpart step, so
`at_fork` is `None` by construction, never a fabricated `0`). Lives in `amberfork-align/src/
deltas.rs`, computed alongside `field_diffs`/`fork` in the crate's one assembly point
(`diff.rs`), no second pass needed (unlike attribution, which needs the already-assembled
result).

**The RFC3339 question.** Nothing in this codebase parses timestamps today — confirmed by
reading every ingest adapter; `otlp.rs`'s own comment says raw nanos ride in `attrs` specifically
*because* the crate declined to synthesize RFC3339. Latency needed a parser to exist at all.
Chose to hand-roll one (`Z`-suffix-only, Howard Hinnant's public-domain `days_from_civil` for the
calendar math) over adding `chrono`/`time` — both carry real transitive deps, and this workspace's
own precedent (`amberfork-judge`'s Ollama slice: "no new mandatory dependency lands in the default
path") reads as a standing anti-dependency bias, not just a per-slice call. Correctness
cross-checked in tests against Python's `datetime` on five reference dates spanning both leap-year
rules (2000 is one, 1900 isn't). Anything outside `Z`-suffixed UTC (an explicit offset, a
malformed field) returns `None` — degrades to "no latency signal for that step," never a wrong
answer. Flagged this choice explicitly for founder review rather than assuming it; approved as-is.

**Tests.** 11 new unit tests in `amberfork-align::deltas` (parser correctness, total/at-fork
computation, the log-only-fork-has-no-counterpart case, the prompt+completion-tokens fallback).
`amberfork-model/tests/diff_result.rs` gained a populated `deltas` in the schema's "exercises
every branch" fixture, plus dedicated coverage that a `None` sub-field (e.g. `at_fork.tokens`) is
omitted from the wire form, not serialized as `null`. Every existing `DiffResult` test-literal
across 8 files needed a mechanical `deltas: None`. Full gate green (`fmt`, `clippy -D warnings`,
`cargo test --workspace`, `ui/` workspace).

**Not in this slice, on purpose.** Terminal rendering and the web fork view (slices B and C) —
the field exists and computes correctly but nothing prints it yet. Cost stays deferred to its own
issue pending a pricing-table design decision.

## 057 · 2026-08-01 · Terminal renders latency/token deltas (#40 slice B of 3)

Slice B of the 3-slice split from 056: `DiffResult.deltas` now reaches the terminal.

**What shipped.** `amberfork-layout::ViewModel` gains `deltas: Option<DeltasView>`, built the
same way `AttributionView` is — pre-formatted strings so no painter reinvents the sign/units
convention (`+5.20s`, `+120ms` below the one-second mark so a fast local step doesn't round to
`+0.00s`, `+300 tok`). `amberfork/src/render.rs` prints it as its own footer block, right after
the attribution line when both are present, styled identically (plain, no color — DR2's
"structure carries the signal" rule holds here too). Deliberately independent of fork state: a
converged diff still prints its total delta alone, since "same behavior, cheaper" is a real
answer to UC1, not just "same behavior, regressed."

**Mechanical fallout.** `ui/`'s `ViewModel` literals needed `deltas: None` to keep compiling —
same pattern the counterfactual-verdict field set: "the web pane doesn't render this yet, its own
slice; the field exists on the contract so payloads carrying one still deserialize." No rendering
logic added to `ui/` (that's slice C).

**Tests.** 8 new unit tests in `amberfork-layout` (formatting edge cases for both units, an
empty-delta-has-no-text case, and two end-to-end `ViewModel::compute` checks — deltas carried
through and deltas absent). 3 new tests in `amberfork`'s render suite via the existing
`paint()` pipeline: forked with both total and at-fork segments, converged with a total-only
delta, and a no-deltas-means-no-line negative case. Existing insta snapshots untouched (none of
their fixtures set `deltas`, so nothing to regenerate). Full gate green (`fmt`, `clippy -D
warnings`, `cargo test --workspace`, `ui/` workspace).

**Next.** Slice C: the web fork view actually renders `DeltasView`, mirroring how
`ui/src/attribution.rs` renders `AttributionView` as a `<dl>`.

## 058 · 2026-08-01 · Web fork view renders deltas — #40 closed (slice C of 3)

Slice C, the last of the 3-slice split from 056. The attribution pane gets a "Deltas"
subsection below "Attribution"; #40 is now fully shipped end to end (compute → terminal → web).

**Design call, made deliberately.** Pulled in the `frontend-design` skill first — this is new
visual surface, and `.claude/rules/ui.md` asks for it proactively even for small additions. The
question that mattered: how does a second, subordinate section under an existing pane title read
as "supplementary evidence" rather than a competing header? The wrong answer is a new, fainter
type style for "subsections" — that's exactly how a design system quietly grows a second voice
over time. Landed instead on: reuse `.attr-title`'s type rule verbatim for "Deltas" (identical
size/weight/case/color to "Attribution"), and carry the hierarchy through structure alone — a
hairline divider using the pane's own existing `--hair` border token (not a new value) plus a
semantic step-down to `<h3>`. Zero new colors, typefaces, or signature elements.

**What shipped.** `Attribution` component gains `deltas: Option<DeltasView>`; a new
`deltas_section()` renders `total`/`at fork` as `.attr-row`s inside a `.attr-section` wrapper,
reusing `.attr-list` verbatim so `tabular-nums` applies for free (DESIGN.md's requirement for
timing/cost/tokens). `App` threads `model.deltas.clone()` through the same way it already does
`attribution`. Shown independent of `attribution`/`verdict`, matching the terminal's call: a
converged diff still has a total delta worth seeing.

**Verified live, not just by tests.** `trunk serve` alone can't show real data — the `csr` app
fetches `/api/document` from a running `amberfork serve` backend, which always fails in this dev
checkout (`ui-dist/` gitignored → `BundleMissing`). Built a throwaway example
(`ui/examples/deltas_preview.rs`, deleted after use) that renders the real `App` component via
the same SSR path the test suite uses, over a fixture carrying both attribution and deltas, and
wrapped the output in `index.html`'s actual `<head>` — a real browser screenshot of the real
component tree and the real CSS, not a mockup. Confirmed the divider + heading-level treatment
reads as intended and DR2's containment holds (amber only the fork, red/green only the field
diff).

**Tests.** 4 new unit tests in `attribution.rs` (both rows render, no section when the view is
`None`, a converged diff still shows its total). 1 new App-level integration test
(`app_threads_deltas_from_the_document_into_the_pane`) proving the prop actually threads through
`App` → `Attribution`, not just that the pane renders one in isolation — the same gap-check
pattern `app_opens_on_the_attribution_answer` already covers for attribution. Clippy clean on
both `ssr` and `csr` feature sets (the two-mutually-exclusive-builds gotcha `ui.md` flags — a
change can pass one and silently break the shipped wasm build). Full gate green.

**#40 is closed.** Cost stays out of scope, tracked separately: no adapter in this workspace
attributes a dollar figure to a step, and building one needs its own pricing-table design
decision this issue never asked for.

## 059 · 2026-08-02 · `amberfork diff --html` ships — self-contained static export (#29)

With v0.9 empty, went to the tracker's lowest-numbered unblocked backlog issue: #29, deferred
since v0.5 (notebook 1506 area) once the view-model seam (#21/#24) it depends on landed.

**The dependency question, asked before building.** The obvious implementation reuses
`amberfork-ui`'s real Leptos component tree — its `ssr` feature already runs natively for host
tests, so `App` renders to an HTML string with no wasm involved. But pulling `amberfork-ui` into
`crates/amberfork` means `leptos` — a full reactive framework, ~40 transitive crates including
`wasm-bindgen-futures`/`js-sys` even under `ssr` — lands in the *shipped* `amberfork` binary,
which today carries none of it. Ran `cargo tree` to confirm the size before asking, then asked:
reuse the real component tree (guaranteed parity, new dependency) or hand-roll a standalone
renderer (zero new dependency, a second implementation to keep in sync). Founder chose hand-roll.

**What shipped.** `crates/amberfork/src/html_export.rs` — plain `format!`/`write!` HTML
generation over the same `ViewModel` seam `amberfork-layout` already computes, reusing the real
CSS class names (`row--fork`, `attr-list`, `content-diff-del`, …) so the export looks identical
to the live view without executing any of its component code. CSS itself is `include_str!`'d
straight from `ui/index.html` at compile time — the export can't visually drift from the live
view's actual stylesheet, only from its component logic (which this hand-written version
deliberately re-derives, the one place drift risk actually lives). `--html <path>` on `diff`
only (not `demo`, matching `--judge`'s precedent for scoping a flag to where a real
reference/observed pair exists); independent of `--json`/`--verify`/`--judge`, always attempted
when given; a write failure is `EXIT_TROUBLE`, not swallowed — the file was explicitly requested.

**Scope line: terminal fidelity, not SPA fidelity.** The export mirrors what the terminal
already renders — rows, the fork's field-diff evidence, attribution, deltas — not the full
interactive canvas (no SVG spine geometry, no per-row selection). That's a deliberate cut, not
laziness: replicating `canvas.rs`'s precise geometry (`row_ys`, the amber connector path) by hand
would be real scope creep for a "paste into a GitHub issue" artifact whose value is the same
information the terminal already carries, just as browsable, stylable HTML.

**No interactivity, and no pretending otherwise.** A second UX question, asked before building:
the exported markup has no JS at all, so a literal copy of the live view's markup would carry
`tabindex`/`role="option"`/a Copy button that visually invite clicks and do nothing. Founder's
answer (asked when scoping, before this hand-roll decision even changed the mechanism): add an
honest note rather than silently leaving dead affordances. Landed on the cleaner version once
hand-rolling was already decided: omit the affordances that would be fake (no tabindex, no
`role="option"`, no Copy button — it would need bespoke inline JS to work, out of scope) and add
a `.static-note` banner up top naming `amberfork serve` as the live-view path. Same honesty
principle, better execution once the constraint set changed.

**One fidelity call beyond pure terminal mirroring:** the counterfactual verdict segment
(`recovered · 3 runs`) the terminal already appends to the attribution line but the *web* pane
doesn't render yet (058's own deferred slice) — the export renders it anyway, since it's
following the terminal's fidelity bar, not blocked on the web SPA catching up.

**Tests.** 6 unit tests in `html_export.rs`: HTML-escaping of untrusted trace content (a step
name containing `<script>` must not break the page), self-containment (no `<link>`, no
`<script>`, no network URL — verified against the real rendered output, not assumed), converged
vs. forked rendering, and a subtlety worth naming — asserting "no fork row" or "no copy button"
by bare substring search fails, because the shared CSS itself *defines* `.row--fork`/
`.content-diff-copy` as selectors whether or not any element uses them; the tests check for the
actual `<li class="row row--fork"` / `<button` markup, not the class name in isolation. 3 CLI
e2e tests in `html_export_cli.rs`: the file writes alongside an unaffected terminal render,
`--html` combines cleanly with `--json` (both outputs land, neither suppresses the other), and an
unwritable path exits `EXIT_TROUBLE` with the path named on stderr. Full gate green, `ui/`
workspace untouched (this slice never depends on it).

## 060 · 2026-08-09 · Record-time privacy warning ships — #43 closed (minimum scope)

Picked over the other three open backlog issues deliberately: #43 is `safety`-tagged, and unlike
#30/#31/#45 (UX polish or a new capability), it names a real gap in something already shipped —
`amberfork record` has captured full, unredacted provider bodies since v0.6 with no warning that
a cassette isn't secret-safe to share.

**Scope check, before building.** The issue offers two bars: minimum (a loud warning + a
documented privacy contract) and better (an opt-in redaction pass over bodies). Headers were
already fully allowlisted (`cassette.rs`'s `credential_headers_never_survive_capture` test
predates this slice). The remaining gap is bodies, which are captured raw by design — full input
fidelity is the record path's entire reason to exist over the passive OTel path. Redacting bodies
is a real design problem of its own (secret-pattern matching over arbitrary JSON: a false
negative is a live leak, a false positive corrupts replay) and the issue's acceptance criteria
already treats it as conditional ("if built"). Shipped the minimum bar as its own complete slice;
the redaction pass stays a separate, later decision rather than scope creep onto this one.

**What shipped.** `run_record` in `crates/amberfork/src/main.rs` prints a stderr warning
immediately after the capture proxy binds and before the wrapped agent runs — its own line, ahead
of any output the agent produces, so it can't get scrolled past. `docs/cassette-format.md`'s
existing body-redaction disclosure (previously a paragraph buried mid-document under "Credentials
are never recorded") is promoted to a named `## Privacy contract` section the warning links to by
anchor, wording matched between the two ("do not share... without scrubbing").

**Tests.** One new CLI e2e test in `record_cli.rs`
(`warns_that_the_cassette_is_unredacted_before_the_agent_runs`) driving a real `record` session
against a stub upstream and asserting the warning lands on stderr. Full gate green (`fmt`,
`clippy -D warnings`, unit tests, CLI e2e, `ui/` workspace).

## 061 · 2026-08-09 · Addressable payload slots — #30 slice A of 3

Next by the lowest-numbered-unblocked rule: #30, expand-on-demand for truncated payloads. Scoped
it before building — the issue only names "server+ui," but truncation turned out to be
destructive (`Document::new` cuts `SlotText` in place, discarding the original bytes) and no slot
had a stable address to ask for its full text by, so a real prerequisite piece lives in
`amberfork-layout` first. Split into three: layout/model (this slice), server (hold the
pre-envelope view, add a fetch route), UI (wire the inert `.slot-trunc` marker to a click →
fetch). Confirmed the split and the addressing shape with the founder before writing code.

**What shipped.** `SlotText` gains `address: Option<SlotAddress>`, `Some` exactly when
`truncated` is. `SlotAddress { row, kind }` names a row index plus a `SlotKind` —
`StepSummary{side}`, `ForkSide{side}`, `FieldRemoved{path}`, `FieldAdded{path}` — reusing the
existing side/path vocabulary (`Side` mirrors the `DiffResult` a/b convention;
`FieldDiffView.path` already existed) rather than inventing per-slot UUIDs. `envelope()` and
`envelope_step()` now enumerate row index and stamp the right address into `truncate_to` via a
closure, so building a `SlotAddress` (a field-diff one clones a path string) costs nothing on the
overwhelmingly common under-limit path. `ViewModel::full_text(&self, &SlotAddress) -> Option<&str>`
resolves an address back to the untruncated text — but only against the view `ViewModel::compute`
produced, before `Document::new`'s envelope ever ran. The truncation mechanism itself is
unchanged: same limit, same in-place cut, same `truncated` marker every existing consumer (CLI,
`html_export`, the web UI) already reads. This slice only gives the marker a return address; slice
B decides how the server actually keeps that pre-envelope view around to answer one.

**Tests.** Extended the existing over-limit test to assert each truncated slot gets its own
distinct address, never aliased between slots that happen to share source text (`side_a` and the
matching step summary here). Two new tests: `full_text` round-trips every slot kind back to the
original — including the wrinkle that a field-diff slot's "full text" is its compact-JSON display
form (quotes included), not the bare payload — and returns `None` for a stale or malformed
address rather than panicking (out-of-range row, a fork-only kind against a plain step row, a
field path that was never in that row's diffs). Full gate green.

## 062 · 2026-08-09 · Expand-on-demand payload endpoint — #30 slice B of 3

Slice B of the 3-slice split from 061: the server side. `Server::bind`/`bind_with_assets` now
take the pre-envelope `ViewModel` alongside the `Document`, both held behind a new `ServerState`.
A new route, `POST /api/payload`, accepts a `SlotAddress` JSON body — the exact type a truncated
slot already carries — and resolves it against the full view via slice A's `ViewModel::full_text`,
answering `{"text": "..."}` or `404`.

**Design call, made deliberately.** `POST` with a JSON body rather than a `GET` with path
segments: a field-diff `SlotAddress`'s path is an arbitrary string that doesn't URL-encode
cleanly, and `SlotAddress` already has `Serialize`/`Deserialize` — the web painter can echo back
the exact object it read off the document rather than the server inventing a second encoding for
the same data. `404` is one status for every way an address fails to resolve (stale, wrong kind,
out of range) — none is actionable differently by a client that only ever sends addresses it just
read off a real document.

**Security, checked not assumed.** The payload route sits inside the same
`.layer(require_local_host)` as every other route — it exists specifically to serve content the
document endpoint deliberately withholds, so it's the highest-value thing that DNS-rebinding
guard protects, and it now has its own test proving that rather than inheriting the assumption.

**The caller.** `run_serve` in `crates/amberfork/src/main.rs` clones the view before
`Document::new` consumes and truncates its own copy — one line, exactly the shape slice A's own
tests and doc comments already described.

**Tests.** 3 new e2e tests in `amberfork-server/tests/serve.rs`, over the real HTTP wire like the
rest of that suite: resolve a genuinely truncated slot end-to-end (read the served document, pull
a real address off it, POST it back, get the exact original text), 404 on an address that doesn't
resolve, 403 from a foreign `Host` header. The other 11 pre-existing tests needed only mechanical
signature updates (`document()` split into `full_view()` + `document()`) and passed unchanged
once a refactor slip in the shared request-sending helper was caught by the pre-existing ETag
test (a dropped `extra`-headers parameter — the fast feedback loop of running the suite after
each edit earned its keep here). Full gate green.

## 063 · 2026-08-10 · Click-to-expand ships in the content-diff pane — #30 closed (slice C of 3)

Slice C, the last of the 3-slice split from 061: the web painter. `Slot` (`ui/src/slot.rs`)
renders a payload's text plus, when it's truncated and carries a real `SlotAddress`, a genuine
`<button>` that `POST`s the address to `/api/payload` on click, swaps the truncated text for the
full response, and removes itself — never leaves a dead affordance next to text that no longer
needs expanding.

**A real finding from live testing, not assumed correctness.** First pass wired `Slot` into both
consumers — the canvas's row summaries and the content-diff pane's field values, which rendered
byte-identical inert markup before this issue. SSR tests (the crate's usual bar) passed for both.
But this feature's whole point is a real click firing a real fetch, which SSR strings can't
exercise — so built the actual wasm bundle with `trunk build --release`, staged it into
`amberfork-server`'s (gitignored) `ui-dist/`, and ran a real `amberfork serve` against fixtures
with genuinely oversized payloads (crafted by hand per `docs/trace-format.md`), driven through a
headless browser via the `browse` skill. Result: a real pointer click on the canvas button timed
out. Root cause, confirmed via computed styles: the row's `.sum` cell is `overflow: hidden;
text-overflow: ellipsis; white-space: nowrap` — a deliberate one-line-gist layout (`StepView`'s
own doc comment calls it that) — which clips a real click target away along with the overflow.
Not a regression (the old inert mark had the identical clipping problem, just invisibly, since
nothing was ever interactive there before), but shipping a button that looks clickable and mostly
isn't is worse than the honest inert mark it would replace — the same "no fake affordances"
principle 059's `--html` export slice already established. Flagged it with the screenshot and a
concrete recommendation rather than picking a direction unilaterally; founder chose scoping the
live affordance to the content-diff pane only, which has no such clipping (`.content-diff-val`
carries none of those three properties) and is the surface actually built to show full evidence.
Re-verified after the fix: a real (non-programmatic) click on the pane's button worked cleanly —
text grew from truncated to the full original, the button disappeared, DR2 red/green containment
held, zero console errors, confirmed by screenshot.

**Wire contract.** `PayloadResponse { text }` moved into `amberfork-layout` next to `SlotAddress`
rather than living as a locally-declared struct in `amberfork-server` — one shared type between
server and UI, the same pattern `Document` already set. That let the `serde` dependency slice B
had tentatively added to `amberfork-server` come back out; it was never actually needed once the
type lives in the shared crate.

**CSS.** `.slot-trunc` stays visually identical at rest whether it's the canvas's inert span or
the pane's real button — same muted color, nothing new — so idle appearance never changes; only
interaction (hover, focus-visible, disabled-while-loading) reveals which is which. Buttons are
never amber (DESIGN.md); this isn't divergence and must not compete with the fork's one scarce
accent.

**Tests.** 3 new SSR tests in `slot.rs`: untruncated passes through as plain text, truncated with
no address stays the old inert mark (back-compat with several other tests in this crate that
hand-set `.truncated` directly without an address), truncated with an address renders a real,
not-yet-disabled button. All 50 pre-existing UI tests pass unchanged. Full gate green on both
`ssr` and the `csr`/wasm32 clippy target.

**#30 is closed** — all three slices (addressable slots, the server route, the UI affordance)
shipped and independently verified, the last one against a real running server in a real browser,
not just SSR strings.

## 064 · 2026-08-10 · Light mode via `prefers-color-scheme` ships — #31 closed

CSS-only, `ui/index.html`. Every color in the shell already routed through a `--token` custom
property except the content-diff pane's red/green, which was hardcoded hex — pulled those into
new `--error`/`--success` (+ `-bg` alpha) tokens alongside the existing `--warning`, then added a
`@media (prefers-color-scheme: light)` block overriding `:root` with DESIGN.md's spec: bg
`#F7F7F5`, surface `#FFFFFF`, hairline `#E2E2DD`, text `#16161A`, muted `#6B6B72`, amber
`#E0570B`, diff `#C7382F`/`#1E9E6A`. Dark stays the `:root` default (DD5), no manual toggle — the
OS preference alone decides. Dropped the `data-theme="dark"` attribute on `<html>`: nothing read
it, and it would've been wrong half the time once light mode existed.

**Two tokens had no explicit spec and needed a derived value**, flagged to the founder before
committing rather than picked silently:
- `--raised` (light) = `--surface` (`#FFFFFF`). Dark's `bg < surface < raised` brightness ladder
  has no headroom above white, so the selected-row "elevated" surface just becomes the same
  white as `surface`.
- `--faint` (light) = `#A3A39C`. DD4 restricts `faint` to decorative-only use (spine lines/dots,
  never readable text) at a low, ~2.8:1-ish contrast against `bg` in dark; picked to land at
  roughly that same low contrast against the light `bg` rather than reuse `muted`.

Founder approved both as proposed, no changes requested.

**Verification.** No Rust changed (zero inline styles in `ui/src/*.rs` — confirmed by grep before
starting), so no new unit tests; `trunk build` succeeds and `scripts/verify.sh --full` is green
(fmt, clippy on both `ssr` and `csr`/wasm32, all 50 pre-existing UI ssr tests unchanged). Visual
check via the `browse` skill against `trunk serve`: headless Chromium's own default preference is
light, so the boot-shell screenshot exercised the new light tokens directly (computed
`--bg`/`--amber` matched `#f7f7f5`/`#e0570b` exactly); forced a dark-token override via injected
stylesheet for a side-by-side. Did not walk the full state matrix (selected row, fork glow,
content-diff cards) — `trunk serve` has no backend behind it, so those states need real trace
fixtures through `amberfork serve` to exercise, not just the static shell. Everything in that
matrix routes through the same tokens verified here, but a real state-matrix pass with genuine
fork/converged/truncated data is worth a follow-up look before calling the light palette fully
QA'd end-to-end.

## 065 · 2026-08-10 · PRE-REGISTRATION: multi-reference consensus vs single reference (#45 slice B)

**Written and committed before the experiment was implemented or run — no number existed when
this entry was authored.** That ordering is the entry's whole point: #45's stated win condition
is that a *null* is decisive enough to kill the POA/consensus-DAG milestone, and a null is only
decisive if the decision rule predates the result.

**The question.** amberfork today compares one bad run against *one* good run. If the reference
is a lucky sample, the fork we report inherits that luck. Does aggregating over N references —
modal fork step, plurality wins — localize better than a single reference draw?

**Corpus (fully offline, no fetch, no API).** The 25 committed chimera dev pairs
(`bench/fixtures/chimera_noise_seed{42,43,44}_dev`, 8 + 7 + 10). For each pair, the committed
reference `b_NN.json` is jittered N=10 times using `make_pairs.py`'s existing benign-noise model
verbatim — reword p=0.4, token dropout 0.12, one retry-duplication — with per-variant seeds
derived deterministically from the pair name, so the corpus regenerates byte-identically and no
variant's draw depends on another's position.

**Gold is untouched.** `gold_step` indexes the *failing* run; jitter only ever rewrites the
reference. The retry-duplication that shifts gold +1 in `make_pairs.py` does so because it lands
in the failing run's prefix — inserting into a reference cannot move an index into the failing
run. So all three arms below are scored against the identical, unmodified gold.

**Arms (identical fixtures, identical gold, identical frozen params).**
- `pristine` — align the failing run against the committed, un-jittered reference. One prediction
  per pair. This is the shipped single-reference engine and reproduces the gate's existing
  per-seed numbers (42→6/8, 43→2/7, 44→6/10 = 14/25). It is the **ceiling**: consensus over
  jittered references can at best recover what the clean reference already said.
- `single` — align against jittered reference *i*. Ten predictions per pair. This is the
  "unlucky draw" condition the whole issue is about.
- `consensus` — `amberfork-align::consensus` over all ten jittered references; modal fork step,
  ties to the lowest step. One prediction per pair.

**Declared before the run, and binding:**
- N = **10**, fixed. No N-sweep, no best-N column, no reporting of N∈{3,5} even descriptively.
  If consensus needs a tuned N to win, that fragility is a finding, not a knob.
- Bootstrap resamples = **10,000**, resampling the 25 *pairs* (the independent unit).
- Per-pair difference statistic `d_p = consensus_hit[p] − mean_i(single_hit[p][i])`, which folds
  the pairing and the draw-averaging into one quantity in [−1, 1].
- Headline metric = **step-level exact match**. Windowed (±1) reported alongside per the metrics
  section, but exact match alone carries the decision.

**Decision rule.** Consensus **pays** iff the bootstrap 95% CI on `mean(d_p)` excludes 0 *and*
the point estimate is positive. Anything else — including a positive point estimate whose CI
straddles 0 — is a **null**, and a null kills the partial-order-alignment / consensus-DAG
milestone outright. Both outcomes get published here; there is no third "needs more data" branch
available without changing the corpus, and changing the corpus after seeing the number would
invalidate this registration.

**Protocol amendment.** This is the first comparison in the project between two arms on shared
fixtures, and rule 6's overlapping-Wilson test is the wrong instrument for it: at n=25 the
intervals overlap essentially regardless of the truth, so a real effect and a real null would
return the same verdict. Added **BENCHMARK.md rule 9** — paired arms on identical fixtures are
decided by a paired bootstrap interval, with the statistic and resample count declared in
advance. Rule 6 is unchanged and still governs every per-arm headline rate. Expect the honest
published shape to be a difference CI that excludes zero while both arms' Wilson intervals
overlap; that is what a paired design is *for*, not a contradiction.

**The caveat that has to travel with whatever number comes out.** The reference variation here is
*our own noise model*, not observed agent non-determinism. A positive result establishes that
consensus survives the specific benign noise we already believe in (rewording + retries — the
noise spike-001 showed breaks positional alignment); it does **not** establish that it survives
the noise real agents emit. Nothing on disk supports the stronger claim: the committed chimeras
carry one reference each, and only one HAL model zip is decrypted locally. Buying the stronger
claim means `amberfork record`-ing N runs of a genuinely flaky task (real API spend) or fetching
further HAL model zips (~450MB, and cross-*model* references are not the same thing as re-runs of
one agent). That upgrade is deliberately out of scope here and must not be implied by this table.

**Also note, to prevent a cross-reading error:** the `single` arm's rate will *not* match the
published 14/25. The shipped number is noised-failing vs *pristine* reference; `single` is
noised-failing vs *jittered* reference — a strictly harder, newly-introduced condition. Only
`pristine` is comparable to previously published tables.

## 066 · 2026-08-10 · RESULT: consensus is a NULL — and the reason is the interesting part (#45 slice B)

Registered in 065 (commit `8f77b14`), run once, reported as it fell. Reproduce:

```
cargo run -p amberfork-bench -- consensus \
  --pairs bench/fixtures/chimera_noise_seed42_dev \
  --pairs bench/fixtures/chimera_noise_seed43_dev \
  --pairs bench/fixtures/chimera_noise_seed44_dev \
  --split dev --json-out bench/results/consensus_multiref_dev.json
```

```
arm         exact  Wilson 95%        ±1     n
pristine    0.560  [0.371, 0.733]  0.720  25
single      0.544  [0.482, 0.605]  0.716  250
consensus   0.560  [0.371, 0.733]  0.720  25

single, per-pair mean (bootstrap): 0.544  [0.372, 0.716]
paired  mean(d_p) = consensus − E[single]: +0.016  [-0.024, +0.056]  (10000 resamples, 25 pairs)
mean agreement (support/forked): 0.908
```

**Verdict by the registered rule: NULL.** The paired 95% CI straddles zero, so the
partial-order-alignment / consensus-DAG milestone is dead. No re-run, no corpus change, no
second look — 065 explicitly closed those doors in advance, which is the only reason this
sentence is worth anything.

**The pristine arm reproduces the shipped number exactly** — 0.560 = 14/25 = the gate's
6/8 + 2/7 + 6/10. The harness is measuring the engine we ship, not a private variant of it.

**Consensus reproduced the pristine *prediction* on 25 of 25 pairs.** Not merely the same hit
rate — the identical predicted step, every pair. The modal vote is a *perfect* recovery of the
clean reference's answer under this noise model. It did not fail. There was nothing to win:

- A single jittered reference already scores 0.544 against the clean reference's 0.560. Jitter
  costs one draw **1.6 points**.
- So the maximum gain any aggregation could book over the expected draw is **+0.016**.
- Consensus booked **+0.016** — the entire available headroom, all of it, and no more.

The CI straddles zero because 1.6 points on 25 pairs is far below what n=25 can resolve, not
because consensus underperformed. The experiment was ceiling-limited from the moment the noise
model was fixed, and 065 fixed it before any of this was visible.

**This kills POA on a stronger argument than "it didn't help."** A consensus DAG's ceiling is
also the clean reference's answer — it cannot beat perfectly recovering the run the references
are noisy copies of. A trivial modal vote already reaches that ceiling on every pair here. The
expensive version therefore has *provably* no headroom on this class of noise, which is a
better reason to skip a milestone than a flat null would have been.

**References did disagree — consensus just resolved it correctly every time.** 17 of 25 pairs
had at least two distinct predictions among their ten draws; support ran 7–10 of 10 (mean
agreement 0.908). The minority votes were real, they were simply almost never the difference
between a hit and a miss. The support count is honest signal about reference agreement; it is
not, on this corpus, signal that improves localization.

**Where this does NOT generalize** (065's caveat, restated because the result makes it load-
bearing): the references here are jittered by *our* benign-noise model. Its measured cost to a
single draw is 1.6 points, and that number is what bounded the whole experiment. Real agent
non-determinism could degrade a single reference far more, and in that regime consensus would
have genuine headroom this corpus cannot show. What is established: under rewording + retry
noise of the kind spike-001 showed breaks positional alignment, **a single reference is already
within 1.6 points of the clean-reference ceiling, and ten references recover exactly that much.**

**The skeptic's attack gets a measured answer, just not the expected one.** "Your reference is
one run — how do you know the fork is the regression?" The answer is no longer "we assume it's
representative"; it is "we measured how much a bad draw costs, and under benign non-determinism
it costs 1.6 points, which ten-reference consensus recovers in full." That argues you do not
*need* consensus, which is a better outcome for a tool that ships one — and it cost one spike
instead of the POA milestone the issue was filed to justify.

**Slice C (`DiffResult` reference-collection + per-fork `support`) is not built.** It was
conditional on this result paying. Adding a reference-collection to a versioned seam that
nothing populates, to support a milestone this entry just killed, is contract surface bought
for a feature we now have evidence not to ship. If real-non-determinism data ever changes the
picture, it re-enters through a new registration, not this one.

**Engineering notes.** `hash.rs` gained `splitmix64`/`bounded` (moved from `arms.rs`, where the
random arm still uses them — the chimera gate numbers confirm the stream is unchanged) and
`unit()` for probability draws. `jitter.rs` re-applies `make_pairs.py`'s constants verbatim
(reword 0.4 / dropout 0.12 / one retry-dup) with our own in-crate RNG — these are new runs, not
a regeneration of committed fixtures, so matching CPython's Mersenne Twister would buy nothing;
what matters is that they rebuild byte-identically here. Jitter keys are namespaced by fixture
directory: `pair_00` exists in all three dev seeds and a bare name would hand three different
pairs the same ten variants. The `single` arm's pooled Wilson interval over 250 draws is
reported but flagged optimistic — ten draws of one suspect are not ten independent
observations; `single_per_pair`'s pair-resampled bootstrap is the one to read.

## 067 · 2026-08-11 · v0.9.0 + v0.9.1 released, and the CLI crate had been unpublishable since #29

Started from a plain question — "is anything left?" — and the tracker said no: all 45 issues
closed, zero `TODO`/`todo!()`/`unimplemented!()` in `crates/` or `ui/src`, `scripts/verify.sh
--full` green. The code was finished. The *metadata* was four steps behind reality, and one of
those steps turned out to be a real defect.

**v0.9.0 was sitting unreleased, and unpushed.** 24 commits past the `v0.8.0` tag — seven issues'
worth (#10 judge, #29 `--html`, #30 expand-on-demand, #31 light mode, #40 deltas, #43 privacy
warning, #45 consensus) — with an empty `CHANGELOG.md` `[Unreleased]` and `Cargo.toml` still at
`0.8.0`. The same shape 054 recorded for v0.8.0, so it was cut the same way: workspace version +
the 9 internal path-dep pins, both lockfiles, a CHANGELOG entry, and the "N crates at vX.Y.0"
lines in `CLAUDE.md`/`CONTRIBUTING.md` (10 → 11, `amberfork-judge` landed post-tag). `README.md`
also still called the OpenInference / `gen_ai.*` adapters "planned" three weeks after 052 shipped
them, and its working-surface list predated `--html` and `--judge`.

Worse than the missing tag: **`git push` moved `main` from `23c15f8` to `af5afca`** — everything
after #10 slice B existed only on this machine. The per-slice ritual in CONTRIBUTING.md covers
verify-then-commit and never says push, so nine slices sat locally without anything looking wrong.

**Then the packaging bug.** With v0.9.0 tagged and released, `cargo install amberfork` still
served **0.4.0** (2026-07-11) — the registry had `model`/`align`/`ingest` at 0.4.0 and had never
heard of `layout`/`server`/`record`/`replay`/`attrib`/`judge`. `cargo publish --workspace
--dry-run` packaged and verified all nine libraries and then died on the tenth:

```
error[E0432]: unresolved import ... /  couldn't read `src/../../../ui/index.html`
  --> src/html_export.rs:27
```

`html_export.rs` reached the live stylesheet with `include_str!("../../../ui/index.html")`. That
path escapes the crate root. It resolves in the repo, so `cargo build`, the whole test suite, the
release binary, and every CI job were green — but `cargo package` carries nothing above the crate
directory, so **the `amberfork` crate has not been packageable since `--html` shipped in #29**,
and nothing noticed for a full release cycle.

**Why CI structurally could not catch it.** `release.yml`'s packaging check was
`cargo package -p amberfork-server --list --allow-dirty | grep ui-dist/index.html`. It inspects a
*different* crate, and `--list` only enumerates filenames — it never compiles. A path escape is
invisible to it by construction.

**The fix, and the one that actually works.** The founder chose keeping a single canonical
stylesheet over vendoring a copy. The naive form of that still escapes the crate root, so the
mechanism had to be ownership: `amberfork-layout` — a crate both painters already depend on —
now owns `ui.css` and exports `pub const UI_CSS`, making the export's reference an ordinary
dependency. `ui/index.html` pulls the same bytes back with trunk's `rel="inline"` rather than
`rel="css"`, deliberately: inlining reproduces the old single `<style>` block, so `dist/` keeps
its three files, the page makes no extra request, and `rust-embed` has no new asset to carry. One
stylesheet, no hand-copied second copy, and #29's anti-drift property survives intact.

**The guard was wrong on the first attempt, and CI proved it.** Added `cargo package -p amberfork
--allow-dirty` (no `--list`, so it compiles) — and the v0.9.1 release run failed on it:

```
failed to select a version for the requirement `amberfork-align = "^0.9.1"`
candidate versions found which didn't match: 0.4.0
```

A single-crate package resolves its siblings from crates.io, and the version being released is by
definition not there yet — the check could never pass *before* the publish it guards. `cargo
package --workspace` is the right form: it stages every member in a temporary local registry and
verifies them against each other, the same mechanism `cargo publish --workspace` uses. Green on
both platforms now. Worth stating plainly: the guard's own first version was caught by running
it, not by reasoning about it.

**Two traps that cost real time.**

- **A staged `ui-dist/` and the test suite are mutually exclusive.** `verify.sh --full` hung for
  ten minutes on `valid_pair_without_a_ui_bundle_refuses_before_binding`: staging the bundle for
  packaging makes `serve` actually bind, so the test asserting it *refuses* never returns. CI
  never hits this because `ci.yml` and `release.yml` are separate workflows. Order is forced —
  verify with `ui-dist/` empty, then stage, then package, then clear it again.
- **Same version, different content.** After the fix was correct, the dry run failed three more
  times: first a stale extraction in the tmp-registry's src cache, then a stale compiled rlib in
  the shared `target/debug/deps`. Cargo keys both on the version string, so iterating on an
  unpublished `0.9.1` silently reuses the previous build of a different source. Cure:
  `rm -rf target/package ~/.cargo/registry/src/<tmp-registry-hash>` plus deleting `*amberfork*`
  from `target/debug/deps` and `.fingerprint`. Roughly forty minutes went into debugging a fix
  that had already been correct since the first attempt.

**Cut v0.9.1 rather than publishing 0.9.0.** The `v0.9.0` tag points at `af5afca`, which is the
commit *with* the broken `include_str!` — it cannot be packaged at all. Publishing the fixed
source as `0.9.0` would have put source on crates.io matching no tag and no release binary. In a
project where BENCHMARK.md rule 2 makes tags load-bearing, a patch release was the cheaper
honesty. The `v0.9.1` tag was later re-pointed from `f7e1068` to `cf4adcf` (the CI fix); safe
because that tag's release run had failed, so no GitHub release or crates.io version ever
referenced it, and `.github/` is not carried in any crate tarball — the published source is
byte-identical either way.

**Published, and verified against the registry rather than the repo.** All 10 crates at 0.9.1
(`amberfork-bench` stays `publish = false`). crates.io rate-limits *new* crate names, so the run
429'd at `amberfork-attrib` with eight through; the remaining two went in after the window. Then
the checks that actually matter: the published `amberfork-server` `.crate` carries
`ui-dist/index.html` (22 KB) plus the `.js`/`.wasm`, so `serve` ships a real UI rather than
`BundleMissing`; and `cargo install amberfork --version 0.9.1` into a clean root produces a
binary where `demo` renders the fork and exits 1, and `serve --demo` boots, serves
`<title>amberfork</title>` with the relocated stylesheet inlined, and answers `/api/document`
with `schema_version 0.2`. The CSS move survives the round trip through the registry.

`README.md`'s very first instruction — `cargo install amberfork` — is true again, five releases
after it quietly stopped being.

## 068 · 2026-08-11 · full-project audit, and the v1.0 plan: the baseline that was silently dropped

067 ended with the code finished and the metadata four steps behind. This session asked the next
question — *where did we compromise, and what is actually left?* — as a full audit: the tracker, the
design corpus, `BENCHMARK.md`, the notebook arc, the bench arms, the CLI surface, the release
workflow, and the registry/GitHub state. `scripts/verify.sh` green, 45/45 issues closed, ~25k LOC
across 11 crates, 557 tests, zero `todo!()`/`unsafe`.

**The finding that matters: the LLM-judge baseline was specified, never built, and never discussed.**
`BENCHMARK.md` states the project's *purpose* as proving the aligner localizes "at least as well as
the LLM-judge attribution the SOTA reports". Its Baselines section lists all-in-one and step-by-step
judges under the heading *"the number is meaningless without them"*. Its Definition of done says the
README shows amberfork vs shallow-diff vs **LLM-judge** vs random. The shipped bench has four arms —
`random`, `pos-lexical`, `nw-structural/resync`, `nw-lexical/resync` — and this notebook, 3,254 lines
of it, contains **zero** occurrences of a judge baseline. Not a recorded decision; a silent drop.
`amberfork-judge` shipped in #10 as a *narration* layer, which is a different thing wearing the same
name, and its existence is probably why nobody noticed. Agent-level accuracy (also in Metrics, also
against a published SOTA number) went the same way — zero occurrences in `crates/`.

The consequence is precise: the headline beats random, an index-diff, and a content-blind aligner.
The first question a strong engineer asks — *does it beat just asking a model which step went wrong?*
— has no answer in the repo, despite the protocol saying it must.

**The protocol change: `judge-paired` is the baseline that actually decides the claim.** Replicating
only the single-trace Who&When methods leaves a hole a skeptic drives straight through — amberfork
sees two runs, the judge sees one, so the comparison is confounded by information asymmetry. So #46
runs three conditions: `judge-single` (replicates the SOTA method, comparable to the published 14.2%
/ ~11%), `judge-stepwise` (the other Who&When method, on a cheap tier because it is ~20x the calls),
and **`judge-paired`** — both runs in the prompt, the same information the aligner gets. Nobody has
published that number. It is the one that decides whether the alignment algorithm does real work or
exploits an information advantage, and it is the headline comparison.

Both outcomes are publishable, which is the point of registering the interpretation first. A tie on
`judge-paired` reads *"matches a frontier model given identical information — deterministically,
offline, in milliseconds, no API key"*, which is a stronger claim than beating a positional diff. A
loss is a real finding about where the ceiling is. This gets written down in #46's pre-registration
before a token is spent, not after the number lands.

**Models, verified against current sources 2026-08-11** (the knowledge cutoff predates all of them):
frontier arm `gpt-5.6-sol` ($5/$30 per MTok) so nobody can say a weak judge was picked;
cross-provider check `gemini-3.6-flash` (free tier) so the result is not OpenAI-specific; local arm
`qwen3:8b` (Apache 2.0) for the no-API-key condition. `judge-stepwise` on `gpt-5.6-luna` for the call
volume. Budget ~$20-25 one-time on OpenAI, $0 elsewhere, and cassettes make it free forever after.

**A side-finding the model survey produced:** `--judge local` ships pointing at `smollm2:135m`. A
135M model narrating a fork cannot be producing useful output — a shipped feature that is
effectively inert. #47 upgrades it to `qwen3:8b`, riding on the local provider work #46 needs anyway.

**The other three unmeasured things.** `--verify` — the actual moat, the half nobody else ships,
since the frontier's Causal Agent Replay *requires* re-running and we do not — has an e2e happy-path
test (053) and **no rate**. No confirmation rate, no ddmin minimal-cause precision, no
origination/propagation accuracy (#48). The by-construction catch from 040 still stands with three
nulls against it (016, 051, 066) and no natural-fork win; #49 attacks it differently — record a real
agent N times, perturb exactly one tool response, and take gold from the *perturbation* rather than
from splicing logs, so everything between cause and outcome is real agent behaviour. And the O(n*m)
wall is a projection (022/023), not a measurement (#50).

**Doc drift, recorded rather than fixed here.** `design-run-diff-debugger.md`'s authoritative header
still says "10 crates shipped at v0.7.0", last Amendment 2026-07-21; all of v0.8/v0.9 has notebook
entries and no amendment. 040's watch-item 2 was never resolved — `Step` carries `edges`/`parent_idx`
and the aligner linearizes them, while line 361 still names GumTree tree-diffing as the approach.
#51 reconciles both. Either answer is fine there; silence is not.

**Founder decisions this session.** (1) Judge models as above — OpenAI paid, Gemini free tier, plus a
free local arm. (2) Cut line: evidence + doc honesty + distribution surface; `amberfork-store`,
`fto.md`, `--timings`, tree alignment and failure clustering are deferred with reasons on the
tracker. (3) The writeup ships the *same day* as v1.0.0, since the sealed reveal happens at the tag
anyway and the numbers and the story should land together.

**Consensus gets a conditional second look, and only because #49 pays for it.** 066 killed
multi-reference consensus on a pre-registered null and closed the door explicitly — re-entry requires
a new registration. Re-reading it for this audit sharpened *why* it was a null: consensus reproduced
the pristine reference's exact predicted step on 25 of 25 pairs and captured +0.016 of +0.016
available headroom. It took the entire prize; the prize was 1.6 points, because a single jittered
reference already scored 0.544 against the clean reference's 0.560. The general form is
**consensus's value equals the single-draw penalty**. Under our own benign-noise model that penalty
is small. Under real agent non-determinism — different tool-call orders, different step counts,
temperature-driven paths — it could be much larger, and #49 records exactly the multiple *real*
references 065 said would otherwise cost API spend or a ~450MB HAL fetch. Filed as #62 in the
backlog, decided after #49's numbers, requiring a fresh registration. Null again would be a *stronger*
published claim than 066 can make.

**Why this entry exists at all.** The audit and the plan lived only in one session's context. The
SessionStart hook reads open issues plus their milestone, and the tracker was empty and the last
milestone was v0.9 — so a fresh "build what's next?" would have found nothing and invented something.
Filed as milestone **v1.0 — the credibility release** (#46-#57, numbered in build order so
"lowest-numbered unblocked" resolves correctly) and **backlog (post-v1)** (#58-#63, out of the
current milestone so they stay visible as decided deferrals without being picked up).
`CONTRIBUTING.md`'s loop still named v0.8/v0.9 as current, and — the gap 067 identified and did not
fix — **still never said push**. Both corrected here.
