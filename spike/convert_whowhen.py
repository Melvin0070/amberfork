#!/usr/bin/env python3
"""Convert Who&When failure logs (ag2ai/Agents_Failure_Attribution, MIT) into the
canonical trace JSON (docs/trace-format.md). Throwaway spike code.

Split quirks handled (verified empirically 2026-07-07):
  Hand-Crafted:        history[{content, role}]        speaker = role
  Algorithm-Generated: history[{content, role, name}]  speaker = name
  mistake_step: STRING-encoded 0-indexed int into history
  ground-truth key: 'groundtruth' (HF Hand-Crafted) vs 'ground_truth' (elsewhere)
  agent-name casing drift: 'Websurfer' vs 'WebSurfer'
"""

import argparse
import json
from pathlib import Path


def convert_log(raw, split, stem):
    steps = []
    for i, entry in enumerate(raw.get("history", [])):
        speaker = entry.get("name") or entry.get("role") or "unknown"
        steps.append({
            "idx": i,
            "kind": "agent",
            "name": speaker.replace("Websurfer", "WebSurfer"),
            "outputs": entry.get("content", ""),
            "attrs": {},
        })
    gt = raw.get("ground_truth", raw.get("groundtruth", ""))
    mistake_step = raw.get("mistake_step")
    return {
        "schema_version": "0.1",
        "id": f"whowhen_{split}_{stem}",
        "task": raw.get("question", ""),
        "outcome": "fail",
        "steps": steps,
    }, {
        "file": f"whowhen_{split}_{stem}.json",
        "split": split,
        "question_id": raw.get("question_ID", ""),
        "n_steps": len(steps),
        "gold_step": int(mistake_step) if mistake_step is not None else None,
        "mistake_agent": (raw.get("mistake_agent") or "").replace("Websurfer", "WebSurfer"),
        "ground_truth": gt,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, help="dir containing Hand-Crafted/ and Algorithm-Generated/")
    ap.add_argument("--out", default="spike/data/canonical")
    args = ap.parse_args()

    src = Path(args.src)
    outdir = Path(args.out)
    outdir.mkdir(parents=True, exist_ok=True)

    index = []
    for split_dir, split in (("Hand-Crafted", "hand"), ("Algorithm-Generated", "algo")):
        d = src / split_dir
        if not d.is_dir():
            print(f"skip missing split dir: {d}")
            continue
        for f in sorted(d.glob("*.json"), key=lambda p: int(p.stem)):
            raw = json.loads(f.read_text())
            run, meta = convert_log(raw, split, f.stem)
            (outdir / meta["file"]).write_text(json.dumps(run, indent=1))
            index.append(meta)

    (outdir / "index.json").write_text(json.dumps(index, indent=1))
    n_hand = sum(1 for m in index if m["split"] == "hand")
    n_algo = sum(1 for m in index if m["split"] == "algo")
    bad_gold = [m["file"] for m in index if m["gold_step"] is None or m["gold_step"] >= m["n_steps"]]
    print(f"converted {len(index)} logs (hand={n_hand}, algo={n_algo}) -> {outdir}")
    print(f"gold sanity: {len(bad_gold)} logs with missing/out-of-range mistake_step: {bad_gold}")


if __name__ == "__main__":
    main()
