#!/usr/bin/env python3
"""Score the replay gate on real recordings (notebook 085).

Every cassette under bench/data/perturbation is replayed twice through the *shipped*
`amberfork replay` binary in strict mode, with the real agent that recorded it:

  unchanged  the agent in the configuration that made the tape. Should exit 0; an exit 1
             is a false positive.
  changed    the same agent with --perturb flipped, so one tool returns a different value
             than the tape's run saw. Should exit 1, at the exchange where that value first
             reaches the model.

Gold for the changed class comes from the tape alone: (index of the first exchange whose
recorded response calls the task's designated tool) + 1. For perturbed tapes it must equal
the gold_step the agent wrote at record time; a disagreement is a harness defect and is
reported before any rate.

The same-fixture contrast (rule 8): `amberfork diff` on every pair of perturbed sessions
within a task — one agent configuration sampled five times, i.e. two live runs with nothing
changed — so the replay false-positive rate sits beside the live-diff rate on one corpus.

Every replay is scored. No retries, no exclusions on outcome.

Usage:
    python3 spike/score_replay_gate.py --amberfork-bin target/release/amberfork
"""

from __future__ import annotations

import argparse
import itertools
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# Matches `Z95` in crates/amberfork-bench/src/score.rs.
Z95 = 1.959963984540054
FOLLOWED, DIVERGED, TROUBLE = 0, 1, 2
DIVERGED_AT = re.compile(r"replay diverged at exchange (\d+)")
AGENT = Path(__file__).resolve().parent / "perturbation_agent.py"


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


def gold_from_tape(cassette: dict, designated_tool: str) -> int | None:
    """The exchange whose request first carries the designated tool's result."""
    for exchange in cassette["exchanges"]:
        message = exchange["response"]["body"].get("message", {})
        for call in message.get("tool_calls") or []:
            if call["function"]["name"] == designated_tool:
                return exchange["idx"] + 1
    return None


def replay(amberfork_bin: Path, cassette: Path, task: str, perturb: bool, scratch: Path) -> dict:
    agent = ["python3", str(AGENT), "--task", task, "--summary-out", str(scratch / "summary.json")]
    if perturb:
        agent.append("--perturb")
    proc = subprocess.run(
        [str(amberfork_bin), "replay", str(cassette), "--base-url-env", "AMBERFORK_BASE_URL", "--"]
        + agent,
        capture_output=True,
        text=True,
    )
    verdict = next((line for line in proc.stderr.splitlines() if "amberfork: replay" in line), "")
    match = DIVERGED_AT.search(verdict)
    return {
        "exit": proc.returncode,
        "diverged_at": int(match.group(1)) if match else None,
        "verdict": verdict.removeprefix("amberfork: "),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--amberfork-bin", default="target/release/amberfork")
    parser.add_argument("--data", default="bench/data/perturbation")
    parser.add_argument("--out", default="bench/results/replay_gate_all.json")
    args = parser.parse_args()

    amberfork_bin = Path(args.amberfork_bin).resolve()
    if not amberfork_bin.exists():
        print(f"error: {amberfork_bin} does not exist — build it first", file=sys.stderr)
        return 1
    data_root = Path(args.data)
    if not data_root.is_dir():
        print(f"error: {data_root} does not exist", file=sys.stderr)
        return 1

    unchanged: list[dict] = []
    changed: list[dict] = []
    live_pairs: list[dict] = []
    gold_defects: list[str] = []

    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        for task_dir in sorted(p for p in data_root.iterdir() if p.is_dir()):
            task = task_dir.name
            tapes = sorted(task_dir.glob("*.cassette.json"))
            for tape in tapes:
                session = tape.name[: -len(".cassette.json")]
                summary = json.loads((task_dir / f"{session}.summary.json").read_text())
                recorded_perturbed = bool(summary["perturbed"])
                gold = gold_from_tape(json.loads(tape.read_text()), summary["designated_tool"])
                if recorded_perturbed and gold != summary["gold_step"]:
                    gold_defects.append(
                        f"{task}/{session}: tape gold {gold} != recorded gold_step {summary['gold_step']}"
                    )

                same = replay(amberfork_bin, tape, task, recorded_perturbed, scratch)
                unchanged.append({"task": task, "tape": session, **same})
                print(f"  unchanged {task:22s} {session:14s} -> exit {same['exit']}", flush=True)

                flipped = replay(amberfork_bin, tape, task, not recorded_perturbed, scratch)
                changed.append({"task": task, "tape": session, "gold": gold, **flipped})
                print(
                    f"  changed   {task:22s} {session:14s} -> exit {flipped['exit']}"
                    f" at {flipped['diverged_at']} (gold {gold})",
                    flush=True,
                )

            perturbed = [t for t in tapes if t.name.startswith("perturbed")]
            for good, bad in itertools.combinations(perturbed, 2):
                code = subprocess.run(
                    [str(amberfork_bin), "diff", str(bad), "--against", str(good)],
                    capture_output=True,
                    text=True,
                ).returncode
                live_pairs.append({"task": task, "good": good.name, "bad": bad.name, "exit": code})

    def rate(rows: list[dict], hit) -> dict:
        return wilson95(sum(1 for r in rows if hit(r)), len(rows))

    doc = {
        "protocol": "replay-gate",
        "notebook_registration": 85,
        "model": "qwen3:8b",
        "gold_rule": "first exchange whose response calls the designated tool, plus one",
        "gold_defects": gold_defects,
        "unchanged": {
            "false_positive": rate(unchanged, lambda r: r["exit"] == DIVERGED),
            "trouble": sum(1 for r in unchanged if r["exit"] == TROUBLE),
        },
        "changed": {
            "detected": rate(changed, lambda r: r["exit"] == DIVERGED),
            "exact_exchange": rate(
                changed, lambda r: r["exit"] == DIVERGED and r["diverged_at"] == r["gold"]
            ),
            "trouble": sum(1 for r in changed if r["exit"] == TROUBLE),
        },
        "live_diff_same_config": {
            "note": "amberfork diff on pairs of perturbed sessions of one task: two live runs, nothing changed",
            "forked": rate(live_pairs, lambda r: r["exit"] == DIVERGED),
            "trouble": sum(1 for r in live_pairs if r["exit"] == TROUBLE),
        },
        "raw": {"unchanged": unchanged, "changed": changed, "live_pairs": live_pairs},
    }
    out = Path(args.out)
    out.write_text(json.dumps(doc, indent=2) + "\n")

    def show(label: str, r: dict) -> None:
        print(f"{label:34s} {r['hits']}/{r['n']}  {r['rate']:.3f} [{r['ci95_lo']:.3f}, {r['ci95_hi']:.3f}]")

    print(f"\nwrote {out}")
    for defect in gold_defects:
        print(f"GOLD DEFECT: {defect}")
    show("replay false positives (unchanged)", doc["unchanged"]["false_positive"])
    show("replay detected (changed)", doc["changed"]["detected"])
    show("replay exact exchange (changed)", doc["changed"]["exact_exchange"])
    show("live diff forked (same config)", doc["live_diff_same_config"]["forked"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
