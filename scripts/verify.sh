#!/usr/bin/env bash
# amberfork verification gate. Exit 0 = clean, non-zero = blocked.
#
#   scripts/verify.sh          fast tier: fmt, smoke, clippy, unit tests   (~7s warm, ~25s worst)
#   scripts/verify.sh --full   the exact CI ritual + the ui/ workspace     (~90s worst)
#
# The global Stop hook shows only the LAST 50 LINES of this script's merged output to the
# agent. So: stages run cheapest-first, the script stops at the first failure, and each
# failure prints a short file:line-shaped report and nothing else. Only one report is ever
# emitted, which is what keeps it inside the tail budget.

set -uo pipefail

cd "$(dirname "$0")/.." || { printf 'verify: cannot reach repo root\n'; exit 1; }

FULL=0
case "${1:-}" in
  --full) FULL=1 ;;
  '')     ;;
  *)      printf 'usage: scripts/verify.sh [--full]\n'; exit 1 ;;
esac

# Not `mktemp -t`: on macOS that targets confstr's per-user tmp dir regardless of $TMPDIR,
# which the Claude Code Bash sandbox denies. This form honors $TMPDIR explicitly.
log=$(mktemp "${TMPDIR:-/tmp}/amberfork-verify.XXXXXX") || { printf 'verify: mktemp failed\n'; exit 1; }
trap 'rm -f "$log"' EXIT

fail() {
  printf '\n===== verify FAILED: %s =====\n' "$1"
  shift
  printf '%s\n' "$@"
  exit 1
}

# --- preflight: a missing prerequisite must never look like a pass -----------------
missing=''
need() { command -v "$1" >/dev/null 2>&1 || missing="$missing  - $1 ($2)"$'\n'; }
need cargo   'https://rustup.rs'
need python3 'required by spike/test_smoke.py and by two CLI integration tests'
need git     'used to locate the repo root'
cargo fmt --version    >/dev/null 2>&1 || missing="$missing  - rustfmt (rustup component add rustfmt)"$'\n'
cargo clippy --version >/dev/null 2>&1 || missing="$missing  - clippy (rustup component add clippy)"$'\n'
for f in Cargo.toml spike/test_smoke.py; do
  [ -f "$f" ] || missing="$missing  - $f missing — not an amberfork checkout?"$'\n'
done
[ -z "$missing" ] || fail 'missing prerequisites (gate could not run)' "$missing"

started=$(date +%s)

# --- stage 1: formatting (0.1s) ---------------------------------------------------
if ! cargo fmt --all --check >"$log" 2>&1; then
  fail 'cargo fmt --all --check' \
       'unformatted files:' \
       "$(grep -oE '^Diff in [^ ]+' "$log" | sed 's/^Diff in //' | sort -u | head -20)" \
       '' 'fix: cargo fmt --all'
fi

# --- stage 2: the offline spike smoke invariant (0.1s) ----------------------------
if ! python3 spike/test_smoke.py >"$log" 2>&1; then
  fail 'python3 spike/test_smoke.py' "$(tail -20 "$log")"
fi

# --- stage 3: clippy == the typecheck + lint, all targets (3-6s) ------------------
# --message-format=short collapses each diagnostic to one `file:line:col: error: ...`
# line, which is both actionable and bounded. CI runs the same check without short format.
if ! cargo clippy --all-targets --workspace --message-format=short -- -D warnings >"$log" 2>&1; then
  fail 'cargo clippy --all-targets --workspace -- -D warnings' \
       "$(grep -E ':[0-9]+:[0-9]+: (error|warning)' "$log" | head -25)" \
       "$(grep -E '^error(\[|:)' "$log" | grep -v ':[0-9]*:' | head -5)" \
       '' 'full context: cargo clippy --all-targets --workspace -- -D warnings'
fi

# --- stage 4: tests ---------------------------------------------------------------
# Loopback probe. Under the Claude Code Bash sandbox, binding 127.0.0.1 is denied, which
# fails 25 socket tests for a reason that has nothing to do with the code. Rather than
# report those as real failures, skip exactly the 3 that fall inside the fast tier and say
# so out loud. If a name here ever rots, the skip stops matching and the test runs and
# fails loudly — the safe direction.
skip=()
if ! python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); s.close()' 2>/dev/null; then
  skip=(--skip driver::tests::a_rerun_that_follows_the_good_path_is_recovered
        --skip driver::tests::a_rerun_that_still_diverges_is_not_recovered
        --skip verify::tests::verify_upgrades_a_recovering_fork_to_counterfactual)
  printf 'verify: DEGRADED — loopback bind denied here (sandbox); skipping 3 amberfork-attrib socket tests.\n'
fi

test_report() {
  fail "$1" \
       'failing tests:' \
       "$(sed -n '/^failures:$/,$p' "$log" | grep -E '^    [a-zA-Z_]' | sort -u | head -15)" \
       '' \
       "$(grep -E 'panicked at |assertion .* failed|^error: ' "$log" | head -12)" \
       '' \
       "reproduce: $1 -- <test_name> --exact --nocapture"
}

if [ "$FULL" = 0 ]; then
  # ${skip[@]+...} guard: bash 3.2 (macOS /bin/bash) errors on an empty array under set -u.
  if ! cargo test --workspace --lib --bins -q -- ${skip[@]+"${skip[@]}"} >"$log" 2>&1; then
    test_report 'cargo test --workspace --lib --bins'
  fi
  printf 'verify: OK — fmt, smoke, clippy(all-targets), unit tests (%ds)\n' "$(( $(date +%s) - started ))"
  printf 'verify: NOT covered by the fast tier — integration tests (chimera_parity, self_align,\n'
  printf '        all of amberfork-ingest, CLI e2e, insta snapshots) and the ui/ workspace.\n'
  printf '        Before committing: scripts/verify.sh --full\n'
  exit 0
fi

# --- --full: the exact CI ritual, plus ui/ ---------------------------------------
# Streams live: a human reads this one, and it is the pre-commit gate.
set -e
echo '=== engine workspace (ci.yml checks) ==='
cargo test --workspace
echo '=== ui workspace (ui.yml) ==='
if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
  printf 'verify: FAILED — wasm32-unknown-unknown not installed; the ui csr clippy pass cannot run.\n'
  printf '        fix: rustup target add wasm32-unknown-unknown\n'
  exit 1
fi
cd ui
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo clippy --no-default-features --features csr --target wasm32-unknown-unknown -- -D warnings
cargo test
echo "verify: OK --full ($(( $(date +%s) - started ))s)"
