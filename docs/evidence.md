# The evidence, in full

Every measurement amberfork has published, including the ones that went against it. The
[README](../README.md) carries a one-table summary; this is the detail behind each row. The dated
record of how each number was arrived at — registrations, amendments, dead ends — is
[`notebook.md`](notebook.md), and the protocol that governs all of them is
[`BENCHMARK.md`](../BENCHMARK.md).

Commands below run from the repository root.

## Natural same-agent pairs from Terminal-Bench 2.0 (n=685 dev pairs)

The first natural corpus whose run structure varies on its own: published trials of the same
agent, model and task, some passing and some failing. One pass×fail pair and one pass×pass pair
per group; 23 dev tasks, 9 scaffolds. Pre-registered in notebook 083 and amended twice before any
score (083a, 083b); result in 084. There are no step-level cause labels, so nothing here is a
localization score.

```sh
cargo run -q --release -p amberfork-bench -- score-tbench --pairs bench/data/tbench/pairs --split dev
```

(The corpus is benchmark-use only and is not committed; notebook 083a has the two commands that
rebuild `bench/data/tbench/pairs`. The per-pair scores are committed as
`bench/results/tbench_natural_dev.json`.)

**How often `diff` reports a fork:**

| pair class | forked | Wilson 95% | n |
|---|---|---|---|
| both runs passed | **0.967** | [0.939, 0.983] | 273 |
| one passed, one failed | 0.956 | [0.932, 0.972] | 412 |

**Whether the in-sync fraction separates the two classes** (AUROC, 95% CI by bootstrap over tasks):

| score | AUROC | 95% CI |
|---|---|---|
| run length only | 0.614 | [0.580, 0.652] |
| positional diff | 0.569 | [0.532, 0.607] |
| **the engine** | **0.614** | [0.580, 0.649] |
| engine − positional (paired) | **+0.045** | [+0.020, +0.069] |
| engine − length (paired) | −0.000 | [−0.033, +0.032] |

Read it honestly: alignment beats the positional baseline on real runs, with an interval that
excludes zero — the first time that has been shown without injected noise. It does **not** beat a
score that knows only the two runs' lengths, which by the rule fixed in 083 is a null for the
claim "diff two independent runs to find the failure". Two passing runs of the same agent already
share only about a third of their steps. The test split is sealed.

## The replay gate on real recordings (n=30 tapes)

`amberfork replay` serves a recorded run back to the agent offline. Pre-registered in notebook
085; result in 086. Thirty real recordings of a small tool-using agent driven by a local model:

```sh
python3 spike/score_replay_gate.py --amberfork-bin target/release/amberfork
```

| measurement | count | Wilson 95% |
|---|---|---|
| unchanged agent diverged (false positive) | 0 / 30 | [0.000, 0.114] |
| agent with one tool's return value changed diverged | 30 / 30 | [0.886, 1.000] |
| `diff` on two live runs of one configuration forked | 2 / 50 | [0.011, 0.135] |

Read it honestly: this is a round-trip check on one short, deterministic agent, not a
false-positive rate for yours. Every change in this corpus lands at the same exchange, so it says
nothing about localization. And on this corpus diffing two live runs was nearly as quiet, so
replay's advantage over live runs — large in principle on the long, free-running agents the 96.7%
above comes from — has not itself been measured.

## The headline number — sealed test split

