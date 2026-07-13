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

use std::fmt::Write as _;

use amberfork_layout::{FieldDiffView, SlotText};
use leptos::prelude::*;

/// The pinned empty-diff line (issue #27): a selected pair whose payloads matched on the wire —
/// honest now that the layout only leaves this empty when the engine truly found no change.
const EMPTY: &str = "no field changes for this pair — payloads identical on the wire";
/// The truncation title, shared verbatim with the canvas so a cut slot reads the same everywhere.
const TRUNC_TITLE: &str = "payload truncated — full text in the terminal";

/// The copied evidence for a selected pair: its field diff in the grayscale-safe terminal
/// unified `-`/`+` format, then the repro command so pasted evidence is re-runnable (DESIGN.md
/// evidence-out, 2026-07-12). Pure — the clipboard write is the browser edge, this string is not.
/// The `-`/`+` lines mirror the terminal painter's fork block verbatim (`- path: value`); a slot
/// the envelope cut keeps its honest `…` so the paste never reads a shortened payload as whole.
fn copy_text(diffs: &[FieldDiffView], bad: &str, good: &str) -> String {
    let mut out = String::new();
    for fd in diffs {
        if let Some(removed) = &fd.removed {
            let _ = writeln!(out, "- {}: {}", fd.path, slot_display(removed));
        }
        if let Some(added) = &fd.added {
            let _ = writeln!(out, "+ {}: {}", fd.path, slot_display(added));
        }
    }
    // The repro command uses the CLI's canonical form with the real run names (observed first,
    // `--against` the reference) — the same evidence-out convention as the disconnect banner.
    let _ = write!(out, "\namberfork diff {bad} --against {good}");
    out
}

/// A slot's value for the copy, keeping the honest cut mark on a truncated payload (D17).
fn slot_display(slot: &SlotText) -> String {
    if slot.truncated {
        format!("{}…", slot.text)
    } else {
        slot.text.clone()
    }
}

/// The content-diff card. `field_diffs` is the per-row evidence indexed to match the canvas rows,
/// so the pane resolves the selected row without reaching into the canvas; `bad`/`good` are the
/// run names the copy affordance bakes into its repro command (issue #27 evidence-out).
#[component]
pub(crate) fn ContentDiff(
    selected: RwSignal<Option<usize>>,
    field_diffs: Vec<Vec<FieldDiffView>>,
    bad: String,
    good: String,
) -> impl IntoView {
    let field_diffs = StoredValue::new(field_diffs);
    let names = StoredValue::new((bad, good));
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
        let copy = names.with_value(|(bad, good)| copy_text(&diffs, bad, good));
        let fields: Vec<AnyView> = diffs.iter().map(field_view).collect();
        view! {
            <section class="content-diff" aria-label="field diff">
                <CopyButton copy=copy />
                {fields}
            </section>
        }
        .into_any()
    }
}

/// The copy affordance on the card: one click puts [`copy_text`]'s evidence on the clipboard.
/// The button and its label are pure markup an SSR test pins; the clipboard write and the reset
/// timer are the browser edges, csr-gated out of the host build (the same pure-render/impure-edge
/// split every prior UI slice draws). The label confirms the copy for ~1.5s, then reverts —
/// neutral, never the diff's red/green, so the pane's one scarce pair of hues stays evidence-only.
#[component]
fn CopyButton(copy: String) -> impl IntoView {
    let copied = RwSignal::new(false);
    let label = move || if copied.get() { "Copied ✓" } else { "Copy" };
    let on_click = move |_| {
        write_to_clipboard(&copy);
        copied.set(true);
        schedule_copied_reset(copied);
    };
    view! {
        <button
            type="button"
            class="content-diff-copy"
            title="copy the field diff as terminal-format evidence, with the repro command"
            on:click=on_click
        >
            {label}
        </button>
    }
}

/// Place the evidence on the clipboard — the one browser edge here, so under the `ssr` host
/// build (where the copy text is asserted directly) it compiles to a no-op. `write_text` returns
/// a promise we intentionally drop: the browser performs the write, and a copy button needs no
/// completion handshake.
#[cfg(feature = "csr")]
fn write_to_clipboard(text: &str) {
    let _ = window().navigator().clipboard().write_text(text);
}

