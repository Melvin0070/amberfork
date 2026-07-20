//! Tool-call-ID canonicalization — the normalization the matcher runs before comparing bodies.
//!
//! Replay matches a re-issued request to the tape on its body. But an agent SDK mints a **fresh
//! tool-call ID** on every run — OpenAI `call_…`, Anthropic `toolu_…` — and those IDs ride along
//! in the request body: the assistant turn *declares* one (`id`), and the tool-result turn that
//! follows *references* it (`tool_call_id` / `tool_use_id`). Compare bodies raw and every turn
//! after the first tool call becomes a spurious miss, because one opaque, nondeterministic string
//! changed. This module rewrites those IDs to positional tokens so a re-issued call still matches
//! its recorded counterpart, while preserving the *linkage* — which result answers which call.
//!
//! ## What counts as a tool-call ID (provider-agnostic, and deliberately narrow)
//!
//! - A value under a **reference key** — `tool_call_id`, `tool_use_id`, `call_id` — is always a
//!   tool-call ID; those keys exist for nothing else.
//! - A value under **`id`** is a tool-call ID *only inside a tool-call object* — one the provider
//!   stamped `"type": "tool_use"` (Anthropic) or `"type": "function"` (OpenAI). A bare `id` on any
//!   other object (say a record id inside a tool's arguments) is left untouched, so a genuine data
//!   difference there still registers as a real divergence rather than being normalized away. That
//!   narrowness is the whole reason to prefer this over blanket-normalizing every `id`.
//!
//! Each distinct ID is mapped to a token by **first appearance** in document order, so two bodies
//! that are structurally identical modulo their fresh IDs canonicalize to the same value.
//!
//! ## Stated limits
//!
//! Only tool-call *linkage* IDs are normalized. Other fresh-per-run opaque IDs some providers put
//! in the body (e.g. an OpenAI Responses item `id` like `fc_…`, which no reference key points at)
//! are not — matching those would need provider-specific shape knowledge this crate avoids, and
//! the counterfactual oracle already tolerates the residual miss by consensus. The token sentinel
//! could in principle collide with a real ID that happens to equal it; the odds are astronomical
//! and a collision would at worst cost one exchange its match, which again degrades, never lies.

use std::collections::HashMap;

use amberfork_record::Body;
use serde_json::{Map, Value};

/// Prefix of the synthetic token a tool-call ID is rewritten to. Distinctive enough that a real
/// provider ID will not equal it (see the collision note in the module docs).
const TOKEN_PREFIX: &str = "__amberfork_tool_call_";

/// Canonicalize a request body for matching: JSON bodies have their tool-call IDs normalized; a
/// non-JSON body carries none and is compared verbatim.
pub(crate) fn canonicalize_body(body: &Body) -> Body {
    match body {
        Body::Json(value) => Body::Json(canonicalize_tool_call_ids(value)),
        Body::Text(text) => Body::Text(text.clone()),
    }
}

/// Rewrite every tool-call ID in `value` to a first-appearance-ordered token.
pub(crate) fn canonicalize_tool_call_ids(value: &Value) -> Value {
    let mut tokens = IdTokens::default();
    rewrite(value, &mut tokens)
}

/// Whether `key` is a reference to a tool call — a key whose value is unconditionally a tool-call
/// ID across the providers amberfork sees.
fn is_reference_key(key: &str) -> bool {
    matches!(key, "tool_call_id" | "tool_use_id" | "call_id")
}

/// Whether `map` is a tool-call/tool-use object — the site that *declares* an ID under `id`.
/// Recognized by the `type` marker both dominant providers stamp on it; a bare `id` on anything
/// else is not a tool-call ID.
fn declares_tool_call_id(map: &Map<String, Value>) -> bool {
    matches!(
        map.get("type").and_then(Value::as_str),
        Some("tool_use" | "function")
    )
}

