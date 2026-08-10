//! Deterministic benign-noise jitter of a *reference* run (issue #45 slice B, pre-registered
//! in notebook 065).
//!
//! Consensus needs N good runs per suspect and nothing on disk carries them: the committed
//! chimera pairs ship exactly one reference each. Rather than invent a new noise model, this
//! re-applies `spike/make_pairs.py`'s existing one — the noise spike-001 showed actually breaks
//! positional alignment — to the reference side: reword a fraction of steps with token dropout,
//! plus one duplicated "(retry)" step.
//!
//! Two boundaries worth stating plainly, because they bound what any resulting number means:
//!
//! - **The constants are `make_pairs.py`'s verbatim; the RNG stream is ours.** Reproducing
//!   CPython's Mersenne Twister to match a *different* corpus step-for-step would buy nothing —
//!   these are new runs, not a regeneration of committed fixtures. What must hold is that they
//!   regenerate byte-identically here, which an in-crate splitmix64 stream (rule 5) gives.
//! - **This is our noise model, not observed agent non-determinism.** A reference jittered this
//!   way differs from its original the way we *believe* agent re-runs differ. Notebook 065
//!   carries the full caveat; it must travel with every number this module feeds.
//!
//! Jitter never touches the failing run, so `gold_step` — an index into the failing run — is
//! invariant under everything here. That is what keeps all three arms scored against identical
//! gold.

use crate::hash::{bounded, fnv1a64, splitmix64, unit};
use amberfork_model::{Payload, Run, Step};

/// Fraction of steps that get reworded. `make_pairs.py`'s `P_REWORD`.
pub const REWORD_P: f64 = 0.4;
/// Token dropout rate within a reworded step. `make_pairs.py`'s `P_DROP`.
pub const DROP_P: f64 = 0.12;
/// Duplicated "(retry)" steps inserted per variant. `make_pairs.py`'s `retries` default.
pub const RETRIES: usize = 1;
/// Shortest token count worth rewording. `make_pairs.py` leaves anything shorter alone —
/// dropping tokens from a 5-word output is mutilation, not jitter.
const MIN_TOKENS: usize = 8;

/// Base seed for the jitter stream. Arbitrary, committed, part of the frozen protocol.
const JITTER_SEED: u64 = 0x4A17_7E52;

/// Build variant `variant` of `reference`, keyed by `key`.
///
/// `key` must be unique per pair *across fixture directories* — `pair_00` exists in all three
/// committed dev seeds, and keying on the bare name would hand three different pairs the same
/// ten reference variants.
#[must_use]
pub fn jitter_reference(reference: &Run, key: &str, variant: usize) -> Run {
    let mut state = JITTER_SEED ^ fnv1a64(format!("{key}#{variant}").as_bytes());
    let mut steps = reference.steps.clone();

    // `make_pairs.py` rewords the chimera's shared *prefix* — the region before the fork, i.e.
    // the part that is supposed to be the same run. A reference has no fork, so the whole run
    // is that region.
    for step in &mut steps {
        if unit(&mut state) < REWORD_P {
            reword_step(step, &mut state);
        }
    }

    for _ in 0..RETRIES {
        if steps.len() < 2 {
            break;
        }
        // Never duplicate step 0: an agent retrying its very first move before making it is not
        // the benign non-determinism being modelled.
        let at = 1 + bounded(splitmix64(&mut state), steps.len() - 1);
        let mut dup = steps[at].clone();
        reword_step(&mut dup, &mut state);
        prefix_retry(&mut dup);
        steps.insert(at + 1, dup);
    }

    for (i, step) in steps.iter_mut().enumerate() {
        step.idx = i;
    }

    Run {
        id: format!("{}__jitter{variant:02}", reference.id),
        steps,
        ..reference.clone()
    }
}

/// Drop tokens from a step's text output. Object payloads are left alone: dropping keys from a
/// structured payload models a schema change, not a reword, and the chimera fixtures this runs
/// on carry text outputs anyway.
fn reword_step(step: &mut Step, state: &mut u64) {
    if let Some(Payload::Text(text)) = &step.outputs {
        let tokens: Vec<&str> = text.split(' ').collect();
        if tokens.len() < MIN_TOKENS {
            return;
        }
        let kept: Vec<&str> = tokens
            .iter()
            .copied()
            .filter(|_| unit(state) >= DROP_P)
            .collect();
        // An all-dropped output would be an empty step, which is a different perturbation
        // entirely — `make_pairs.py` keeps the original in that case and so do we.
        if !kept.is_empty() {
            step.outputs = Some(Payload::Text(kept.join(" ")));
        }
    }
}

