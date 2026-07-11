# Run amberfork on your own agent

The README's `demo` shows a bundled pair. This guide is the real path: get two runs of
*your* agent into the trace format, diff them, and read the answer. Everything here was
exercised against genuinely messy production traces before it was written down
(`docs/notebook.md` 020).

## 1 · Install

Pick one:

```sh
# a) prebuilt binary (macOS arm64 / Linux x86_64) — no toolchain needed
#    download amberfork-<version>-<target>.tar.gz from the releases page,
#    verify the .sha256, untar, put `amberfork` on your PATH
#    https://github.com/Melvin0070/amberfork/releases

# b) via cargo
cargo install amberfork

# c) from source
git clone https://github.com/Melvin0070/amberfork && cd amberfork
cargo run --release -q -p amberfork -- demo
```

## 2 · Get two runs into the trace format

amberfork reads one plain-JSON file per run — a run is an ordered list of steps. The full
contract is [`trace-format.md`](trace-format.md); the minimal shape is:

```json
{
  "schema_version": "0.1",
  "id": "checkout-agent_2026-07-11_bad",
  "outcome": "fail",
  "steps": [
    { "idx": 0, "kind": "llm",  "name": "planner",      "outputs": { "content": "I'll fetch the cart first." } },
    { "idx": 1, "kind": "tool", "name": "cart.fetch",   "inputs": { "cart_id": "c-118" }, "outputs": { "status": "ok" } }
  ]
}
```

Rules of thumb: `kind` is one of `llm` / `tool` / `agent` / `other`; every step needs a
`name` (the aligner keys on it) and at least one of `inputs`/`outputs`; unknown fields are
kept and surfaced as warnings, never fatal. You need **two** files: the failing run and a
known-good reference of the same task.

### A worked, real example: Claude Code sessions

