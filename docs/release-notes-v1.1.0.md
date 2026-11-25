# v1.1.0: record once, replay

amberfork finds where an AI-agent run stopped doing what it did before. This release changes how
it does that, because the measurements said to. Comparing two independent runs does not find a
failure; replaying an agent against a recording of itself does.
[`docs/writeup.md`](writeup.md) is the full account.

## What's new since v1.0.0

- **`amberfork replay`.** Serves a recorded run back to your agent with no provider, no network
  and no key. Exit 0 means the agent followed the recording, 1 means it diverged (and names the
  exchange), 2 means trouble. [`docs/ci.md`](ci.md) has it as a CI gate.
- **Four trace formats load as they are.** `diff` and `serve` read Claude Code session
  transcripts, Codex CLI rollouts, ATIF `trajectory.json` files and OpenTelemetry OTLP/JSON
  exports directly, beside plain JSON and `record` cassettes.
- **`diff` refuses runs that share nothing.** Two runs with no step in common used to produce
  "fork at step 0" at high confidence. They now exit 2 with "no common ground";
  `--allow-unrelated` aligns them anyway. **This is a behaviour change:** such pairs used to
  exit 1.
- **Prebuilt binaries for five targets**: macOS and Linux on x86-64 and ARM64, and Windows x86-64.

## What was measured

- **`replay` against a live diff.** On 35 recordings of a tool-using agent (11 to 20 exchanges),
  an unchanged agent replayed clean 35 of 35 times. Diffing two live runs of the same unchanged
  agent forked on 56 of 70 pairs. Replay caught 105 of 105 changed tool results at the exact
  exchange.
- **Natural pairs from Terminal-Bench 2.0** (685 dev pairs). Alignment beats a positional diff
  (AUROC +0.045 [+0.020, +0.069]) but does no better than run length alone, and `diff` reports a
  fork on 96.7% of pairs where both runs passed.
- **The scale wall.** 35 s and 152 MB at the 2000-step default guard; 4.6 min and 789 MB at 5000
  steps per side.
- **The TRAIL↔HAL corpus is retired.** After a pre-registered ingest repair, the engine scored 0
  of 9 on the comparable pairs against 1 of 9 for random.

## What this release does not claim

- It does not find a failure's cause by comparing two independent runs.
- `replay` does not test the model: responses come from the recording. It also needs
  reproducible requests.
- The 91% localization number is on injected forks. On the one natural corpus with an LLM-judge
  comparison, three of five judge arms beat the engine.
- The replay measurements are one local model, one agent and one invented world.

## Upgrading

- **Minimum Rust is 1.88.** v1.0.0 declared 1.85 but did not build on it.
- **Exit codes:** a `diff` of two unrelated runs now exits 2 where it exited 1.
- **`amberfork_ingest::hal`** produces differently shaped runs: tool steps are named for their
  call, and an LLM step's `outputs` is the assistant message.

## Try it

```sh
cargo install amberfork
amberfork serve --demo
```

Every number above re-renders offline from a committed file; the commands are in
[`docs/evidence.md`](evidence.md). Full change list: [`CHANGELOG.md`](../CHANGELOG.md).
