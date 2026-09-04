#!/usr/bin/env python3
"""Scale verify_cli.rs's single-example real-provider test to n=15 (issue #48, notebook 079).

Reuses crates/amberfork/tests/fixtures/verify_agent.py verbatim. For each pair: record `good`
once, record `bad` with the same retry-until-forked rule verify_cli.rs uses (a mechanical
precondition — never retried or discarded based on what --verify later says), then run
`diff --verify --runs 3 --json` and capture the full attribution.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

N_PAIRS = 15
MAX_FORK_ATTEMPTS = 6
UPSTREAM = "http://127.0.0.1:11434"
BASE_URL_ENV = "AMBERFORK_VERIFY_BASE_URL"
AMBERFORK_BIN = "target/debug/amberfork"
AGENT_SCRIPT = "crates/amberfork/tests/fixtures/verify_agent.py"
OUT_ROOT = Path("bench/data/verify_realprovider")


def record(out_path: Path, session_id: str) -> None:
    subprocess.run(
        [
            AMBERFORK_BIN, "record",
            "--upstream", UPSTREAM,
            "--base-url-env", BASE_URL_ENV,
            "--out", str(out_path),
            "--id", session_id,
            "--", "python3", AGENT_SCRIPT,
        ],
        check=True, capture_output=True, text=True,
    )


def diff_json(bad: Path, good: Path, extra: list[str]) -> dict:
    result = subprocess.run(
        [AMBERFORK_BIN, "diff", str(bad), "--against", str(good), "--json", *extra],
        capture_output=True, text=True,
    )
    if not result.stdout.strip():
        raise RuntimeError(f"empty diff output (exit {result.returncode}): {result.stderr[-500:]}")
    return json.loads(result.stdout)


def build_pair(pair_dir: Path, index: int) -> dict | None:
    pair_dir.mkdir(parents=True, exist_ok=True)
    good = pair_dir / "good.cassette.json"
    bad = pair_dir / "bad.cassette.json"

    record(good, f"good_{index:02d}")

    forked = False
    for attempt in range(1, MAX_FORK_ATTEMPTS + 1):
        record(bad, f"bad_{index:02d}_a{attempt}")
        static = diff_json(bad, good, [])
        if static.get("fork") is not None:
            forked = True
            print(f"  pair {index:02d}: forked on attempt {attempt}")
            break
        print(f"  pair {index:02d}: attempt {attempt} converged, retrying")

    if not forked:
        print(f"  pair {index:02d}: EXCLUDED — never forked in {MAX_FORK_ATTEMPTS} attempts")
        return None

    verified = diff_json(bad, good, [
        "--verify", "--runs", "3",
        "--upstream", UPSTREAM,
        "--base-url-env", BASE_URL_ENV,
        "--", "python3", AGENT_SCRIPT,
    ])
    attribution = verified.get("attribution")
    (pair_dir / "verify_result.json").write_text(json.dumps(verified, indent=2))
    return attribution


def wilson95(k: int, n: int) -> tuple[float, float, float]:
    if n == 0:
        return (0.0, 0.0, 0.0)
    z = 1.96
    p = k / n
    denom = 1 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    half = (z * ((p * (1 - p) / n + z * z / (4 * n * n)) ** 0.5)) / denom
    return (p, max(0.0, center - half), min(1.0, center + half))


def main() -> int:
    OUT_ROOT.mkdir(parents=True, exist_ok=True)
    attributions: list[dict] = []
    excluded = 0

    for i in range(N_PAIRS):
        attr = build_pair(OUT_ROOT / f"pair_{i:02d}", i)
        if attr is None:
            excluded += 1
        else:
            attributions.append(attr)

    n = len(attributions)
    two_sided = [a for a in attributions if a.get("mode") == "counterfactual"]
    recovered = sum(1 for a in two_sided if a["counterfactual"]["recovered"] == "recovered")
    not_recovered = sum(1 for a in two_sided if a["counterfactual"]["recovered"] == "not_recovered")
    unverified = sum(1 for a in two_sided if a["counterfactual"]["recovered"] == "unverified")
    n_two_sided = len(two_sided)
    non_empty_propagation = sum(1 for a in two_sided if a.get("propagation"))

    results = {
        "protocol": "verify-realprovider",
        "notebook_registration": 79,
        "n_pairs_attempted": N_PAIRS,
        "n_pairs_excluded_never_forked": excluded,
        "n_pairs_scored": n,
        "two_sided_fork_rate": {
            "k": n_two_sided, "n": n,
            "wilson95": wilson95(n_two_sided, n) if n else None,
        },
        "tri_state_distribution_on_two_sided": {
            "n": n_two_sided,
            "recovered": {"k": recovered, "wilson95": wilson95(recovered, n_two_sided) if n_two_sided else None},
            "not_recovered": {"k": not_recovered, "wilson95": wilson95(not_recovered, n_two_sided) if n_two_sided else None},
            "unverified": {"k": unverified, "wilson95": wilson95(unverified, n_two_sided) if n_two_sided else None},
        },
        "ddmin_non_empty_propagation_rate_on_two_sided": {
            "k": non_empty_propagation, "n": n_two_sided,
            "wilson95": wilson95(non_empty_propagation, n_two_sided) if n_two_sided else None,
        },
        "raw_attributions": attributions,
    }

    out_file = Path("bench/results/verify_realprovider_all.json")
    out_file.write_text(json.dumps(results, indent=2) + "\n")
    print(f"\nwrote {out_file}")
    print(json.dumps({k: v for k, v in results.items() if k != "raw_attributions"}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
