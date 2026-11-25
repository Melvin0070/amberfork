#!/usr/bin/env python3
"""Measure `amberfork diff` wall time and peak memory against run length (issue #50).

Notebooks 022/023 measured the aligner's time up to 1000 steps inside a criterion bench and
*projected* 5000 steps and all memory. This measures both, end to end, through the shipped
binary — file read, ingest, alignment, fork, field diffs, attribution, JSON out — because that is
what a user waits for.

Method:
- Runs are stitched from the committed seed-42 dev fixture exactly as
  `crates/amberfork-align/benches/align_scale.rs` does: each side's runs concatenated in filename
  order and cycled to the target length, re-indexed, parentage cleared. Real GAIA-derived step
  content (sanitized), so tokenization and payload cost are represented. Deterministic.
- Each measurement is one fresh `amberfork diff b.json --against a.json --json --max-steps N`
  process, timed with a monotonic clock around spawn-to-exit. Peak memory is that process's own
  `ru_maxrss` from `wait4`, so sizes cannot contaminate each other.
- One discarded warm-up per size (page cache, binary load), then REPS measured runs. The table
  reports the median time and the maximum peak RSS.

Usage:
    cargo build --release -p amberfork
    python3 spike/measure_scale.py --amberfork-bin target/release/amberfork
    python3 spike/measure_scale.py --render     # re-print the table from the committed document
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SIZES = [100, 500, 1000, 2000, 5000]
REPS = 3
FIXTURE = Path("bench/fixtures/chimera_noise_seed42_dev")
RESULT = Path("bench/results/scale_all.json")
# `ru_maxrss` is bytes on macOS and kilobytes on Linux.
RSS_UNIT = 1 if sys.platform == "darwin" else 1024


def side_steps(side: str) -> list[dict]:
    steps: list[dict] = []
    for path in sorted(FIXTURE.glob(f"{side}_*.json")):
        steps.extend(json.loads(path.read_text())["steps"])
    if not steps:
        raise SystemExit(f"error: no {side}_*.json runs in {FIXTURE}")
    return steps


def stitched(run_id: str, source: list[dict], n: int) -> dict:
    steps = []
    for idx in range(n):
        step = dict(source[idx % len(source)])
        step["idx"] = idx
        step.pop("parent_idx", None)
        steps.append(step)
    return {"schema_version": "0.1", "id": run_id, "task": "scale probe (issue #50)", "steps": steps}


def run_once(amberfork_bin: Path, bad: Path, good: Path, n: int) -> dict:
    started = time.perf_counter()
    proc = subprocess.Popen(
        [str(amberfork_bin), "diff", str(bad), "--against", str(good), "--json", "--max-steps", str(n)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    _, status, usage = os.wait4(proc.pid, 0)
    seconds = time.perf_counter() - started
    proc.returncode = os.waitstatus_to_exitcode(status)
    return {"seconds": seconds, "peak_rss_bytes": usage.ru_maxrss * RSS_UNIT, "exit": proc.returncode}


def machine() -> dict:
    def sh(*cmd: str) -> str:
        try:
            return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            return "unknown"

    cpu = sh("sysctl", "-n", "machdep.cpu.brand_string") if sys.platform == "darwin" else platform.processor()
    return {
        "os": f"{platform.system()} {platform.release()}",
        "arch": platform.machine(),
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "rustc": sh("rustc", "--version"),
        "commit": sh("git", "rev-parse", "--short", "HEAD"),
        "profile": "release",
    }


def human_time(seconds: float) -> str:
    if seconds < 1:
        return f"{seconds * 1000:.0f} ms"
    if seconds < 60:
        return f"{seconds:.2f} s"
    return f"{seconds / 60:.1f} min"


def render(doc: dict) -> str:
    lines = [
        "| steps per side | wall time (median) | vs previous | peak memory |",
        "|---|---|---|---|",
    ]
    previous = None
    for row in doc["sizes"]:
        ratio = "—"
        if previous is not None:
            growth = row["median_seconds"] / previous["median_seconds"]
            scale = (row["steps"] / previous["steps"]) ** 2
            ratio = f"{growth:.1f}× (n² predicts {scale:.1f}×)"
        lines.append(
            f"| {row['steps']} | {human_time(row['median_seconds'])} | {ratio} "
            f"| {row['peak_rss_bytes'] / 2**20:.0f} MB |"
        )
        previous = row
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--amberfork-bin", default="target/release/amberfork")
    parser.add_argument("--out", default=str(RESULT))
    parser.add_argument("--render", action="store_true", help="print the table from the committed document")
    args = parser.parse_args()

    if args.render:
        print(render(json.loads(Path(args.out).read_text())))
        return 0

    amberfork_bin = Path(args.amberfork_bin).resolve()
    if not amberfork_bin.exists():
        print(f"error: {amberfork_bin} does not exist — build it with --release first", file=sys.stderr)
        return 1

    a_source, b_source = side_steps("a"), side_steps("b")
    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        for n in SIZES:
            good, bad = Path(tmp) / f"a_{n}.json", Path(tmp) / f"b_{n}.json"
            good.write_text(json.dumps(stitched(f"scale-a-{n}", a_source, n)))
            bad.write_text(json.dumps(stitched(f"scale-b-{n}", b_source, n)))
            run_once(amberfork_bin, bad, good, n)  # warm-up, discarded
            runs = [run_once(amberfork_bin, bad, good, n) for _ in range(REPS)]
            row = {
                "steps": n,
                "trace_bytes": [good.stat().st_size, bad.stat().st_size],
                "median_seconds": statistics.median(r["seconds"] for r in runs),
                "min_seconds": min(r["seconds"] for r in runs),
                "max_seconds": max(r["seconds"] for r in runs),
                "peak_rss_bytes": max(r["peak_rss_bytes"] for r in runs),
                "exit_codes": sorted({r["exit"] for r in runs}),
                "runs": runs,
            }
            rows.append(row)
            print(f"  {n:5d} steps: {human_time(row['median_seconds'])}, "
                  f"{row['peak_rss_bytes'] / 2**20:.0f} MB peak, exit {row['exit_codes']}", flush=True)

    doc = {
        "protocol": "scale",
        "issue": 50,
        "method": {
            "command": "amberfork diff <b> --against <a> --json --max-steps <n>",
            "source": f"{FIXTURE}, sides stitched and cycled to length (as benches/align_scale.rs)",
            "warmup_runs": 1,
            "measured_runs": REPS,
            "time": "wall clock, spawn to exit, median of measured runs",
            "memory": "ru_maxrss of each process from wait4, maximum over measured runs",
        },
        "machine": machine(),
        "sizes": rows,
    }
    Path(args.out).write_text(json.dumps(doc, indent=2) + "\n")
    print(f"\nwrote {args.out}\n")
    print(render(doc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