#[cfg(not(feature = "csr"))]
fn write_to_clipboard(_text: &str) {}

/// Revert the "Copied ✓" confirmation after a short beat. Browser-only (a real timer), so it is
/// csr-gated; under `ssr` the confirmation never fires, so there is nothing to reset.
#[cfg(feature = "csr")]
fn schedule_copied_reset(copied: RwSignal<bool>) {
    set_timeout(
        move || copied.set(false),
        std::time::Duration::from_millis(1500),
    );
}

#[cfg(not(feature = "csr"))]
fn schedule_copied_reset(_copied: RwSignal<bool>) {}

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
            view! {
                <ContentDiff
                    selected=selected
                    field_diffs=field_diffs
                    bad="bad.json".to_string()
                    good="good.json".to_string()
                />
            }
            .to_html()
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
    fn copy_text_is_the_unified_diff_with_the_repro_command_appended() {
        // The evidence a debugger pastes: the terminal `-`/`+` format, a blank line, then the
        // re-runnable command with the real run names (observed first, `--against` the reference).
        let diffs = vec![
            changed("outputs.city", "\"Austin\"", "\"Dallas\""),
            changed("outputs.temp", "72", "55"),
        ];
        let text = copy_text(&diffs, "bad.json", "good.json");
        assert_eq!(
            text,
            "- outputs.city: \"Austin\"\n+ outputs.city: \"Dallas\"\n\
             - outputs.temp: 72\n+ outputs.temp: 55\n\
             \n\
             amberfork diff bad.json --against good.json"
        );
    }

    #[test]
    fn copy_text_renders_only_the_present_side_of_a_one_sided_field() {
        // An added-only field has no `-` line (and a removed-only field no `+`), matching the
        // terminal painter — the copy never invents a side the engine didn't emit.
        let added_only = FieldDiffView {
            path: "outputs.retry".to_string(),
            removed: None,
            added: Some(SlotText::new("\"true\"")),
        };
        let text = copy_text(&[added_only], "bad.json", "good.json");
        assert_eq!(
            text,
            "+ outputs.retry: \"true\"\n\namberfork diff bad.json --against good.json"
        );
    }

    #[test]
    fn copy_text_keeps_the_honest_truncation_mark() {
        // A slot the envelope cut carries its `…` into the paste, so the evidence never reads a
        // shortened payload as the whole payload (D17).
        let mut fd = changed("outputs.body", "kept", "cut");
        fd.removed.as_mut().unwrap().truncated = true;
        let text = copy_text(&[fd], "bad.json", "good.json");
        assert!(
            text.contains("- outputs.body: kept…"),
            "the cut slot keeps its mark in the copy: {text}"
        );
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
    fn a_card_with_a_diff_carries_the_copy_button() {
        // The evidence-out affordance: a real diff renders a copy button, defaulting to the
        // "Copy" label (the "Copied ✓" confirmation is the browser transition, verified in /qa).
        let html = render(
            Some(0),
            vec![vec![changed("outputs.city", "\"a\"", "\"b\"")]],
        );
        assert!(
            html.contains("content-diff-copy"),
            "the copy button renders on a card with a diff: {html}"
        );
        assert!(
            html.contains(">Copy</button>") || html.contains(">Copy<"),
            "it opens on the `Copy` label, not the confirmation: {html}"
        );
    }

    #[test]
    fn the_empty_line_and_the_no_selection_state_carry_no_copy_button() {
        // Nothing to copy when the pair matched (the pinned empty line) or nothing is selected —
        // the affordance appears only where there is evidence to hand out.
        let empty = render(Some(0), vec![vec![]]);
        assert!(
            !empty.contains("content-diff-copy"),
            "no copy button on the pinned empty line: {empty}"
        );
        let none = render(None, vec![vec![changed("outputs", "a", "b")]]);
        assert!(
            !none.contains("content-diff-copy"),
            "no copy button when nothing is selected: {none}"
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
