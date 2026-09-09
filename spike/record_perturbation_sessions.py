#!/usr/bin/env python3
"""Orchestrator for #49's real-agent perturbation protocol (notebook 074).

For each task: record ONE clean reference and FIVE independently-sampled perturbed sessions,
via the real, unmodified `amberfork record` binary wrapping `perturbation_agent.py` against a
real local Ollama server. Retries (cap 6, notebook 074's retry rule) fire ONLY when the designated
tool was never invoked or the session ran long — never because of what the aligner would later say.
A task that exhausts its retry budget is recorded as an exclusion, not silently dropped or
backfilled.

Output layout (bench/data/perturbation/, gitignored — see notebook 074, no sanitization needed
since none of this is GAIA-derived):
    <task_id>/reference.cassette.json + reference.summary.json
    <task_id>/perturbed_NN.cassette.json + perturbed_NN.summary.json
    manifest.json   — what `amberfork-bench build-perturbation-pairs` reads

Usage: python3 spike/record_perturbation_sessions.py --amberfork-bin target/debug/amberfork
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from perturbation_world import TASKS

N_PERTURBED_PER_TASK = 5
# Total clean sessions per task, `reference` included (notebook 082): the negative class
# is every unordered clean-vs-clean pair, so 5 sessions yield C(5,2)=10 pairs per task.
# #49 only ever needed one clean run; raising this is additive and leaves `reference`
# exactly where its pipeline expects it.
N_CLEAN_PER_TASK = 5
MAX_RETRIES = 6
UPSTREAM = "http://127.0.0.1:11434"
BASE_URL_ENV = "AMBERFORK_BASE_URL"
AGENT_SCRIPT = (Path(__file__).parent / "perturbation_agent.py").resolve()


def record_one(amberfork_bin: Path, out_dir: Path, session_id: str, task_id: str, perturb: bool) -> dict | None:
    """Try up to MAX_RETRIES times; return the summary dict on success, None on exhaustion."""
    cassette_path = out_dir / f"{session_id}.cassette.json"
    summary_path = out_dir / f"{session_id}.summary.json"
    last_reason = "unknown"

    for attempt in range(1, MAX_RETRIES + 1):
        agent_cmd = [
            "python3",
            str(AGENT_SCRIPT),
            "--task",
            task_id,
            "--summary-out",
            str(summary_path),
        ]
        if perturb:
            agent_cmd.append("--perturb")

        record_cmd = [
            str(amberfork_bin),
            "record",
            "--upstream",
            UPSTREAM,
            "--base-url-env",
            BASE_URL_ENV,
            "--out",
            str(cassette_path),
            "--id",
            session_id,
            "--",
            *agent_cmd,
        ]
        result = subprocess.run(record_cmd, capture_output=True, text=True)

        if result.returncode == 0 and summary_path.exists():
            summary = json.loads(summary_path.read_text())
            if summary.get("exit_code") == 0:
                print(f"  [{session_id}] attempt {attempt}: ok (gold_step={summary.get('gold_step')})")
                return summary
            last_reason = summary.get("reason", "non-zero exit with no reason")
        else:
            last_reason = (result.stderr or result.stdout or "record failed with no output").strip()[-300:]

        print(f"  [{session_id}] attempt {attempt}: retrying — {last_reason}")

    print(f"  [{session_id}] EXCLUDED after {MAX_RETRIES} attempts — {last_reason}")
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--amberfork-bin", default="target/debug/amberfork")
    parser.add_argument("--out", default="bench/data/perturbation")
    parser.add_argument("--tasks", nargs="*", default=sorted(TASKS), choices=sorted(TASKS))
    parser.add_argument(
        "--only-clean",
        action="store_true",
        help="record ONLY the extra clean sessions 082 needs, leaving `reference` and the "
        "perturbed sessions on disk untouched. Use this on an existing "
        "bench/data/perturbation: a full re-record would replace the raw recordings #49's "
        "published table was computed from (BENCHMARK.md rule 10).",
    )
    args = parser.parse_args()

    amberfork_bin = Path(args.amberfork_bin).resolve()
    if not amberfork_bin.exists():
        print(f"error: {amberfork_bin} does not exist — build it first", file=sys.stderr)
        return 1

    out_root = Path(args.out)
    manifest: dict = {"tasks": {}}

    for task_id in args.tasks:
        print(f"=== {task_id} ===")
        task_dir = out_root / task_id
        task_dir.mkdir(parents=True, exist_ok=True)
        entry: dict = {"reference": None, "clean": [], "perturbed": [], "excluded": []}

        ref_cassette = task_dir / "reference.cassette.json"
        if args.only_clean and ref_cassette.exists():
            print("  reference: already on disk, kept (--only-clean)")
            entry["reference"] = "reference"
            entry["clean"].append("reference")
        else:
            ref_summary = record_one(amberfork_bin, task_dir, "reference", task_id, perturb=False)
            if ref_summary is None:
                entry["excluded"].append("reference")
            else:
                entry["reference"] = "reference"
                entry["clean"].append("reference")

        # The extra clean sessions (082's negative class). `reference` is the first of the
        # N_CLEAN_PER_TASK, so only the remainder are recorded here.
        for i in range(1, N_CLEAN_PER_TASK):
            session_id = f"clean_{i:02d}"
            if args.only_clean and (task_dir / f"{session_id}.cassette.json").exists():
                print(f"  {session_id}: already on disk, kept (--only-clean)")
                entry["clean"].append(session_id)
                continue
            summary = record_one(amberfork_bin, task_dir, session_id, task_id, perturb=False)
            if summary is None:
                entry["excluded"].append(session_id)
            else:
                entry["clean"].append(session_id)

        for i in range(N_PERTURBED_PER_TASK):
            session_id = f"perturbed_{i:02d}"
            if args.only_clean:
                if (task_dir / f"{session_id}.cassette.json").exists():
                    entry["perturbed"].append(session_id)
                continue
            summary = record_one(amberfork_bin, task_dir, session_id, task_id, perturb=True)
            if summary is None:
                entry["excluded"].append(session_id)
            else:
                entry["perturbed"].append(session_id)

        manifest["tasks"][task_id] = entry

    manifest_path = out_root / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2))
    print(f"\nmanifest written to {manifest_path}")

    total_perturbed = sum(len(e["perturbed"]) for e in manifest["tasks"].values())
    total_clean = sum(len(e["clean"]) for e in manifest["tasks"].values())
    total_excluded = sum(len(e["excluded"]) for e in manifest["tasks"].values())
    neg_pairs = sum(
        len(e["clean"]) * (len(e["clean"]) - 1) // 2 for e in manifest["tasks"].values()
    )
    print(f"total usable pairs: {total_perturbed}  ·  total exclusions: {total_excluded}")
    print(f"clean sessions: {total_clean}  ·  clean-vs-clean pairs (082 negative class): {neg_pairs}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
