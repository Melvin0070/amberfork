"""Reading a long-run cassette: which world calls it made, and which three get changed (088).

Shared by the recorder (which needs each tape's middle change for class D) and the scorer, so the
selection rule exists once.
"""

from __future__ import annotations

from longrun_world import call_key, is_changeable

POSITIONS = ("first", "middle", "last")


def first_calls(cassette: dict) -> list[tuple[str, int]]:
    """Distinct changeable world calls in order of first appearance, each with the index of the
    exchange whose recorded response first makes it."""
    seen: dict[str, int] = {}
    for exchange in cassette["exchanges"]:
        message = exchange["response"]["body"].get("message", {})
        for call in message.get("tool_calls") or []:
            key = call_key(call["function"]["name"], call["function"].get("arguments") or {})
            if key is not None and key not in seen and is_changeable(key):
                seen[key] = exchange["idx"]
    return list(seen.items())


def gold_exchange(cassette: dict, key: str) -> int | None:
    """The exchange whose request first carries `key`'s result: one past the call. None when the
    run never made the call."""
    for made, idx in first_calls(cassette):
        if made == key:
            return idx + 1
    return None


def registered_changes(cassette: dict) -> dict[str, dict]:
    """088's selection: the first, the middle (index n // 2) and the last distinct changeable call.
    Positions that land on the same call are kept once, under the earliest position's name."""
    calls = first_calls(cassette)
    if not calls:
        return {}
    picks = {"first": 0, "middle": len(calls) // 2, "last": len(calls) - 1}
    chosen: dict[str, dict] = {}
    used: set[int] = set()
    for position in POSITIONS:
        index = picks[position]
        if index in used:
            continue
        used.add(index)
        key, idx = calls[index]
        chosen[position] = {"change": key, "gold": idx + 1}
    return chosen


def context_defects(cassette: dict, limit: int) -> list[int]:
    """Exchanges whose prompt plus completion reached the context limit (088's harness check)."""
    defects = []
    for exchange in cassette["exchanges"]:
        body = exchange["response"]["body"]
        if body.get("prompt_eval_count", 0) + body.get("eval_count", 0) >= limit:
            defects.append(exchange["idx"])
    return defects
