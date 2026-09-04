# amberfork: what changed between two agent runs

*The v1.0 writeup (issue #57). Every number below traces to a committed file in this repo — the
links are real paths, not citations you have to trust. One caveat up front: the headline benchmark
number is the sealed reveal from the v0.2.0 tag (reproduced identically at v0.4.0 and v0.5.0); the
v1.0.0 sealed reveal itself is a separate, later event (issue #56) under the protocol's own
"scored once per tag" rule, not yet run as this is written.*

## The asymmetry, not the pitch

"Diff two agent runs" is a pitch that has already flopped twice on Show HN — two unrelated prior
attempts at this exact framing sit at 1 point / 0 comments each
([`docs/design/POSITIONING.md`](design/POSITIONING.md)). It's builder vocabulary, and this audience
has watched the shallow version of it before: a handful of tools that align two trajectories by
index and call the first mismatch a bug.

Here's the actual claim, and it's narrower and more useful than "diff":

> Two independent runs of the same agent — one that worked, one that didn't — can be aligned with a
> real sequence-alignment algorithm (move-typed, affine-gap Needleman-Wunsch, not positional
> index-matching), which localizes the one decisive step where they forked. On a run captured under
> `amberfork record`, you can then work toward *confirming* that step caused the failure: replay the
> recording forward from the fork with that step patched, and see whether the run recovers.

That confirmation step is real but it isn't magic: it stays live past the patch point (against
whatever upstream the original recording used — that can be a local model, but it is still a live
call, not a cache hit), and because a live provider won't answer bit-identically twice, the verdict
is a majority vote over several replays rather than one call — a disclosed tri-state (confirmed /
refuted / unverified), not a boolean. What's real about the wedge isn't "zero network calls"; it's
that nothing about the *original agent framework* has to be runnable again. amberfork owns the
recording, not the orchestration, so a frozen incident trace can be replayed and mutated without
anyone standing the agent back up. How reliable that confirmation actually is: on n=15 real forks
(scaling the existing real-provider test past its single example), it confirms **87%** of the time
— with the one further caveat below (see "Where it fails") that not every case has an independently
knowable ground truth to check precision against.

Every other *shipping* tool in this space either shows you two trees and makes you find the fork by
eye (LangSmith, Langfuse), or diffs a run against its own saved baseline and lists every tool call
that drifted (EvalView — the closest shipping analog). No shipping tool does run-vs-run fork
localization plus this kind of no-reconstruction confirmation (a few research papers explore
adjacent ground — WebStep, AgenTracer — but they're unreleased, not shipping competitors). That
pairing is the wedge, not "local" or "offline" — those are table stakes now (Phoenix, self-hosted
Langfuse, and EvalView all have them too).

```sh
cargo install amberfork
amberfork serve --demo
```

![amberfork serve --demo: two agent runs aligned side by side in the browser on one shared timeline. The steps they agree on recede in gray; the step where the bad run fetched a stale refund policy ignites amber as the fork, the divergent path glows amber down to the wrong answer, and the field diff on the right shows the swapped policy doc in red and green.](assets/hero-web.gif)

Two runs align in gray, the step where the bad run fetched a stale refund policy ignites amber, the
field diff on the right shows the swapped document in red and green. The same fork, in your
terminal:

![amberfork demo in the terminal: the two runs align in gray, a rate-limit retry is absorbed as a log-move, and the step where the bad run fetched a stale policy doc glows amber as the fork, closing with a one-line attribution footer.](assets/hero.gif)

If you want the mechanism instead of the demo, keep reading.

## The number, and what it actually measures

On the sealed chimera test split — a controlled fork spliced into real Who&When agent logs —
the full engine localizes the fork within 3 steps **91% of the time** (Wilson 95% CI [0.78, 0.97])
against the best baseline's 49% [0.33, 0.64], n=35 across three seeds
([`BENCHMARK.md`](../BENCHMARK.md), [`docs/notebook.md`](notebook.md) entry 017). Reproduces
offline, no network, no API key:

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/chimera_noise_multiseed_test.json
```

Two things to know before trusting that number too far. First, **exact-step localization is
seed-sensitive** — 0.49 exact pooled, but 0.75 / 0.23 / 0.50 on the three individual seeds — which
is exactly why the claim is the ±3 window, not the exact match; a single seed's draw is not the
number to remember. Second, and the bigger caveat: these are **injected** forks, so sustained
divergence at the gold step is true by construction — that structurally favors the content-aware
alignment arms. The claim the number supports is the *shape*: content-aware alignment localizes
within a few steps where position and structure do not. It is not a claim about attribution on a
natural failure. For that, we went looking — repeatedly — and the honest results are the reason this
writeup is worth reading past the hero number.

## Three attempts at a natural fork, and one earlier question about trusting a single reference

The by-construction caveat above surfaced in this project's own mid-project self-retrospective
(notebook 040) as the sharpest thing working against the headline number, and rather than leave it
as an acknowledged weakness, we spent real effort trying to close it. Three separate attempts at a
genuinely *natural* (not injected) fork follow below, plus one earlier, different experiment on the
*same* injected corpus that's worth including for what it proves about ceilings — clearly labeled as
such, because conflating it with the natural-fork attempts would be exactly the kind of thing this
writeup exists to not do.

**Mode A′ — cross-system natural pairs (n=4).** A failing Who&When run against a passing reference
from a *different* agent system on the same GAIA task. Honest null: the engine reaches 50% at a ±3
window while random guessing reaches 75%, because these runs are short enough (7–10 steps) that a
±3 window covers most of the run, and a reference from a different system legitimately diverges
from step 0. Pre-registered and disclosed as a limit before the measurement, not after
(notebook 016).

**An aside, not a natural-fork attempt: multi-reference consensus (n=25, on the injected chimera
pairs).** Different question, same corpus as the headline number above: if a single reference run
might be an unlucky draw, does voting across several references localize better than trusting one?
Pre-registered before the code existed (notebook 065), and its own registration is explicit that a
result here "does not establish that it survives the noise real agents emit" — the variation tested
is chimera's own benign-noise model (reword, dropout, one retry-duplication), not observed agent
non-determinism. The result: consensus over ten jittered references reproduced the single clean
reference's exact predicted step on 25 of 25 pairs. A jittered reference already scores within 1.6
points of the clean one, so the maximum any aggregation could book was 1.6 points — and consensus
booked exactly that, all of it (notebook 066). It's a null that doubles as a stronger argument than a
flat null would be: there was no headroom for voting to find, proven rather than merely observed.
Real, and worth knowing — but it says nothing about natural forks, and shouldn't be read as if it
did.

**TRAIL↔HAL natural pairs (23 of 69 total, dev split) — the engine loses.** The strongest attempt at
a real natural-fork number: a real TRAIL failing trace against a real HAL passing reference on the
same GAIA task, same agent scaffolding, differing backing model, gold from TRAIL's human-annotated
error span (69 pairs total; the 46-pair test split is reserved for the v1.0.0 sealed reveal per
protocol rule 2 and hasn't been scored yet — the 23-pair dev split below is what's measured today).
On the full 69, the engine scores **0.00 exact** against random's 0.145. Rather than leave that
unexplained, we built the missing baseline the field's own published numbers are compared against —
*does just asking a frontier model beat the aligner?* — pre-registered before writing a line of
judge code (notebook 069), scored on the 23-pair dev split:

| arm | model | exact | ±3 | n |
|---|---|---|---|---|
| random | — | 0.174 | 0.565 | 23 |
| **nw-lexical/resync (the engine)** | — | **0.000** | 0.348 | 23 |
| judge-single | gpt-5.6-sol (frontier) | **0.261** | 0.522 | 23 |
| judge-paired | gemini-3.6-flash (frontier) | 0.217 | 0.478 | 23 |
| judge-paired | gpt-5.6-sol (frontier) | 0.174 | 0.478 | 23 |
| judge-paired | qwen3:8b (local, free) | 0.087 | 0.565 | 23 |
| judge-stepwise | gpt-5.6-luna (frontier) | 0.087 | 0.217 | 23 |

We don't get to soften this: **three of five judge arms beat the engine, with paired-bootstrap
intervals excluding zero** (notebook 071). That's a real loss on the metric that decides the
protocol's own comparisons. It's mitigated, not erased, by a second finding in the same table: **no
arm meaningfully clears random at the ±3 window** — random ties or beats every arm there, including
the frontier judges (qwen3:8b's ±3 rate is exactly random's; the others are below it). Inspecting
predictions rather than rates explains the engine's specific 0.000: all three aligner arms predict
step 0 on every one of the 23 pairs, because TRAIL and HAL log with *zero* shared step-name
vocabulary (`CodeAgent.run`/`web_search` vs `openai.chat.completions.create`/`tool_result`), leaving
no common prefix for a run-vs-run comparison to fork from (notebook 070). That specific mechanism
explains the aligner's constant prediction — it does not explain `judge-single`'s result, since a
single-trace judge never sees a second run and so has no "common prefix between runs" to lose in the
first place; the diagnosis applies to run-vs-run methods, not single-trace ones, and the writeup
should not imply otherwise. The vocabulary gap is also diagnosed as a repairable ingest limitation,
not necessarily a permanent fact about these two systems (070's simulated repair recovers real tool
names on a meaningful fraction of steps) — a fix is registered, not yet built. Net: **v1.0's public
claim narrows explicitly to the chimera controlled-injection protocol** because of this result,
stated here rather than left for a reader to notice on their own.

**Real-agent perturbation (n=25) — no corpus defect to blame, and still not a win.** If found
corpora keep having representation problems, build one from scratch: a real local model
(`qwen3:8b`, via Ollama, zero API cost) drives a real tool-using agent through the product's own
unmodified `amberfork record`, with one tool's return value swapped for a wrong one — a stale
refund policy, a wrong order status, an outdated fee schedule. Gold is the exchange where the bad
value first reaches the model, known from the harness's own bookkeeping the instant it happens,
never annotated after the fact. Pre-registered before any session was recorded (notebook 074);
result as it fell (notebook 075):

| arm | exact | ±1 | ±3 | n |
|---|---|---|---|---|
| random | 0.32 | 1.00 | 1.00 | 25 |
| pos-lexical | **1.00** | 1.00 | 1.00 | 25 |
| nw-structural/resync | 0.00 (100% abstain) | 0.00 | 0.00 | 25 |
| **nw-lexical/resync (the engine)** | **1.00** | 1.00 | 1.00 | 25 |

(Random reaches 1.00 at ±1 and ±3 too — these are short runs, 2-4 steps, so a loose window covers
almost the whole trajectory; exact match is the only column with any spread, which is why it's the
one worth reading.) Read that plainly: **this is not a win.** The engine is right on every pair —
and so is the shallow positional diff it exists to beat. The reason is mechanical, not flattering:
alignment only earns its keep over position when two sequences can differ in *length or order*
before the point that matters (that's exactly what the chimera benign-noise model manufactures on
purpose — reword, dropout, one retry-duplication). This harness perturbs a tool's *content* but
never the *structure* around it, so the two runs stay index-aligned by construction and position
ties alignment on every pair. All 25 gold steps happened to land at the same index too (every task
calls its designated tool first) — which alone would explain a tie — but the deeper mechanism means
varying that index wouldn't have changed the outcome either, since these two runs can never go out
of step before the fork regardless of where it falls. The honest lesson is about the harness, not
the algorithm: real non-determinism has to be allowed to change *how many steps happen*, not just
what they say, before a natural-fork protocol can tell position and alignment apart. What a version
that could actually discriminate the two would need is written down (notebook 075) for whoever
builds it next.

One more thing this run surfaced for free: `nw-structural/resync` — the content-blind arm that costs
alignment purely on `(step kind, step name)` — abstains on all 25 pairs, for a confirmed, unrelated
reason: today's cassette-recording path collapses every tool-calling turn into one undifferentiated
`Llm` step, so every step in every run shares one name. That's a real, disclosed limitation of the
recording path (`amberfork-record`'s own module documentation says so), not a claim about real
agents in general.

## The known defect, stated plainly

On two runs that share no common ground — like TRAIL vs HAL above — amberfork reports "fork at step
0" with **confidence 0.97.** Confidently wrong. The fix (an abstention guard when two runs have no
shared vocabulary to align against) is designed and pre-registered (notebook 070) but not yet
shipped. This is the single highest-value item on the open roadmap, because it affects real use on
out-of-domain input, not a benchmark row.

## What it took to earn the numbers above

Every result on this page follows one pre-registered protocol
([`BENCHMARK.md`](../BENCHMARK.md)): the decision rule is written and committed *before* the
measurement runs, dev/test splits are separated, a baseline never disturbs a previously published
product number, exclusions are counted and reported rather than dropped from the denominator, and a
null — or a loss — gets published exactly as readily as a win. That discipline is why this writeup
can put a real loss to an LLM judge next to the one clean win without either undermining the win or
reading as an excuse for the loss: both were possible outcomes before anyone knew which way they'd
fall, and both were registered as publishable in advance.

If you're evaluating this repo rather than using it: the honest entry point is
[`docs/notebook.md`](notebook.md) — 75 dated entries, including every experiment above in full and
the mechanistic diagnosis behind each one.

## Where it fails, on purpose stated here

- **You need a comparable pair.** A regression-after-a-change or a flaky-failure case hands you one
  for free (old vs new, pass vs fail); picking a "representative" run for an A/B comparison is
  judgment, not a free pair.
- **Short trajectories don't need this.** The value is upstream-of-visible-failure localization on
  long, branchy runs. On a 4-step agent, read the two traces yourself.
- **Two runs that share no vocabulary produce a confidently wrong answer today** (above) — the
  abstention guard is designed, not shipped.
- **Replay reproduces the recorded path, not the divergent one**, and stays live past the patched
  step against the original upstream. Counterfactual confirmation needs a run captured under
  `amberfork record`; a passively-ingested trace can locate a fork but not confirm its cause by
  re-execution.
- **`--verify` confirms 87% of real forks (n=15), and zero came back indeterminate** — but that
  number has no independently-known ground truth behind it, because it's measured on genuine LLM
  sampling-variance forks, not manufactured ones. The one corpus in this repo with a truly known
  cause (#49's perturbation harness) turns out to be unreachable by the patch mechanism: the fault
  lives in the agent's own local code, never in a recorded network response, so there's nothing for
  `--verify` to patch. Precision against a known cause — as opposed to a confirmation rate — remains
  unmeasured, and notebook 079 explains exactly why rather than papering over it (issue #48).
- **On natural (non-injected) agent divergence, this benchmark shows a loss to LLM judges, not
  parity.** The 91% headline number has no judge comparison at all — chimera's injected gold step is
  a splice point, which would hand a judge arm an unfair tell, so BENCHMARK.md's own protocol
  excludes that comparison by design. Where a judge comparison *does* exist (TRAIL/HAL, above), three
  of five judge arms win. The honest claim is narrower than "matches a frontier judge": it's
  "beats position and structure baselines by a wide margin on controlled divergence; loses to
  frontier judges, so far, on the one natural corpus tried."

## Try it

```sh
cargo install amberfork
amberfork serve --demo        # the fork above, in your browser
amberfork demo                 # the same fork, in your terminal
```

Every table on this page reproduces offline, with no key, from a committed results file:

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/chimera_noise_multiseed_test.json
cargo run -q -p amberfork-bench -- report --results bench/results/trail_hal_natural_all.json
cargo run -q -p amberfork-bench -- report --results bench/results/perturbation_all.json
```

The `--verify` confirmation-rate table doesn't fit that schema (it's a tri-state distribution, not a
step-localization score), so it's a plain committed JSON instead — still zero live calls to read:

```sh
python3 -c "import json; d=json.load(open('bench/results/verify_realprovider_all.json')); \
print(json.dumps({k:v for k,v in d.items() if k!='raw_attributions'}, indent=2))"
```

Point `amberfork diff`/`serve` at your own traces from there —
[the trace format](trace-format.md), [a worked example](run-on-your-own-agent.md). If you find a
case where it's confidently wrong, that's the known defect above; an issue with the trace attached
is the most useful thing you could send.
