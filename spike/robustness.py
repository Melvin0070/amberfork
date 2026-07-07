#!/usr/bin/env python3
"""Spike 002: robustness of the fork-rule + cost-model decisions (notebook 002).

Sweeps seeds x noise levels on chimera pairs; reports each arm's BEST-tau (oracle) result —
these are method ceilings, honestly labeled as such. Adds BGE-small via fastembed (the
embedding model the design doc actually specs) on the base noise level.
"""

import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from align_spike import (  # noqa: E402
    TfIdf,
    fork_from_moves,
    nw_affine,
    positional_fork_from_diag,
    score,
    sim_lexical,
    step_text,
    try_bge,
)
from make_pairs import GOLD_MARGIN, MAX_LEN, MIN_LEN, make_pair  # noqa: E402

TAUS = [0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6]
SEEDS = [42, 43, 44]
NOISE = {"low": (0.2, 1), "base": (0.4, 1), "high": (0.6, 2)}  # (reword_p, retries)
N = 20
CANON = Path("spike/data/canonical")


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
            s["_text"] = step_text(s)  # recompute: prefix was reworded after deepcopy
        pairs.append({"a": a, "b": runs[mx["file"]], "gold": gold})
    return pairs


def best_tau(fork_of_tau, golds):
    best = None
    for tau in TAUS:
        r = score(fork_of_tau(tau), golds)
        if best is None or (r["exact"], r["w1"]) > (best[1]["exact"], best[1]["w1"]):
            best = (tau, r)
    return {"tau": best[0], **best[1]}


def main():
    index = json.loads((CANON / "index.json").read_text())
    eligible = [m for m in index if m["split"] == "hand" and MIN_LEN <= m["n_steps"] <= MAX_LEN]
    runs = {m["file"]: json.loads((CANON / m["file"]).read_text()) for m in eligible}
    for r in runs.values():
        for s in r["steps"]:
            s["_text"] = step_text(s)
    bge = try_bge()
    print(f"eligible logs: {len(eligible)}; bge arm: {'ON' if bge else 'OFF'}", file=sys.stderr)

    out = []
    for level, (rp, rt) in NOISE.items():
        for seed in SEEDS:
            pairs = build_pairs(runs, eligible, seed, rp, rt)
            golds = [p["gold"] for p in pairs]
            tfidf = TfIdf([p["a"] for p in pairs] + [p["b"] for p in pairs])
            pre = []
            for p in pairs:
                a, b = p["a"]["steps"], p["b"]["steps"]
                entry = {
                    "n_a": len(a), "n_b": len(b),
                    "lex_moves": nw_affine(a, b, sim_lexical),
                    "lex_diag": [1.0 - sim_lexical(a[i], b[i])
                                 for i in range(min(len(a), len(b)))],
                    "tfidf_moves": nw_affine(a, b, tfidf.sim),
                }
                if bge and level == "base":
                    entry["bge_moves"] = nw_affine(a, b, bge)
                pre.append(entry)

            arms = {}
            arms["pos-lexical"] = best_tau(
                lambda t: [positional_fork_from_diag(e["lex_diag"], e["n_a"], e["n_b"], t)
                           for e in pre], golds)
            arms["nw-lexical/first"] = best_tau(
                lambda t: [fork_from_moves(e["lex_moves"], e["n_a"], t, rule="first")
                           for e in pre], golds)
            for k in (1, 2, 3):
                arms[f"nw-lexical/resync-k{k}"] = best_tau(
                    lambda t, k=k: [fork_from_moves(e["lex_moves"], e["n_a"], t, resync_k=k)
                                    for e in pre], golds)
            arms["nw-tfidf/resync-k2"] = best_tau(
                lambda t: [fork_from_moves(e["tfidf_moves"], e["n_a"], t) for e in pre], golds)
            if bge and level == "base":
                arms["nw-bge/first"] = best_tau(
                    lambda t: [fork_from_moves(e["bge_moves"], e["n_a"], t, rule="first")
                               for e in pre], golds)
                arms["nw-bge/resync-k2"] = best_tau(
                    lambda t: [fork_from_moves(e["bge_moves"], e["n_a"], t) for e in pre], golds)
            out.append({"level": level, "seed": seed, "n": len(pairs), "arms": arms})
            print(f"done {level}/seed{seed} (n={len(pairs)})", file=sys.stderr)

    agg = {}
    for row in out:
        for arm, r in row["arms"].items():
            agg.setdefault((row["level"], arm), []).append(r)
    lines = ["| noise | arm | exact (mean/min/max over seeds) | ±1 mean | best-τ set |",
             "|---|---|---|---|---|"]
    for (level, arm), rs in sorted(agg.items()):
        ex = [r["exact"] for r in rs]
        w1 = sum(r["w1"] for r in rs) / len(rs)
        taus = sorted({r["tau"] for r in rs})
        lines.append(f"| {level} | {arm} | {sum(ex)/len(ex):.2f} ({min(ex):.2f}–{max(ex):.2f}) "
                     f"| {w1:.2f} | {taus} |")
    outdir = Path("spike/out/robustness")
    outdir.mkdir(parents=True, exist_ok=True)
    (outdir / "results.json").write_text(json.dumps(out, indent=1))
    report = "\n".join(lines)
    (outdir / "results.md").write_text(report + "\n")
    print(report)


if __name__ == "__main__":
    main()