Fork-localization on chimera pairs: a controlled fork spliced into real agent logs
([Who&When](https://github.com/ag2ai/Agents_Failure_Attribution)-derived), every arm on
identical pairs with an identical denominator. Protocol: [`BENCHMARK.md`](../BENCHMARK.md).

**The headline** (sealed test split, three seeds, n=35 — revealed once, at the v0.2.0 tag,
under params frozen before any test pair was scored): the full engine localizes the fork
**within 3 steps 91% of the time** — 0.91 [0.78, 0.97] vs the best baseline's 0.49
[0.33, 0.64], non-overlapping intervals — and takes **0.49 exact** where the baselines'
best is 0.03 (notebook 017).

| arm (test, n=35 across 3 seeds) | exact | ±1 | ±3 |
|---|---|---|---|
| random | 0.00 | 0.09 | 0.29 |
| pos-lexical | 0.00 | 0.20 | 0.49 |
| nw-structural/resync | 0.03 | 0.20 | 0.20 |
| **nw-lexical/resync** (full engine) | **0.49** | **0.71** | **0.91** |

The dev side these params were tuned on agrees: exact 0.56 · ±1 0.72 · ±3 1.00 (n=25) — no
overfitting cliff. **Exact-step** localization is *seed-sensitive* on unseen pairs too (0.75 /
0.23 / 0.50 on seeds 42 / 43 / 44), the same shape dev showed — which is why the ±3 window,
not exact, is the claim (notebooks 014, 017). The engine's fork confidence is calibrated on
unseen data: exact-hit rate climbs monotonically from 0.08 in the lowest-confidence bin to
1.00 in the two highest.

Every number above renders from a committed document. The headline table IS
`bench/results/chimera_noise_multiseed_test.json` — an exact pool (hits and n summed,
Wilson intervals recomputed) of the **three sealed per-seed test documents**
(`bench/results/chimera_noise_seed*_test.json`), each of which it names by sha256; a test
rebuilds the aggregate from its committed sources byte-for-byte. **Seed 42's dev split** is
the default `report` below (the CI gate additionally pins seeds 43/44 —
`bench/fixtures/chimera_noise_seed*_dev/`):

```text
params: bench/params.toml sha256:8ebd95ce8f3d · tau 0.3 · resync_k 2 · gap 0.6+0.3
| nw-lexical/resync | 0.75 [0.41, 0.93] exact | 0.88 ±1 | 1.00 ±3 | n=8 |  (dev, seed 42)
```

Read it honestly: these are **injected** forks — a controlled divergence spliced into real logs — so
sustained divergence at the gold step is true *by construction*, which structurally favors the
content-aware alignment arms. What the headline measures is the aligner's tolerance to benign noise
around a known fork, not attribution on a *natural* failure (for that, see the Mode A′ null below).
Wilson 95% intervals are wide at these n, and the test split is scored exactly once per release tag
(protocol rule 2) — scored fresh at every tag since. The v0.4.0, v0.5.0, and now v1.0.0 reveals all
reproduced these numbers identically on every arm and metric — `bench/results/*_test_v0.4.0.json` /
`*_test_v0.5.0.json` / `*_test_v1.0.0.json`, notebooks 021, 037, and 081. Six releases and two new
`amberfork-align` features (consensus, deltas) later, the frozen params and the core alignment path
haven't moved the number by a single pair. The claim the numbers support is the *shape* —
content-aware alignment localizes within a few steps where position and structure do not.

Reproduce the tables offline — they render from the committed results documents, zero fetch:

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/chimera_noise_multiseed_test.json   # the headline (test, n=35)
cargo run -q -p amberfork-bench -- report    # seed-42 dev (the default)
cargo run -q -p amberfork-bench -- report --results bench/results/chimera_noise_seed42_test.json      # one seed's test slice
```

The dev-split pairs are committed (GAIA-sanitized — see `bench/fixtures/`); the CI gate scores
them on every `amberfork-align` change. Regenerate the full sets from raw upstream data with the
recipe in [`bench/fixtures/chimera_noise_seed42_dev/README.md`](../bench/fixtures/chimera_noise_seed42_dev/README.md).

The table above is the number on **injected** forks — a controlled divergence spliced into real
logs. Three separate attempts at a **natural**-fork corpus follow, and not one of them produced a
headline: [the natural-fork attempts](#the-natural-fork-attempts-and-what-each-one-measured). They
are the honest context for the 0.91, and they are worth reading before you believe it.

## `--verify` confirmation rate (n=15) — the other half of the moat, measured

The localization numbers above are half the pitch; `--verify`'s counterfactual confirmation is the
other half, and until now it had a single-example smoke test and no rate. Scaled the existing
real-provider test (a toy two-turn agent engineered to fork on genuine LLM sampling variance, not a
scripted difference) from n=1 to n=15. [Pre-registered in notebook 079](notebook.md); result in
080:

```sh
python3 -c "import json; d=json.load(open('bench/results/verify_realprovider_all.json')); \
print(json.dumps({k:v for k,v in d.items() if k!='raw_attributions'}, indent=2))"
```

| metric | rate | 95% CI | n |
|---|---|---|---|
| two-sided fork (ddmin engages) | 1.00 | [0.80, 1.00] | 15 |
| **Recovered** | **0.867** | [0.62, 0.96] | 15 |
| NotRecovered | 0.133 | [0.04, 0.38] | 15 |
| **Unverified (indeterminate)** | **0.00** | [0.00, 0.20] | 15 |

Read both bolded numbers together: 87% confirmation, and — more notable on a fixture specifically
built to produce real sampling variance — the 3-run majority vote never split evenly across all 15
calls (n is small; the CI says "low so far," not "zero"). The two `NotRecovered` cases are as
informative as the eleven that recovered: both patched the correctly-identified cause and still
didn't reconverge, because the downstream step carries its *own* independent sampling variance the
patch can't reach — a confirmed-sufficient cause and a non-recovering re-run aren't a contradiction,
they're two independent sources of real non-determinism, which is exactly why `Recovery` is a
tri-state and not a boolean. Not measured here: origination/propagation accuracy against a known
cause — the one corpus with a known cause (#49's) turns out to be structurally unreachable by the
patch mechanism (the fault lives in local agent code, never a recorded response); notebook 079 has
the full reasoning and what fixing it would take.

## The natural-fork attempts, and what each one measured

Three corpora, three different reasons the headline does not transfer. Each was diagnosed
rather than dropped, and each diagnosis is dated in the notebook.

### Mode A′ — real cross-system pairs (directional, n=4)

The second protocol scores *natural* failures: a failing Who&When run against a passing
reference from a **different agent system** (a ServiceNow TapeAgents tape) on the same GAIA
task. The whole pipeline is in-tree, pinned, and reproducible:

```sh
cargo run -q -p amberfork-bench -- fetch    # pinned upstream data → bench/data/ (never committed)
cargo run -q -p amberfork-bench -- build-pairs --tapes bench/data/tapes --logs bench/data/whowhen --out bench/data/pairs_real
cargo run -q -p amberfork-bench -- run --pairs bench/data/pairs_real --split all
```

Public data yields exactly **4 pairs** (only 4 published tapes pass their task), and the honest
result is a **null**: the engine reaches 0.50 ±3 while *random guessing reaches 0.75*, because
these runs are 7–10 steps long (a ±3 window covers most of the run) and a reference from a
different system legitimately diverges from step 0, making the annotated "decisive step" a weak
gold target. Both limits were pre-registered in [`BENCHMARK.md`](../BENCHMARK.md)'s
threats-to-validity before the measurement (notebook 016). So Mode A′ ships as a disclosed
limit, not a headline — the table (cross-system banner included) re-renders offline:

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/mode_a_prime_realpairs_all.json
```

### TRAIL↔HAL natural pairs (n=69) — a null, and the LLM-judge baseline that measured it

The third protocol is the strongest attempt at a natural-fork number: a real **TRAIL** failing
trace against a real **HAL** passing reference on the same GAIA task, same Open Deep Research
scaffolding, differing backing model, with gold taken from TRAIL's *human-annotated* error span.
117 traces yield **69 pairs** (23 dev / 46 test).

The engine scores **0.00 exact** on it, against random's 0.14 — worse than guessing:

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/trail_hal_natural_all.json
```

Rather than leave that as an unexplained null, the missing baseline was built and
[pre-registered first](notebook.md) (notebook 069, written before any judge code existed):
**does just asking a frontier model beat the aligner?** Five arms on the 23 dev pairs, zero
exclusions, every answer cached so the table replays offline:

| arm | model | exact | ±3 | n |
|---|---|---|---|---|
| `random` | — | 0.174 | 0.565 | 23 |
| `nw-lexical/resync` (the engine) | — | **0.000** | 0.348 | 23 |
| `judge-single` | gpt-5.6-sol | **0.261** | 0.522 | 23 |
| `judge-paired` | gemini-3.6-flash | 0.217 | 0.478 | 23 |
| `judge-paired` | gpt-5.6-sol | 0.174 | 0.478 | 23 |
| `judge-paired` | qwen3:8b (local) | 0.087 | 0.565 | 23 |
| `judge-stepwise` | gpt-5.6-luna | 0.087 | 0.217 | 23 |

**Three of five judge arms beat the engine with paired-bootstrap intervals excluding zero.** The
result is published as measured (`bench/results/judge_*.json`, notebook 071).

Two findings matter more than the ranking. First, **no arm meaningfully clears random** — at ±3
random matches or beats every judge arm (qwen3:8b, the local/free arm, ties it exactly; the four
frontier-model arms all fall below it), so this corpus barely discriminates *any* method. Second,
inspecting per-pair predictions rather than rates: **all three aligner arms
predict step 0 on all 23 pairs.** The pairs share *zero* step names — TRAIL logs
`CodeAgent.run`/`web_search`, HAL logs `openai.chat.completions.create`/`tool_result` — so there is
no common prefix to fork from and the fork rule degenerates to a constant. The 0.348 at ±3 is
entirely the 8 of 23 pairs whose gold happens to sit at step ≤ 3.

That diagnosis, and the repair it implies, are [pre-registered in notebook 070](notebook.md)
with the decision rule fixed in advance — including the branch where the corpus is retired as a
documented negative result. **The chimera numbers above are untouched** (protocol rule 10: a
baseline never disturbs the product's published table).

### Real-agent perturbation (n=25) — ties the weak baseline, and the reason is the finding

The fourth attempt at natural-fork evidence, and the first with no found-corpus defect to blame: a
real local model (`qwen3:8b`) drives a real tool-using agent through `amberfork record`, with one
tool's return value swapped for a wrong one (a stale doc, a wrong status) — gold is the exchange
where the bad value first reaches the model, known from the harness's own bookkeeping, not
annotated. [Pre-registered in notebook 074](notebook.md); result in 075.

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/perturbation_all.json
```

| arm | exact | ±3 | n |
|---|---|---|---|
| random | 0.32 | 1.00 | 25 |
| pos-lexical | **1.00** | 1.00 | 25 |
| nw-structural/resync | 0.00 (100% abstain) | 0.00 | 25 |
| nw-lexical/resync (the engine) | **1.00** | 1.00 | 25 |

The engine is right on every pair — and so is the shallow positional diff it exists to beat. Read
plainly: **this is not a win.** Alignment only earns its keep over position when two runs can
differ in length or order before the point that matters (chimera's benign-noise model manufactures
exactly that, on purpose); this harness perturbs *content* but never *structure*, so the two runs
stay index-aligned by construction and position ties alignment every time, regardless of where the
fork falls. `nw-structural/resync` abstains on all 25 for an unrelated, confirmed reason: its
content-blind cost model keys on `(kind, name)`, and every step here shares one name — a mechanical
fact about today's cassette-normalization path (one exchange, one undifferentiated `Llm` step),
not a claim about real agents in general. Full diagnosis, including what a version that could
actually discriminate the two arms would need, is in notebook 075.

## The step-0 defect, and its guard

On two runs that share no common prefix, amberfork used to report "fork at step 0" with high
confidence (measured: 0.97) rather than admitting the runs were not comparable — on the TRAIL↔HAL
corpus above, one constant answer for all 23 dev pairs.

Since [notebook 087](notebook.md) it refuses: when the alignment has no in-sync step, `diff`
exits 2 with "no common ground", and `--allow-unrelated` aligns the runs anyway. The guard is a
strict no-op on the injected-fork dev fixtures (every arm's row identical before and after), so
the headline table is untouched. On the two out-of-domain corpora the engine now abstains where it
used to answer: 21 of 23 TRAIL↔HAL dev pairs and 3 of 4 Mode A′ pairs. The tables above are the
numbers as published at the time and are not rewritten; the before-and-after rows are in 087.
