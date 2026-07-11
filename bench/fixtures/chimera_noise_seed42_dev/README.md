# chimera_noise dev-split parity fixtures (canonical provenance doc)

The **dev-split** subsets of the n=20 benign-noise chimera sets, one directory per RNG seed:

| directory | dev pairs | pinned exact baseline |
|---|---|---|
| `chimera_noise_seed42_dev/` | 8 | 6/8 |
| `chimera_noise_seed43_dev/` | 7 | 2/7 |
| `chimera_noise_seed44_dev/` | 10 | 6/10 |

`crates/amberfork-align/tests/chimera_parity.rs` reads all three and asserts each seed's own
baseline in CI (25 dev pairs total). This directory is the canonical provenance doc for the whole
family; the sibling dirs carry a short pointer back here. Committed per the issue-#11 decision
(2026-07-10); seeds 43/44 added under notebook 014.

**Why three seeds, and the honest number.** Exact-step localization at the frozen τ=0.3 is
*seed-sensitive* (notebook 014): seed 42 is a favorable draw (0.75), seed 43 hard (0.29), seed 44
middling (0.60) — aggregate **14/25 ≈ 0.56 exact**. The *stable* signal is the window: **±3 = 0.95
across all three seeds (n=60)**. The published claim leads with that window, not seed 42's exact;
pinning per-seed baselines here means the gate cannot rest on one lucky draw.

The **test split is deliberately not here.** A committed test set invites tuning-on-test
(BENCHMARK.md protocol rule 2). The test side is regenerated under the frozen protocol once per
release tag; only dev material lives in the repo.

## What a pair is

Each `pair_NN.json` names a `failing`/`reference` run and the `gold_step` (the known fork):

- `reference` = a real Who&When failure log **X**, unmodified.
- `failing`   = **X**.steps[0:g] + **Y**.steps[g:] — a *chimera* splicing a different real log Y
  onto X's prefix, with benign noise (one duplicated "retry" step + token-dropout rewording) on
  the shared prefix to simulate the non-determinism that breaks positional alignment.
- `gold_step` = g, the injected fork (shifted +1 per retry insertion landing before it).

The controlled, known fork is what makes this a localization-*mechanics* fixture. It does **not**
claim "first divergence ≈ decisive error" on natural failures (see BENCHMARK.md threats-to-validity).

## Provenance & licensing

- **Source:** Who&When failure logs — `ag2ai/Agents_Failure_Attribution`, **MIT**, sourced from
  GitHub (never the HF mirror, which declares no license). See `../../../LICENSE` and BENCHMARK.md
  "Data & licensing".
- **GAIA lineage:** Who&When questions/answers originate in **GAIA**, gated upstream with a
  "no crawlable resharing" clause. Per BENCHMARK.md's conservative rule, every GAIA question and
  ground-truth answer is **redacted** from these files before commit — in step content, not just
  the `task` field.

### Sanitization (two-stage; re-runnable)

`amberfork-bench sanitize` (`crates/amberfork-bench/src/sanitize.rs`; issue #17, the Rust port
of the original `spike/sanitize_gaia.py` with byte parity proven in notebook 019) performs
deterministic, one-way, alignment-preserving redaction: question text → `q<sha8><i>`
placeholders, answers → `a<sha8><i>`, `task` → a hashed marker. It edits only `[A-Za-z0-9]`
spans, so every whitespace character is preserved and a `--seed 42` regeneration reproduces
bit-identical pair structure. Its invariants (space-count preservation, no residue,
determinism, idempotence, the cross-log sweep) and a byte-exact round trip over every file in
these directories run inside `cargo test --workspace`.

- **canonical stage** (pre-`make_pairs`): redact each log against its own Q&A, so placeholders
  bake into the prefix before `reword()` adds noise. Sanitizing *after* generation would redact
  the failing side's noised prefix and the reference's clean prefix differently and tank the
  number (measured 0.75 → 0.55; notebook 013).
- **pairs stage** (post-`make_pairs`): sweep each pair against *both* source logs' Q&A. A chimera
  mixes X and Y, so X's question phrasing can reappear through Y's real tail content — a cross-log
  leak the canonical stage structurally cannot see. Running on canonical-sanitized pairs, the
  sweep only touches post-fork tail residue, so the number holds.

Residual (recorded, not a defect): no ≥4-token run of any question survives, but ~86% of a
question's individual content *words* still appear scattered (never as a reconstructable phrase).
The issue-#11 decision accepts this for a dev-only, provenance-noted fixture; it is why the pairs
carry hashed placeholders rather than natural text.

### Reproduce / audit from scratch

```
# 1. fetch Who&When from GitHub (MIT), convert to canonical trace JSON
python3 spike/convert_whowhen.py --src <Agents_Failure_Attribution checkout>
# 2. GAIA-sanitize the canonical logs (stage 1)
cargo run -p amberfork-bench -- sanitize canonical \
    --src spike/data/canonical --out spike/data/canonical_sanitized
# 3. generate the noise pairs from sanitized logs (repeat per seed: 42, 43, 44)
python3 spike/make_pairs.py --canonical spike/data/canonical_sanitized --seed 42 \
    --out-noise spike/data/pairs_noise --out-clean spike/data/pairs_clean
# 4. cross-log sweep (stage 2)
cargo run -p amberfork-bench -- sanitize pairs --pairs spike/data/pairs_noise \
    --canonical spike/data/canonical --out spike/data/pairs_noise
# 5. the dev-split subset of each seed is the matching chimera_noise_seed<N>_dev/ dir,
#    byte-identical. Seed 42 dev = pairs 03,06,09,10,14,15,16,18.
```

The dev/test split is `amberfork-bench`'s stable FNV-1a hash of the reference run id (dev iff
bucket < 30 of 100); `amberfork-bench run --pairs <seed dir> --split dev` lists each seed's set.
