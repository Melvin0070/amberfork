#!/usr/bin/env python3
"""A two-turn toy agent for the CI-gate example (docs/ci.md).

It talks to whatever AGENT_BASE_URL points at — a real provider, `amberfork record`'s capture
proxy, or `amberfork replay`'s tape — and has no other dependency. AGENT_SYSTEM_PROMPT stands in
for "a change someone made to the agent": leave it unset and the agent sends exactly what the
committed reference recording holds; set it and the very first request differs.
"""

import json
import os
import urllib.request

BASE = os.environ["AGENT_BASE_URL"]
SYSTEM = os.environ.get(
    "AGENT_SYSTEM_PROMPT", "You are a refund assistant. Apply the current refund policy."
)


def ask(messages):
    body = json.dumps({"model": "example-model", "messages": messages}).encode()
    request = urllib.request.Request(
        BASE + "/v1/chat", data=body, headers={"content-type": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read())["content"]


def main():
    history = [
        {"role": "system", "content": SYSTEM},
        {"role": "user", "content": "Order 8841 was delivered 23 days ago. Can it be refunded?"},
    ]
    plan = ask(history)
    history += [
        {"role": "assistant", "content": plan},
        {"role": "user", "content": "Then carry out that decision."},
    ]
    print(ask(history))


if __name__ == "__main__":
    main()
