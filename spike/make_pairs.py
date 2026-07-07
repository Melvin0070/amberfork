#!/usr/bin/env python3
"""Build controlled failing<->reference pairs from REAL Who&When trajectories.

Why chimeras: Who&When ships NO passing runs (verified 2026-07-07 — notebook 001), so true
Mode-A pairs are not constructible from published data. For the alignment-MECHANICS question
we build pairs with a controlled, known fork from real content:

  reference B = real log X, unmodified
  failing   A = X.steps[0:g] + Y.steps[g:]   (Y = different real log; sustained real divergence)
  gold      = g   (shifted +1 if the retry-noise insertion lands before g)

Benign-noise condition (simulates benign non-determinism, the thing that breaks positional
alignment): one duplicated "retry" step in A's shared prefix + token-dropout rewording of a
fraction of prefix steps. A clean condition (no noise) is emitted as the control.

This tests localization mechanics on real material. It does NOT validate "first divergence
~= decisive error" on natural failures — that needs real pass/fail pairs (see notebook 001).
"""

import argparse
import copy
import json
import random
from pathlib import Path

MIN_LEN, MAX_LEN = 12, 60
P_REWORD = 0.4     # fraction of prefix steps that get reworded (noise condition)
P_DROP = 0.12      # token dropout rate within a reworded step
GOLD_MARGIN = 4    # keep the fork away from both ends


def reword(text, rng):
    toks = text.split(" ")
    if len(toks) < 8:
        return text
    kept = [t for t in toks if rng.random() > P_DROP]
    return " ".join(kept) if kept else text


def reindex(steps):
    for i, s in enumerate(steps):
        s["idx"] = i
    return steps


def make_pair(x_run, y_run, rng, noise):
    x, y = x_run["steps"], y_run["steps"]
    hi = min(len(x), len(y)) - GOLD_MARGIN
    g = rng.randrange(3, hi)
    a_steps = copy.deepcopy(x[:g]) + copy.deepcopy(y[g:])
    gold = g

    if noise:
        # rewording: benign content jitter on the shared prefix
        for s in a_steps[:g]:
            if rng.random() < P_REWORD:
                s["outputs"] = reword(str(s.get("outputs", "")), rng)
        # retry: duplicate one prefix step (structural offset)
        r = rng.randrange(1, g)
        dup = copy.deepcopy(a_steps[r])
        dup["outputs"] = "(retry) " + reword(str(dup.get("outputs", "")), rng)
        a_steps.insert(r + 1, dup)
        gold += 1  # insertion is always before the fork

    a = {
        "schema_version": "0.1",
        "id": f"{x_run['id']}__chimera__{y_run['id']}",
        "task": x_run.get("task", ""),
        "outcome": "fail",
        "steps": reindex(a_steps),
    }
    return a, gold


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--canonical", default="spike/data/canonical")
    ap.add_argument("--out-noise", default="spike/data/pairs_noise")
    ap.add_argument("--out-clean", default="spike/data/pairs_clean")
    ap.add_argument("--split", default="hand", help="which Who&When split to draw from")
    ap.add_argument("--n", type=int, default=20)
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    canon = Path(args.canonical)
    index = json.loads((canon / "index.json").read_text())
    eligible = [m for m in index
                if m["split"] == args.split and MIN_LEN <= m["n_steps"] <= MAX_LEN]
    if len(eligible) < 4:
        raise SystemExit(f"only {len(eligible)} eligible logs in split '{args.split}'")

    rng = random.Random(args.seed)
    runs = {m["file"]: json.loads((canon / m["file"]).read_text()) for m in eligible}

    for outdir_name, noise in ((args.out_noise, True), (args.out_clean, False)):
        outdir = Path(outdir_name)
        outdir.mkdir(parents=True, exist_ok=True)
        pair_rng = random.Random(args.seed)  # same X/Y/g draws in both conditions
        made = 0
        attempts = 0
        while made < args.n and attempts < args.n * 20:
            attempts += 1
            mx, my = pair_rng.sample(eligible, 2)
            x_run, y_run = runs[mx["file"]], runs[my["file"]]
            if min(mx["n_steps"], my["n_steps"]) < 3 + GOLD_MARGIN + 1:
                continue
            a, gold = make_pair(x_run, y_run, pair_rng, noise)
            b = runs[mx["file"]]
            (outdir / f"a_{made:02d}.json").write_text(json.dumps(a, indent=1))
            (outdir / f"b_{made:02d}.json").write_text(json.dumps(b, indent=1))
            (outdir / f"pair_{made:02d}.json").write_text(json.dumps({
                "failing": f"a_{made:02d}.json",
                "reference": f"b_{made:02d}.json",
                "gold_step": gold,
                "meta": {"x": mx["file"], "y": my["file"], "noise": noise},
            }, indent=1))
            made += 1
        print(f"{outdir}: wrote {made} pairs (noise={noise})")


if __name__ == "__main__":
    main()
