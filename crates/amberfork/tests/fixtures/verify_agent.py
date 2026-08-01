"""Toy two-turn agent for the real-provider `diff --verify` e2e (issue #44).

Talks to a local Ollama instance (its OpenAI-compatible base-URL-env-var pattern is what any real
SDK uses) via the native, non-streaming `/api/generate` endpoint — one JSON request/response, no
extra parsing. Turn 1's prompt embeds turn 0's answer verbatim, so once `--verify` patches turn 0's
response, turn 1's request genuinely changes and cache-misses the replay tape, forcing a live relay
to the real upstream. That live relay — never exercised by the offline stub-driven suite — is the
coverage hole this whole fixture exists to close.

Temperature is high on purpose: recording `good` and `bad` independently from the *same* prompts
must sometimes diverge by real sampling variance, not by the script asking a different question
(the counterfactual patch only ever swaps a response, never a request — see `patch.rs`).
"""

import json
import os
import sys
import urllib.request

BASE = os.environ["AMBERFORK_VERIFY_BASE_URL"]
MODEL = os.environ.get("AMBERFORK_VERIFY_MODEL", "smollm2:135m")


def generate(prompt: str) -> str:
    body = json.dumps(
        {
            "model": MODEL,
            "prompt": prompt,
            "stream": False,
            "options": {"temperature": 1.4},
        }
    ).encode()
    req = urllib.request.Request(
        BASE + "/api/generate", data=body, headers={"content-type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())["response"].strip()


def main() -> None:
    turn0 = generate("Name a random fruit. Reply with just the fruit's name, one word, nothing else.")
    generate(
        f'You just said "{turn0}". Name a color that fruit could be. '
        "Reply with just the color, one word, nothing else."
    )


if __name__ == "__main__":
    sys.exit(main())