fn prefix_retry(step: &mut Step) {
    if let Some(Payload::Text(text)) = &step.outputs {
        step.outputs = Some(Payload::Text(format!("(retry) {text}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Outcome, test_support};

    fn reference(id: &str) -> Run {
        let steps = (0..6)
            .map(|i| {
                test_support::step(i, "act")
                    .text_output(format!(
                        "step {i} consulted the source and recorded a population figure of 8,443,000"
                    ))
                    .build()
            })
            .collect();
        test_support::run(id, steps)
            .task("find the census figure")
            .outcome(Outcome::Pass)
            .build()
    }

    fn text(step: &Step) -> &str {
        match &step.outputs {
            Some(Payload::Text(t)) => t,
            other => panic!("fixture steps carry text outputs, got {other:?}"),
        }
    }

    #[test]
    fn the_same_key_and_variant_regenerate_byte_identically() {
        let r = reference("b_00");
        assert_eq!(
            jitter_reference(&r, "seed42/pair_00", 3),
            jitter_reference(&r, "seed42/pair_00", 3),
            "the corpus must rebuild from scratch to the same bytes (rule 5)"
        );
    }

    #[test]
    fn different_variants_of_one_reference_differ() {
        let r = reference("b_00");
        let variants: Vec<Run> = (0..10)
            .map(|i| jitter_reference(&r, "seed42/pair_00", i))
            .collect();

        assert!(
            variants.iter().any(|v| v.steps != r.steps),
            "jitter that changes nothing is not a corpus"
        );
        let distinct = variants
            .iter()
            .map(|v| format!("{:?}", v.steps))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert!(
            distinct > 5,
            "10 variants collapsed to {distinct} distinct runs — the stream is not advancing"
        );
    }

    #[test]
    fn the_same_variant_of_different_pairs_diverges() {
        let r = reference("b_00");
        assert_ne!(
            jitter_reference(&r, "seed42/pair_00", 0).steps,
            jitter_reference(&r, "seed43/pair_00", 0).steps,
            "pair_00 exists in every fixture dir; the key must namespace them apart"
        );
    }

    #[test]
    fn a_retry_step_is_inserted_and_marked() {
        let r = reference("b_00");
        let v = jitter_reference(&r, "seed42/pair_00", 0);

        assert_eq!(
            v.steps.len(),
            r.steps.len() + RETRIES,
            "one duplicated step per retry"
        );
        assert_eq!(
            v.steps
                .iter()
                .filter(|s| text(s).starts_with("(retry) "))
                .count(),
            RETRIES
        );
    }

    #[test]
    fn step_0_is_never_the_duplicated_one() {
        let r = reference("b_00");
        for variant in 0..200 {
            let v = jitter_reference(&r, "seed42/pair_00", variant);
            let at = v
                .steps
                .iter()
                .position(|s| text(s).starts_with("(retry) "))
                .expect("every variant inserts a retry");
            assert!(at >= 2, "retry landed at {at}: step 0 was duplicated");
        }
    }

    #[test]
    fn indices_stay_dense_after_insertion() {
        let r = reference("b_00");
        let v = jitter_reference(&r, "seed42/pair_00", 0);
        assert!(
            v.steps.iter().enumerate().all(|(i, s)| s.idx == i),
            "an insertion that leaves stale idx values corrupts every downstream index"
        );
    }

    #[test]
    fn rewording_only_ever_removes_tokens() {
        let r = reference("b_00");
        for variant in 0..50 {
            let v = jitter_reference(&r, "seed42/pair_00", variant);
            for step in v.steps.iter().filter(|s| !text(s).starts_with("(retry) ")) {
                let original = r
                    .steps
                    .iter()
                    .find(|o| text(o).starts_with(&format!("step {} ", step.idx.min(5))));
                if let Some(original) = original {
                    assert!(
                        text(step).split(' ').count() <= text(original).split(' ').count(),
                        "reword must be dropout, never insertion"
                    );
                }
            }
        }
    }

    #[test]
    fn short_outputs_survive_untouched() {
        let steps = vec![
            test_support::step(0, "act")
                .text_output("too short")
                .build(),
            test_support::step(1, "act")
                .text_output("also brief here")
                .build(),
        ];
        let r = test_support::run("b_short", steps)
            .outcome(Outcome::Pass)
            .build();

        for variant in 0..20 {
            let v = jitter_reference(&r, "seed42/pair_short", variant);
            let reworded: Vec<&str> = v
                .steps
                .iter()
                .map(text)
                .map(|t| t.trim_start_matches("(retry) "))
                .collect();
            assert!(
                reworded
                    .iter()
                    .all(|t| *t == "too short" || *t == "also brief here"),
                "sub-{MIN_TOKENS}-token outputs must pass through whole, got {reworded:?}"
            );
        }
    }

    #[test]
    fn the_variant_id_names_its_origin() {
        let v = jitter_reference(&reference("b_07"), "seed44/pair_07", 4);
        assert_eq!(v.id, "b_07__jitter04");
    }
}
