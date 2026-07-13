//! The attribution pane: the diff's one answer, in DR5 reading order (issue #26 slice 2).
//!
//! The engine's attribution — mode, origin, propagation, confidence — is already resolved to its
//! designed wording in [`AttributionView`] (the terminal flattens the same parts to one footer
//! line). Here each part is its own element so the pane reads top-to-bottom as the reasoning
//! order a debugger wants: *how* it was attributed, *where* it started, *how far* it spread, *how
//! sure* we are. When there is nothing to attribute the pane still speaks — a converged diff has
//! no fork, and an unlocalized fork says so — so the pane is never a dead zone.
//!
//! This pane holds the diff-level answer; the per-selection red/green field diff lands inside it
//! as issue #27. No amber and no red/green live here — attribution is a statement *about* the
//! divergence, not the divergence itself.

use amberfork_layout::{AttributionView, FieldDiffView, Verdict};
use leptos::prelude::*;

use crate::content_diff::ContentDiff;

/// The right pane. Takes the diff's answer (attribution + verdict) plus the lifted `selected`
/// signal and the per-row field diffs the content-diff card reads (issue #27) — but never the
/// rows themselves: the canvas owns those, and the pane only reflects which one is selected.
#[component]
pub(crate) fn Attribution(
    attribution: Option<AttributionView>,
    verdict: Verdict,
    selected: RwSignal<Option<usize>>,
    field_diffs: Vec<Vec<FieldDiffView>>,
    bad: String,
    good: String,
) -> impl IntoView {
    let body = match attribution {
        Some(view) => attribution_rows(view),
        None => empty_state(verdict),
    };
    view! {
        <aside class="attr" aria-label="attribution">
            <h2 class="attr-title">"Attribution"</h2>
            {body}
            <ContentDiff selected=selected field_diffs=field_diffs bad=bad good=good />
        </aside>
    }
}

/// The four parts as a description list, in DR5 order. Confidence carries the fork's marginal
/// rule for free — it is the same designed string the fork tag shows.
fn attribution_rows(view: AttributionView) -> AnyView {
    view! {
        <dl class="attr-list">
            <div class="attr-row"><dt>"mode"</dt><dd>{view.mode}</dd></div>
            <div class="attr-row"><dt>"origin"</dt><dd>{view.origin}</dd></div>
            <div class="attr-row"><dt>"propagation"</dt><dd>{view.propagation}</dd></div>
            <div class="attr-row"><dt>"confidence"</dt><dd>{view.confidence}</dd></div>
        </dl>
    }
    .into_any()
}

/// The pane's answer when there is no attribution: a converged diff has no fork to attribute; a
/// forked-but-unlocalized diff says the origin escaped localization. Direct, in the interface's
/// voice — never a blank pane.
fn empty_state(verdict: Verdict) -> AnyView {
    let message = if matches!(verdict, Verdict::Forked) {
        "Fork found, but its origin couldn't be localized."
    } else {
        "The runs converged — no fork to attribute."
    };
    view! { <p class="attr-empty">{message}</p> }.into_any()
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    fn render(attribution: Option<AttributionView>, verdict: Verdict) -> String {
        let owner = Owner::new();
        owner.with(|| {
            // These attribution-pane assertions are about the answer, not the field diff, so the
            // content-diff is rendered inert: nothing selected, no per-row evidence.
            let selected = RwSignal::new(None::<usize>);
            view! {
                <Attribution
                    attribution=attribution
                    verdict=verdict
                    selected=selected
                    field_diffs=vec![]
                    bad="bad.json".to_string()
                    good="good.json".to_string()
                />
            }
            .to_html()
        })
    }

    fn attributed() -> AttributionView {
        AttributionView {
            mode: "static".to_string(),
            origin: "origin step 11".to_string(),
            propagation: "step 12".to_string(),
            confidence: "conf 0.86".to_string(),
        }
    }

    #[test]
    fn renders_the_four_parts_in_dr5_order() {
        let html = render(Some(attributed()), Verdict::Forked);
        let mode = html.find("static").expect("mode present");
        let origin = html.find("origin step 11").expect("origin present");
        let propagation = html.find("step 12").expect("propagation present");
        let confidence = html.find("conf 0.86").expect("confidence present");
        assert!(
            mode < origin && origin < propagation && propagation < confidence,
            "DR5 reading order: mode → origin → propagation → confidence: {html}"
        );
    }

    #[test]
    fn labels_each_part() {
        let html = render(Some(attributed()), Verdict::Forked);
        for label in ["mode", "origin", "propagation", "confidence"] {
            assert!(html.contains(label), "part label `{label}` present: {html}");
        }
    }

    #[test]
    fn converged_pane_states_there_is_no_fork() {
        let html = render(None, Verdict::Identical { steps: 25 });
        assert!(html.contains("converged"), "converged answer given: {html}");
        assert!(!html.contains("attr-list"), "no attribution list: {html}");
    }

    #[test]
    fn unlocalized_fork_says_so() {
        let html = render(None, Verdict::Forked);
        assert!(
            html.contains("couldn't be localized"),
            "forked-but-unattributed answer given: {html}"
        );
    }

    #[test]
    fn the_pane_is_a_landmark() {
        let html = render(Some(attributed()), Verdict::Forked);
        assert!(
            html.contains("aria-label=\"attribution\""),
            "attribution pane is a labelled landmark: {html}"
        );
    }

    #[test]
    fn no_amber_role_hooks_leak_into_the_pane() {
        // Attribution is a statement ABOUT the divergence; the amber role classes belong to the
        // canvas alone. The pane must not reuse them (red/green stays confined to #27's card).
        let html = render(Some(attributed()), Verdict::Forked);
        for forbidden in ["row--fork", "row--down", "spine-path", "fork-node"] {
            assert!(
                !html.contains(forbidden),
                "no `{forbidden}` amber hook in the attribution pane: {html}"
            );
        }
    }
}
