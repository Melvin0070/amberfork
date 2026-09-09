#!/usr/bin/env python3
"""Score the CI gate's false-positive rate on real recordings (notebook 082 + 082a).

Two classes, both scored by shelling the *shipped* `amberfork diff` binary and reading
its exit code — never by calling the library, because the thing under test is the gate a
user actually puts in CI:

  negative  clean vs clean, the same unchanged agent recorded twice. Should exit 0.
            Every exit 1 is a false positive. This is the number the entry exists for.
  positive  perturbed vs reference, the 25 pairs #49 already recorded. Should exit 1.
            A weaker question than #49's localization table — "did the gate fire", not
            "was the fork in the right place" — and published as its own document so
            BENCHMARK.md rule 10 holds: #49's row is not disturbed.

Exit codes are diff(1)-style: 0 converged, 1 forked, 2 trouble. The exit-2 rate is
reported on its own for both classes; folding an infrastructure failure into either
rate would misstate both.

Every recorded pair is scored. There is no retry-until-converged and no dropping a pair
because of how it came out — discarding a forking clean pair would manufacture exactly
the result this measures (the discipline 074 set and 079 reused).

Direction is fixed, not averaged: pairs are ordered by session id, earlier as the
reference (`--against`) side. CI has a definite direction (candidate vs golden), and
scoring both directions would double n with correlated samples rather than add
information.

Usage:
    python3 spike/score_gate_falsepositive.py \
        --amberfork-bin target/debug/amberfork \
        --data bench/data/perturbation \
        --out bench/results/gate_falsepositive_all.json
"""

from __future__ import annotations

import argparse
import itertools
import json
import subprocess
import sys
from pathlib import Path

# Matches `Z95` in crates/amberfork-bench/src/score.rs so this document's intervals are
# computed the same way as every table the Rust harness renders.
Z95 = 1.959963984540054

CONVERGED, FORKED, TROUBLE = 0, 1, 2


def wilson95(k: int, n: int) -> dict:
    if n == 0:
        return {"hits": 0, "n": 0, "rate": 0.0, "ci95_lo": 0.0, "ci95_hi": 0.0}
    p = k / n
    z2 = Z95 * Z95
    denom = 1.0 + z2 / n
    center = (p + z2 / (2.0 * n)) / denom
    half = Z95 * ((p * (1.0 - p) / n + z2 / (4.0 * n * n)) ** 0.5) / denom
    return {
        "hits": k,
        "n": n,
        "rate": p,
        "ci95_lo": max(0.0, center - half),
        "ci95_hi": min(1.0, center + half),
    }


def sessions(task_dir: Path, prefix: str) -> list[str]:
    """Session ids with a cassette, sorted — the fixed pair ordering depends on this."""
    return sorted(p.name[: -len(".cassette.json")] for p in task_dir.glob(f"{prefix}*.cassette.json"))


def run_gate(amberfork_bin: Path, bad: Path, good: Path) -> int:
    proc = subprocess.run(
        [str(amberfork_bin), "diff", str(bad), "--against", str(good)],
        capture_output=True,
        text=True,
    )
    return proc.returncode


def score_class(amberfork_bin: Path, pairs: list[tuple[str, Path, Path]]) -> tuple[dict, list[dict]]:
    per_pair: list[dict] = []
    for task_id, good, bad in pairs:
        code = run_gate(amberfork_bin, bad, good)
        per_pair.append(
            {
                "task": task_id,
                "good": good.name,
                "bad": bad.name,
                "exit": code,
            }
        )
        print(f"  {task_id:22s} {good.stem:14s} vs {bad.stem:14s} -> exit {code}", flush=True)

    n = len(per_pair)
    forked = sum(1 for r in per_pair if r["exit"] == FORKED)
    converged = sum(1 for r in per_pair if r["exit"] == CONVERGED)
    trouble = sum(1 for r in per_pair if r["exit"] == TROUBLE)
    return (
        {
            "n": n,
            "forked": wilson95(forked, n),
            "converged": wilson95(converged, n),
            "trouble": wilson95(trouble, n),
        },
        per_pair,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--amberfork-bin", default="target/debug/amberfork")
    parser.add_argument("--data", default="bench/data/perturbation")
    parser.add_argument("--out", default="bench/results/gate_falsepositive_all.json")
    parser.add_argument(
        "--model",
        default="qwen3:8b",
        help="the model the recordings were made with — stamped into the document, since "
        "082a registers the false-positive rate as a per-model quantity",
    )
    args = parser.parse_args()

    amberfork_bin = Path(args.amberfork_bin).resolve()
    if not amberfork_bin.exists():
        print(f"error: {amberfork_bin} does not exist — build it first", file=sys.stderr)
        return 1

    data_root = Path(args.data)
    if not data_root.is_dir():
        print(f"error: {data_root} does not exist — record the sessions first", file=sys.stderr)
        return 1

    negative: list[tuple[str, Path, Path]] = []
    positive: list[tuple[str, Path, Path]] = []

    for task_dir in sorted(p for p in data_root.iterdir() if p.is_dir()):
        task_id = task_dir.name
        clean = sessions(task_dir, "reference") + sessions(task_dir, "clean")
        perturbed = sessions(task_dir, "perturbed")

        for a, b in itertools.combinations(sorted(clean), 2):
            negative.append((task_id, task_dir / f"{a}.cassette.json", task_dir / f"{b}.cassette.json"))
        for ref in sessions(task_dir, "reference"):
            for pert in perturbed:
                positive.append(
                    (task_id, task_dir / f"{ref}.cassette.json", task_dir / f"{pert}.cassette.json")
                )

    print(f"negative class (clean vs clean): {len(negative)} pairs")
    neg_summary, neg_pairs = score_class(amberfork_bin, negative) if negative else (None, [])
    if not negative:
        print("  none — no extra clean sessions recorded yet (see N_CLEAN_PER_TASK)")

    print(f"positive class (perturbed vs reference): {len(positive)} pairs")
    pos_summary, pos_pairs = score_class(amberfork_bin, positive) if positive else (None, [])

    doc = {
        "protocol": "gate-falsepositive",
        "notebook_registration": 82,
        "model": args.model,
        "pair_direction": "sorted by session id; earlier id is the --against (reference) side",
        "negative_class": neg_summary,
        "positive_class": pos_summary,
        "raw_pairs": {"negative": neg_pairs, "positive": pos_pairs},
    }

    if neg_summary is None:
        doc["incomplete"] = (
            "negative class not recorded — this document is not a result under 082 until it is"
        )

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=2) + "\n")
    print(f"\nwrote {out}")

    if neg_summary:
        fp = neg_summary["forked"]
        print(
            f"FALSE POSITIVE RATE: {fp['rate']:.3f} "
            f"[{fp['ci95_lo']:.3f}, {fp['ci95_hi']:.3f}]  n={fp['n']}"
        )
    if pos_summary:
        fire = pos_summary["forked"]
        print(
            f"gate fires on a real change: {fire['rate']:.3f} "
            f"[{fire['ci95_lo']:.3f}, {fire['ci95_hi']:.3f}]  n={fire['n']}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
