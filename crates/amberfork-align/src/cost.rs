//! The cost-model seam: how unlike two steps are, as a number the aligner can minimize.
//!
//! [`CostModel`] is the trait every similarity signal implements; [`LexicalCost`] is the v1
//! default. Per the 2026-07-08 Amendment B (and notebook 003), the default is deliberately
//! lexical — deterministic, dependency-free, seed-stable — and any richer signal (tf-idf,
//! embeddings) competes through this same trait and must beat it on dev fixtures to replace it.
//!
//! `LexicalCost` is a token-level gestalt (Ratcliff–Obershelp) ratio, not a port of the spike's
//! character-level `difflib` call: notebook 003 measured the token variant equal-or-better on
//! every dev-fixture condition (0.75 vs 0.70 exact on the committed noise pairs) while dropping
//! `difflib`'s autojunk quirk and ~36× of the DP work. The fixtures are the contract, not
//! bit-parity with Python.

use amberfork_model::{Payload, Step};
use serde_json::Value;

/// Cap on the step text used for similarity, in characters. Long tool dumps otherwise dominate
/// both the signal and the runtime (spike constant, kept as-is).
const TEXT_CAP: usize = 600;

/// A step-vs-step alignment cost in `[0, 1]`: `0.0` = identical content, `1.0` = nothing in
/// common. Implementations must be deterministic and symmetric — the aligner's output is part
/// of the reproducibility promise, and the fork rule's `tau` thresholds compare against these
/// values directly.
///
/// The trait is split at the per-step precomputation seam: [`prepare`](CostModel::prepare)
/// digests one step into whatever form the model compares — [`LexicalCost`]'s token sequence,
/// a future tf-idf model's term vector, an embedding — and
/// [`cost_prepared`](CostModel::cost_prepared) scores two digests. The aligner prepares each
/// run once (O(n+m)) and only scores inside the O(n·m) cost matrix, so per-step work never
/// multiplies by the number of cells (issue #16).
pub trait CostModel {
    /// The per-step precomputation this model compares by.
    type Prepared;

    /// Digest one step into its comparable form.
    fn prepare(&self, step: &Step) -> Self::Prepared;

    /// Cost of aligning two prepared steps.
    fn cost_prepared(&self, a: &Self::Prepared, b: &Self::Prepared) -> f64;

    /// Cost of aligning `a` with `b`: prepare both, score once. A one-off convenience — code
    /// that compares many pairs prepares each step once and calls
    /// [`cost_prepared`](CostModel::cost_prepared).
    fn cost(&self, a: &Step, b: &Step) -> f64 {
        self.cost_prepared(&self.prepare(a), &self.prepare(b))
    }
}

/// The v1 default cost model: `1 - gestalt_ratio` over the steps' token sequences.
///
/// A step's comparable text is `"{name}: {outputs}"` capped at [`TEXT_CAP`] characters —
/// outputs only, not inputs, because outputs carry the step's observable behavior while inputs
/// largely echo the previous step's outputs (spike 001 design, kept). The text is tokenized to
/// lowercase ASCII-alphanumeric runs, the same vocabulary a future tf-idf model would use.
///
/// Its prepared form is the token sequence itself, so a full alignment tokenizes each step
/// once (O(n+m)) and the O(n·m) cells spend only the gestalt ratio (issue #16; before/after
/// curves in notebook 022/023).
#[derive(Debug, Default, Clone, Copy)]
pub struct LexicalCost;

impl CostModel for LexicalCost {
    type Prepared = Vec<String>;

    fn prepare(&self, step: &Step) -> Self::Prepared {
        tokens(&step_text(step))
    }

    fn cost_prepared(&self, a: &Self::Prepared, b: &Self::Prepared) -> f64 {
        1.0 - gestalt_ratio(a, b)
    }
}

