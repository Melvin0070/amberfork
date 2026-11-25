#!/usr/bin/env python3
"""Score replay on the long, free-running corpus (notebook 088, issue #65).

Four classes, all through the shipped `amberfork` binary, none calling a model:

  A  replay, unchanged agent      each reference tape replayed strict; exit 1 = false positive
  B  replay, changed agent        each tape replayed with its first / middle / last registered
                                  change; should exit 1 at the gold exchange
  C  live diff, unchanged         `diff` on every pair of reference tapes within a task
  D  live diff, changed           `diff <changed> --against <reference> --json`, where the changed
                                  session was recorded live with the tape's middle change

Gold comes from the tape alone (see `longrun_tape.py`). Every run is scored; no retries, no
exclusions on outcome. Harness defects (a context-limit hit, a missing changed session) are
printed before any rate.

Usage:
    python3 spike/score_replay_longrun.py --amberfork-bin target/release/amberfork
"""

from __future__ import annotations

import argparse
import itertools
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

from longrun_agent import NUM_CTX
from longrun_tape import POSITIONS, context_defects, gold_exchange, registered_changes

# Matches `Z95` in crates/amberfork-bench/src/score.rs.
Z95 = 1.959963984540054
FOLLOWED, DIVERGED, TROUBLE = 0, 1, 2
DIVERGED_AT = re.compile(r"replay diverged at exchange (\d+)")
AGENT = Path(__file__).resolve().parent / "longrun_agent.py"
NEAR = 3


def wilson95(k: int, n: int) -> dict:
    if n == 0:
        return {"hits": 0, "n": 0, "rate": 0.0, "ci95_lo": 0.0, "ci95_hi": 0.0}
    p = k / n
    z2 = Z95 * Z95
    denom = 1.0 + z2 / n
    center = (p + z2 / (2.0 * n)) / denom
    half = Z95 * ((p * (1.0 - p) / n + z2 / (4.0 * n * n)) ** 0.5) / denom
    return {"hits": k, "n": n, "rate": p, "ci95_lo": max(0.0, center - half), "ci95_hi": min(1.0, center + half)}


def rate(rows: list[dict], hit) -> dict:
    return wilson95(sum(1 for r in rows if hit(r)), len(rows))


