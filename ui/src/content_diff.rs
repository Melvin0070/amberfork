//! The content-diff pane: the selected pair's field-level `-`/`+` evidence (issue #27).
//!
//! This is the ONE surface that spends red/green (DESIGN.md containment): removed in red,
//! added in green, confined to this card and nowhere else — kept deliberately distinct from the
//! amber fork. It lives inside the attribution `<aside>`, below the diff's one answer, and reads
//! the SAME lifted `selected` signal the canvas commits (click/Enter). Every row carries its own
//! field diff now (the layout attaches it to the aligned pair, issue #27), so selecting any sync
//! pair opens its evidence — the fork by default, so the pane is never a dead zone.
//!
//! The render is a pure function of `(selected, field_diffs)`: in the host SSR string render the
//! reactive closure runs once at the initial selection, which is exactly the static contract the
//! tests pin; the live re-render when the canvas commits a new selection is browser behaviour
//! (verified in `/qa`), the same SSR-vs-live split every prior UI slice draws.

use amberfork_layout::{FieldDiffView, SlotText};
use leptos::prelude::*;

/// The pinned empty-diff line (issue #27): a selected pair whose payloads matched on the wire —
/// honest now that the layout only leaves this empty when the engine truly found no change.
const EMPTY: &str = "no field changes for this pair — payloads identical on the wire";
/// The truncation title, shared verbatim with the canvas so a cut slot reads the same everywhere.
const TRUNC_TITLE: &str = "payload truncated — full text in the terminal";

/// The content-diff card. `field_diffs` is the per-row evidence indexed to match the canvas rows,
/// so the pane resolves the selected row without reaching into the canvas.
#[component]
pub(crate) fn ContentDiff(
    selected: RwSignal<Option<usize>>,
    field_diffs: Vec<Vec<FieldDiffView>>,
) -> impl IntoView {
    let field_diffs = StoredValue::new(field_diffs);
    move || {
        // Nothing selectable (a converged diff has no fork) — the attribution empty-state already
        // speaks, so the pane shows no card at all rather than an out-of-context empty line.
        let Some(i) = selected.get() else {
            return ().into_any();
        };
        let diffs = field_diffs.with_value(|all| all.get(i).cloned().unwrap_or_default());
        if diffs.is_empty() {
            return view! { <p class="content-diff-empty">{EMPTY}</p> }.into_any();
        }
        let fields: Vec<AnyView> = diffs.iter().map(field_view).collect();
        view! {
            <section class="content-diff" aria-label="field diff">
                {fields}
            </section>
        }
        .into_any()
    }
}

/// One field's `-`/`+` block: the JSON path, then the removed and/or added value. An added field
/// has no removed side and a removed field no added side, so each line is conditional.
fn field_view(fd: &FieldDiffView) -> AnyView {
    let path = fd.path.clone();
    let removed = fd
        .removed
        .as_ref()
        .map(|slot| line_view('-', "content-diff-del", "removed", slot));
    let added = fd
        .added
        .as_ref()
        .map(|slot| line_view('+', "content-diff-add", "added", slot));
    view! {
        <div class="content-diff-field">
            <span class="content-diff-path">{path}</span>
            {removed}
            {added}
        </div>
    }
    .into_any()
}

