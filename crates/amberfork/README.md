# amberfork

Find where an AI-agent run stopped doing what it did before. Record a run once, replay your
agent against it offline, and amberfork names the first step where its behaviour changed. Then
it aligns the two runs and shows what differed. Local, deterministic, no account, no API key.

```sh
cargo install amberfork
amberfork demo        # a bundled sample pair: see a fork with no setup and no files
```

Gate a change to your agent:

```sh
amberfork record --upstream https://api.openai.com --base-url-env OPENAI_BASE_URL \
    --out good.cassette.json -- python agent.py
amberfork replay good.cassette.json --base-url-env OPENAI_BASE_URL -- python agent.py
# exit 0: followed the recording · 1: diverged, and at which exchange · 2: trouble
```

See what changed between two runs:

```sh
amberfork diff bad.json --against good.json    # exits 1 on a fork; --json for machines
```

`diff` reads [plain JSON](https://github.com/Melvin0070/amberfork/blob/main/docs/trace-format.md),
`record` cassettes, Claude Code session transcripts, Codex CLI rollouts, ATIF trajectories and
OpenTelemetry OTLP/JSON exports as they are.

## What has been measured

- **`replay` is quiet where a live diff is not.** On 35 recordings of a tool-using agent, an
  unchanged agent replayed clean 35 of 35 times, while diffing two live runs of it forked on 56 of
  70 pairs. Replay caught 105 of 105 changed tool results at the exact exchange.
- **Alignment finds an injected fork under noise.** On the sealed test split (n=35) the engine
  places the fork within 3 steps 91% of the time, against 49% for the best baseline.
- **It does not find a failure's cause by comparing two independent runs.** On 685 natural pairs,
  and again on a sealed test side of 2,670, two passing runs of the same agent differ about as
  much as a passing and a failing one.

Every number, with its command, its caveats and the results that went against the tool:
[docs/evidence.md](https://github.com/Melvin0070/amberfork/blob/main/docs/evidence.md). The
protocol is [BENCHMARK.md](https://github.com/Melvin0070/amberfork/blob/main/BENCHMARK.md).

Full documentation and the engineering notebook live in the
[repository](https://github.com/Melvin0070/amberfork).
