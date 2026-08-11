//! Reading a judge's answer back (issue #46, registered in notebook 069).
//!
//! The registered contract, verbatim: take the **last** `{...}` JSON object in the response,
//! parse it, and read `step` (single, paired) or `decisive` (stepwise). Anything else — no
//! object, unparseable, wrong type, out of range — is a **parse failure**, and a parse failure
//! is *scored as a miss*, never as an exclusion. A judge that cannot obey its own output
//! contract is worse at the task, not un-evaluable, and quietly dropping those cases would
//! inflate a baseline over a shrunken denominator (protocol rule 4).
//!
//! Strictly the *last* object, not the last one that happens to parse. Walking backwards until
//! something works would silently rescue a model that emitted two contradictory answers, which
//! is exactly the behaviour the frozen prompt's "final line, nothing after it" instruction is
//! there to test.

use std::fmt;

/// Why an answer could not be read. Each variant is a miss, and each is counted separately so
/// a published table can say *how* a judge failed rather than only that it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFailure {
    /// No `{...}` object anywhere in the response.
    NoJsonObject,
    /// The last object is not valid JSON.
    Malformed(String),
    /// Valid JSON, but the contracted field is absent.
    MissingField(&'static str),
    /// The field is present with the wrong JSON type — a float, a string, a null.
    WrongType(&'static str),
    /// A step index the failing run does not have. Registered as a parse failure rather than
    /// a wrong answer: the judge did not name a step, it named a non-step.
    OutOfRange { step: u64, len: usize },
}

impl fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoJsonObject => f.write_str("no JSON object in the response"),
            Self::Malformed(msg) => write!(f, "last JSON object is malformed: {msg}"),
            Self::MissingField(field) => write!(f, "last JSON object has no `{field}` field"),
            Self::WrongType(field) => write!(f, "`{field}` is not the contracted type"),
            Self::OutOfRange { step, len } => {
                write!(f, "step {step} is outside the failing run's {len} steps")
            }
        }
    }
}

impl std::error::Error for ParseFailure {}

/// `judge-single` / `judge-paired`: the predicted failing-run step.
///
/// # Errors
/// [`ParseFailure`] describing which clause of the output contract the response broke.
pub fn parse_step(response: &str, n_steps: usize) -> Result<usize, ParseFailure> {
    let value = last_object(response)?;
    let step = value
        .get("step")
        .ok_or(ParseFailure::MissingField("step"))?;
    // `as_u64` rejects floats and strings, which is the point: `{"step": 3.0}` and
    // `{"step": "3"}` are contract breaks, not answers to be rounded or coerced into one.
    let step = step.as_u64().ok_or(ParseFailure::WrongType("step"))?;
    if usize::try_from(step).is_ok_and(|step| step < n_steps) {
        Ok(usize::try_from(step).expect("checked above"))
    } else {
        Err(ParseFailure::OutOfRange { step, len: n_steps })
    }
}

/// `judge-stepwise`: whether the candidate step is the decisive error.
///
/// # Errors
/// [`ParseFailure`] describing which clause of the output contract the response broke.
pub fn parse_decisive(response: &str) -> Result<bool, ParseFailure> {
    let value = last_object(response)?;
    value
        .get("decisive")
        .ok_or(ParseFailure::MissingField("decisive"))?
        .as_bool()
        .ok_or(ParseFailure::WrongType("decisive"))
}

fn last_object(response: &str) -> Result<serde_json::Value, ParseFailure> {
    let span = last_object_span(response).ok_or(ParseFailure::NoJsonObject)?;
    serde_json::from_str(span).map_err(|err| ParseFailure::Malformed(err.to_string()))
}

