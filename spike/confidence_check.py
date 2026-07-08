#!/usr/bin/env python3
"""Spike 003/004 addendum (notebook 005): is fork *confidence* informative?

The Rust engine stamps every fork with confidence = (evidence - tau) / (1 - tau), where
evidence is the fork move's sync cost or, for gap moves, its distance-to-closest-counterpart.
The formula is designed-not-validated: nothing yet shows high-confidence forks are more often
CORRECT. Before any UI renders it as a trust meter, measure exactly that.

Replicates the Rust pipeline in the spike harness (token-level gestalt sim, tau=0.3, k=2 —
parity verified in notebook 004) over the robustness protocol's 9 cells (seeds 42/43/44 x
noise low/base/high, N=20), then asks: do hits carry higher confidence than misses?
"""

import difflib
import random
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from align_spike import nw_affine, step_text  # noqa: E402
from make_pairs import GOLD_MARGIN, MAX_LEN, MIN_LEN, make_pair  # noqa: E402

import json  # noqa: E402

TAU = 0.3
RESYNC_K = 2
SEEDS = [42, 43, 44]
NOISE = {"low": (0.2, 1), "base": (0.4, 1), "high": (0.6, 2)}
N = 20
CANON = Path(__file__).parent / "data" / "canonical"
TOKEN_RE = re.compile(r"[a-z0-9]+")


def toks(step):
    if "_toks" not in step:
        step["_toks"] = TOKEN_RE.findall(step["_text"].lower())
    return step["_toks"]


def sim_token(a, b):
    return difflib.SequenceMatcher(None, toks(a), toks(b), autojunk=False).ratio()


def cost(a, b):
    return 1.0 - sim_token(a, b)


def fork_entry(moves, n_a):
    """The spike resync walk, but returning the fork's (a_pred, move) instead of the index."""
    entries = []
    a_pos = 0
    for mv in moves:
        if mv[0] == "sub":
            entries.append((mv[3] <= TAU, mv[1], mv))
            a_pos = mv[1] + 1
        elif mv[0] == "del":
            entries.append((False, mv[1], mv))
            a_pos = mv[1] + 1
        else:  # ins
            entries.append((False, min(a_pos, max(n_a - 1, 0)), mv))
    i, n = 0, len(entries)
    while i < n:
        if entries[i][0]:
            i += 1
            continue
        start = i
        while i < n and not entries[i][0]:
            i += 1
        j, syncs = i, 0
        while j < n and entries[j][0]:
            syncs += 1
            j += 1
        if syncs >= RESYNC_K:
            continue
        return entries[start][1], entries[start][2]
    return None


def fork_confidence(mv, a_steps, b_steps):
    """Mirror of amberfork-align::fork::divergence_confidence + the gap-move confidence."""
    if mv[0] == "sub":
        evidence = mv[3]
    elif mv[0] == "del":  # failing-only step: distance to closest reference step
        evidence = min((cost(a_steps[mv[1]], bs) for bs in b_steps), default=1.0)
    else:  # ins: reference-only step: distance to closest failing step
        evidence = min((cost(b_steps[mv[1]], a)) for a in a_steps) if a_steps else 1.0
    return max(0.0, min(1.0, (evidence - TAU) / (1.0 - TAU)))


def build_pairs(runs, eligible, seed, reword_p, retries):
    rng = random.Random(seed)
    pairs, guard = [], 0
    while len(pairs) < N and guard < N * 30:
        guard += 1
        mx, my = rng.sample(eligible, 2)
        if min(mx["n_steps"], my["n_steps"]) < 3 + GOLD_MARGIN + 1:
            continue
        a, gold = make_pair(runs[mx["file"]], runs[my["file"]], rng, noise=True,
                            reword_p=reword_p, retries=retries)
        for s in a["steps"]:
            s["_text"] = step_text(s)
            s.pop("_toks", None)
        pairs.append({"a": a, "b": runs[mx["file"]], "gold": gold})
    return pairs


def main():
    index = json.loads((CANON / "index.json").read_text())
    eligible = [m for m in index if m["split"] == "hand" and MIN_LEN <= m["n_steps"] <= MAX_LEN]
    runs = {m["file"]: json.loads((CANON / m["file"]).read_text()) for m in eligible}
    for r in runs.values():
        for s in r["steps"]:
            s["_text"] = step_text(s)

    samples = []  # (confidence, hit) for every pair that produced a fork
    no_pred = 0
    for level, (rp, rt) in NOISE.items():
        for seed in SEEDS:
            for p in build_pairs(runs, eligible, seed, rp, rt):
                a_steps, b_steps = p["a"]["steps"], p["b"]["steps"]
                entry = fork_entry(nw_affine(a_steps, b_steps, sim_token), len(a_steps))
                if entry is None:
                    no_pred += 1
                    continue
                pred, mv = entry
                samples.append((fork_confidence(mv, a_steps, b_steps), pred == p["gold"]))
            print(f"done {level}/seed{seed}", file=sys.stderr)

    n = len(samples)
    hits = [c for c, h in samples if h]
    misses = [c for c, h in samples if not h]
    mean = lambda xs: sum(xs) / len(xs) if xs else float("nan")

    # point-biserial r == pearson(confidence, hit)
    mc, mh = mean([c for c, _ in samples]), len(hits) / n
    cov = sum((c - mc) * (h - mh) for c, h in samples) / n
    sc = (sum((c - mc) ** 2 for c, _ in samples) / n) ** 0.5
    sh = (mh * (1 - mh)) ** 0.5
    r = cov / (sc * sh) if sc > 0 and sh > 0 else float("nan")

    by_conf = sorted(samples)
    terciles = [by_conf[i * n // 3:(i + 1) * n // 3] for i in range(3)]

    print(f"\nn={n} forked pairs (+{no_pred} no-fork), overall exact {len(hits)}/{n} = {mh:.2f}")
    print(f"mean confidence: hits {mean(hits):.3f}  misses {mean(misses):.3f}")
    print(f"point-biserial r(confidence, hit) = {r:.3f}")
    for name, t in zip(("low-conf", "mid-conf", "high-conf"), terciles):
        t_hits = sum(h for _, h in t)
        lo, hi = t[0][0], t[-1][0]
        print(f"{name} tercile [{lo:.2f}..{hi:.2f}]: {t_hits}/{len(t)} = {t_hits / len(t):.2f}")


if __name__ == "__main__":
    main()
