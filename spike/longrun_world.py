"""The synthetic world for the long-run replay measurement (issue #65).

Where `perturbation_world.py` answers a fixed value whatever the model asks, every tool here is a
pure function of its arguments over one invented dataset, so the model chooses its own path: which
orders to open, in what order, how many calls per turn, when to stop. That freedom is the point —
#65 measures replay where two live runs have room to differ.

Everything is invented for this experiment (no upstream-dataset content), and built from fixed
arithmetic rather than a seeded RNG so no library version can shift it.

A *change* is one tool call, identified by `call_key(tool, args)`, whose result is altered. It
models the thing a replay gate exists to catch: a dependency that now answers differently.
"""

from __future__ import annotations

import ast
import json
import operator
from dataclasses import dataclass

SYSTEM_PROMPT = (
    "You are an operations analyst. Work only from what the tools return — never assume a value "
    "you have not looked up. Look up every record the task depends on before you answer, then "
    "give a concise final answer with the figures you found."
)

REGIONS = ["north", "south", "east", "west"]
TIERS = ["standard", "plus", "premier"]
ITEMS = ["router", "keyboard", "monitor", "dock", "webcam", "headset", "tablet", "printer"]
CARRIERS = ["Kestrel", "Halden", "Brightline"]
STATUSES = ["delivered", "delivered", "in_transit", "delivered", "returned", "delivered", "lost"]

POLICIES = {
    "refunds": (
        "Refund windows by tier, counted in days since the order was placed: standard 30, "
        "plus 45, premier 60. Only delivered orders can be refunded. Returned and lost orders "
        "are handled separately and are never refunded under this policy."
    ),
    "delays": (
        "Delay compensation: a shipment delayed by 3 or more days earns a credit of 10% of the "
        "order amount; 7 or more days earns 25%. Lost shipments earn a credit of 100%. "
        "Shipments delayed by fewer than 3 days earn nothing."
    ),
    "carriers": (
        "Carrier review: a carrier is flagged when it has 2 or more shipments for one customer "
        "that are lost or delayed by 7 or more days."
    ),
}


def _customers() -> dict[str, dict]:
    return {
        f"C{100 + i}": {
            "customer_id": f"C{100 + i}",
            "tier": TIERS[i % 3],
            "region": REGIONS[(i * 3) % 4],
            "account_age_months": 6 + i * 7,
        }
        for i in range(8)
    }


def _orders() -> dict[str, dict]:
    orders = {}
    n = 0
    for i in range(8):
        for j in range(6 + i % 3):
            oid = f"O{2000 + n}"
            status = STATUSES[(i * 5 + j * 3) % len(STATUSES)]
            orders[oid] = {
                "order_id": oid,
                "customer_id": f"C{100 + i}",
                "item": ITEMS[(i + j * 3) % len(ITEMS)],
                "amount": 40 + ((i * 37 + j * 53) % 23) * 15,
                "placed_days_ago": 5 + ((i * 11 + j * 17) % 13) * 6,
                "status": status,
                "shipment_id": f"S{7000 + n}",
            }
            n += 1
    return orders


def _shipments(orders: dict[str, dict]) -> dict[str, dict]:
    shipments = {}
    for n, order in enumerate(orders.values()):
        lost = order["status"] == "lost"
        shipments[order["shipment_id"]] = {
            "shipment_id": order["shipment_id"],
            "order_id": order["order_id"],
            "carrier": CARRIERS[(n * 2 + n // 3) % len(CARRIERS)],
            "status": "lost" if lost else order["status"],
            "delay_days": 0 if lost else [0, 0, 2, 4, 0, 8, 1, 3, 0, 11][n % 10],
        }
    return shipments


CUSTOMERS = _customers()
ORDERS = _orders()
SHIPMENTS = _shipments(ORDERS)


def _fn(name: str, description: str, param: str, param_description: str) -> dict:
    return {
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": {param: {"type": "string", "description": param_description}},
                "required": [param],
            },
        },
    }


TOOL_SCHEMAS = [
    _fn("get_customer", "Look up a customer's tier, region and account age.", "customer_id", "e.g. C100"),
    _fn("list_orders", "List the order ids belonging to a customer.", "customer_id", "e.g. C100"),
    _fn("get_order", "Look up one order: item, amount, age, status, shipment id.", "order_id", "e.g. O2000"),
    _fn("get_shipment", "Look up one shipment: carrier, status, delay in days.", "shipment_id", "e.g. S7000"),
    _fn("get_policy", "Read a policy document.", "topic", "one of: refunds, delays, carriers"),
    _fn("calculate", "Evaluate a numeric arithmetic expression.", "expression", "e.g. '0.10 * 900'"),
]

# The single argument each world tool reads. `calculate` is not a world tool: it has no state to change.
WORLD_TOOLS = {
    "get_customer": "customer_id",
    "list_orders": "customer_id",
    "get_order": "order_id",
    "get_shipment": "shipment_id",
    "get_policy": "topic",
}