/// The text a step is compared by: `"{name}: {outputs}"`, capped at [`TEXT_CAP`] characters.
/// Structured payloads serialize through [`sorted_json`], so the text — and every cost derived
/// from it — is deterministic regardless of a payload's key order. That property is
/// established HERE, deliberately: the workspace's `serde_json` preserves insertion order (the
/// issue-#17 byte-parity requirement), so an engine invariant must not lean on the map type.
fn step_text(step: &Step) -> String {
    let out = match &step.outputs {
        None => String::new(),
        Some(Payload::Text(s)) => s.clone(),
        Some(Payload::Object(map)) => {
            let mut buf = String::new();
            sorted_json(&mut buf, &Value::Object(map.clone()));
            buf
        }
        Some(Payload::Other(value)) => {
            let mut buf = String::new();
            sorted_json(&mut buf, value);
            buf
        }
    };
    format!("{}: {}", step.name, out)
        .chars()
        .take(TEXT_CAP)
        .collect()
}

/// Compact JSON with keys sorted at every nesting level — the canonical comparable form of a
/// structured payload. Non-object leaves render through `serde_json` (`Value`'s `Display`),
/// so string escaping and number formatting stay standard.
fn sorted_json(out: &mut String, value: &Value) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*key).clone()).to_string());
                out.push(':');
                sorted_json(out, &map[key.as_str()]);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                sorted_json(out, item);
            }
            out.push(']');
        }
        leaf => out.push_str(&leaf.to_string()),
    }
}

/// Lowercase ASCII-alphanumeric runs of `text`; everything else is a separator.
fn tokens(text: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            toks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        toks.push(current);
    }
    toks
}

/// Gestalt (Ratcliff–Obershelp) similarity of two token sequences: `2·M / (|a| + |b|)`, where
/// `M` totals the recursively-found longest matching blocks. Two empty sequences are identical
/// (`1.0`).
fn gestalt_ratio(a: &[String], b: &[String]) -> f64 {
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let mut matches = 0;
    // Explicit work list instead of recursion; block order doesn't affect the sum.
    let mut ranges = vec![(0, a.len(), 0, b.len())];
    while let Some((a_lo, a_hi, b_lo, b_hi)) = ranges.pop() {
        let (i, j, size) = longest_match(a, b, a_lo, a_hi, b_lo, b_hi);
        if size > 0 {
            matches += size;
            ranges.push((a_lo, i, b_lo, j));
            ranges.push((i + size, a_hi, j + size, b_hi));
        }
    }
    2.0 * matches as f64 / total as f64
}

