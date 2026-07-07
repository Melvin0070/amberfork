#!/usr/bin/env python3
"""Throwaway spike: can DP alignment localize the decisive error step better than
positional first-mismatch? Consumes canonical trace JSON (docs/trace-format.md).

Arms (x tau grid):
  random                  seeded uniform over failing-run steps (floor)
  pos-structural          first index where (kind, name) differ
  pos-lexical             first index where lexical similarity < tau
  nw-structural/resync    affine-gap NW, cost 0/1 on (kind, name) equality
  nw-{lexical,tfidf,embed}/{first,resync}
                          affine-gap NW over content costs; fork rule:
                          first  = first non-sync move (naive)
                          resync = skip non-sync blocks that re-sync within k moves
                                   (benign blips: retries, rewordings); fork = first
                                   block the alignment never recovers from

Metrics: exact, within +/-1, within +/-3 vs gold step (failing-run index).
Not the benchmark. Directional only. See BENCHMARK.md pre-registered protocol.
"""

import argparse
import difflib
import json
import math
import random
import re
import sys
from collections import Counter
from pathlib import Path

TEXT_CAP = 600  # chars per step used for similarity (speed; long tool dumps dominate otherwise)
TOKEN_RE = re.compile(r"[a-z0-9]+")
INF = float("inf")


# ---------- loading ----------

def load_run(path):
    run = json.loads(Path(path).read_text())
    for s in run["steps"]:
        s["_text"] = step_text(s)
    return run


def step_text(step):
    out = step.get("outputs", "")
    if isinstance(out, (dict, list)):
        out = json.dumps(out, sort_keys=True)
    return f"{step.get('name', '')}: {out}"[:TEXT_CAP]


def norm_tokens(text):
    return TOKEN_RE.findall(text.lower())


# ---------- similarities ----------

def sim_structural(a, b):
    return 1.0 if (a.get("kind"), a.get("name")) == (b.get("kind"), b.get("name")) else 0.0


def sim_lexical(a, b):
    return difflib.SequenceMatcher(None, a["_text"], b["_text"]).ratio()


class TfIdf:
    """Corpus-level idf over all steps of all runs in the experiment; cached step vectors."""

    def __init__(self, runs):
        self.df = Counter()
        self.n_docs = 0
        for run in runs:
            for s in run["steps"]:
                self.n_docs += 1
                self.df.update(set(norm_tokens(s["_text"])))
        self._cache = {}

    def vec(self, step):
        key = id(step)
        if key not in self._cache:
            tf = Counter(norm_tokens(step["_text"]))
            v = {
                t: (1 + math.log(c)) * math.log((1 + self.n_docs) / (1 + self.df[t]))
                for t, c in tf.items()
            }
            norm = math.sqrt(sum(w * w for w in v.values())) or 1.0
            self._cache[key] = {t: w / norm for t, w in v.items()}
        return self._cache[key]

    def sim(self, a, b):
        va, vb = self.vec(a), self.vec(b)
        if len(vb) < len(va):
            va, vb = vb, va
        return sum(w * vb.get(t, 0.0) for t, w in va.items())


def try_embedder():
    """Optional arm: light static embeddings (no torch). Returns sim fn or None."""
    try:
        from model2vec import StaticModel  # type: ignore

        model = StaticModel.from_pretrained("minishlab/potion-base-8M")
        cache = {}

        def emb(step):
            key = id(step)
            if key not in cache:
                v = model.encode([step["_text"]])[0]
                n = float((v * v).sum()) ** 0.5 or 1.0
                cache[key] = v / n
            return cache[key]

        def sim(a, b):
            return float((emb(a) * emb(b)).sum())

        return sim
    except Exception:
        return None


def try_bge():
    """The embedding model the design doc actually specs (BGE-small-en-v1.5 via fastembed/ONNX).
    Returns sim fn or None."""
    try:
        from fastembed import TextEmbedding  # type: ignore

        model = TextEmbedding("BAAI/bge-small-en-v1.5")
        cache = {}

        def emb(step):
            k = id(step)
            if k not in cache:
                v = next(iter(model.embed([step["_text"]])))
                n = float(v @ v) ** 0.5 or 1.0
                cache[k] = v / n
            return cache[k]

        def sim(a, b):
            return float(emb(a) @ emb(b))

        return sim
    except Exception:
        return None


# ---------- affine-gap Needleman-Wunsch (minimizing cost) ----------

def _argmin3(m_val, ix_val, iy_val):
    """Ties prefer the diagonal (M) so identical runs align as pure sync."""
    best, name = m_val, "M"
    if ix_val < best:
        best, name = ix_val, "Ix"
    if iy_val < best:
        best, name = iy_val, "Iy"
    return name


