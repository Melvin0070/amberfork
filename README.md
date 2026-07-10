# amberfork

[![ci](https://github.com/Melvin0070/amberfork/actions/workflows/ci.yml/badge.svg)](https://github.com/Melvin0070/amberfork/actions/workflows/ci.yml)

Point at a failing AI-agent run. amberfork aligns it against a known-good run, finds the
exact step where they diverged, and shows what changed. Local, deterministic, no account.

![amberfork demo: two agent runs aligned in the terminal — a rate-limit retry is absorbed as a log-move, and the step where the bad run fetched a stale policy doc glows amber as the fork](docs/assets/hero.gif)

## Try it (30 seconds)

```sh
git clone https://github.com/Melvin0070/amberfork && cd amberfork
cargo run --release -q -p amberfork-cli -- demo
```

`demo` diffs a sample pair bundled inside the binary — no files, no setup, offline. Then
point it at your own traces ([plain-JSON format](docs/trace-format.md)):

```sh
cargo run --release -q -p amberfork-cli -- diff bad.json --against good.json   # exits 1 on a fork; --json for machines
```

> **Status: pre-v1.** `diff` and `demo` work from source (the v0.1 walking skeleton); not yet
> published to crates.io. The feasibility spike behind the core bet — semantic move-typed
> alignment beats a positional diff at localizing the decisive step — is done; measurements
> in [`docs/notebook.md`](docs/notebook.md).

## What v1 will do

- `amberfork diff <bad> --against <good>` — align two agent-run traces (OTel GenAI /
  OpenInference / [plain JSON](docs/trace-format.md)), light the fork up in the terminal,
  `--json` for machines.
- `cargo run -p amberfork-bench` — reproduce the scoring table offline, deterministically, no
  API key. Protocol: [`BENCHMARK.md`](BENCHMARK.md).

## Benchmark — dev baseline (pre-release)

Fork-localization on chimera pairs: a controlled fork spliced into real agent logs
([Who&When](https://github.com/ag2ai/Agents_Failure_Attribution)-derived), every arm on
identical pairs with an identical denominator. Protocol: [`BENCHMARK.md`](BENCHMARK.md).

**The robust result** (dev split, three seeds, n=25): the full engine localizes the fork
**within 3 steps 100% of the time** — vs the best baseline's 0.40 — where no baseline lands a
single exact hit. **Exact-step** localization is *seed-sensitive*: 0.75 / 0.29 / 0.60 on seeds
42 / 43 / 44 (aggregate **0.56**), so we lead with the ±3 window, not a single lucky seed
(notebook 014).

| arm (dev, n=25 across 3 seeds) | exact | ±1 | ±3 |
|---|---|---|---|
| random | 0.00 | 0.04 | 0.08 |
| pos-lexical | 0.00 | 0.12 | 0.40 |
| nw-structural/resync | 0.00 | 0.08 | 0.16 |
| **nw-lexical/resync** (full engine) | **0.56** | **0.72** | **1.00** |

The committed, offline-reproducible slice is **seed 42's** dev split (the CI gate additionally
pins seeds 43/44 — `bench/fixtures/chimera_noise_seed*_dev/`):

```text
params: bench/params.toml sha256:8ebd95ce8f3d · tau 0.3 · resync_k 2 · gap 0.6+0.3
| nw-lexical/resync | 0.75 [0.41, 0.93] exact | 0.88 ±1 | 1.00 ±3 | n=8 |
```

Read it honestly: these are **dev-split** numbers — the side all tuning happens on. The test
split stays sealed until a release tag (protocol rule 2). Wilson 95% intervals are wide at these
n; the claim they support is the *shape* — content-aware alignment localizes within a few steps
where position and structure do not.

Reproduce the seed-42 table offline — it renders from the committed results document, zero fetch:

```sh
cargo run -q -p amberfork-bench -- report
```

The dev-split pairs are committed (GAIA-sanitized — see `bench/fixtures/`); the CI gate scores
them on every `amberfork-align` change. Regenerate the full sets from raw upstream data with the
recipe in [`bench/fixtures/chimera_noise_seed42_dev/README.md`](bench/fixtures/chimera_noise_seed42_dev/README.md).

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
gold target. Both limits were pre-registered in [`BENCHMARK.md`](BENCHMARK.md)'s
threats-to-validity before the measurement (notebook 016). So Mode A′ ships as a disclosed
limit, not a headline — the table (cross-system banner included) re-renders offline:

```sh
cargo run -q -p amberfork-bench -- report --results bench/results/mode_a_prime_realpairs_all.json
```

## What exists today

| Artifact | What it is |
|---|---|
| [`crates/`](crates/) | The Rust workspace: model → ingest → align → CLI (`diff`, `demo`) — the walking skeleton |
| [`spike/`](spike/) | Throwaway feasibility spike (Python): alignment vs positional baseline on real multi-agent failure logs |
| [`docs/notebook.md`](docs/notebook.md) | Engineering notebook: questions, measurements, dead ends |
| [`docs/trace-format.md`](docs/trace-format.md) | The canonical plain-JSON trace format v1 accepts |
| [`BENCHMARK.md`](BENCHMARK.md) | Pre-registered evaluation protocol (splits, baselines, threats to validity) |
| [`DESIGN.md`](DESIGN.md) | Visual system ("sameness recedes, divergence glows") |
| [`docs/design/`](docs/design/) | Architecture + positioning corpus (the locked build plan) |
