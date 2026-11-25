# Launch drafts (issue #57)

Drafts for the day v1.1.0 is released. Nothing here has been posted.

Before posting:

- **Wait for the tag.** `cargo install amberfork` gives v1.0.0, which has no `replay`, until
  v1.1.0 is on crates.io (#64).
- **Check each venue's current rules.** The tags and flair below are suggestions and were not
  verified against the sites.
- Every number is copied from [`writeup.md`](writeup.md). If one is edited there, edit it here.

Links used throughout:

- Repo: `https://github.com/Melvin0070/amberfork`
- Writeup: `https://github.com/Melvin0070/amberfork/blob/main/docs/writeup.md`
- Evidence: `https://github.com/Melvin0070/amberfork/blob/main/docs/evidence.md`

## Show HN

**Title** (74 characters; the limit is 80):

```text
Show HN: Amberfork – diffing two agent runs didn't work, so it replays one
```

**URL:** the repo.

**First comment:**

```text
I built this to answer one question: an agent run failed, a similar run passed, where did they
part ways? The plan was to align the two trajectories (affine-gap Needleman-Wunsch over steps,
with a lexical cost) and mark the first step where they stop agreeing.

On a controlled benchmark that works. With a known fork spliced into real agent logs and noise
added around it, the aligner places the fork within 3 steps 91% of the time, against 49% for a
positional diff (n=35, sealed test split).

Then I ran it on natural pairs: 685 pairs of published Terminal-Bench 2.0 trials, same agent,
model and task. It reported a fork on 96.7% of pairs where both runs passed. Its score separated
pass/fail pairs from pass/pass pairs no better than a score that only knows the two run lengths
(AUROC 0.614 for both). Two passing runs just don't share much. The sealed test side, 2,670 more
pairs scored once at release, said the same: 97.5%, and 0.593 against 0.605 for length. That is
one corpus, but it was enough to change the product.

What works is narrower. Record a run once, then replay the agent against the recording, and
report the first request the recording never saw. On 35 recordings of a local tool-using agent
(11 to 20 model calls each):

- replay, agent unchanged: 0 of 35 false alarms
- diff of two live runs, agent unchanged: 56 of 70 false alarms
- replay with one tool result changed: caught 105 of 105, each at the exact call

Record-and-replay is an old idea and I'm not claiming it. The part I think is worth your time is
the measurement, and the rest of the writeup: a corpus where 3 of 5 LLM-judge arms beat my
aligner, a corpus I retired after a pre-registered repair failed, and an experiment that killed
its own milestone. Every number re-renders offline from a committed file.

It's a single Rust binary, local, no account and no API key:

  cargo install amberfork
  amberfork serve --demo

Limits: replay doesn't test the model (responses come from the recording), requests must be
reproducible, and alignment is quadratic (35 s and 152 MB at 2000 steps per side).

Writeup: https://github.com/Melvin0070/amberfork/blob/main/docs/writeup.md

I'd most like to hear from anyone gating agent changes in CI today, and what you compare against.
```

## lobste.rs

**Title:**

```text
Diffing two agent runs doesn't find the failure. Replaying one does.
```

**URL:** the writeup. **Suggested tags:** `rust`, `testing`, plus whichever AI tag is current.
Tick "I am the author".

**Comment:**

```text
Author here. This is a writeup of a tool whose first idea failed its own benchmark.

The idea was to align a failing agent run against a passing one and report where they fork. It
holds on injected forks (within 3 steps 91% of the time, against 49% for a positional diff, n=35)
and fails on natural ones: on 685 Terminal-Bench pairs it reports a fork on 96.7% of pairs where
both runs passed, and does no better than run length at telling the two kinds of pair apart. A
sealed test side of 2,670 pairs, scored once at release, confirmed both.

What survived is record-and-replay as a CI gate, measured against the live diff it replaced:
0 of 35 false alarms against 56 of 70.

All results are pre-registered in an append-only notebook in the repo, including the ones that
went against the tool. It is Rust, local, and needs no API key.
```

## r/rust

**Title:**

```text
amberfork: a local Rust CLI that records an AI agent's run and replays it as a regression gate
```

**Body:**

```text
I've been building amberfork, a tool for finding where an AI-agent run stopped doing what it did
before. It is a single binary: record a run's provider traffic once, replay your agent against
the recording offline, and it names the first request that differs. `diff` then aligns two runs
and shows what changed, in the terminal or in a browser view.

Some Rust details people here may care about:

- 11 crates. The engine crates (model, align, layout, attribution) are sync and pure; tokio and
  reqwest are confined to the I/O edges (record, replay, server, ingest).
- The alignment is move-typed, affine-gap Needleman-Wunsch over steps with a tf-idf cost. It is
  quadratic in time and memory, and I measured the wall instead of projecting it: 35 s and 152 MB
  at 2000 steps per side, 4.6 min and 789 MB at 5000. No banding yet.
- The web UI is Leptos in a separate workspace, built with trunk and embedded into the binary
  with rust-embed, so `amberfork serve` needs no files beside the binary.
- serde_json's `preserve_order` is load-bearing: benchmark fixtures must regenerate
  byte-for-byte, and CI rebuilds a committed results aggregate and compares bytes.
- Releases build natively on five targets (macOS and Linux on x86-64 and ARM64, Windows x86-64)
  so each binary's smoke test runs on its own architecture.

The honest part: the tool's original idea, diffing two independent runs to find a failure, did
not survive measurement on real agent runs. The writeup covers that and what replaced it:
https://github.com/Melvin0070/amberfork/blob/main/docs/writeup.md

Repo: https://github.com/Melvin0070/amberfork

Feedback on the crate layout and the aligner is very welcome.
```

## r/LocalLLaMA

**Title:**

```text
I measured how often two runs of the same local agent "diverge" when nothing changed: 56 of 70
```

**Body:**

```text
I build a local tool (amberfork, Rust, no account or API key) for catching regressions in agents.
One measurement from it may be useful to anyone testing agents on local models.

Setup: a tool-using agent on qwen3:8b through Ollama, temperature 0.8, choosing its own lookups
against a small invented world. 35 recorded sessions, 11 to 20 model calls each.

- Diffing two live runs of the unchanged agent flagged a difference on 56 of 70 pairs (80%).
- When I changed one tool's result, the live diff flagged 30 of 35 (86%). So it could not tell a
  changed agent from an unchanged one.
- Nearly all of those differences (54 of 56) were the final answer worded differently, not a
  different sequence of tool calls.
- Replaying the agent against its own recording raised 0 false alarms in 35, and caught 105 of
  105 changed tool results at the exact call.

So if you compare two live runs to decide whether a prompt or tool change broke your agent, most
of what you see is sampling noise in the last message. Replaying against a recording removes
that, at a cost: the model's responses come from the recording, so it tests your agent's code
and prompts, not a new model.

It is one model, one agent and one invented world, so read 80% as "this agent", not "agents".

The recordings are committed and the table re-scores offline. Everything ran locally at zero API
cost. Writeup, including the results that went against the tool:
https://github.com/Melvin0070/amberfork/blob/main/docs/writeup.md
```