def nw_affine(a_steps, b_steps, sim_fn, gap_open=0.6, gap_ext=0.3):
    """Global alignment, 3-state affine gaps. Returns ordered moves:
    ('sub', i, j, cost) | ('del', i) | ('ins', j).
    del = step only in A (failing run); ins = step only in B (reference)."""
    n, m = len(a_steps), len(b_steps)
    cost = [[1.0 - sim_fn(a_steps[i], b_steps[j]) for j in range(m)] for i in range(n)]

    M = [[INF] * (m + 1) for _ in range(n + 1)]
    Ix = [[INF] * (m + 1) for _ in range(n + 1)]  # gap in B: A step unmatched (del)
    Iy = [[INF] * (m + 1) for _ in range(n + 1)]  # gap in A: B step unmatched (ins)
    M[0][0] = 0.0
    for i in range(1, n + 1):
        Ix[i][0] = gap_open + (i - 1) * gap_ext
    for j in range(1, m + 1):
        Iy[0][j] = gap_open + (j - 1) * gap_ext

    for i in range(1, n + 1):
        row_c = cost[i - 1]
        for j in range(1, m + 1):
            M[i][j] = row_c[j - 1] + min(M[i - 1][j - 1], Ix[i - 1][j - 1], Iy[i - 1][j - 1])
            Ix[i][j] = min(M[i - 1][j] + gap_open, Ix[i - 1][j] + gap_ext)
            Iy[i][j] = min(M[i][j - 1] + gap_open, Iy[i][j - 1] + gap_ext)

    moves = []
    i, j = n, m
    state = _argmin3(M[i][j], Ix[i][j], Iy[i][j])
    while i > 0 or j > 0:
        if state == "M" and i > 0 and j > 0:
            moves.append(("sub", i - 1, j - 1, cost[i - 1][j - 1]))
            state = _argmin3(M[i - 1][j - 1], Ix[i - 1][j - 1], Iy[i - 1][j - 1])
            i, j = i - 1, j - 1
        elif state == "Ix" and i > 0:
            moves.append(("del", i - 1))
            state = "M" if M[i - 1][j] + gap_open <= Ix[i - 1][j] + gap_ext else "Ix"
            i -= 1
        elif state == "Iy" and j > 0:
            moves.append(("ins", j - 1))
            state = "M" if M[i][j - 1] + gap_open <= Iy[i][j - 1] + gap_ext else "Iy"
            j -= 1
        elif i > 0:  # safety: exhausted B
            moves.append(("del", i - 1))
            i -= 1
        else:  # safety: exhausted A
            moves.append(("ins", j - 1))
            j -= 1
    moves.reverse()
    return moves


def entries_from_moves(moves, n_a, tau):
    """Ordered (is_sync, a_pred_idx) per move; a_pred_idx = failing-run step it points at."""
    entries = []
    a_pos = 0
    for mv in moves:
        if mv[0] == "sub":
            entries.append((mv[3] <= tau, mv[1]))
            a_pos = mv[1] + 1
        elif mv[0] == "del":
            entries.append((False, mv[1]))
            a_pos = mv[1] + 1
        else:  # ins: reference-only step; failing run skipped something here
            entries.append((False, min(a_pos, max(n_a - 1, 0))))
    return entries


def fork_from_moves(moves, n_a, tau, rule="resync", resync_k=2):
    """Fork as a failing-run index, or None if converged (or all-blips under 'resync')."""
    entries = entries_from_moves(moves, n_a, tau)
    nonsync = [k for k, (s, _) in enumerate(entries) if not s]
    if not nonsync:
        return None
    if rule == "first":
        return entries[nonsync[0]][1]
    k, n = 0, len(entries)
    while k < n:
        if entries[k][0]:
            k += 1
            continue
        start = k
        while k < n and not entries[k][0]:
            k += 1
        syncs_after = 0
        j = k
        while j < n and entries[j][0]:
            syncs_after += 1
            j += 1
        if syncs_after >= resync_k:
            continue  # benign blip; alignment recovered
        return entries[start][1]
    return None


# ---------- baselines ----------

def positional_fork_from_diag(diag_costs, n_a, n_b, tau):
    for i, c in enumerate(diag_costs):
        if c > tau:
            return i
    if n_a != n_b:
        return min(len(diag_costs), n_a - 1)
    return None


# ---------- scoring ----------

def score(preds, golds):
    deltas, misses = [], 0
    for p, g in zip(preds, golds):
        if p is None:
            misses += 1
            deltas.append(10**9)
        else:
            deltas.append(abs(p - g))
    n = len(deltas)
    return {
        "exact": sum(d == 0 for d in deltas) / n,
        "w1": sum(d <= 1 for d in deltas) / n,
        "w3": sum(d <= 3 for d in deltas) / n,
        "no_pred": misses / n,
    }


