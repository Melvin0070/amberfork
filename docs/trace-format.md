# Canonical trace format (plain JSON) — v0.1

> The zero-dependency way in. OTel GenAI / OpenInference ingestion is the framework-agnostic
> path, but nobody should need OTel to try agentdiff: any log that can be massaged into this
> shape (a run = an ordered list of steps) is a valid input to `adiff diff a.json b.json`.
> This file is the public contract for that shape. It mirrors the canonical model in
> `docs/design/design-run-diff-debugger.md` (`Run`/`Step`); once `adiff-model` exists, the Rust
> types are the source of truth and this document tracks them.

## Shape

```json
{
  "schema_version": "0.1",
  "id": "refund-triage_2026-07-07_bad",
  "task": "refund-triage #4512",
  "outcome": "fail",
  "steps": [
    {
      "idx": 0,
      "kind": "llm",
      "name": "planner",
      "inputs": { "messages": [{ "role": "user", "content": "Handle refund for order 8841" }] },
      "outputs": { "content": "I'll look up the order first." },
      "attrs": { "model": "claude-sonnet-5" },
      "t_start": null,
      "t_end": null,
      "parent_idx": null
    },
    {
      "idx": 1,
      "kind": "tool",
      "name": "lookup_order",
      "inputs": { "order_id": "8841" },
      "outputs": { "status": "shipped", "total": 129.0 },
      "attrs": {},
      "parent_idx": 0
    }
  ],
  "edges": [[0, 1]]
}
```

## Field semantics

| Field | Required | Meaning |
|---|---|---|
| `schema_version` | yes | version of this contract; breaking changes bump it |
| `id` | yes | unique run id (any string) |
| `task` | no | human label of what the run attempted |
| `outcome` | no | `pass` / `fail` / `unknown` — run-level verdict if known. NEVER inferred from span status (architecture rule) |
| `steps[].idx` | yes | 0-based position in the trajectory |
| `steps[].kind` | yes | `llm` \| `tool` \| `agent` \| `other` |
| `steps[].name` | yes | agent or tool name — part of the structural identity the aligner keys on |
| `steps[].inputs` / `outputs` | at least one | string or object; objects get field-level diffing, strings get text diffing |
| `steps[].attrs` | no | anything else worth keeping (model, tokens, cost) |
| `steps[].t_start` / `t_end` | no | RFC3339; timing is display-only, never an alignment signal |
| `steps[].parent_idx` | no | caller step (builds the DAG); absent/null on every step = linear chain |
| `edges` | no | explicit DAG edges; if absent, derived from `parent_idx`, else linear |

Minimal valid step: `{"idx": n, "kind": "…", "name": "…", "outputs": "…"}`. The format is
deliberately forgiving: unknown fields are preserved into `attrs` and reported by the
"unmapped attributes" warning rather than failing the parse.

## Mappings (informative)

- **OTel GenAI** (`gen_ai.*` spans): span → step; operation/tool name → `kind`/`name`; opt-in
  content events → `inputs`/`outputs` (absent content ⇒ metadata-only step + banner).
- **OpenInference** (`llm.*` / `openinference.*`): span kind LLM/TOOL/AGENT/CHAIN → `kind`;
  `llm.input_messages` / `llm.output_messages`, `tool.name` → fields above.
- **Who&When failure logs** (used by the spike): each history entry → one step; entry
  name/role → `name` with `kind=agent`; entry content → `outputs`; the annotated mistake step
  maps to `idx`.

## Versioning

Additive changes (new optional fields) keep the version; renames, removals, or semantic changes
bump it. The parser accepts every published version and reports what it upgraded.
