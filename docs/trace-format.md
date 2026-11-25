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

- **Claude Code session transcripts** (`~/.claude/projects/<project>/<session-id>.jsonl`) —
  **implemented, auto-detected by `amberfork diff` / `serve`**
  (`amberfork_ingest::claudecode`, one `Run` per transcript). Assistant `text` → `llm` step named
  `assistant` (model id in `attrs`, never the name); `tool_use` → `tool` step, its `tool_result`
  paired back by id as the output (`is_error` → `attrs`); a prompt → `other` step named `user`.
  Sidechain (subagent) and meta records and `thinking` blocks are skipped; `outcome` is never set.
  The format is undocumented upstream, so unknown record and block types are skipped, not fatal.
- **Codex CLI rollouts** (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`) — **implemented,
  auto-detected by `amberfork diff` / `serve`** on a leading `session_meta` record
  (`amberfork_ingest::codex`, one `Run` per rollout). Only `response_item` records — the
  model-facing transcript — become steps; `event_msg` records are a UI stream that repeats the
  same content and are skipped, as is all bookkeeping. Assistant `message` → `llm` step named
  `assistant` (model from the latest `turn_context`, and `phase`, in `attrs`); `user` /
  `developer` message → `other` step named for the role; `function_call` (JSON arguments decoded
  to a field-diffable object) and `custom_tool_call` (free-text input) → `tool` step named by
  `name`, with the `*_output` record of the same `call_id` paired back as the output;
  inter-agent `agent_message` → `other` step with `author` / `recipient` in `attrs`. `reasoning`
  is skipped (encrypted on disk); `outcome` is never set. Limits: built against codex-cli 0.171
  and checked by loading 634 real rollouts, but the format is undocumented; a subagent's own
  trajectory lives in its own rollout file and is not merged; the replacement history inside a
  `compacted` record is not expanded; `.zst`-compressed rollouts are not read. Unknown record and
  payload types are skipped; a line that fails to parse is an error.
- **ATIF** (Agent Trajectory Interchange Format — Harbor's `trajectory.json`, the format
  Terminal-Bench trials are published in; RFC versions 1.0–1.8) — **implemented, auto-detected by
  `amberfork diff` / `serve`** on a `schema_version` starting `ATIF-` (`amberfork_ingest::atif`,
  one `Run` per document). An ATIF step is a whole agent turn, so it fans out: the turn's
  `message` → `llm` step named `assistant` (model in `attrs`); each `tool_calls` entry → a `tool`
  step named by `function_name`, with the observation result matching its `tool_call_id` as the
  output; a result with no `source_call_id` → `other` step named `observation`; `system` / `user`
  steps → `other` steps named for their source. Every canonical step keeps its ATIF `step_id` in
  `attrs`. Skipped: `reasoning_content`, `is_copied_context` steps, embedded
  `subagent_trajectories`. `outcome` is never set (the verdict lives in the harness's result
  file). A version outside the v1 line loads with a `schema-version-mismatch` warning.

- **OTLP/JSON span exports** (a top-level `resourceSpans`) — **implemented, auto-detected by
  `amberfork diff` / `serve`** (`amberfork_ingest::otel`). The vocabulary is read off the spans:
  each span votes `gen_ai.*` or `openinference.*` by its attribute keys, the majority wins, and a
  tie or an export with neither goes to OpenInference. **One trace is one run.** A single-trace
  export loads as it is; for an export holding several, name the trace with `--trace-id <ID>`
  (the observed side) or `--against-trace-id <ID>` (the reference side) — the full id or a unique
  prefix. Without one, the CLI exits 2 and lists the trace ids with their step counts. Both sides
  may be the same file. The two vocabularies:
- **OpenInference** (`openinference.*` / `llm.*` / `tool.*` over OTLP/JSON) — **implemented**
  (`amberfork_ingest::openinference`, one `Run` per `traceId`). Covered: `openinference.span.kind`
  LLM/TOOL/AGENT → `kind` (CHAIN/RETRIEVER/EMBEDDING/… fold to `other`); `tool.name` → `name`;
  `input.value`/`output.value` honoring `*.mime_type` (JSON → field-diffable object, else text) →
  `inputs`/`outputs`; steps ordered by `startTimeUnixNano`, `parentSpanId` → `parent_idx`. Absent
  content ⇒ metadata-only step + `content-absent` warning; a non-OpenInference attribute is
  preserved to `attrs` + an `unmapped-attributes` warning. `outcome` is **never** inferred from
  span status. Deferred: reconstructing structured messages from flattened `llm.input_messages.*`
  (they ride in `attrs` meanwhile); RFC3339 timing (raw nanos are preserved in `attrs`).
- **OTel GenAI** (native `gen_ai.*` spans) — **implemented** (`amberfork_ingest::genai`, same
  `otlp` envelope as the OpenInference adapter — one `Run` per `traceId`, start-time ordered,
  `outcome` never inferred from span status — a different attribute vocabulary). Covered:
  `gen_ai.operation.name` `chat`/`text_completion`/`generate_content` → `kind=llm`,
  `execute_tool` → `kind=tool` (named by `gen_ai.tool.name`), `create_agent`/`invoke_agent` →
  `kind=agent` (named by `gen_ai.agent.name`); everything else (`embeddings`, …) folds to `other`.
  Content carrier is kind-conditional: `gen_ai.input.messages`/`output.messages` for LLM/agent/other
  spans, `gen_ai.tool.call.arguments`/`tool.call.result` for tool spans — structured (object/array)
  or a pre-serialized JSON string, either way; a `messages` array wraps to a field-diffable
  `{"messages": [...]}` object rather than an opaque blob. Absent content ⇒ metadata-only step +
  `content-absent` warning; a non-`gen_ai.*` attribute is preserved to `attrs` + an
  `unmapped-attributes` warning. Deferred: the superseded per-role event convention
  (`gen_ai.user.message`/… ) is not read, only the current span-attribute convention.

  The adapters below are library APIs today — the CLI does not route files to them.
- **TRAIL / Patronus trace trees** — **implemented** (`amberfork_ingest::trail`, one `Run` per
  trace file). A *different envelope* — a nested `child_spans` tree, not a flat OTLP export — over
  the *same* OpenInference vocabulary, so it shares the mapping above (`openinference.span.kind` →
  `kind`, `tool.name` → `name`, `input.value`/`output.value` + `*.mime_type` → `inputs`/`outputs`,
  foreign attrs → `attrs` + `unmapped-attributes` warning, `outcome` never from `status_code`).
  Envelope specifics: steps ordered by a pre-order walk of the tree (which *is* execution order for
  a single-SDK trace) with `parent_idx` from the nesting; semantic `kind` from the attribute, never
  the wire `span_kind` (always `"Internal"`); RFC3339 `timestamp` → `t_start`; the source `span_id`
  is retained in `attrs` (`otel.span_id`). TRAIL's error annotations
  (`processed_annotations_gaia/<id>.json`) are parsed *beside* the run (never merged) by
  `trail::load_annotations` into a `TrailGold` — each error's span-located `location` + taxonomy
  category + typed impact — and `TrailGold::resolve(&run)` maps each `location` span id to a step
  index via that retained `otel.span_id`; an unresolvable span id is reported, never dropped.
- **Who&When failure logs** — **implemented** (`amberfork_ingest::whowhen`): each history entry →
  one step; entry name/role → `name` with `kind=agent`; entry content → `outputs`; the dataset's
  blame annotation is returned *beside* the run as gold, never merged into it.
- **TapeAgents tapes** — **implemented** (`amberfork_ingest::tape`): each typed node → one
  `kind=agent` step whose body survives as a field-diffable object; GAIA pairing metadata is
  returned beside the run.

## Versioning

Additive changes (new optional fields) keep the version; renames, removals, or semantic changes
bump it. The parser accepts every published version and reports what it upgraded.
