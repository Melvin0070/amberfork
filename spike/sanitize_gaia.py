#!/usr/bin/env python3
"""GAIA-sanitize Who&When-derived fixtures for redistribution (issue #11, BENCHMARK.md rule).

The logs are MIT (ag2ai/Agents_Failure_Attribution, sourced from GitHub), but their questions
and ground-truth answers originate in gated GAIA ("no crawlable resharing" — notebook 001/T30).
This redacts BOTH, covering STEP CONTENT and not just the `task` field: agents restate the
question — whole and in fragments (search queries) — and occasionally the answer, throughout
their steps.

Two modes, because a committed *pair* is where the constraint actually bites and where naive
sanitization silently fails (notebook 013):

  canonical  Redact each run against its OWN question+answer, BEFORE pair generation. Must run
             pre-`make_pairs` so placeholders bake into the prefix before reword() adds noise;
             sanitizing after generation redacts A's noised prefix and B's clean prefix
             differently, breaking alignment symmetry and tanking the number (measured 0.75->0.55).

  pairs      Sweep generated pairs against BOTH source questions+answers (from each manifest's
             meta.x/meta.y). A chimera splices log X's prefix onto log Y's tail, so X's question
             phrasing can reappear via Y's real content — a cross-log leak canonical mode cannot
             see. Run on canonical-sanitized pairs: the prefix is already clean (re-redaction is
             a no-op there), so the sweep only touches post-fork tail residue and the number holds.

Redaction scheme (deterministic, one-way, alignment/reproducibility-preserving):
  - question -> any run of >= NGRAM consecutive question tokens, wherever it appears, becomes
                per-question placeholders  q<sha8><i>
  - answer   -> token-boundary, case-insensitive match (and each line) -> placeholders a<sha8><i>
  - `task`   -> "[GAIA question redacted; sha256:<hash8>]"
  - index ground_truth (canonical mode) -> "sha256:<hash8>"

Invariants enforced by verify():
  - per-step ' ' count is unchanged (only [A-Za-z0-9] spans are edited), so a --seed 42
    regeneration off canonical-sanitized logs reproduces bit-identical pair STRUCTURE — the
    controlled before/after the #11 decision rests on;
  - no surviving question NGRAM and no boundary-matched answer against every relevant Q&A.

NGRAM=4: runs of >=4 consecutive question tokens are GAIA-specific composition; 1-3 token
phrases are generic English / proper nouns that don't reconstruct a gated question (notebook 013
records the residual: no >=4-gram survives, but ~86% of question content *words* still appear
individually scattered — a licensing judgment recorded in the #11 decision, not a bug here).
Answers below MIN_ANSWER_LEN (bare digits) aren't searched in step text (boundary-matching "3"
shreds CSV cells); they are still hashed in task/index regardless of length.

Usage:
  python3 spike/sanitize_gaia.py canonical --src spike/data/canonical --out spike/data/canonical_sanitized
  python3 spike/sanitize_gaia.py pairs --pairs <dir> --canonical spike/data/canonical --out <dir>
"""

import argparse
import hashlib
import json
import re
from pathlib import Path

MIN_ANSWER_LEN = 3
DEFAULT_NGRAM = 4
# One tokenizer everywhere: maximal alphanumeric runs. Whitespace (incl. newlines) and
# punctuation are pure separators, so tokenization is identical however a step wraps the text,
# and redacting only these spans leaves every space character (and its count) untouched.
_WORD = re.compile(r"[A-Za-z0-9]+")


def hash8(text):
    return hashlib.sha256(text.encode()).hexdigest()[:8]


def word_tokens(text):
    """List of (start, end, lowercased) for each alphanumeric run."""
    return [(m.start(), m.end(), m.group(0).lower()) for m in _WORD.finditer(text)]


def question_ngrams(question, n):
    """Set of n-length tuples of lowercased question word tokens."""
    norm = [t for _, _, t in word_tokens(question)]
    return {tuple(norm[i : i + n]) for i in range(len(norm) - n + 1)}


def redact_question_spans(text, ngrams, n, qh):
    """Replace every word token lying on a matched question n-gram, editing only word-character
    spans so all whitespace (and its count) is preserved exactly."""
    if not ngrams:
        return text
    toks = word_tokens(text)
    norm = [t for _, _, t in toks]
    covered = [False] * len(norm)
    for j in range(len(norm) - n + 1):
        if tuple(norm[j : j + n]) in ngrams:
            for k in range(j, j + n):
                covered[k] = True
    if not any(covered):
        return text
    out, cursor = [], 0
    for j, (start, end, _) in enumerate(toks):
        if covered[j]:
            out.append(text[cursor:start])
            out.append(f"q{qh}{j:x}")
            cursor = end
    out.append(text[cursor:])
    return "".join(out)


def redact_answer(text, answer, ah):
    """Case-insensitive, token-boundary replacement of an answer and each of its lines."""
    for unit in [answer] + (answer.splitlines() if "\n" in answer else []):
        unit = unit.strip()
        if len(unit) < MIN_ANSWER_LEN:
            continue
        pattern = re.compile(
            r"(?<![A-Za-z0-9])" + re.escape(unit) + r"(?![A-Za-z0-9])", re.IGNORECASE
        )
        text = pattern.sub(
            lambda m: " ".join(f"a{ah}{i:x}" for i in range(m.group(0).count(" ") + 1)),
            text,
        )
    return text