@dataclass(frozen=True)
class Task:
    id: str
    user_message: str


def _task(tid: str, customer: str, ask: str) -> Task:
    return Task(tid, f"Customer {customer}: {ask}")


TASKS = {
    t.id: t
    for t in [
        _task("pilot_refunds", "C107", "which of their orders are refundable today, and what is the total refundable amount?"),
        _task("refund_audit_a", "C100", "which of their orders are refundable today, and what is the total refundable amount?"),
        _task("refund_audit_b", "C104", "which of their orders are refundable today, and what is the total refundable amount?"),
        _task("delay_credit_a", "C101", "what delay compensation is owed across all of their orders, in total and per order?"),
        _task("delay_credit_b", "C105", "what delay compensation is owed across all of their orders, in total and per order?"),
        _task("carrier_review_a", "C102", "should any carrier be flagged for their shipments, and which shipments are the reason?"),
        _task("carrier_review_b", "C106", "should any carrier be flagged for their shipments, and which shipments are the reason?"),
        _task("account_summary", "C103", "summarise the account: orders by status, total spend on delivered orders, refundable total, and delay compensation owed."),
    ]
}

# Never part of the scored corpus: used only to check the harness runs end to end.
PILOT_TASKS = {"pilot_refunds"}

_SAFE_OPS = {
    ast.Add: operator.add,
    ast.Sub: operator.sub,
    ast.Mult: operator.mul,
    ast.Div: operator.truediv,
    ast.USub: operator.neg,
    ast.UAdd: operator.pos,
}


def safe_calculate(expression: str) -> float:
    """Evaluate a numeric expression with no names, calls, or attribute access allowed."""

    def _eval(node: ast.AST) -> float:
        if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
            return node.value
        if isinstance(node, ast.BinOp) and type(node.op) in _SAFE_OPS:
            return _SAFE_OPS[type(node.op)](_eval(node.left), _eval(node.right))
        if isinstance(node, ast.UnaryOp) and type(node.op) in _SAFE_OPS:
            return _SAFE_OPS[type(node.op)](_eval(node.operand))
        raise ValueError("unsupported expression")

    return _eval(ast.parse(expression, mode="eval").body)


def call_key(tool: str, args: dict) -> str | None:
    """Identity of a world call — the unit a change targets. None for `calculate` and bad args."""
    param = WORLD_TOOLS.get(tool)
    if param is None or not isinstance(args, dict):
        return None
    value = args.get(param)
    if not isinstance(value, str):
        return None
    return f"{tool}:{value.strip()}"


def _lookup(tool: str, value: str) -> dict | None:
    if tool == "get_customer":
        return CUSTOMERS.get(value)
    if tool == "list_orders":
        ids = [o["order_id"] for o in ORDERS.values() if o["customer_id"] == value]
        return {"customer_id": value, "order_ids": ids} if ids else None
    if tool == "get_order":
        return ORDERS.get(value)
    if tool == "get_shipment":
        return SHIPMENTS.get(value)
    if tool == "get_policy":
        return {"topic": value, "text": POLICIES[value]} if value in POLICIES else None
    return None


def _alter(tool: str, result: dict) -> dict:
    """The changed dependency's answer. Each alteration moves a value the tasks reason over."""
    changed = dict(result)
    if tool == "get_customer":
        changed["tier"] = TIERS[(TIERS.index(result["tier"]) + 1) % 3]
    elif tool == "list_orders":
        changed["order_ids"] = result["order_ids"][:-1]
    elif tool == "get_order":
        changed["status"] = "returned" if result["status"] == "delivered" else "delivered"
    elif tool == "get_shipment":
        changed["delay_days"] = result["delay_days"] + 7
    elif tool == "get_policy":
        changed["text"] = result["text"].replace("30", "14").replace("10%", "5%").replace("2 or more", "3 or more")
    return changed


def is_changeable(key: str) -> bool:
    """True when `key` names a world call that returns a record, so altering it is meaningful."""
    tool, _, value = key.partition(":")
    return tool in WORLD_TOOLS and _lookup(tool, value) is not None


def execute_tool(tool: str, args: dict, change: str | None) -> dict:
    if tool == "calculate":
        try:
            return {"result": safe_calculate(str(args.get("expression", "")))}
        except Exception as exc:  # noqa: BLE001 - surfaced to the model as a tool error
            return {"result": None, "error": str(exc)}
    key = call_key(tool, args)
    if key is None:
        return {"error": f"bad arguments for {tool}: {json.dumps(args, sort_keys=True)}"}
    value = key.partition(":")[2]
    result = _lookup(tool, value)
    if result is None:
        return {"error": f"{tool}: nothing found for {value!r}"}
    return _alter(tool, result) if change == key else result