/// Longest block of tokens common to `a[a_lo..a_hi]` and `b[b_lo..b_hi]`, as
/// `(start_in_a, start_in_b, len)`; ties resolve to the earliest position in `a`, then `b`.
/// One row of a longest-common-substring DP at a time, keyed by end position in `b`.
fn longest_match(
    a: &[String],
    b: &[String],
    a_lo: usize,
    a_hi: usize,
    b_lo: usize,
    b_hi: usize,
) -> (usize, usize, usize) {
    let mut best = (a_lo, b_lo, 0);
    let mut run_ending_at = vec![0usize; b_hi.saturating_sub(b_lo)];
    for (i, a_tok) in a.iter().enumerate().take(a_hi).skip(a_lo) {
        let mut next_run = vec![0usize; run_ending_at.len()];
        for (offset, run) in next_run.iter_mut().enumerate() {
            let j = b_lo + offset;
            if *a_tok != b[j] {
                continue;
            }
            let len = if offset > 0 {
                run_ending_at[offset - 1]
            } else {
                0
            } + 1;
            *run = len;
            if len > best.2 {
                best = (i + 1 - len, j + 1 - len, len);
            }
        }
        run_ending_at = next_run;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support::step;
    use serde_json::{Map, Value};

    /// Idx is pinned to 0 everywhere here: the cost model reads only `name` and content.
    fn text_step(name: &str, outputs: &str) -> Step {
        step(0, name).text_output(outputs).build()
    }

    #[test]
    fn identical_steps_cost_zero() {
        let a = text_step("web.search", "q='census 2020' -> 9 results");
        assert_eq!(LexicalCost.cost(&a, &a.clone()), 0.0);
    }

    #[test]
    fn disjoint_steps_cost_one() {
        let a = text_step("alpha", "one two three");
        let b = text_step("beta", "four five six");
        assert_eq!(LexicalCost.cost(&a, &b), 1.0);
    }

    #[test]
    fn cost_is_symmetric() {
        let a = text_step("planner", "search for census data, then verify the figure");
        let b = text_step(
            "planner",
            "verify the census figure against an official source",
        );
        assert_eq!(LexicalCost.cost(&a, &b), LexicalCost.cost(&b, &a));
    }

    #[test]
    fn known_ratio_by_hand() {
        // Tokens: [step, alpha, beta, gamma] vs [step, alpha, gamma].
        // Longest block [step, alpha] (2) + remainder [gamma] (1) -> M = 3.
        // ratio = 2*3 / (4+3) = 6/7, cost = 1/7.
        let a = text_step("step", "alpha beta gamma");
        let b = text_step("step", "alpha gamma");
        let expected = 1.0 - 6.0 / 7.0;
        assert!((LexicalCost.cost(&a, &b) - expected).abs() < 1e-12);
    }

    #[test]
    fn tokenization_folds_case_and_punctuation() {
        // Same tokens once lowercased and split on non-alphanumerics.
        let a = text_step("web.fetch", "URL=census.gov/data; Status: OK!");
        let b = text_step("web.fetch", "url census gov data status ok");
        assert_eq!(LexicalCost.cost(&a, &b), 0.0);
    }

    #[test]
    fn object_payloads_compare_deterministically() {
        // Same object regardless of insertion order — at every nesting level. The workspace's
        // serde_json preserves insertion order (issue #17), so this invariant is the cost
        // model's own sorted serialization, not the map type's.
        let mut inner1 = Map::new();
        inner1.insert("page".into(), Value::from(1));
        inner1.insert("lang".into(), Value::from("en"));
        let mut inner2 = Map::new();
        inner2.insert("lang".into(), Value::from("en"));
        inner2.insert("page".into(), Value::from(1));
        let mut m1 = Map::new();
        m1.insert("status".into(), Value::from("ok"));
        m1.insert("count".into(), Value::from(9));
        m1.insert("meta".into(), Value::Object(inner1));
        let mut m2 = Map::new();
        m2.insert("meta".into(), Value::Object(inner2));
        m2.insert("count".into(), Value::from(9));
        m2.insert("status".into(), Value::from("ok"));
        let a = step(0, "web.search").outputs(Payload::Object(m1)).build();
        let b = step(0, "web.search").outputs(Payload::Object(m2)).build();
        assert_eq!(LexicalCost.cost(&a, &b), 0.0);
    }

    #[test]
    fn text_beyond_cap_is_ignored() {
        // Identical up to the cap, wildly different after: the cap must make them equal.
        let shared = "token ".repeat(120); // 720 chars > TEXT_CAP
        let a = text_step("dump", &format!("{shared}completely different tail one"));
        let b = text_step("dump", &format!("{shared}nothing alike in this suffix"));
        assert_eq!(LexicalCost.cost(&a, &b), 0.0);
    }

    #[test]
    fn contentless_steps_compare_by_name() {
        let a = step(0, "fetch").build();
        assert_eq!(LexicalCost.cost(&a, &a.clone()), 0.0);
        let b = step(0, "q7").build();
        assert_eq!(LexicalCost.cost(&a, &b), 1.0);
    }

    #[test]
    fn non_ascii_text_is_safe() {
        let a = text_step("reader", "café population ≈ 8,443,000 ☕");
        let b = text_step("reader", "cafe population 8 443 000");
        let cost = LexicalCost.cost(&a, &b);
        assert!((0.0..=1.0).contains(&cost));
        // "café" tokenizes to "caf" (ASCII vocabulary), so it cannot fully match "cafe" —
        // but the numbers and "population" do.
        assert!(cost < 0.5);
    }
}
