"""The synthetic "world" for #49's real-agent perturbation protocol (notebook 074).

Five tasks, three mock tools, one designated tool call per task whose result is swapped for a
wrong one in the "perturbed" condition. Everything here is invented for this experiment — no GAIA
or upstream-dataset content — so, unlike every other corpus in this repo, none of it needs
sanitization before being committed (see notebook 074).

`correct_result`/`perturbed_result` are fixed per task: the mock tools do not parse the model's
query text, they answer whatever the task calls for. That is deliberate — this experiment measures
whether the *aligner* localizes a real agent's reaction to a bad tool value, not whether the agent
phrases a good search query.
"""

from __future__ import annotations

import ast
import operator
from dataclasses import dataclass

SYSTEM_PROMPT = (
    "You are a support agent. Before answering definitively, use the available tools to check "
    "current information — do not answer from assumption. Cite what you found, then give a "
    "direct, concise answer."
)

TOOL_SCHEMAS: dict[str, dict] = {
    "search_docs": {
        "type": "function",
        "function": {
            "name": "search_docs",
            "description": "Search internal policy documents.",
            "parameters": {
                "type": "object",
                "properties": {"query": {"type": "string", "description": "search query"}},
                "required": ["query"],
            },
        },
    },
    "check_status": {
        "type": "function",
        "function": {
            "name": "check_status",
            "description": "Check the current status of an order, shipment, or reference id.",
            "parameters": {
                "type": "object",
                "properties": {
                    "reference_id": {"type": "string", "description": "the id to check"}
                },
                "required": ["reference_id"],
            },
        },
    },
    "calculate": {
        "type": "function",
        "function": {
            "name": "calculate",
            "description": "Evaluate a numeric arithmetic expression.",
            "parameters": {
                "type": "object",
                "properties": {
                    "expression": {"type": "string", "description": "e.g. '0.10 * 900'"}
                },
                "required": ["expression"],
            },
        },
    },
}

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
        if isinstance(node, ast.Expression):
            return _eval(node.body)
        if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
            return node.value
        if isinstance(node, ast.BinOp) and type(node.op) in _SAFE_OPS:
            return _SAFE_OPS[type(node.op)](_eval(node.left), _eval(node.right))
        if isinstance(node, ast.UnaryOp) and type(node.op) in _SAFE_OPS:
            return _SAFE_OPS[type(node.op)](_eval(node.operand))
        raise ValueError(f"disallowed expression: {ast.dump(node)}")

    return _eval(ast.parse(expression, mode="eval"))


@dataclass(frozen=True)
class Task:
    id: str
    user_message: str
    tools: tuple[str, ...]
    designated_tool: str
    perturbation_type: str
    correct_result: dict
    perturbed_result: dict


TASKS: dict[str, Task] = {
    "refund_window": Task(
        id="refund_window",
        user_message=(
            "I bought a damaged item 40 days ago. Can I get a refund? Check the current policy "
            "first."
        ),
        tools=("search_docs",),
        designated_tool="search_docs",
        perturbation_type="swapped_doc",
        correct_result={
            "doc_id": "POL-77",
            "title": "Refund Policy (rev. 12)",
            "content": (
                "Items may be refunded within 45 days of purchase if defective or damaged. "
                "Store credit only after 45 days."
            ),
        },
        perturbed_result={
            "doc_id": "POL-31",
            "title": "Refund Policy (rev. 9, superseded)",
            "content": (
                "Items may be refunded within 30 days of purchase if defective or damaged. "
                "No refunds after 30 days."
            ),
        },
    ),
    "order_status": Task(
        id="order_status",
        user_message="Where is my order A193? It was supposed to arrive yesterday.",
        tools=("check_status",),
        designated_tool="check_status",
        perturbation_type="wrong_status",
        correct_result={
            "reference_id": "A193",
            "status": "delayed",
            "detail": "Carrier delay reported. New ETA in 3 days.",
        },
        perturbed_result={
            "reference_id": "A193",
            "status": "delivered",
            "detail": "Delivered yesterday at 2:14pm, signed for at the front desk.",
        },
    ),
    "cancellation_fee": Task(
        id="cancellation_fee",
        user_message=(
            "I want to cancel my annual subscription today, 100 days into a 365-day term, on a "
            "$900 plan. Check the cancellation policy, then calculate my early-cancellation fee "
            "as a percentage of the $900."
        ),
        tools=("search_docs", "calculate"),
        designated_tool="search_docs",
        perturbation_type="stale_value",
        correct_result={
            "doc_id": "POL-14",
            "title": "Cancellation Policy (rev. 4)",
            "content": (
                "Early cancellation fee = 10% of the plan value, regardless of remaining term."
            ),
        },
        perturbed_result={
            "doc_id": "POL-14",
            "title": "Cancellation Policy (rev. 2, outdated)",
            "content": (
                "Early cancellation fee = 25% of the plan value, regardless of remaining term."
            ),
        },
    ),
    "shipping_cutoff": Task(
        id="shipping_cutoff",
        user_message=(
            "I need an order to arrive by Friday. Is standard shipping still fast enough, or do "
            "I need to pay for expedited? Check the current shipping status for reference "
            "SHIP-CUTOFF first."
        ),
        tools=("check_status",),
        designated_tool="check_status",
        perturbation_type="wrong_status",
        correct_result={
            "reference_id": "SHIP-CUTOFF",
            "status": "on-time",
            "detail": (
                "Standard shipping cutoff for Friday delivery is Tuesday 5pm. It is currently "
                "Monday, so standard shipping is fine."
            ),
        },
        perturbed_result={
            "reference_id": "SHIP-CUTOFF",
            "status": "delayed",
            "detail": (
                "Carrier network delay in effect. Standard shipping cutoff for Friday delivery "
                "already passed last Friday; expedited shipping is required."
            ),
        },
    ),
    "warranty_eligibility": Task(
        id="warranty_eligibility",
        user_message=(
            "My blender stopped working after 14 months. Is it still under warranty? Check the "
            "warranty policy first."
        ),
        tools=("search_docs",),
        designated_tool="search_docs",
        perturbation_type="swapped_doc",
        correct_result={
            "doc_id": "POL-52",
            "title": "Warranty Policy — Kitchen Appliances",
            "content": (
                "Kitchen appliances (blenders, mixers, kettles) carry an 18-month limited "
                "warranty from the purchase date."
            ),
        },
        perturbed_result={
            "doc_id": "POL-53",
            "title": "Warranty Policy — Refurbished Kitchen Appliances",
            "content": (
                "Refurbished kitchen appliances carry a 12-month limited warranty from the "
                "purchase date."
            ),
        },
    ),
}


def execute_tool(task: Task, tool_name: str, perturbed: bool) -> dict:
    """Return the mock tool's result. Only `task.designated_tool` ever varies by `perturbed`;
    `calculate` always evaluates for real, on whatever the model sends it."""
    if tool_name == "calculate":
        return {"result": None, "error": "no expression given"}
    if tool_name == task.designated_tool:
        return dict(task.perturbed_result if perturbed else task.correct_result)
    raise ValueError(f"task {task.id!r} has no mock result for tool {tool_name!r}")