/// The last top-level `{...}` span in `text`.
///
/// String-aware: a brace inside a JSON string (or inside the model's prose) must not open or
/// close a span, or `{"why": "it said {done}"}` would be cut in half and scored as malformed.
fn last_object_span(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut start = None;
    let mut last = None;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' if depth > 0 => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(open) = start.take()
                {
                    last = Some(&text[open..=idx]);
                }
            }
            _ => {}
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_step_from_a_well_formed_answer() {
        let response = "The run goes wrong when it schedules a calibration pass for a restock \
                        task.\n{\"step\": 4}";
        assert_eq!(parse_step(response, 10), Ok(4));
    }

    #[test]
    fn extra_keys_are_tolerated() {
        // The contract fixes what must be present, not what must be absent — a model that
        // adds its reasoning inside the object has obeyed the instruction that matters.
        let response = "{\"step\": 2, \"why\": \"wrong tool\"}";
        assert_eq!(parse_step(response, 10), Ok(2));
    }

    #[test]
    fn the_last_object_wins_not_the_first() {
        // A model that "thinks out loud" in JSON and then answers must be scored on its
        // answer; walking backwards for the first *parseable* object would instead rescue a
        // model that contradicted itself.
        let response = "{\"step\": 1}\nOn reflection:\n{\"step\": 7}";
        assert_eq!(parse_step(response, 10), Ok(7));
    }

    #[test]
    fn a_brace_inside_a_string_does_not_split_the_object() {
        let response = "{\"why\": \"it printed {done} then stopped\", \"step\": 3}";
        assert_eq!(parse_step(response, 10), Ok(3));
    }

    #[test]
    fn an_escaped_quote_inside_a_string_does_not_end_it() {
        let response = r#"{"why": "it said \"done\" {here}", "step": 5}"#;
        assert_eq!(parse_step(response, 10), Ok(5));
    }

    #[test]
    fn a_nested_object_closes_at_the_outer_brace() {
        let response = "{\"meta\": {\"conf\": 1}, \"step\": 6}";
        assert_eq!(parse_step(response, 10), Ok(6));
    }

    #[test]
    fn prose_with_no_json_is_a_parse_failure() {
        let response = "I think the fourth step is where it goes wrong.";
        assert_eq!(parse_step(response, 10), Err(ParseFailure::NoJsonObject));
    }

    #[test]
    fn a_malformed_last_object_is_a_parse_failure_not_a_fallback_to_an_earlier_one() {
        let response = "{\"step\": 2}\nfinal: {\"step\": }";
        assert!(
            matches!(parse_step(response, 10), Err(ParseFailure::Malformed(_))),
            "the last object is the answer, broken or not"
        );
    }

    #[test]
    fn a_missing_field_names_the_field() {
        let response = "{\"answer\": 4}";
        assert_eq!(
            parse_step(response, 10),
            Err(ParseFailure::MissingField("step"))
        );
    }

    #[test]
    fn a_float_or_string_step_is_the_wrong_type() {
        assert_eq!(
            parse_step("{\"step\": 3.0}", 10),
            Err(ParseFailure::WrongType("step"))
        );
        assert_eq!(
            parse_step("{\"step\": \"3\"}", 10),
            Err(ParseFailure::WrongType("step"))
        );
        assert_eq!(
            parse_step("{\"step\": null}", 10),
            Err(ParseFailure::WrongType("step"))
        );
    }

    #[test]
    fn a_step_the_run_does_not_have_is_out_of_range() {
        // Off-by-one at the boundary: the last valid index is len - 1.
        assert_eq!(parse_step("{\"step\": 9}", 10), Ok(9));
        assert_eq!(
            parse_step("{\"step\": 10}", 10),
            Err(ParseFailure::OutOfRange { step: 10, len: 10 })
        );
    }

    #[test]
    fn an_enormous_step_does_not_overflow_on_the_way_to_being_rejected() {
        assert_eq!(
            parse_step("{\"step\": 18446744073709551615}", 10),
            Err(ParseFailure::OutOfRange {
                step: u64::MAX,
                len: 10
            })
        );
    }

    #[test]
    fn reads_the_stepwise_verdict() {
        assert_eq!(parse_decisive("nope\n{\"decisive\": false}"), Ok(false));
        assert_eq!(parse_decisive("{\"decisive\": true}"), Ok(true));
    }

    #[test]
    fn a_stringy_verdict_is_the_wrong_type() {
        // "true" is not true: a model that quotes its boolean has broken the contract, and
        // coercing it here would hide that from the parse-failure count.
        assert_eq!(
            parse_decisive("{\"decisive\": \"true\"}"),
            Err(ParseFailure::WrongType("decisive"))
        );
    }
}
