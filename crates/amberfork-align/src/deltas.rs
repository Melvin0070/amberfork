//! Latency / token deltas between the two runs — issue #40's first slice. Cost is deliberately
//! deferred: no adapter in this workspace attributes a dollar figure to a step yet, and
//! building one is its own design decision (a per-model price table), not this slice's job.
//!
//! Two granularities, both `b − a` (observed minus reference), matching the rest of this
//! contract's before/after convention (see [`amberfork_model::FieldDiff`]):
//! - `total`: whole-run wall-clock latency (first `t_start` to last `t_end`) and the token sum
//!   across every step's `outputs.usage`.
//! - `at_fork`: the same two measurements for just the diverging step pair, when the fork
//!   lands on a synchronous move (both [`Fork::a_step`] and [`Fork::b_step`] present) — a
//!   log/model-only fork has no counterpart step to diff against, so `at_fork` is `None` by
//!   construction, never a fabricated `0`.
//!
//! RFC3339 parsing is hand-rolled rather than a new dependency, matching this workspace's
//! standing anti-dependency bias (`amberfork-judge`'s Ollama slice: "no new mandatory
//! dependency lands in the default path"). Only `Z`-suffixed UTC timestamps are recognized;
//! anything else (an explicit offset, a malformed field) degrades to "no latency signal for
//! that step," not a parse failure — the same posture `amberfork-ingest`'s adapters already
//! take on unfamiliar input.

use amberfork_model::{Fork, Payload, ResourceDelta, ResourceDeltas, Step};
use serde_json::Value;

pub(crate) fn resource_deltas(
    reference: &[Step],
    observed: &[Step],
    fork: Option<&Fork>,
) -> Option<ResourceDeltas> {
    let total = ResourceDelta {
        latency_ms: latency_delta_ms(reference, observed),
        tokens: tokens_delta(reference, observed),
    };
    let at_fork = fork.and_then(|f| at_fork_delta(f, reference, observed));
    (total.latency_ms.is_some() || total.tokens.is_some() || at_fork.is_some())
        .then_some(ResourceDeltas { total, at_fork })
}

fn at_fork_delta(fork: &Fork, reference: &[Step], observed: &[Step]) -> Option<ResourceDelta> {
    let a = fork.a_step.and_then(|i| reference.get(i))?;
    let b = fork.b_step.and_then(|i| observed.get(i))?;
    let delta = ResourceDelta {
        latency_ms: step_duration_ms(b)
            .zip(step_duration_ms(a))
            .map(|(b, a)| b - a),
        tokens: step_tokens(b).zip(step_tokens(a)).map(|(b, a)| b - a),
    };
    (delta.latency_ms.is_some() || delta.tokens.is_some()).then_some(delta)
}

fn latency_delta_ms(reference: &[Step], observed: &[Step]) -> Option<i64> {
    Some(wall_clock_ms(observed)? - wall_clock_ms(reference)?)
}

fn wall_clock_ms(steps: &[Step]) -> Option<i64> {
    let start = steps
        .iter()
        .find_map(|s| s.t_start.as_deref().and_then(parse_rfc3339_nanos))?;
    let end = steps
        .iter()
        .rev()
        .find_map(|s| s.t_end.as_deref().and_then(parse_rfc3339_nanos))?;
    (end >= start).then(|| ((end - start) / 1_000_000) as i64)
}

fn step_duration_ms(step: &Step) -> Option<i64> {
    let start = parse_rfc3339_nanos(step.t_start.as_deref()?)?;
    let end = parse_rfc3339_nanos(step.t_end.as_deref()?)?;
    (end >= start).then(|| ((end - start) / 1_000_000) as i64)
}

fn tokens_delta(reference: &[Step], observed: &[Step]) -> Option<i64> {
    Some(total_tokens(observed)? - total_tokens(reference)?)
}

fn total_tokens(steps: &[Step]) -> Option<i64> {
    let found = steps.iter().filter_map(step_tokens);
    let mut any = false;
    let mut sum = 0i64;
    for tokens in found {
        any = true;
        sum += tokens;
    }
    any.then_some(sum)
}

/// Extract a step's token count from `outputs.usage`, the shape `amberfork-ingest`'s
/// HAL/OpenAI-family adapters already produce (`amberfork-ingest/src/hal.rs`'s
/// `OUTPUT_KEYS = ["choices", "usage"]`). Prefers `total_tokens`; falls back to
/// `prompt_tokens + completion_tokens` when only those are present.
fn step_tokens(step: &Step) -> Option<i64> {
    let Payload::Object(obj) = step.outputs.as_ref()? else {
        return None;
    };
    let usage = obj.get("usage")?.as_object()?;
    if let Some(total) = usage.get("total_tokens").and_then(Value::as_i64) {
        return Some(total);
    }
    let prompt = usage.get("prompt_tokens").and_then(Value::as_i64);
    let completion = usage.get("completion_tokens").and_then(Value::as_i64);
    match (prompt, completion) {
        (None, None) => None,
        (p, c) => Some(p.unwrap_or(0) + c.unwrap_or(0)),
    }
}

