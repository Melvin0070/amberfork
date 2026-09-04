#!/usr/bin/env python3
"""The real tool-using agent for #49's perturbation protocol (notebook 074).

Run under `amberfork record --base-url-env AMBERFORK_BASE_URL -- python3 perturbation_agent.py
...` — `amberfork record` sets AMBERFORK_BASE_URL to its own capture-proxy URL, which relays to
the real Ollama server named at `--upstream`, so every /api/chat call this script makes is recorded
verbatim, with no code here aware that recording is happening.

Gold labeling happens here, not after the fact: this script counts its own exchanges (one per
/api/chat call, matching amberfork-record's normalize() one-exchange-per-step contract exactly)
and, on the first call to the task's designated tool in a --perturb run, records the index of the
*next* request — the one whose message history will carry the bad result — as gold_step.

Exit codes drive the orchestrator's retry decision (never a fork-outcome decision — see notebook
074's retry rule):
  0 = usable recording (designated tool was called; a final answer was reached)
  2 = designated tool was never called — discard and re-record
  3 = exceeded --max-turns without a final answer — discard and re-record
  1 = unexpected error (network, malformed response) — not silently retried by the orchestrator
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request

from perturbation_world import SYSTEM_PROMPT, TASKS, TOOL_SCHEMAS, execute_tool, safe_calculate

MODEL = "qwen3:8b"
TEMPERATURE = 0.8
NUM_CTX = 40960
REQUEST_TIMEOUT_S = 180


def chat(base_url: str, messages: list[dict], tools: list[dict]) -> dict:
    body = json.dumps(
        {
            "model": MODEL,
            "messages": messages,
            "tools": tools,
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


def run_session(task_id: str, perturb: bool, max_turns: int) -> dict:
    task = TASKS[task_id]
    base_url = os.environ.get("AMBERFORK_BASE_URL")
    if not base_url:
        raise SystemExit("AMBERFORK_BASE_URL is not set — run this under `amberfork record`")

    tools = [TOOL_SCHEMAS[name] for name in task.tools]
    messages: list[dict] = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": task.user_message},
    ]

    exchange_count = 0
    designated_calls = 0
    gold_step: int | None = None
    final_answer: str | None = None

    for _turn in range(max_turns):
        response = chat(base_url, messages, tools)
        message = response["message"]
        exchange_count += 1  # this request just became step idx (exchange_count - 1)
        messages.append(message)

        tool_calls = message.get("tool_calls") or []
        if not tool_calls:
            final_answer = message.get("content", "")
            break

        for call in tool_calls:
            name = call["function"]["name"]
            call_id = call.get("id") or f"call_{exchange_count}_{name}"
            if name == "calculate":
                args = call["function"].get("arguments", {})
                try:
                    result = {"result": safe_calculate(str(args.get("expression", "")))}
                except Exception as exc:  # noqa: BLE001 - surfaced to the model as a tool error
                    result = {"result": None, "error": str(exc)}
            else:
                result = execute_tool(task, name, perturbed=perturb)
                if name == task.designated_tool:
                    if perturb and designated_calls == 0:
                        # exchange_count is already the count of *completed* exchanges, which is
                        # exactly the 0-based idx normalize() will assign the *next* exchange —
                        # the one whose request body first carries this tool result.
                        gold_step = exchange_count
                    designated_calls += 1
            messages.append(
                {"role": "tool", "tool_call_id": call_id, "content": json.dumps(result)}
            )
    else:
        return {"exit_code": 3, "reason": "exceeded max_turns without a final answer"}

    if designated_calls == 0:
        return {"exit_code": 2, "reason": "designated tool was never called"}

    return {
        "exit_code": 0,
        "task": task.id,
        "perturbed": perturb,
        "perturbation_type": task.perturbation_type if perturb else None,
        "designated_tool": task.designated_tool,
        "designated_calls": designated_calls,
        "gold_step": gold_step,
        "turns": exchange_count,
        "final_answer": final_answer,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--task", required=True, choices=sorted(TASKS))
    parser.add_argument("--perturb", action="store_true")
    parser.add_argument("--max-turns", type=int, default=8)
    parser.add_argument("--summary-out", required=True, help="path to write the session summary JSON")
    args = parser.parse_args()

    try:
        summary = run_session(args.task, args.perturb, args.max_turns)
    except (urllib.error.URLError, TimeoutError, OSError, KeyError, json.JSONDecodeError) as exc:
        summary = {"exit_code": 1, "reason": f"{type(exc).__name__}: {exc}"}

    with open(args.summary_out, "w") as f:
        json.dump(summary, f, indent=2)

    exit_code = summary["exit_code"]
    if exit_code != 0:
        print(f"perturbation_agent: {summary['reason']}", file=sys.stderr)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