def random_baseline(golds, lens, seed=7, iters=2000):
    rng = random.Random(seed)
    ex = w1 = w3 = 0
    for _ in range(iters):
        for g, ln in zip(golds, lens):
            d = abs(rng.randrange(ln) - g)
            ex += d == 0
            w1 += d <= 1
            w3 += d <= 3
    n = iters * len(golds)
    return {"exact": ex / n, "w1": w1 / n, "w3": w3 / n, "no_pred": 0.0}


# ---------- experiment ----------

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pairs", required=True,
                    help="dir of pair manifests: {failing, reference, gold_step} JSON")
    ap.add_argument("--out", default="spike/out")
    ap.add_argument("--taus", default="0.2,0.3,0.4,0.5,0.6,0.7")
    args = ap.parse_args()

    pairs_dir = Path(args.pairs)
    pair_files = sorted(p for p in pairs_dir.glob("pair_*.json"))
    if not pair_files:
        sys.exit(f"no pair_*.json manifests in {pairs_dir}")

    pairs = []
    for pf in pair_files:
        man = json.loads(pf.read_text())

        def resolve(rel):
            p = Path(rel)
            return p if p.is_absolute() else pairs_dir / p

        pairs.append({
            "name": pf.stem,
            "a": load_run(resolve(man["failing"])),
            "b": load_run(resolve(man["reference"])),
            "gold": man["gold_step"],
        })

    # sanity: self-alignment must be all-sync (fork None)
    p0 = pairs[0]
    self_moves = nw_affine(p0["a"]["steps"], p0["a"]["steps"], sim_lexical)
    assert fork_from_moves(self_moves, len(p0["a"]["steps"]), 0.5) is None, "self-align failed"

    tfidf = TfIdf([p["a"] for p in pairs] + [p["b"] for p in pairs])
    emb_sim = try_embedder()

    sims = {"structural": sim_structural, "lexical": sim_lexical, "tfidf": tfidf.sim}
    if emb_sim:
        sims["embed"] = emb_sim
    bge_sim = try_bge()
    if bge_sim:
        sims["bge"] = bge_sim

    golds = [p["gold"] for p in pairs]
    lens = [len(p["a"]["steps"]) for p in pairs]
    taus = [float(t) for t in args.taus.split(",")]

    # precompute per (pair, sim): alignment moves once + positional diagonal costs once
    print(f"aligning {len(pairs)} pairs x {list(sims)} ...", file=sys.stderr)
    cache = {}
    for sim_name, fn in sims.items():
        for p in pairs:
            a, b = p["a"]["steps"], p["b"]["steps"]
            moves = nw_affine(a, b, fn)
            diag = [1.0 - fn(a[i], b[i]) for i in range(min(len(a), len(b)))]
            cache[(sim_name, p["name"])] = (moves, diag, len(a), len(b))

    results = {"n_pairs": len(pairs), "arms": {}}
    results["arms"]["random"] = {"tau=n/a": random_baseline(golds, lens)}

    def run_arm(arm, pred_fn):
        results["arms"][arm] = {}
        for tau in taus:
            preds = [pred_fn(p, tau) for p in pairs]
            results["arms"][arm][f"tau={tau}"] = score(preds, golds)

    for sim_name in sims:
        def pos(p, tau, s=sim_name):
            moves, diag, n_a, n_b = cache[(s, p["name"])]
            return positional_fork_from_diag(diag, n_a, n_b, tau)

        def nw_first(p, tau, s=sim_name):
            moves, diag, n_a, n_b = cache[(s, p["name"])]
            return fork_from_moves(moves, n_a, tau, rule="first")

        def nw_resync(p, tau, s=sim_name):
            moves, diag, n_a, n_b = cache[(s, p["name"])]
            return fork_from_moves(moves, n_a, tau, rule="resync")

        if sim_name in ("structural", "lexical"):
            run_arm(f"pos-{sim_name}", pos)
        if sim_name == "structural":
            run_arm("nw-structural/resync", nw_resync)
        else:
            run_arm(f"nw-{sim_name}/first", nw_first)
            run_arm(f"nw-{sim_name}/resync", nw_resync)

    outdir = Path(args.out)
    outdir.mkdir(parents=True, exist_ok=True)
    (outdir / "results.json").write_text(json.dumps(results, indent=2))

    lines = ["| arm | tau | exact | ±1 | ±3 | no-pred |", "|---|---|---|---|---|---|"]
    for arm, by_tau in results["arms"].items():
        for tau, r in by_tau.items():
            lines.append(f"| {arm} | {tau.split('=')[1]} | {r['exact']:.2f} "
                         f"| {r['w1']:.2f} | {r['w3']:.2f} | {r['no_pred']:.2f} |")
    report = "\n".join(lines)
    (outdir / "results.md").write_text(report + "\n")
    print(f"n_pairs={len(pairs)}  arms={list(results['arms'])}\n")
    print(report)


if __name__ == "__main__":
    main()