/// Parse a `Z`-suffixed RFC3339 UTC timestamp into nanoseconds since the Unix epoch. Any other
/// shape (an explicit offset, out-of-range fields, a missing `Z`) returns `None`.
fn parse_rfc3339_nanos(s: &str) -> Option<i128> {
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    let last = *bytes.last()?;
    if last != b'Z' && last != b'z' {
        return None;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || (bytes[10] != b'T' && bytes[10] != b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&min)
        || !(0..60).contains(&sec)
    {
        return None;
    }

    let frac_str = s.get(19..s.len() - 1)?;
    let nanos: i128 = if frac_str.is_empty() {
        0
    } else {
        let digits = frac_str.strip_prefix('.')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut padded = digits.to_string();
        padded.truncate(9);
        while padded.len() < 9 {
            padded.push('0');
        }
        padded.parse().ok()?
    };

    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + min * 60 + sec;
    Some(secs as i128 * 1_000_000_000 + nanos)
}

/// Days since the Unix epoch for a proleptic-Gregorian civil date. Howard Hinnant's
/// public-domain `days_from_civil` algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html>).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support;
    use serde_json::json;

    fn step_at(idx: usize, start: &str, end: &str) -> Step {
        test_support::step(idx, "call")
            .t_start(start)
            .t_end(end)
            .build()
    }

    fn step_with_usage(idx: usize, usage: Value) -> Step {
        test_support::step(idx, "call")
            .outputs(Payload::Object(
                json!({ "usage": usage }).as_object().unwrap().clone(),
            ))
            .build()
    }

    // --- parse_rfc3339_nanos / days_from_civil, cross-checked against Python's datetime ----

    #[test]
    fn epoch_is_day_zero() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn known_reference_dates_match_python_datetime() {
        assert_eq!(days_from_civil(2026, 8, 1), 20666);
        assert_eq!(days_from_civil(2000, 2, 29), 11016);
        assert_eq!(days_from_civil(1900, 3, 1), -25508);
        assert_eq!(days_from_civil(2024, 2, 29), 19782);
    }

    #[test]
    fn parses_fractional_seconds_to_nanos() {
        let base = parse_rfc3339_nanos("2026-08-01T00:00:00Z").unwrap();
        let frac = parse_rfc3339_nanos("2026-08-01T00:00:00.5Z").unwrap();
        assert_eq!(frac - base, 500_000_000);
    }

    #[test]
    fn rejects_a_non_z_offset() {
        assert_eq!(parse_rfc3339_nanos("2026-08-01T00:00:00+02:00"), None);
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_rfc3339_nanos("not-a-timestamp"), None);
    }

    // --- resource_deltas -------------------------------------------------------------------

    #[test]
    fn no_timestamps_or_usage_means_no_deltas_at_all() {
        let a = vec![test_support::step(0, "call").build()];
        let b = vec![test_support::step(0, "call").build()];
        assert!(resource_deltas(&a, &b, None).is_none());
    }

    #[test]
    fn total_latency_is_observed_minus_reference_wall_clock() {
        let a = vec![step_at(0, "2026-08-01T00:00:00Z", "2026-08-01T00:00:10Z")];
        let b = vec![step_at(0, "2026-08-01T00:00:00Z", "2026-08-01T00:00:15Z")];
        let deltas = resource_deltas(&a, &b, None).expect("both sides carry timestamps");
        assert_eq!(deltas.total.latency_ms, Some(5_000));
        assert_eq!(deltas.total.tokens, None);
        assert!(deltas.at_fork.is_none());
    }

    #[test]
    fn total_tokens_sum_across_steps_and_missing_usage_is_skipped() {
        let a = vec![
            step_with_usage(0, json!({"total_tokens": 100})),
            test_support::step(1, "no_usage").build(),
        ];
        let b = vec![step_with_usage(0, json!({"total_tokens": 130}))];
        let deltas = resource_deltas(&a, &b, None).expect("both sides carry usage");
        assert_eq!(deltas.total.tokens, Some(30));
        assert_eq!(deltas.total.latency_ms, None);
    }

    #[test]
    fn falls_back_to_prompt_plus_completion_tokens() {
        let steps = vec![step_with_usage(
            0,
            json!({"prompt_tokens": 40, "completion_tokens": 10}),
        )];
        assert_eq!(total_tokens(&steps), Some(50));
    }

    #[test]
    fn at_fork_delta_needs_both_sides_of_a_synchronous_fork() {
        let a = vec![step_with_usage(0, json!({"total_tokens": 100}))];
        let b = vec![step_with_usage(0, json!({"total_tokens": 400}))];
        let fork = Fork {
            index: 0,
            a_step: Some(0),
            b_step: Some(0),
            confidence: 1.0,
        };
        let deltas = resource_deltas(&a, &b, Some(&fork)).expect("synchronous fork step diff");
        let at_fork = deltas.at_fork.expect("both sides have a step to compare");
        assert_eq!(at_fork.tokens, Some(300));
    }

    #[test]
    fn log_only_fork_has_no_at_fork_delta() {
        let a = vec![step_with_usage(0, json!({"total_tokens": 100}))];
        let b = vec![
            step_with_usage(0, json!({"total_tokens": 100})),
            step_with_usage(1, json!({"total_tokens": 900})),
        ];
        let fork = Fork {
            index: 1,
            a_step: None,
            b_step: Some(1),
            confidence: 1.0,
        };
        assert!(
            resource_deltas(&a, &b, Some(&fork))
                .and_then(|d| d.at_fork)
                .is_none(),
            "no `a` side means no counterpart to diff against"
        );
    }
}
