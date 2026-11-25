#!/usr/bin/env python3
"""Record the long-run corpus registered in notebook 088 (issue #65).

Two passes through the shipped `amberfork record` binary and a live local model:

  reference  7 tasks x 5 sessions of the unchanged agent           -> ref_NN.cassette.json
  changed    one session per reference tape, with that tape's      -> changed_NN.cassette.json
             *middle* registered change active (class D)

A session is re-recorded (at most MAX_ATTEMPTS, every attempt logged in the manifest) only when
the agent exits 3 (no final answer within its turn budget) or 1 (transport error) — never on what
the run did. Sessions already on disk are kept, so an interrupted run resumes; nothing is ever
re-recorded because of how it scored, because nothing is scored here.

Usage (from the repo root, Ollama serving qwen3:8b):
    python3 spike/record_longrun_sessions.py --amberfork-bin target/release/amberfork
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from longrun_agent import MODEL
from longrun_tape import registered_changes
from longrun_world import PILOT_TASKS, TASKS

MAX_ATTEMPTS = 3
SESSIONS_PER_TASK = 5
UPSTREAM = "http://127.0.0.1:11434"
BASE_URL_ENV = "AMBERFORK_BASE_URL"
AGENT = (Path(__file__).parent / "longrun_agent.py").resolve()


def require_model() -> None:
    """Abort the whole run unless the model server answers and has the model (088a). A session
    that fails because nothing is listening is not an attempt: no model was asked anything, so it
    must not spend the retry budget or be written down as an exclusion."""
    try:
        with urllib.request.urlopen(f"{UPSTREAM}/api/tags", timeout=10) as resp:
            names = {m["name"] for m in json.loads(resp.read())["models"]}
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        raise SystemExit(f"error: no model server at {UPSTREAM} ({exc}) — nothing recorded, nothing logged")
    if MODEL not in names:
        raise SystemExit(f"error: {UPSTREAM} does not have {MODEL} — nothing recorded, nothing logged")


def record_one(amberfork_bin: Path, task_dir: Path, session: str, task: str, change: str | None, log: list) -> bool:
    cassette = task_dir / f"{session}.cassette.json"
    summary_path = task_dir / f"{session}.summary.json"
    if cassette.exists() and summary_path.exists():
        print(f"  [{task}/{session}] already recorded", flush=True)
        return True

    agent = ["python3", str(AGENT), "--task", task, "--summary-out", str(summary_path)]
    if change:
        agent += ["--change", change]
    for attempt in range(1, MAX_ATTEMPTS + 1):
        require_model()
        cassette.unlink(missing_ok=True)
        summary_path.unlink(missing_ok=True)
        started = time.monotonic()
        proc = subprocess.run(
            [str(amberfork_bin), "record", "--upstream", UPSTREAM, "--base-url-env", BASE_URL_ENV,
             "--out", str(cassette), "--id", f"{task}_{session}", "--", *agent],
            capture_output=True,
            text=True,
        )
        summary = json.loads(summary_path.read_text()) if summary_path.exists() else {}
        entry = {
            "task": task,
            "session": session,
            "attempt": attempt,
            "agent_exit": proc.returncode,
            "exchanges": summary.get("exchanges"),
            "seconds": round(time.monotonic() - started, 1),
        }
        log.append(entry)
        print(f"  [{task}/{session}] attempt {attempt}: exit {proc.returncode}, "
              f"{entry['exchanges']} exchanges, {entry['seconds']}s", flush=True)
        if proc.returncode == 0 and cassette.exists():
            return True
        if proc.returncode not in (1, 3):
            print(f"error: unexpected exit {proc.returncode}: {proc.stderr.strip()[-300:]}", file=sys.stderr)
            raise SystemExit(1)
    cassette.unlink(missing_ok=True)
    summary_path.unlink(missing_ok=True)
    return False


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--amberfork-bin", default="target/release/amberfork")
    parser.add_argument("--out", default="bench/data/longrun")
    args = parser.parse_args()

    amberfork_bin = Path(args.amberfork_bin).resolve()
    if not amberfork_bin.exists():
        print(f"error: {amberfork_bin} does not exist — build it first", file=sys.stderr)
        return 1
    out_root = Path(args.out)
    manifest_path = out_root / "manifest.json"
    out_root.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {"attempts": [], "excluded": []}

    def save() -> None:
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")

    tasks = sorted(t for t in TASKS if t not in PILOT_TASKS)
    sessions = [f"{n:02d}" for n in range(1, SESSIONS_PER_TASK + 1)]

    for task in tasks:
        task_dir = out_root / task
        task_dir.mkdir(exist_ok=True)
        for n in sessions:
            if not record_one(amberfork_bin, task_dir, f"ref_{n}", task, None, manifest["attempts"]):
                manifest["excluded"].append({"task": task, "session": f"ref_{n}"})
            save()

    for task in tasks:
        task_dir = out_root / task
        for n in sessions:
            reference = task_dir / f"ref_{n}.cassette.json"
            if not reference.exists():
                continue
            middle = registered_changes(json.loads(reference.read_text())).get("middle")
            if middle is None:
                manifest["excluded"].append({"task": task, "session": f"changed_{n}", "reason": "no middle change"})
            elif not record_one(amberfork_bin, task_dir, f"changed_{n}", task, middle["change"], manifest["attempts"]):
                manifest["excluded"].append({"task": task, "session": f"changed_{n}"})
            save()

    print(f"wrote {manifest_path}; {len(manifest['excluded'])} exclusion(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
