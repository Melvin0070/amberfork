#!/usr/bin/env python3
"""The free-running tool agent for the long-run replay measurement (issue #65).

Run under `amberfork record --base-url-env AMBERFORK_BASE_URL -- python3 longrun_agent.py ...`
or `amberfork replay <cassette> --base-url-env AMBERFORK_BASE_URL -- ...`. Like
`perturbation_agent.py` it knows nothing about recording: it reads a base URL and calls /api/chat.

Unlike that agent, nothing here scripts the path. The model picks which records to open and when
to stop; the loop only executes what it asks for. `--change tool:value` alters the result of that
one world call (see `longrun_world.py`) and is the controlled change a replay should catch.

The request body is a pure function of the conversation so far — no timestamps, no random ids —
which is what makes a recording of this agent replayable at all.

Exit codes: 0 = reached a final answer; 3 = hit --max-turns without one; 1 = unexpected error.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request

from longrun_world import SYSTEM_PROMPT, TASKS, TOOL_SCHEMAS, execute_tool

MODEL = "qwen3:8b"
TEMPERATURE = 0.8
NUM_CTX = 8192
REQUEST_TIMEOUT_S = 600


def chat(base_url: str, messages: list[dict]) -> dict:
    body = json.dumps(
        {
            "model": MODEL,
            "messages": messages,
            "tools": TOOL_SCHEMAS,
            "stream": False,
            "think": False,
            "options": {"temperature": TEMPERATURE, "num_ctx": NUM_CTX},
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        f"{base_url.rstrip('/')}/api/chat",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=REQUEST_TIMEOUT_S) as resp:
        return json.loads(resp.read().decode("utf-8"))


def run_session(task_id: str, change: str | None, max_turns: int) -> dict:
    base_url = os.environ.get("AMBERFORK_BASE_URL")
    if not base_url:
        raise SystemExit("AMBERFORK_BASE_URL is not set — run this under `amberfork record` or `replay`")

    messages: list[dict] = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": TASKS[task_id].user_message},
    ]
    exchanges = 0
    tool_calls_made = 0
    final_answer: str | None = None

    for _turn in range(max_turns):
        message = chat(base_url, messages)["message"]
        exchanges += 1
        messages.append(message)

        calls = message.get("tool_calls") or []
        if not calls:
            final_answer = message.get("content", "")
            break
        for position, call in enumerate(calls):
            name = call["function"]["name"]
            args = call["function"].get("arguments") or {}
            call_id = call.get("id") or f"call_{exchanges}_{position}_{name}"
            result = execute_tool(name, args, change)
            tool_calls_made += 1
            messages.append({"role": "tool", "tool_call_id": call_id, "content": json.dumps(result)})

    return {
        "exit_code": 0 if final_answer is not None else 3,
        "task": task_id,
        "change": change,
        "exchanges": exchanges,
        "tool_calls": tool_calls_made,
        "final_answer": final_answer,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--task", required=True, choices=sorted(TASKS))
    parser.add_argument("--change", help="alter one world call's result, e.g. get_order:O2003")
    parser.add_argument("--max-turns", type=int, default=60)
    parser.add_argument("--summary-out", help="optional path to write the session summary JSON")
    args = parser.parse_args()

    try:
        summary = run_session(args.task, args.change, args.max_turns)
    except (urllib.error.URLError, TimeoutError, OSError, KeyError, json.JSONDecodeError) as exc:
        summary = {"exit_code": 1, "reason": f"{type(exc).__name__}: {exc}"}

    if args.summary_out:
        with open(args.summary_out, "w") as f:
            json.dump(summary, f, indent=2)
    if summary["exit_code"] == 1:
        print(f"longrun_agent: {summary['reason']}", file=sys.stderr)
    return summary["exit_code"]


if __name__ == "__main__":
    sys.exit(main())
