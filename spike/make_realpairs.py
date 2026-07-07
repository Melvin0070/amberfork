#!/usr/bin/env python3
"""Build the first REAL cross-system Mode A' pairs: Who&When failing log (CaptainAgent team)
vs TapeAgents successful tape (Apache-2.0) on the SAME GAIA task. Gold = Who&When annotated
mistake_step. n is tiny — directional; proves cross-system constructibility end-to-end."""

import json
import sys
from pathlib import Path

CANON = Path("spike/data/canonical")
OUT = Path("spike/data/pairs_real")


def convert_tape(path):
    d = json.loads(Path(path).read_text())
    steps = []
    for i, st in enumerate(d["steps"]):
        body = {k: v for k, v in st.items() if k != "metadata"}
        kind = body.pop("kind", "step")
        agent = (st.get("metadata") or {}).get("agent") or ""
        steps.append({
            "idx": i,
            "kind": "agent",
            "name": f"{agent}:{kind}" if agent else kind,
            "outputs": json.dumps(body, sort_keys=True),
            "attrs": {},
        })
    md = d.get("metadata", {})
    task = md.get("task") if isinstance(md.get("task"), dict) else {}
    run = {"schema_version": "0.1", "id": f"tape_{Path(path).stem}",
           "task": (task or {}).get("Question", ""), "outcome": "pass", "steps": steps}
    return run, (task or {}).get("task_id"), (task or {}).get("Final answer", ""), md.get("result", "")


def main():
    tapes_dir = Path(sys.argv[sys.argv.index("--tapes") + 1])
    index = {m["question_id"]: m for m in json.loads((CANON / "index.json").read_text())}
    OUT.mkdir(parents=True, exist_ok=True)
    made = 0
    for tape_file in sorted(tapes_dir.glob("l1_task*.json")):
        tape, tid, gt, res = convert_tape(tape_file)
        if str(res).strip().lower() != str(gt).strip().lower():
            continue  # only successful tapes serve as references
        m = index.get(tid)
        if not m:
            continue
        failing = json.loads((CANON / m["file"]).read_text())
        (OUT / f"a_{made:02d}.json").write_text(json.dumps(failing, indent=1))
        (OUT / f"b_{made:02d}.json").write_text(json.dumps(tape, indent=1))
        (OUT / f"pair_{made:02d}.json").write_text(json.dumps({
            "failing": f"a_{made:02d}.json",
            "reference": f"b_{made:02d}.json",
            "gold_step": m["gold_step"],
            "meta": {"whowhen": m["file"], "tape": tape_file.name,
                     "task_id": tid, "cross_system": True},
        }, indent=1))
        print(f"pair_{made:02d}: {m['file']} (gold={m['gold_step']}, {m['n_steps']} steps) "
              f"vs {tape_file.name} ({len(tape['steps'])} steps)")
        made += 1
    print(f"built {made} real cross-system pairs -> {OUT}")


if __name__ == "__main__":
    main()
