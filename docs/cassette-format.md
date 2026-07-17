# Cassette format (plain JSON) — v0.1

> What `amberfork record` writes: a full-content recording of one agent run, captured at the
> provider HTTP boundary. This is the **record path**'s artifact. The passive path
> (`docs/trace-format.md`) aligns traces you already have; a cassette is what you get when
> amberfork ran the agent itself, and it carries the two things a trace usually cannot —
> guaranteed content and the ability to re-execute.
>
> This document is the public contract for the shape. The Rust types in
> `crates/amberfork-record/src/cassette.rs` are the source of truth and this file tracks them.

## Why this exists next to the trace format

They version independently and are not interchangeable:

| | trace (`docs/trace-format.md`) | cassette (this file) |
|---|---|---|
| what it is | a run as *steps* — the aligner's input | a run as *provider exchanges* — the recorder's output |
| content | opt-in, often absent | guaranteed |
| re-executable | no (a telemetry photo) | yes, which is what counterfactual attribution needs |
| version field | `schema_version` | `cassette_version` |

A cassette becomes a `Run` by normalization; the trace format stays the one seam the aligner
reads. Nothing forks the trace contract per-consumer.

## Shape

```json
{
  "cassette_version": "0.1",
  "id": "refund-triage_2026-07-17_bad",
  "exchanges": [
    {
      "idx": 0,
      "request": {
        "method": "POST",
        "path": "/v1/chat/completions",
        "headers": [["content-type", "application/json"]],
        "body": {
          "model": "claude-sonnet-5",
          "messages": [{ "role": "user", "content": "Handle refund for order 8841" }]
        }
      },
      "response": {
        "status": 200,
        "headers": [["content-type", "application/json"]],
        "body": {
          "id": "chatcmpl-…",
          "choices": [{ "message": { "role": "assistant", "content": "I'll look up the order first." } }]
        }
      }
    }
  ]
}
```

## Field semantics

| Field | Required | Meaning |
|---|---|---|
| `cassette_version` | yes | version of *this* contract; breaking changes bump it |
| `id` | yes | unique recording id (any string) |
| `exchanges[].idx` | yes | 0-based **capture order**. Concurrent agent calls are ordered by *completion* — the only order a proxy can observe. It is a record of what happened, not a causal claim |
| `exchanges[].request.method` | yes | HTTP method as sent |
| `exchanges[].request.path` | yes | path and query as sent; the upstream origin is excluded — it belongs to the session, not the exchange |
| `exchanges[].request.headers` | no | `[name, value]` pairs surviving the allowlist (see below); names lowercased |
| `exchanges[].request.body` | yes | the **full input**, parsed as JSON when it parses, otherwise verbatim text |
| `exchanges[].response.status` | yes | upstream HTTP status |
| `exchanges[].response.headers` | no | as above |
| `exchanges[].response.body` | yes | as above |

An exchange that never completed upstream is not recorded — a request that failed to reach the
provider has no round trip to describe.

## Credentials are never recorded

**Headers are captured by allowlist, not denylist**, and that is a contract promise, not an
implementation detail. A cassette is meant to be shared — committed as a fixture, attached to a
bug report, pasted into an issue — so an unrecognized header is dropped rather than written.
Providers spell credentials differently (`authorization`, `x-api-key`, `x-goog-api-key`, …) and
a denylist leaks the next scheme the day it ships; a leaked key is also the one mistake here
that no later fix undoes. Replay keys on method, path, and body, so dropping headers costs
nothing.

Kept today: `content-type` and `accept` on requests, `content-type` on responses.

Your **credential still reaches the provider** — the proxy relays it. The allowlist governs
what reaches disk, which is a different question.

What is *not* redacted: the bodies. Capturing them is the entire point, and they are your
content. A prompt containing a secret puts that secret in the cassette.

## Fidelity limits (stated, not discovered)

- **Streamed responses are buffered.** An agent using `stream: true` receives every byte and
  parses the stream correctly, but gets it in one piece at the end rather than incrementally.
  Content is faithful; arrival timing is not.
- **Non-JSON bodies are preserved as text.** A provider's HTML 502 does not fail the capture —
  that is exactly the run worth having recorded. The fidelity loss shows up as shape (a string
  body rather than an object), never as silence.

## Versioning

Additive changes (new optional fields) keep the version; renames, removals, or semantic changes
bump it. `cassette_version` moves independently of the trace format's `schema_version` — the
two contracts have two audiences and neither should announce a break in the other.
