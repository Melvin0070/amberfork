//! A JSON writer byte-compatible with Python's `json.dumps(obj, indent=1)` (issue #17).
//!
//! The committed fixtures under `bench/fixtures/` were written by the Python pipeline, and
//! their provenance doc promises byte-identical regeneration from the recipe. Porting the
//! GAIA sanitizer to Rust therefore means matching Python's serialization exactly — the
//! redaction can be perfect and the artifact still wrong by a byte. Three properties carry
//! the parity:
//!
//! - **indent=1 layout**: one-space indent per depth, `", "`-free item separators, empty
//!   containers collapsed to `{}` / `[]`.
//! - **`ensure_ascii` escaping**: every char outside printable ASCII (`' '..='~'`) becomes a
//!   lowercase `\uXXXX` escape, non-BMP chars as UTF-16 surrogate pairs, plus the usual short
//!   escapes (`\n`, `\t`, …). `serde_json` leaves non-ASCII raw, so this cannot be delegated.
//! - **insertion-order keys**: guaranteed by `serde_json`'s `preserve_order` feature
//!   (workspace-wide), mirroring Python dicts.
//!
//! Numbers are integers only: the corpus contains none but ints, and Python's shortest-repr
//! float formatting differs from Rust's in edge cases (`1e+22` vs `1e22`) — a float is a loud
//! [`PyJsonError::NonIntegerNumber`], never a near-miss byte stream. This module's round-trip
//! test proves parity over every committed fixture file in CI.

use serde_json::Value;
use std::fmt;

/// The one way this writer can fail: a number Python and Rust would format differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyJsonError {
    /// A non-integer number was encountered; its `serde_json` rendering is carried for the
    /// error message.
    NonIntegerNumber(String),
}

impl fmt::Display for PyJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonIntegerNumber(repr) => write!(
                f,
                "non-integer number {repr}: Python float formatting is not reproduced; \
                 the sanitizer corpus is integer-only"
            ),
        }
    }
}

impl std::error::Error for PyJsonError {}

/// Serialize `value` exactly as Python's `json.dumps(value, indent=1)` would.
pub fn to_string_indent1(value: &Value) -> Result<String, PyJsonError> {
    let mut out = String::new();
    write_value(&mut out, value, 1)?;
    Ok(out)
}

fn write_value(out: &mut String, value: &Value, depth: usize) -> Result<(), PyJsonError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            // Python prints ints as bare digits — identical to Rust. Anything else (floats,
            // and integers wide enough that serde_json parsed them as f64) is refused.
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                return Err(PyJsonError::NonIntegerNumber(n.to_string()));
            }
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                push_indent(out, depth);
                write_value(out, item, depth + 1)?;
            }
            out.push('\n');
            push_indent(out, depth - 1);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                push_indent(out, depth);
                write_string(out, key);
                out.push_str(": ");
                write_value(out, item, depth + 1)?;
            }
            out.push('\n');
            push_indent(out, depth - 1);
            out.push('}');
        }
    }
    Ok(())
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push(' ');
    }
}

/// Python's `ensure_ascii` string encoder: raw printable ASCII, short escapes where JSON has
/// them, lowercase `\uXXXX` for everything else (surrogate pairs beyond the BMP).
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let code = c as u32;
                if code <= 0xFFFF {
                    push_u_escape(out, code);
                } else {
                    // UTF-16 surrogate pair, exactly as Python emits astral-plane chars.
                    let v = code - 0x1_0000;
                    push_u_escape(out, 0xD800 + (v >> 10));
                    push_u_escape(out, 0xDC00 + (v & 0x3FF));
                }
            }
        }
    }
    out.push('"');
}

fn push_u_escape(out: &mut String, code: u32) {
    out.push_str(&format!("\\u{code:04x}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every expected string below was generated by CPython 3.12 `json.dumps(case, indent=1)`
    /// and hardcoded — the test pins parity to the reference implementation, not to our own
    /// reading of its docs.
    #[test]
    fn matches_python_reference_outputs() {
        let cases: Vec<(Value, &str)> = vec![
            (json!({"a": "café"}), "{\n \"a\": \"caf\\u00e9\"\n}"),
            (json!({"e": "🙂"}), "{\n \"e\": \"\\ud83d\\ude42\"\n}"),
            (
                json!({"c": "\u{7f}\u{01}\u{08}\u{0c}\n\r\t\"\\"}),
                "{\n \"c\": \"\\u007f\\u0001\\b\\f\\n\\r\\t\\\"\\\\\"\n}",
            ),
            (
                json!({"k": [1, -2, true, false, null]}),
                "{\n \"k\": [\n  1,\n  -2,\n  true,\n  false,\n  null\n ]\n}",
            ),
            (json!({}), "{}"),
            (json!([]), "[]"),
            (
                json!({"outer": {"inner": [{"x": 1}, []]}, "s": "a b"}),
                "{\n \"outer\": {\n  \"inner\": [\n   {\n    \"x\": 1\n   },\n   []\n  ]\n },\n \"s\": \"a b\"\n}",
            ),
            (json!("It’s — done"), "\"It\\u2019s \\u2014 done\""),
        ];
        for (value, expected) in cases {
            assert_eq!(to_string_indent1(&value).unwrap(), expected);
        }
    }

    #[test]
    fn preserves_key_insertion_order() {
        let parsed: Value =
            serde_json::from_str("{\"z\": 1, \"a\": {\"m\": 2, \"b\": 3}}").unwrap();
        assert_eq!(
            to_string_indent1(&parsed).unwrap(),
            "{\n \"z\": 1,\n \"a\": {\n  \"m\": 2,\n  \"b\": 3\n }\n}"
        );
    }

    #[test]
    fn refuses_floats() {
        let err = to_string_indent1(&json!(0.5)).unwrap_err();
        assert!(matches!(err, PyJsonError::NonIntegerNumber(_)));
    }

    /// The whole committed fixture corpus — every byte of it Python-written — must survive a
    /// parse -> serialize round trip unchanged. This is the parity proof that runs in CI
    /// without any gated data: if this writer (or `preserve_order`) drifted from
    /// `json.dumps(obj, indent=1)` in any way the corpus exercises, some file would differ.
    #[test]
    fn round_trips_every_committed_fixture_byte_for_byte() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/fixtures");
        let mut checked = 0;
        for dir in std::fs::read_dir(&root).expect("bench/fixtures must exist") {
            let dir = dir.expect("readable dir entry").path();
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&dir).expect("readable seed dir") {
                let path = entry.expect("readable entry").path();
                if path.extension().is_none_or(|ext| ext != "json") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("readable fixture");
                let value: Value = serde_json::from_str(&text).expect("parseable fixture");
                assert_eq!(
                    to_string_indent1(&value).expect("integer-only corpus"),
                    text,
                    "round trip diverged from Python bytes: {}",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(checked > 70, "expected the full corpus, checked {checked}");
    }
}
