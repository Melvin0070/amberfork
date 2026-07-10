#!/usr/bin/env python3
"""CI test: GAIA sanitizer invariants on synthetic data. Offline, <5s.

Proves the properties the committed-fixture provenance relies on (notebook 013), using only
fabricated questions/answers — never benchmark-derived data (notebook 001/T30):

  1. space-count preservation per step  (make_pairs reword() reproducibility)
  2. no surviving question >=4-gram and no boundary-matched answer
  3. determinism   (same input -> same output)
  4. idempotence   (redacting twice == redacting once)
  5. cross-log sweep: one log's question phrasing appearing in another's tail is removed
     (the leak class that forced the two-stage design)
"""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
from sanitize_gaia import (  # noqa: E402
    DEFAULT_NGRAM,
    question_ngrams,
    redact_answer,
    redact_question_spans,
    word_tokens,
)

N = DEFAULT_NGRAM
QUESTION = "what is the total penguin population on dream island at the end of 2012"
ANSWER = "42000"
STEP = (
    "the agent searched for the total penguin population on dream island\n"
    "and reported 42000 as the final count for the year"
)


def surviving_ngrams(text):
    norm = [t for _, _, t in word_tokens(text)]
    return {tuple(norm[i : i + N]) for i in range(len(norm) - N + 1)} & question_ngrams(QUESTION, N)


def answer_survives(text):
    return re.search(r"(?<![A-Za-z0-9])" + re.escape(ANSWER) + r"(?![A-Za-z0-9])", text) is not None


def sanitize_step(text):
    out = redact_question_spans(text, question_ngrams(QUESTION, N), N, "deadbeef")
    return redact_answer(out, ANSWER, "cafebabe")


def test_space_count_and_residue():
    red = sanitize_step(STEP)
    assert STEP.count(" ") == red.count(" "), (STEP.count(" "), red.count(" "))
    assert not surviving_ngrams(red), f"question {N}-gram survived: {surviving_ngrams(red)}"
    assert not answer_survives(red), "answer survived redaction"


def test_determinism():
    assert sanitize_step(STEP) == sanitize_step(STEP), "sanitizer is non-deterministic"


def test_idempotence():
    once = sanitize_step(STEP)
    twice = sanitize_step(once)
    assert once == twice, "re-sanitizing is not a no-op"


def _run(run_id, question, steps):
    return {
        "schema_version": "0.1",
        "id": run_id,
        "task": question,
        "outcome": "fail",
        "steps": [{"idx": i, "kind": "agent", "name": "a", "outputs": s} for i, s in enumerate(steps)],
    }


def test_cross_log_sweep_end_to_end():
    """A chimera whose tail (from log Y) quotes log X's question must come out clean when swept
    against BOTH source questions — the exact leak canonical-only sanitization cannot see."""
    qx = "count the penguins on dream island in the attached file"
    qy = "list the highest grossing movies released in the year 2020"
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        canon = root / "canonical"
        canon.mkdir()
        (canon / "x.json").write_text(json.dumps(_run("x", qx, ["intro"])))
        (canon / "y.json").write_text(json.dumps(_run("y", qy, ["intro"])))
        (canon / "index.json").write_text(
            json.dumps(
                [
                    {"file": "x.json", "ground_truth": "7"},
                    {"file": "y.json", "ground_truth": "tenet"},
                ]
            )
        )
        pairs = root / "pairs"
        pairs.mkdir()
        # failing = X prefix + Y tail, and Y's tail happens to quote X's whole question.
        a = _run("a", qx, ["intro step", f"agent restated: {qx}"])
        b = _run("b", qx, ["intro step"])
        (pairs / "a_00.json").write_text(json.dumps(a))
        (pairs / "b_00.json").write_text(json.dumps(b))
        (pairs / "pair_00.json").write_text(
            json.dumps(
                {
                    "failing": "a_00.json",
                    "reference": "b_00.json",
                    "gold_step": 1,
                    "meta": {"x": "x.json", "y": "y.json"},
                }
            )
        )
        out = root / "out"
        subprocess.run(
            [
                sys.executable,
                str(HERE / "sanitize_gaia.py"),
                "pairs",
                "--pairs",
                str(pairs),
                "--canonical",
                str(canon),
                "--out",
                str(out),
            ],
            check=True,
            capture_output=True,
        )
        body = " ".join(
            s["outputs"]
            for f in ("a_00.json", "b_00.json")
            for s in json.loads((out / f).read_text())["steps"]
        )
        norm = [t for _, _, t in word_tokens(body)]
        present = {tuple(norm[i : i + N]) for i in range(len(norm) - N + 1)}
        assert not (present & question_ngrams(qx, N)), "X's question leaked through Y's tail"


def main():
    test_space_count_and_residue()
    test_determinism()
    test_idempotence()
    test_cross_log_sweep_end_to_end()
    print("sanitize OK: space-count, no-residue, determinism, idempotence, cross-log sweep")


if __name__ == "__main__":
    main()
