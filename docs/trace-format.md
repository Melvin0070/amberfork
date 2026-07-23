# Canonical trace format (plain JSON) — v0.1

> The zero-dependency way in. OTel GenAI / OpenInference ingestion is the framework-agnostic
> path, but nobody should need OTel to try amberfork: any log that can be massaged into this
> shape (a run = an ordered list of steps) is a valid input to `amberfork diff a.json b.json`.
> This file is the public contract for that shape. It mirrors the canonical model in
> `docs/design/design-run-diff-debugger.md` (`Run`/`Step`); once `amberfork-model` exists, the Rust
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

## Mappings

Each foreign shape has its own namespaced adapter in `amberfork-ingest`; the canonical loader
above never bends to fit one. Status is stated honestly — "framework-agnostic" is a covered
subset today, not a blanket claim.

- **OpenInference** (`openinference.*` / `llm.*` / `tool.*` over OTLP/JSON) — **implemented**
  (`amberfork_ingest::openinference`, one `Run` per `traceId`). Covered: `openinference.span.kind`
  LLM/TOOL/AGENT → `kind` (CHAIN/RETRIEVER/EMBEDDING/… fold to `other`); `tool.name` → `name`;
  `input.value`/`output.value` honoring `*.mime_type` (JSON → field-diffable object, else text) →
  `inputs`/`outputs`; steps ordered by `startTimeUnixNano`, `parentSpanId` → `parent_idx`. Absent
  content ⇒ metadata-only step + `content-absent` warning; a non-OpenInference attribute is
  preserved to `attrs` + an `unmapped-attributes` warning. `outcome` is **never** inferred from
  span status. Deferred: reconstructing structured messages from flattened `llm.input_messages.*`
  (they ride in `attrs` meanwhile); RFC3339 timing (raw nanos are preserved in `attrs`).
- **OTel GenAI** (native `gen_ai.*` spans) — **planned** (next slice). Same OTLP/JSON envelope as
  the OpenInference adapter, different attribute vocabulary: `gen_ai.operation.name` → `kind`/`name`,
  opt-in content events → `inputs`/`outputs` (absent content ⇒ metadata-only step + banner).
- **TRAIL / Patronus trace trees** — **implemented** (`amberfork_ingest::trail`, one `Run` per
  trace file). A *different envelope* — a nested `child_spans` tree, not a flat OTLP export — over
  the *same* OpenInference vocabulary, so it shares the mapping above (`openinference.span.kind` →
  `kind`, `tool.name` → `name`, `input.value`/`output.value` + `*.mime_type` → `inputs`/`outputs`,
  foreign attrs → `attrs` + `unmapped-attributes` warning, `outcome` never from `status_code`).
  Envelope specifics: steps ordered by a pre-order walk of the tree (which *is* execution order for
  a single-SDK trace) with `parent_idx` from the nesting; semantic `kind` from the attribute, never
  the wire `span_kind` (always `"Internal"`); RFC3339 `timestamp` → `t_start`; the source `span_id`
  is retained in `attrs` (`otel.span_id`) so a TRAIL error annotation's span-located `location` can
  be resolved to a step by the benchmark layer.
- **Who&When failure logs** — **implemented** (`amberfork_ingest::whowhen`): each history entry →
  one step; entry name/role → `name` with `kind=agent`; entry content → `outputs`; the dataset's
  blame annotation is returned *beside* the run as gold, never merged into it.
- **TapeAgents tapes** — **implemented** (`amberfork_ingest::tape`): each typed node → one
  `kind=agent` step whose body survives as a field-diffable object; GAIA pairing metadata is
  returned beside the run.

## Versioning

Additive changes (new optional fields) keep the version; renames, removals, or semantic changes
bump it. The parser accepts every published version and reports what it upgraded.
