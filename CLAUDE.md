# amberfork

Local all-Rust tool that diffs two AI-agent run trajectories, finds the fork point, and
attributes the regression. 11 crates at v0.9.0. `ui/` is a SEPARATE workspace (`exclude`d).

## Commands

- Gate (every turn, auto via Stop hook): `scripts/verify.sh` — fmt, smoke, clippy, unit tests.
- Before committing: `scripts/verify.sh --full` — the exact CI ritual + the `ui/` workspace.
- Single test: `cargo test --workspace --lib --bins -- <name> --exact --nocapture`
- Clippy is the typecheck; there is no `cargo check` in CI.
- `amberfork-bench` resolves `bench/params.toml` from CWD — run it only from the repo root.

## Hard constraints

- Engine crates are sync + pure. `tokio`/`reqwest` stay quarantined to I/O edges (record, replay, server, ingest). Never introduce async into model/align/layout/attrib.
- `DiffResult` + trace-format is the versioned seam (`schema_version`). Never fork it per
  consumer; bump the version deliberately.
- `serde_json`'s `preserve_order` feature is load-bearing for byte-parity, not a convenience.
- Exit codes are diff(1)-style: `diff` 0=converged, 1=forked, 2=trouble. `demo` exits 1 on success; `record` propagates the *agent's* exit code instead.
- Commit straight to `main` — no branches, no PRs. `(#N)` in a message is a GitHub *issue* ref.
- One vertical slice at a time; WAIT for founder review before committing. See CONTRIBUTING.md.

## Gotchas that have already cost time

- `cargo test --workspace` does NOT cover `ui/`, and `ui.yml` runs on every PR. Green locally
  can still be a red CI. `scripts/verify.sh --full` covers both.
- `amberfork serve` ALWAYS fails in a dev checkout (`ui-dist/` is gitignored → `BundleMissing`,
  exit 2). A test asserts this. To see the UI: `cd ui && trunk serve`.
- Under the Claude Code Bash sandbox, binding 127.0.0.1 is denied, so 25 socket tests fail for
  reasons unrelated to the code. `scripts/verify.sh` detects this and says so.
- `python3` is a hard dep of `cargo test`: two CLI tests read `spike/fixtures/smoke/`.
- Render output depends on `NO_COLOR`/`TERM`/`COLORTERM`; insta snapshots pin the no-color form.
- Only 3 tests are `#[ignore]`d (all network, all in `amberfork-bench`), so a bad upstream pin
  looks green in CI. Validate pin changes by running them by hand.

## Settled — do not relitigate

- Fork rule = first non-sync BLOCK that never re-syncs (resync-k, k=2). "First non-sync move"
  measured 0.00. k=1 and k=3 both measured worse.
- Cost model is lexical/tf-idf. Embeddings already lost a fair test against it — do not add
  ONNX/`ort`. Bar to change: beat lexical on the dev fixtures.
- Benchmark numbers only via BENCHMARK.md's pre-registered protocol. The test split is sealed:
  scored once per release tag, never tuned on.

## Deeper docs

`docs/design/design-run-diff-debugger.md` (architecture, dated Amendments win) · `docs/notebook.md`
(append-only decisions/measurements) · `BENCHMARK.md` · `DESIGN.md` · `docs/trace-format.md` · `CONTRIBUTING.md`.