/// One diff line. The `-`/`+` sign is the grayscale-safe, colorblind-safe cue (color is never the
/// only signal — DESIGN.md), decorative to assistive tech; the line's accessible name carries the
/// side in words so a screen reader hears "removed …"/"added …" without relying on the color. A
/// slot the envelope cut keeps its honest `…` mark — a silently shortened payload reads as the
/// payload.
fn line_view(sign: char, class: &'static str, label: &str, slot: &SlotText) -> AnyView {
    let text = slot.text.clone();
    let aria = format!("{label} {}", slot.text);
    let trunc = slot.truncated.then(|| {
        view! { <span class="slot-trunc" title=TRUNC_TITLE>"…"</span> }
    });
    view! {
        <div class=class aria-label=aria>
            <span class="content-diff-sign" aria-hidden="true">{sign}</span>
            <span class="content-diff-val">{text}{trunc}</span>
        </div>
    }
    .into_any()
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    /// Render the pane in isolation at a chosen selection — this is how the "select a sync pair"
    /// behaviour is pinned without a browser: the SSR string render reads the signal's initial
    /// value, so presetting it exercises any row's diff (issue #26 D16).
    fn render(selected: Option<usize>, field_diffs: Vec<Vec<FieldDiffView>>) -> String {
        let owner = Owner::new();
        owner.with(|| {
            let selected = RwSignal::new(selected);
            view! { <ContentDiff selected=selected field_diffs=field_diffs /> }.to_html()
        })
    }

    fn changed(path: &str, removed: &str, added: &str) -> FieldDiffView {
        FieldDiffView {
            path: path.to_string(),
            removed: Some(SlotText::new(removed)),
            added: Some(SlotText::new(added)),
        }
    }

    #[test]
    fn renders_the_selected_pairs_red_green_evidence() {
        let rows = vec![
            vec![],
            vec![changed("outputs.city", "\"Austin\"", "\"Dallas\"")],
        ];
        let html = render(Some(1), rows);

        // Both sides render, each in its own class (the stylesheet keys red/green to these).
        assert!(
            html.contains("content-diff-del"),
            "removed line present: {html}"
        );
        assert!(
            html.contains("content-diff-add"),
            "added line present: {html}"
        );
        // The `-`/`+` signs are the non-color redundancy cue — one per side — and each line's
        // accessible name says the side in words, so color is never the only signal (DESIGN.md).
        assert_eq!(
            html.matches("content-diff-sign").count(),
            2,
            "both sign glyphs render: {html}"
        );
        assert!(
            html.contains("aria-label=\"removed") && html.contains("aria-label=\"added"),
            "each side is named for a screen reader: {html}"
        );
        // The path and both values are real selectable text.
        assert!(html.contains("outputs.city"), "field path present: {html}");
        assert!(
            html.contains("Austin") && html.contains("Dallas"),
            "values present: {html}"
        );
    }

    #[test]
    fn selecting_a_pair_with_no_change_shows_the_pinned_empty_line() {
        // A sync pair the engine found identical: the pane speaks the pinned line, never blank.
        let html = render(Some(0), vec![vec![], vec![changed("outputs", "a", "b")]]);
        assert!(
            html.contains("no field changes for this pair — payloads identical on the wire"),
            "the pinned empty-diff line renders: {html}"
        );
        assert!(
            !html.contains("content-diff-del") && !html.contains("content-diff-add"),
            "no red/green when the pair is identical: {html}"
        );
    }

    #[test]
    fn nothing_selected_shows_no_card() {
        // A converged diff selects nothing; the attribution empty-state carries the pane, so the
        // content-diff renders neither a card nor an out-of-context empty line.
        let html = render(None, vec![vec![changed("outputs", "a", "b")]]);
        assert!(
            !html.contains("content-diff"),
            "no card when nothing selected: {html}"
        );
        assert!(
            !html.contains("payloads identical"),
            "no empty line when nothing selected: {html}"
        );
    }

    #[test]
    fn a_truncated_value_keeps_its_honest_mark() {
        let mut fd = changed("outputs", "kept", "cut");
        fd.added.as_mut().unwrap().truncated = true;
        let html = render(Some(0), vec![vec![fd]]);
        assert!(
            html.contains("slot-trunc"),
            "the cut slot keeps its mark: {html}"
        );
    }

    #[test]
    fn a_one_sided_field_renders_only_that_side() {
        // An added field has no removed side (and vice versa); the missing side is simply absent.
        let added_only = FieldDiffView {
            path: "outputs.retry".to_string(),
            removed: None,
            added: Some(SlotText::new("\"true\"")),
        };
        let html = render(Some(0), vec![vec![added_only]]);
        assert!(
            html.contains("content-diff-add"),
            "added side renders: {html}"
        );
        assert!(
            !html.contains("content-diff-del"),
            "no removed side: {html}"
        );
    }
}