/// Recursively rebuild `value`, replacing tool-call IDs with their canonical tokens.
fn rewrite(value: &Value, tokens: &mut IdTokens) -> Value {
    match value {
        Value::Object(map) => {
            let declares = declares_tool_call_id(map);
            let mut out = Map::new();
            for (key, child) in map {
                let rewritten = match child {
                    Value::String(id) if is_reference_key(key) || (key == "id" && declares) => {
                        Value::String(tokens.token_for(id))
                    }
                    _ => rewrite(child, tokens),
                };
                out.insert(key.clone(), rewritten);
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| rewrite(item, tokens)).collect())
        }
        scalar => scalar.clone(),
    }
}

/// Assigns each distinct tool-call ID a stable token by first appearance, so the same underlying
/// call — even under a fresh provider ID on a re-run — always maps to the same token.
#[derive(Default)]
struct IdTokens {
    assigned: HashMap<String, usize>,
}

impl IdTokens {
    fn token_for(&mut self, id: &str) -> String {
        let next = self.assigned.len();
        let index = *self.assigned.entry(id.to_owned()).or_insert(next);
        format!("{TOKEN_PREFIX}{index}__")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_body_without_tool_ids_is_unchanged() {
        // Canonicalization must be the identity on a body with nothing to normalize, or it would
        // perturb ordinary matching.
        let body =
            json!({"model": "claude-sonnet-5", "messages": [{"role": "user", "content": "hi"}]});
        assert_eq!(canonicalize_tool_call_ids(&body), body);
    }

    #[test]
    fn fresh_openai_tool_call_ids_canonicalize_to_the_same_value() {
        // Same turn, re-run: the SDK minted a fresh `call_…` for the assistant's tool call and the
        // tool result that answers it. Declaration (`id`, inside a `type:"function"` object) and
        // reference (`tool_call_id`) both normalize, and the two bodies become equal.
        let recorded = json!({
            "messages": [
                {"role": "assistant", "tool_calls": [
                    {"id": "call_ABC", "type": "function",
                     "function": {"name": "search", "arguments": "{}"}}
                ]},
                {"role": "tool", "tool_call_id": "call_ABC", "content": "result"}
            ]
        });
        let reissued = json!({
            "messages": [
                {"role": "assistant", "tool_calls": [
                    {"id": "call_XYZ", "type": "function",
                     "function": {"name": "search", "arguments": "{}"}}
                ]},
                {"role": "tool", "tool_call_id": "call_XYZ", "content": "result"}
            ]
        });

        assert_eq!(
            canonicalize_tool_call_ids(&recorded),
            canonicalize_tool_call_ids(&reissued)
        );
    }

    #[test]
    fn anthropic_linkage_normalizes_but_a_real_argument_difference_still_differs() {
        // Anthropic shape (`type:"tool_use"` / `tool_use_id`), and the precision guard for
        // approach A: the fresh `toolu_…` is normalized, but the `id` *inside the tool input* is
        // not — so a genuine change there (record-1 → record-2) still reads as a divergence
        // instead of being masked. Blanket-normalizing every `id` would wrongly call these equal.
        let recorded = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_A", "name": "lookup", "input": {"id": "record-1"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_A", "content": "ok"}
                ]}
            ]
        });
        let same_call_same_arg = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_Z", "name": "lookup", "input": {"id": "record-1"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_Z", "content": "ok"}
                ]}
            ]
        });
        let same_call_different_arg = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_Z", "name": "lookup", "input": {"id": "record-2"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_Z", "content": "ok"}
                ]}
            ]
        });

        assert_eq!(
            canonicalize_tool_call_ids(&recorded),
            canonicalize_tool_call_ids(&same_call_same_arg),
            "only the fresh tool-call IDs differed — should match"
        );
        assert_ne!(
            canonicalize_tool_call_ids(&recorded),
            canonicalize_tool_call_ids(&same_call_different_arg),
            "a real change to a non-tool-call `id` must not be normalized away"
        );
    }
}