def redact_steps(run, specs, n):
    """Apply every (ngrams, qh, answer, ah) spec to each step's outputs."""
    for step in run["steps"]:
        out = str(step.get("outputs", ""))
        for ngrams, qh, answer, ah in specs:
            out = redact_answer(redact_question_spans(out, ngrams, n, qh), answer, ah)
        step["outputs"] = out


def verify(original_steps, sanitized_run, specs, n):
    """Post-conditions across every spec: per-step space counts unchanged; no surviving
    question n-gram; no boundary-matched answer (when long enough to be searched)."""
    problems = []
    for orig, san in zip(original_steps, sanitized_run["steps"]):
        o, s = str(orig.get("outputs", "")), str(san.get("outputs", ""))
        if o.count(" ") != s.count(" "):
            problems.append(f"step {orig.get('idx')}: space count {o.count(' ')} -> {s.count(' ')}")
        norm = [t for _, _, t in word_tokens(s)]
        present = {tuple(norm[i : i + n]) for i in range(len(norm) - n + 1)}
        for ngrams, _, answer, _ in specs:
            survivors = present & ngrams
            if survivors:
                problems.append(f"step {orig.get('idx')}: question {n}-gram survives {next(iter(survivors))}")
            ans = answer.strip()
            if len(ans) >= MIN_ANSWER_LEN and re.search(
                r"(?<![A-Za-z0-9])" + re.escape(ans) + r"(?![A-Za-z0-9])", s, re.IGNORECASE
            ):
                problems.append(f"step {orig.get('idx')}: answer survives")
    return problems


def run_canonical(args):
    src, outdir, n = Path(args.src), Path(args.out), args.ngram
    outdir.mkdir(parents=True, exist_ok=True)
    index = json.loads((src / "index.json").read_text())

    new_index, failures = [], []
    for meta in index:
        original = json.loads((src / meta["file"]).read_text())
        run = json.loads((src / meta["file"]).read_text())
        question, answer = original.get("task", ""), str(meta.get("ground_truth", "") or "")
        qh, ah = hash8(question), hash8(answer.lower())
        specs = [(question_ngrams(question, n), qh, answer, ah)]

        redact_steps(run, specs, n)
        failures += [f"{meta['file']}: {p}" for p in verify(original["steps"], run, specs, n)]
        run["task"] = f"[GAIA question redacted; sha256:{qh}]"

        (outdir / meta["file"]).write_text(json.dumps(run, indent=1))
        new_meta = dict(meta)
        new_meta["ground_truth"] = f"sha256:{ah}"
        new_index.append(new_meta)

    (outdir / "index.json").write_text(json.dumps(new_index, indent=1))
    _finish(f"sanitized {len(new_index)} canonical logs -> {outdir} (ngram={n})", failures)


def run_pairs(args):
    pairs_in, canon, outdir, n = Path(args.pairs), Path(args.canonical), Path(args.out), args.ngram
    outdir.mkdir(parents=True, exist_ok=True)
    index = {m["file"]: m for m in json.loads((canon / "index.json").read_text())}

    def spec_for(canonical_file):
        question = json.loads((canon / canonical_file).read_text()).get("task", "")
        answer = str(index[canonical_file].get("ground_truth", "") or "")
        return question_ngrams(question, n), hash8(question), answer, hash8(answer.lower())

    manifests = sorted(pairs_in.glob("pair_*.json"))
    failures = []
    for manifest_path in manifests:
        manifest = json.loads(manifest_path.read_text())
        specs = [spec_for(manifest["meta"]["x"]), spec_for(manifest["meta"]["y"])]
        for side in ("failing", "reference"):
            original = json.loads((pairs_in / manifest[side]).read_text())
            run = json.loads((pairs_in / manifest[side]).read_text())
            redact_steps(run, specs, n)
            failures += [f"{manifest[side]}: {p}" for p in verify(original["steps"], run, specs, n)]
            run["task"] = f"[GAIA question redacted; sha256:{specs[0][1]}]"
            (outdir / manifest[side]).write_text(json.dumps(run, indent=1))
        (outdir / manifest_path.name).write_text(manifest_path.read_text())
    _finish(f"swept {len(manifests)} pairs -> {outdir} (ngram={n})", failures)


def _finish(summary, failures):
    print(summary)
    if failures:
        raise SystemExit("VERIFY FAILED:\n" + "\n".join(failures[:20]))
    print("verify: space counts preserved; no surviving question n-gram or answer residue")


def main():
    ap = argparse.ArgumentParser(description="GAIA-sanitize Who&When-derived fixtures.")
    ap.add_argument("--ngram", type=int, default=DEFAULT_NGRAM)
    sub = ap.add_subparsers(dest="mode", required=True)

    c = sub.add_parser("canonical", help="redact each run against its own Q&A (pre-make_pairs)")
    c.add_argument("--src", default="spike/data/canonical")
    c.add_argument("--out", default="spike/data/canonical_sanitized")

    p = sub.add_parser("pairs", help="sweep generated pairs against both source logs' Q&A")
    p.add_argument("--pairs", required=True)
    p.add_argument("--canonical", default="spike/data/canonical")
    p.add_argument("--out", required=True)

    args = ap.parse_args()
    (run_canonical if args.mode == "canonical" else run_pairs)(args)


if __name__ == "__main__":
    main()
