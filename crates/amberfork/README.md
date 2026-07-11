# amberfork

Point at a failing AI-agent run. amberfork aligns it against a known-good run, finds the
exact step where they diverged, and shows what changed. Local, deterministic, no account.

```sh
cargo install amberfork
amberfork demo        # a bundled sample pair — see a fork with zero setup, no files
```

Then point it at your own traces:

```sh
amberfork diff bad.json --against good.json    # exits 1 on a fork; --json for machines
```

Traces are plain JSON — a run is an ordered list of steps (`llm` / `tool` / `agent` /
`other`, each with a name and inputs/outputs). The format is deliberately forgiving and
documented in
[docs/trace-format.md](https://github.com/Melvin0070/amberfork/blob/main/docs/trace-format.md);
converting your agent framework's log into it is typically a ~50-line script.

## Why trust it

- **Deterministic.** Lexical/tf-idf alignment — same inputs, same answer, no model call, no
  API key, nothing leaves your machine.
- **Measured.** On the pre-registered benchmark (sealed test split, n=35), the engine
  localizes the injected fork within 3 steps 91% of the time vs 49% for the best positional
  baseline. Protocol, splits, and every number:
  [BENCHMARK.md](https://github.com/Melvin0070/amberfork/blob/main/BENCHMARK.md).
- **Honest output.** Absorbed noise (retries, re-orders) is shown as such; the fork ships
  with a confidence, and `--json` exposes the full alignment for machines.

Full documentation, benchmark reproduction, and the engineering notebook live in the
[repository](https://github.com/Melvin0070/amberfork).
