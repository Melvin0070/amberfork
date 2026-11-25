#!/usr/bin/env bash
# PostToolUse formatter. Formats one edited Rust file in place; silent no-op otherwise.
# rustfmt without --edition defaults to 2015 and cannot parse this edition-2024 codebase.
# Never fails the tool call: a formatter is a convenience, the gate is scripts/verify.sh.
set -u
f=${1:-}
case "$f" in
  *.rs) ;;
  *) exit 0 ;;
esac
[ -f "$f" ] || exit 0
command -v rustfmt >/dev/null 2>&1 || exit 0
rustfmt --edition 2024 "$f" >/dev/null 2>&1 || true
exit 0
