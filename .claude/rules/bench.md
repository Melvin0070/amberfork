---
paths:
  - "bench/**"
  - "crates/amberfork-bench/**"
  - "crates/amberfork-align/**"
  - "BENCHMARK.md"
  - "spike/**"
---

# Benchmark and measurement discipline

`BENCHMARK.md` is the pre-registered protocol. Never publish a number produced outside it.
Every experiment gets an append-only `docs/notebook.md` entry — no exceptions.

## Byte-parity rules that silently invalidate published results

- `bench/params.toml` is hashed by its exact BYTES. Editing a comment or the inline changelog
  is a new config revision and invalidates every published table's identity. Two independent
  tests pin `DiffParams::default()` against that file.
- Sealed result documents must not be rewritten — not even to bump a version string.
  `results.rs` accepts `bench_schema_version` ∈ {0.5, 0.6} precisely so the v0.2.0 test docs
  keep their original bytes. CI rebuilds the committed aggregate and byte-compares.
- `serde_json`'s `preserve_order` is load-bearing workspace-wide; removing it breaks the
  fixtures' byte-identical-regeneration promise.
- `pyjson.rs` must stay byte-compatible with CPython `json.dumps(obj, indent=1)`.

## Order-dependent recipes

- Fixture regeneration: `convert_whowhen` → `amberfork-bench sanitize canonical` →
  `make_pairs` → `amberfork-bench sanitize pairs`, seed 42. Sanitize canonical BEFORE
  `make_pairs`; doing it after breaks alignment symmetry and moves the number 0.75 → 0.55.
- `amberfork-bench` resolves `bench/params.toml` from CWD with no fallback — run it only from
  the repo root, or it exits 2.
- `NON_GAIA_FIXTURES` is an opt-OUT allowlist: any new dir under `bench/fixtures/` is subject
  to the GAIA redaction check unless named there.

## Honesty rules

- The test split is sealed: scored once per release tag, never tuned on, never committed. If a
  test run motivates a change, tune on dev and report the new number alongside the old.
- Exact-step localization is seed-sensitive by design; the gate pins each seed's own number
  (42→6/8, 43→2/7, 44→6/10). Seed 43's 0.29 is the honest draw, not a bug.
- Mode A′ is an honest null at n=4 (engine 0.50 vs random 0.75). Do not "fix" that table.
- `blind_cost_model_*` tests exist to prove the fixtures still discriminate. Keep them
  meaningful when touching fixtures.
- Licensing: never source Who&When or TRAIL from their gated HF mirrors (GitHub/MIT only);
  HAL zips are benchmark-use-only, do not redistribute; GAIA Q&A must be redacted from
  anything committed. `bench/data/` and `spike/data/` are gitignored on purpose.
- Only 3 tests are `#[ignore]`d, all network, all here — so a bad upstream pin looks green in
  CI. Validate pin changes by running them by hand.