def replay(amberfork_bin: Path, cassette: Path, task: str, change: str | None) -> dict:
    agent = ["python3", str(AGENT), "--task", task]
    if change:
        agent += ["--change", change]
    proc = subprocess.run(
        [str(amberfork_bin), "replay", str(cassette), "--base-url-env", "AMBERFORK_BASE_URL", "--", *agent],
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


def diff(amberfork_bin: Path, observed: Path, reference: Path) -> dict:
    proc = subprocess.run(
        [str(amberfork_bin), "diff", str(observed), "--against", str(reference), "--json"],
        capture_output=True,
        text=True,
    )
    fork = None
    if proc.returncode == DIVERGED:
        fork = (json.loads(proc.stdout).get("fork") or {}).get("b_step")
    return {"exit": proc.returncode, "fork_b_step": fork}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--amberfork-bin", default="target/release/amberfork")
    parser.add_argument("--data", default="bench/recordings/replay_longrun_v1")
    parser.add_argument("--out", default="bench/results/replay_longrun_all.json")
    args = parser.parse_args()

    amberfork_bin = Path(args.amberfork_bin).resolve()
    data_root = Path(args.data)
    for path in (amberfork_bin, data_root):
        if not path.exists():
            print(f"error: {path} does not exist", file=sys.stderr)
            return 1

    a_rows: list[dict] = []
    b_rows: list[dict] = []
    c_rows: list[dict] = []
    d_rows: list[dict] = []
    defects: list[str] = []
    lengths: list[int] = []

    for task_dir in sorted(p for p in data_root.iterdir() if p.is_dir()):
        task = task_dir.name
        references = sorted(task_dir.glob("ref_*.cassette.json"))
        for tape in references:
            session = tape.name[: -len(".cassette.json")]
            cassette = json.loads(tape.read_text())
            lengths.append(len(cassette["exchanges"]))
            for idx in context_defects(cassette, NUM_CTX):
                defects.append(f"{task}/{session}: exchange {idx} reached the {NUM_CTX}-token context")

            same = replay(amberfork_bin, tape, task, None)
            a_rows.append({"task": task, "tape": session, **same})
            print(f"  A {task:18s} {session} -> exit {same['exit']}", flush=True)

            changes = registered_changes(cassette)
            for position in POSITIONS:
                if position not in changes:
                    continue
                pick = changes[position]
                got = replay(amberfork_bin, tape, task, pick["change"])
                b_rows.append({"task": task, "tape": session, "position": position, **pick, **got})
                print(f"  B {task:18s} {session} {position:6s} {pick['change']:22s}"
                      f" -> exit {got['exit']} at {got['diverged_at']} (gold {pick['gold']})", flush=True)

            changed_tape = task_dir / tape.name.replace("ref_", "changed_")
            middle = changes.get("middle")
            if middle is None or not changed_tape.exists():
                defects.append(f"{task}/{session}: no changed session for class D")
                continue
            changed_gold = gold_exchange(json.loads(changed_tape.read_text()), middle["change"])
            got = diff(amberfork_bin, changed_tape, tape)
            d_rows.append({"task": task, "tape": session, "change": middle["change"],
                           "changed_gold": changed_gold, **got})
            print(f"  D {task:18s} {session} -> exit {got['exit']} fork {got['fork_b_step']}"
                  f" (gold {changed_gold})", flush=True)

        for good, bad in itertools.combinations(references, 2):
            got = diff(amberfork_bin, bad, good)
            c_rows.append({"task": task, "good": good.name, "bad": bad.name, **got})

    def exact_d(r: dict) -> bool:
        return r["changed_gold"] is not None and r["fork_b_step"] == r["changed_gold"]

    def near_d(r: dict) -> bool:
        return (r["changed_gold"] is not None and r["fork_b_step"] is not None
                and abs(r["fork_b_step"] - r["changed_gold"]) <= NEAR)

    def exact_b(r: dict) -> bool:
        return r["exit"] == DIVERGED and r["diverged_at"] == r["gold"]

    golds = Counter(r["gold"] for r in b_rows)
    constant = golds.most_common(1)[0] if golds else (None, 0)

    def per_task(rows: list[dict], hit) -> dict:
        return {t: f"{sum(1 for r in rows if r['task'] == t and hit(r))}/{sum(1 for r in rows if r['task'] == t)}"
                for t in sorted({r["task"] for r in rows})}

    doc = {
        "protocol": "replay-longrun",
        "notebook_registration": 88,
        "model": "qwen3:8b",
        "gold_rule": "first exchange whose response makes the changed call, plus one",
        "defects": defects,
        "tape_exchanges": {"n": len(lengths), "min": min(lengths, default=0),
                           "median": sorted(lengths)[len(lengths) // 2] if lengths else 0,
                           "max": max(lengths, default=0)},
        "A_replay_unchanged": {
            "false_positive": rate(a_rows, lambda r: r["exit"] == DIVERGED),
            "trouble": sum(1 for r in a_rows if r["exit"] == TROUBLE),
            "per_task": per_task(a_rows, lambda r: r["exit"] == DIVERGED),
        },
        "B_replay_changed": {
            "detected": rate(b_rows, lambda r: r["exit"] == DIVERGED),
            "exact_exchange": rate(b_rows, exact_b),
            "by_position": {
                p: {"detected": rate([r for r in b_rows if r["position"] == p], lambda r: r["exit"] == DIVERGED),
                    "exact_exchange": rate([r for r in b_rows if r["position"] == p], exact_b)}
                for p in POSITIONS
            },
            "gold_distribution": {str(g): n for g, n in sorted(golds.items())},
            "best_constant": {"answer": constant[0], **wilson95(constant[1], len(b_rows))},
            "trouble": sum(1 for r in b_rows if r["exit"] == TROUBLE),
        },
        "C_live_diff_unchanged": {
            "false_positive": rate(c_rows, lambda r: r["exit"] == DIVERGED),
            "trouble": sum(1 for r in c_rows if r["exit"] == TROUBLE),
            "per_task": per_task(c_rows, lambda r: r["exit"] == DIVERGED),
        },
        "D_live_diff_changed": {
            "detected": rate(d_rows, lambda r: r["exit"] == DIVERGED),
            "exact_exchange": rate(d_rows, exact_d),
            f"within_{NEAR}": rate(d_rows, near_d),
            "change_not_reached": sum(1 for r in d_rows if r["changed_gold"] is None),
            "trouble": sum(1 for r in d_rows if r["exit"] == TROUBLE),
        },
        "raw": {"A": a_rows, "B": b_rows, "C": c_rows, "D": d_rows},
    }
    out = Path(args.out)
    out.write_text(json.dumps(doc, indent=2) + "\n")

    def show(label: str, r: dict) -> None:
        print(f"{label:42s} {r['hits']}/{r['n']}  {r['rate']:.3f} [{r['ci95_lo']:.3f}, {r['ci95_hi']:.3f}]")

    print(f"\nwrote {out}")
    for defect in defects:
        print(f"DEFECT: {defect}")
    print(f"tape exchanges: {doc['tape_exchanges']}")
    show("A replay false positives (unchanged)", doc["A_replay_unchanged"]["false_positive"])
    show("B replay detected (changed)", doc["B_replay_changed"]["detected"])
    show("B replay exact exchange (changed)", doc["B_replay_changed"]["exact_exchange"])
    show(f"B best constant answer ({constant[0]})", doc["B_replay_changed"]["best_constant"])
    show("C live diff false positives (unchanged)", doc["C_live_diff_unchanged"]["false_positive"])
    show("D live diff detected (changed)", doc["D_live_diff_changed"]["detected"])
    show("D live diff exact exchange (changed)", doc["D_live_diff_changed"]["exact_exchange"])
    show(f"D live diff within {NEAR} (changed)", doc["D_live_diff_changed"][f"within_{NEAR}"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
