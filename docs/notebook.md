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
