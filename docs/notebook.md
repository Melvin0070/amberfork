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
