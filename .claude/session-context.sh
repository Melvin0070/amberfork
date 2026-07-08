#!/usr/bin/env bash
# SessionStart hook — inject live project state into Claude's context every session.
# Every external call is guarded; this script must NEVER fail a session (exit 0 always).
set -uo pipefail

root="$(git rev-parse --show-toplevel 2>/dev/null || echo .)"
cd "$root" 2>/dev/null || true

branch="$(git branch --show-current 2>/dev/null || echo '?')"
head="$(git rev-parse --short HEAD 2>/dev/null || echo '?')"
status="$(git status --short 2>/dev/null | head -n 20)"
[ -z "$status" ] && status="(clean)"
recent="$(git log --oneline -3 2>/dev/null || echo '(no history)')"

# Open issues + milestone — gh may be offline/unauthenticated; degrade gracefully.
issues="$(gh issue list --state open --limit 20 \
  --json number,title,milestone \
  -q '.[] | "  #\(.number) [\(.milestone.title // "no milestone")] \(.title)"' \
  2>/dev/null)"
[ -z "$issues" ] && issues="  (gh unavailable or none — run: gh issue list)"

notebook="$(grep -E '^## ' docs/notebook.md 2>/dev/null | tail -n 1)"
[ -z "$notebook" ] && notebook="(no notebook entries)"

# Build the context with printf (no heredoc, no stray quotes) into one string.
ctx="$(
  printf '%s\n' "amberfork — session-start state (HEAD $head)"
  printf '%s\n' "Branch: $branch"
  printf '%s\n' "Working tree:" "$status"
  printf '%s\n' "Recent commits:" "$recent"
  printf '%s\n' "Open issues (the tracker — for the current milestone, take the lowest-numbered unblocked one):" "$issues"
  printf '%s\n' "Latest notebook entry: $notebook"
  printf '%s\n' ""
  printf '%s\n' "Working agreement (full version in CLAUDE.md Operating manual):"
  printf '%s\n' "- Verify before commit: python3 spike/test_smoke.py (+ cargo fmt/clippy/test once Cargo.toml exists). Red CI stops the line."
  printf '%s\n' "- Walking skeleton: keep amberfork diff working end-to-end; thicken vertical slices, no horizontal layers ahead of need."
  printf '%s\n' "- Every experiment gets a docs/notebook.md entry. Benchmark numbers follow the pre-registered protocol in BENCHMARK.md."
  printf '%s\n' "- Fork rule = first non-sync BLOCK that never re-syncs (resync-k). Cost model = lexical/tf-idf; embeddings must beat it on dev to earn a place."
)"

jq -n --arg ctx "$ctx" \
  '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $ctx}}' \
  2>/dev/null || printf '%s' '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"amberfork session start (context script degraded; see CLAUDE.md)"}}'
exit 0
