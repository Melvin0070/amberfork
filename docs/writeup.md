# amberfork: diffing two agent runs doesn't find the failure. Replaying one does.

*The v1.1 writeup (issue #57). Every number below comes from a file committed in this repo, and
[`docs/evidence.md`](evidence.md) has the full table and the command behind each one. Several
results went against the tool. They are here on purpose.*

I built amberfork to answer one question: an agent run failed, a similar run passed, so where did
they part ways? The plan was to align the two trajectories with a real sequence-alignment
algorithm, mark the first step where they stop agreeing, and call that the fork.

Then I measured it on real agent runs. On the one natural corpus large enough to tell, comparing
two *independent* runs did not work: two passing runs of the same agent on the same task disagree
almost as much as a passing run and a failing one. What does work is narrower: record a run once, replay the agent against that recording, and
report the first request the recording never saw.

This post is the measurements that forced that change.

```sh
cargo install amberfork
amberfork serve --demo        # a bundled sample fork, in your browser
amberfork demo                # the same fork, in your terminal
```

![amberfork serve --demo: two agent runs aligned side by side in the browser on one shared timeline. The steps they agree on recede in gray; the step where the bad run fetched a stale refund policy ignites amber as the fork, the divergent path glows amber down to the wrong answer, and the field diff on the right shows the swapped policy doc in red and green.](assets/hero-web.gif)

## 1. The controlled result: alignment beats position under noise

The first benchmark splices a known fork into real agent logs
([Who&When](https://github.com/ag2ai/Agents_Failure_Attribution)), then adds benign noise around
it: reworded steps with tokens dropped, and one duplicated retry. The question is whether the aligner still
finds the splice.

On the sealed test split (n=35, three seeds), the engine places the fork within 3 steps **91% of
the time** (Wilson 95% CI [0.78, 0.97]). The best baseline, a positional diff with the same
lexical cost, reaches 49% [0.33, 0.64].

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/chimera_noise_multiseed_test.json
```

Two caveats before trusting it:

- **Exact-step accuracy is seed-sensitive.** It is 0.49 pooled, but 0.75, 0.23 and 0.50 on the
  three seeds. That is why the claim is the ±3 window.
- **The forks are injected.** Sustained divergence at the gold step is true by construction, which
  favors content-aware alignment. This measures tolerance to noise around a known fork. It says
  nothing about finding the cause of a natural failure.

The second caveat is the important one, so most of the work since went into looking for a natural
fork.

## 2. Four attempts at a natural fork

| attempt | n | what happened |
|---|---|---|
| Mode A′: a failing run against a passing run from a different agent system | 4 | **Null.** Engine 0.50 within ±3, random 0.75, as published. Runs of 7 to 10 steps make a ±3 window nearly the whole run. Since the guard in section 7, the engine abstains on 3 of the 4 |
| TRAIL↔HAL: human-annotated failures against passing references, same scaffold, different model | 23 dev | **A loss.** Engine 0.000 exact, random 0.174, and three of five LLM-judge arms beat the engine |
| Perturbation: a real local agent with one tool's return value swapped | 25 | **A tie.** Engine 1.00 exact, and so is the positional diff it exists to beat |
| Terminal-Bench 2.0: published trials of the same agent, model and task | 685 dev | **The null that changed the product.** Section 3 |

**TRAIL↔HAL deserves the detail, because it is a real loss.** I built the baseline the field
compares against, "just ask a frontier model", and registered it before writing any judge code
(notebook 069). The best arm, a single-trace judge on gpt-5.6-sol, scored 0.261 exact against the
engine's 0.000, and three of the five judge arms beat the engine with paired-bootstrap intervals
that exclude zero (notebook 071). Two things qualify that without erasing it. No arm clears
random at ±3, so the corpus barely discriminates any method. And the engine's zero had a
mechanical cause: the two sides share no step names at all, so every aligner arm answered "step
0" on all 23 pairs.

That diagnosis came with a registered repair and, fixed in advance, what would follow if the
repair failed (notebook 070). The repair is built. On the nine dev pairs that a pre-set rule calls
comparable, the engine scores **0 of 9** exact against 1 of 9 for random (notebook 093). So the
registered conclusion applied: the corpus is retired as a negative result, nothing more is tuned
against it, and the test side of the repaired corpus was never scored.

**The perturbation tie taught something about harnesses.** That harness changes what a tool
returns but never how many steps happen, so the two runs stay index-aligned and position cannot
lose. Alignment only earns its keep when runs differ in length or order before the fork
(notebook 075).

## 3. The null that changed the product

Terminal-Bench 2.0 publishes many trials of the same agent, model and task, some passing and some
failing. That gives natural pairs whose structure varies on its own: 685 dev pairs across 23 tasks
and 9 scaffolds, registered in notebook 083 and scored in 084. There are no step-level cause
labels, so the question is simpler: can the diff tell a pass-versus-fail pair from a
pass-versus-pass pair?

| measurement | result |
|---|---|
| `diff` reports a fork when **both runs passed** | **96.7%** [93.9%, 98.3%], n=273 |
| `diff` reports a fork when one passed and one failed | 95.6% [93.2%, 97.2%], n=412 |
| AUROC of the engine's in-sync fraction, separating the two classes | 0.614 [0.580, 0.649] |
| AUROC of a score that knows only the two run lengths | 0.614 [0.580, 0.652] |
| engine minus positional diff (paired) | +0.045 [+0.020, +0.069] |

There is one small win in that table. Alignment beats a positional diff on real runs, with an
interval that excludes zero. It is this project's first such result without injected noise.

The rest is a null for the original idea. The engine does no better than run length alone, and it
reports a fork on nearly every pair of *passing* runs. The likely reason, from an exploratory look
that was not registered, is that two passing runs already share only about a third of their
steps. On this corpus a fork between two independent runs is the normal case, not evidence of a
failure.

Two limits on that conclusion. It is one corpus, scored on the dev split; the test split is sealed
until the v1.1.0 tag. And notebook 084 names two things that could be hiding content signal and
were not tested: observations are truncated at 5,000 characters, and the cost model compares
outputs only, so most tool calls are matched by name alone.

## 4. What works instead: record once, replay

If independent runs diverge on their own, compare against a run that cannot. `amberfork record`
captures an agent's provider traffic. `amberfork replay` then answers every provider call from the
recording, so the new run is the old run until the agent sends a request the recording never saw.
It needs no network and no key.

```sh
amberfork record --upstream https://api.openai.com --base-url-env OPENAI_BASE_URL \
    --out good.cassette.json -- python agent.py
amberfork replay good.cassette.json --base-url-env OPENAI_BASE_URL -- python agent.py
# exit 0: followed the recording · 1: diverged, and at which exchange · 2: trouble
```

Record-and-replay is an old idea, and amberfork did not invent it. What this repo adds is a
measurement of how much it matters on one agent. I recorded a tool-using agent, driven by a local
model, that chooses its own lookups: 35 recordings of 11 to 20 exchanges each (notebooks 088 and
089).

| what was run | result |
|---|---|
| `replay`, agent unchanged: false alarms | **0 of 35** [0%, 10%] |
| `diff` on two live runs, agent unchanged: false alarms | **56 of 70** [69%, 88%] |
| `replay`, one lookup's result changed: caught | 105 of 105 |
| `replay`, one lookup's result changed: named the exact exchange | 105 of 105, at exchanges 1 to 19 |
| `diff`, live changed run against its reference: flagged | 30 of 35 |
| `diff`, same pairs: fork within ±3 of the change | 0 of 35 |

A live diff flagged 86% of changed agents and 80% of unchanged ones, so its alarm carries almost
no information about the change. Replay's two rates are 0% and 100%.

Read this narrowly:

- **Most live-diff forks are the last message.** 54 of the 56 false alarms are at the final
  exchange, where the model words its closing answer differently. That is a true false alarm for
  a CI gate, but a narrower effect than Terminal-Bench's, and 80% is not a rate for agents in
  general.
- **The replay rows are what exact request matching implies.** Their value is that 140 replays
  went through the shipped binary on real model output with no fidelity defect. Naming the first
  request that differs is not attribution.
- **It is one model, one invented world, and an agent whose requests are reproducible.** A prompt
  containing the current time diverges at once.
- **`replay` does not test the model.** Responses come from the recording, so a model upgrade with
  unchanged prompts replays clean.

The recordings are committed, so the table re-scores offline:

```sh
cargo build -q --release -p amberfork
python3 spike/score_replay_longrun.py --amberfork-bin target/release/amberfork
```

Alignment is still in the tool. Once two runs share a prefix by construction, a recording and its
replay or a before and after under a pinned environment, `amberfork diff` shows what changed in
the terminal, as JSON, as a self-contained HTML file, or in the browser view above. It reads
plain JSON, its own recordings, Claude Code transcripts, Codex CLI rollouts, ATIF trajectories and
OpenTelemetry exports directly.

## 5. Confirming a cause: `diff --verify`

On two recorded runs, `--verify` re-executes the failing one with the fork step patched to the
good run's response and reports whether it recovers. It stays live past the patched step, so the
verdict is a majority vote over several replays and has three states: recovered, not recovered,
unverified.

On 15 forks it recovered **87%** [62%, 96%], with none unverified (notebook 080). Those forks
come from real sampling variance, but in a two-step toy agent built to fork. Precision against a known cause is unmeasured. The one corpus here
with a known cause puts the fault in the agent's local code, where a patched network response
cannot reach it (notebook 079).

## 6. Why any of these numbers should be believed

The accuracy results follow one pre-registered protocol ([`BENCHMARK.md`](../BENCHMARK.md)): the
decision rule is committed before the measurement runs. The scale timings in section 7 are plain
measurements with no registration. Dev and test splits are separate, and the test
split is scored once per release tag. Exclusions are counted, never dropped. A null is published
as readily as a win.

Three entries in the notebook show the protocol costing something:

- **An experiment that killed its own milestone (notebooks 065 and 066).** I registered a test of
  whether voting across ten noisy copies of a reference run beats trusting one. Consensus
  reproduced the clean reference's predicted step on 25 of 25 pairs. A single noisy copy already
  scored within 1.6 points of the clean one, so 1.6 points was all voting could gain under that
  noise model. The milestone was dropped with the ceiling measured. Real agent non-determinism
  could leave more headroom, and that was not tested.
- **A sequencing failure at v1.0.0 (notebook 081).** The sealed reveal ran after the tag was pushed
  instead of before. No number moved, and it is recorded as a failure anyway.
- **A void run (notebook 088a).** The first long-agent recording session ran with the model server
  down and logged 105 failed attempts in under a minute. The log is kept as void, and the
  protocol was amended before any real session was recorded.

[`docs/notebook.md`](notebook.md) is the honest entry point for anyone evaluating this repo: over
90 dated entries, including every experiment above in full.

## 7. Where it fails

- **It does not find a failure's cause by comparing two independent runs.** That is section 3.
- **It refuses runs that share nothing.** Until notebook 087, two unrelated runs produced "fork at
  step 0", at 0.97 confidence on the pair I inspected. Now `diff` exits 2 with "no common
  ground", and `--allow-unrelated` aligns them anyway. A run that changes at its first step and
  never re-syncs gets the same refusal.
- **On the one natural corpus with a judge comparison, frontier models beat it.** The 91% in
  section 1 has no judge comparison, because the sanitized fixtures would give a judge an unfair
  tell at the splice.
- **Alignment is quadratic in run length, in time and in memory.** Measured end to end on an Apple
  M5 (notebook 090):

  | steps per side | wall time | peak memory |
  |---|---|---|
  | 500 | 2.4 s | 24 MB |
  | 2000 (the default `--max-steps` guard) | 35 s | 152 MB |
  | 5000 | 4.6 min | 789 MB |

- **Short runs do not need it.** On a 4-step agent, read the two traces.

## Try it

```sh
cargo install amberfork        # or a prebuilt binary for macOS, Linux or Windows
amberfork serve --demo
```

[Run it on your own agent](run-on-your-own-agent.md) covers install, `record` and reading a fork.
[`docs/ci.md`](ci.md) has the replay gate as a CI workflow, and what it does not check. If you
find a case where it is confidently wrong, an issue with the trace attached is the most useful
thing you could send.
