# agentdiff

[![ci](https://github.com/Melvin0070/agentdiff/actions/workflows/ci.yml/badge.svg)](https://github.com/Melvin0070/agentdiff/actions/workflows/ci.yml)

Point at a failing AI-agent run. agentdiff aligns it against a known-good run, finds the
exact step where they diverged, and shows what changed. Local, deterministic, no account.

> **Status: pre-v1 — no usable binary yet.** Current work is a feasibility spike on the two
> load-bearing assumptions (can semantic move-typed alignment beat a positional diff at
> localizing the decisive error step, and can failing↔passing reference pairs be constructed
> from public data). Results land in [`docs/notebook.md`](docs/notebook.md).

## What v1 will do

- `adiff diff <bad> --against <good>` — align two agent-run traces (OTel GenAI /
  OpenInference / [plain JSON](docs/trace-format.md)), light the fork up in the terminal,
  `--json` for machines.
- `cargo run -p adiff-bench` — reproduce the scoring table offline, deterministically, no
  API key. Protocol: [`BENCHMARK.md`](BENCHMARK.md).

## What exists today

| Artifact | What it is |
|---|---|
| [`spike/`](spike/) | Throwaway feasibility spike (Python): alignment vs positional baseline on real multi-agent failure logs |
| [`docs/notebook.md`](docs/notebook.md) | Engineering notebook: questions, measurements, dead ends |
| [`docs/trace-format.md`](docs/trace-format.md) | The canonical plain-JSON trace format v1 accepts |
| [`BENCHMARK.md`](BENCHMARK.md) | Pre-registered evaluation protocol (splits, baselines, threats to validity) |
| [`DESIGN.md`](DESIGN.md) | Visual system ("sameness recedes, divergence glows") |
| [`docs/design/`](docs/design/) | Architecture + positioning corpus (the locked build plan) |
