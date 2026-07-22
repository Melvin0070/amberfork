# heldout_react_v1 — generalization probe fixture (issue #42)

A **held-out** set for one question the chimera parity gate cannot answer: do the frozen fork
params (τ=0.3, resync_k=2, gaps 0.6/0.3 — calibrated only on the Who&When-derived chimera family,
notebook 001/007) still localize a fork on a trajectory of a **different structural shape**,
with *no re-tuning*?

The chimera fixtures are all one shape: multi-agent Magentic-One / AutoGen orchestration logs
(every step `kind: agent`, `Orchestrator (thought)` / `WebSurfer`). This set is the opposite
shape — a **single-agent ReAct tool loop** (`llm` think → `tool` act/observe, closing on an
`llm` answer). Read by `crates/amberfork-align/tests/heldout_generalization.rs`.

## ⚠️ Honest caveat: this fixture is synthetic

These six trajectories are **hand-authored**, not real third-party logs. That makes this a
*weaker* form of evidence than the chimera set's real (if injected) Who&When logs: because we
wrote them, we cannot fully rule out having made the fork easy to find. It is a **shape**
generalization probe, not a natural-data one. The strong version — a genuinely external
different-framework set — arrives with the OpenInference/OTel adapter over TRAIL (#39 → #41).
This set is the cheap, honest first datapoint that also proves out the probe mechanism.

## First honest result (frozen params, run once — notebook 041)

| metric | ReAct held-out (n=6) |
|---|---|
| exact | 5/6 |
| ±1 | 5/6 |
| ±3 | **6/6** |

`pair_06` (moon→eiffel) localizes the fork two steps early (predicted 2, gold 4) — the one
near-miss, reported not hidden. `pair_03` fires a *model-only* fork (the fork move has no `b`
side); `fork_step_observed`'s fallback resolves it to the gold step. The test pins ±3 at the
observed 6/6 and reports exact; a regression out of ±3 is a red CI that forces a notebook entry,
never a silent parameter tweak (issue #42 acceptance).

## What a pair is

Each `pair_NN.json` names a `failing`/`reference` run and the `gold_step` (the known fork):

- `reference` (`b_NN`) = base ReAct run **X**, clean and unmodified (`outcome: pass`).
- `failing` (`a_NN`) = `X[0:s]` + one duplicated `(retry)` step (a token-dropout-reworded copy of
  step `r-1`) inserted at prefix position `r` + `Y[s:]`, a different task's suffix (`outcome: fail`).
- `gold_step` = `s + 1` — the fork index **in the failing run**, shifted by the one retry
  insertion that sits before the splice (`r < s`). This is the index `fork_step_observed()`
  returns, matching the `chimera_parity` convention `diff(&reference, &failing)`.

The injection is mechanical and the same benign-noise idea as the chimera set (one duplicated
retry step + light token-dropout rewording), so "gold fork" is honestly defined by construction,
not by inspecting whether the aligner happens to find it.

## Provenance & regeneration

Fully synthetic — **no upstream data, no licensing constraint** (contrast the chimera set's
GAIA-redaction requirement). Six base tasks: `eiffel`, `interest`, `austen`, `currency`, `tokyo`,
`moon`, each a 9-step ReAct trajectory. Pairs `(X, Y, splice s, retry r)`:

| pair | X → Y | s | r | gold |
|---|---|---|---|---|
| 01 | eiffel → interest | 4 | 2 | 5 |
| 02 | interest → austen | 5 | 3 | 6 |
| 03 | austen → currency | 3 | 1 | 4 |
| 04 | currency → tokyo | 4 | 2 | 5 |
| 05 | tokyo → moon | 5 | 1 | 6 |
| 06 | moon → eiffel | 3 | 2 | 4 |

The base runs are recoverable from the `b_NN` reference files; the splice/noise rule above
reproduces every `a_NN` deterministically. The one-shot generator that emitted these files is a
throwaway (per the `spike/`-is-disposable rule); the committed JSON + this recipe are the
source of truth.