Any agent framework's log converts with a few dozen lines of script. Here is the one we
use on [Claude Code](https://claude.com/claude-code) session transcripts
(`~/.claude/projects/<project>/<session-id>.jsonl`) — real multi-megabyte agent
trajectories. The mapping: each assistant text block → an `llm` step, each `tool_use` →
a `tool` step whose `tool_result` is paired back by id, each user message → `other`.

```python
#!/usr/bin/env python3
"""Claude Code session transcript (.jsonl) -> amberfork canonical trace v0.1."""

import json
import sys


def block_text(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for c in content:
            if isinstance(c, dict) and c.get("type") == "text":
                parts.append(c.get("text", ""))
            elif isinstance(c, str):
                parts.append(c)
        return "\n".join(parts)
    return json.dumps(content)


def convert(path, run_id, outcome):
    steps = []
    tool_step_by_id = {}  # tool_use_id -> index into steps, awaiting its result

    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            if rec.get("isSidechain"):  # subagent traffic — skip for a linear trace
                continue
            rtype = rec.get("type")
            msg = rec.get("message") or {}
            content = msg.get("content")
            ts = rec.get("timestamp")

            if rtype == "assistant" and isinstance(content, list):
                for block in content:
                    btype = block.get("type")
                    if btype == "text":
                        steps.append({
                            "idx": len(steps),
                            "kind": "llm",
                            "name": msg.get("model") or "assistant",
                            "outputs": {"content": block.get("text", "")},
                            "t_start": ts,
                        })
                    elif btype == "tool_use":
                        tool_step_by_id[block.get("id")] = len(steps)
                        steps.append({
                            "idx": len(steps),
                            "kind": "tool",
                            "name": block.get("name", "tool"),
                            "inputs": block.get("input", {}),
                            "t_start": ts,
                        })
            elif rtype == "user" and isinstance(content, list):
                for block in content:
                    if block.get("type") == "tool_result":
                        i = tool_step_by_id.pop(block.get("tool_use_id"), None)
                        if i is not None:
                            steps[i]["outputs"] = {"content": block_text(block.get("content"))}
                    elif block.get("type") == "text":
                        steps.append({
                            "idx": len(steps),
                            "kind": "other",
                            "name": "user",
                            "outputs": {"content": block.get("text", "")},
                            "t_start": ts,
                        })
            elif rtype == "user" and isinstance(content, str):
                steps.append({
                    "idx": len(steps),
                    "kind": "other",
                    "name": "user",
                    "outputs": {"content": content},
                    "t_start": ts,
                })

    return {
        "schema_version": "0.1",
        "id": run_id,
        "task": "claude-code session",
        "outcome": outcome,
        "steps": steps,
    }


if __name__ == "__main__":
    src, dst, run_id, outcome = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
    with open(dst, "w", encoding="utf-8") as f:
        json.dump(convert(src, run_id, outcome), f)
```

```sh
python3 cc2trace.py ~/.claude/projects/<project>/<good-session>.jsonl good.json my-agent_good pass
python3 cc2trace.py ~/.claude/projects/<project>/<bad-session>.jsonl  bad.json  my-agent_bad  fail
```

When we ran exactly this on two of our own sessions (133 and 123 steps of large tool
payloads, embedded escape codes, unicode), the diff completed in 0.2 s and put the fork at
the first genuinely divergent step with confidence 0.66 — and a run diffed against itself
converged, as it must (notebook 020).

## 3 · Diff and read the fork

```sh
amberfork diff bad.json --against good.json
```

Output (here from the bundled demo pair — `amberfork demo` prints it on your machine):

```text
  A  refund-good  ·  reference · 10 steps · pass
  B  refund-bad   ·  observed  · 10 steps · fail

  step 00  ·  llm    planner           Plan: look up order 8841, find the refun…  [sync]
  step 01  ·  tool   crm.lookup_order  {"error":"rate_limited","retry_after_ms"…  [log-move]
  step 02  ·  tool   crm.lookup_order  {"status":"delivered","delivered":"2026-…  [sync]
  ...
⑂ step 05  ✗  tool   kb.fetch          A: Refunds v3 (CURRENT, effective          [FORK · conf 0.68]
                                       ...
              - inputs.doc: "policy/refunds_v3.md"
              + inputs.doc: "archive/refunds_v1.md"
```

How to read it:

- **`[sync]` lines recede** — the two runs agree there; content is shown dimmed and truncated.
- **`[log-move]` / `[model-move]`** — absorbed noise: a retry, a re-order, an extra step
  that the alignment recovered from. Shown, not alarmed on.
- **`⑂ … [FORK · conf 0.68]`** — the first divergence the runs never recover from, with
  both sides' content and the engine's confidence in this localization.
- **`- / +` lines** — the field-level diff at the fork: which input/output changed.

## 4 · Machines: `--json` and exit codes

```sh
amberfork diff bad.json --against good.json --json > result.json
```

`result.json` is the versioned `DiffResult` contract (`.meta.schema_version`). Useful
paths:

```sh
jq '.fork.index, .fork.confidence' result.json   # where, how sure
jq '.alignment[] | select(.kind != "sync")' result.json   # every absorbed divergence
jq -r '.warnings[].msg' result.json              # ingest/normalization diagnostics
```

Exit codes follow `diff(1)`: **0** converged, **1** forked, **2** trouble (unreadable or
invalid input). `amberfork diff` in CI gates on a reference run for free.

## 5 · When it goes wrong

- **`failed to parse trace JSON: missing field 'schema_version'`** — you probably pointed
  amberfork at the raw exporter file (e.g. the `.jsonl` transcript itself). It takes one
  JSON trace per file; convert first (§2).
- **`unknown variant 'message', expected one of 'llm', 'tool', 'agent', 'other'`** — your
  converter is passing framework step kinds through. Map them onto the canonical four.
- **A 0-step run "forks at step 0"** — technically true, practically a converter bug:
  check that your script actually emitted steps.
- **Long runs feel slow** — alignment cost grows with the product of the two runs'
  lengths; ~1000×1000 steps takes ~13 s today. Known, measured, tracked
  ([#16](https://github.com/Melvin0070/amberfork/issues/16)).
- **OTel GenAI / OpenInference exports** — direct ingestion is on the roadmap, not built.
  Today the plain-JSON format is the way in; the mappings sketched in
  [`trace-format.md`](trace-format.md) make the conversion mechanical.
