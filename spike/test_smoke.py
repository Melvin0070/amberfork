#!/usr/bin/env python3
"""CI smoke test: aligner sanity on the committed synthetic pair. Offline, <10s.

The fixture encodes the two pathologies the aligner exists for: a benign retry step
(structural offset) and light rewording in the shared prefix. Gold fork = failing-run
step 6 (web.fetch to a blog instead of census.gov)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from align_spike import (  # noqa: E402
    fork_from_moves,
    load_run,
    nw_affine,
    positional_fork_from_diag,
    sim_lexical,
)

FIX = Path(__file__).parent / "fixtures" / "smoke"


def main():
    a = load_run(FIX / "run_a.json")
    b = load_run(FIX / "run_b.json")
    a_steps, b_steps = a["steps"], b["steps"]

    # 1) self-alignment invariant: a run against itself has no fork
    self_moves = nw_affine(a_steps, a_steps, sim_lexical)
    assert fork_from_moves(self_moves, len(a_steps), tau=0.5) is None, "self-align must converge"

    # 2) alignment + resync rule localizes the gold fork through the benign noise
    moves = nw_affine(a_steps, b_steps, sim_lexical)
    fork = fork_from_moves(moves, len(a_steps), tau=0.3, rule="resync")
    assert fork == 6, f"expected fork at 6, got {fork}"

    # 3) control: positional first-mismatch is thrown off by the retry offset
    diag = [1.0 - sim_lexical(a_steps[i], b_steps[i])
            for i in range(min(len(a_steps), len(b_steps)))]
    pos = positional_fork_from_diag(diag, len(a_steps), len(b_steps), tau=0.3)
    assert pos != 6, f"positional found the true fork ({pos}); fixture no longer discriminates"

    print("smoke OK: self-align converged; resync fork = 6; positional misled as expected")


if __name__ == "__main__":
    main()
