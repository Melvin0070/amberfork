//! `--html`: a self-contained static export of the fork view (issue #29) — the same information
//! the terminal renders, laid out with the web painter's real CSS classes so it looks at home
//! pasted into a GitHub issue or opened cold from a CI artifact.
//!
//! A third, deliberately simple painter over the shared `ViewModel` seam (`amberfork-layout`'s
//! crate doc names "one seam, two painters" — this is a low-maintenance third consumer of that
//! same seam, not a clone of the wasm SPA): no interactivity, no SVG spine geometry, just the
//! rows, the fork's field-diff evidence, attribution, and deltas as plain HTML strings. CSS is
//! `include_str!`'d straight from `ui/index.html` — the same source the live view ships, never
//! hand-copied — so the export can't visually drift from it. Founder's call, asked before
//! building: hand-roll this rather than depend on `amberfork-ui`'s real Leptos component tree,
//! keeping `leptos` and its ~40-crate dependency tree out of the shipped `amberfork` binary.
//!
//! Non-interactive by construction, not by disabling anything: rows carry no `tabindex`/
//! `role="option"`/click handlers (there is no JS at all in the output), and there is no Copy
//! button (it would need bespoke inline JS to do anything, which is out of scope). A static-note
//! banner says so explicitly rather than leaving a visitor to wonder why nothing responds.

use amberfork_layout::{
    AlignedStep, AttributionView, DeltasView, Document, FieldDiffView, ForkRow, Row, RowRole,
    SlotText, StepView, Verdict, ViewModel, kind_label,
};
use std::fmt::Write as _;

/// The `<style>` block `ui/index.html` ships to the browser, embedded at compile time so the
/// export can never carry a hand-copied, driftable copy of the live view's CSS.
const UI_INDEX_HTML: &str = include_str!("../../../ui/index.html");

const STATIC_NOTE: &str = "Static export — rows aren't clickable and there's no Copy button \
    here; run `amberfork serve` against these two runs for the live view.";

/// The pinned empty-diff line, matching `ui/src/content_diff.rs`'s wording verbatim so the two
/// surfaces never say this differently.
const EMPTY_FIELD_DIFF: &str = "no field changes for this pair — payloads identical on the wire";

/// The truncation title. Deliberately its own wording, not shared with `ui/`'s live view
/// (issue #30 gave that one a click-to-expand affordance backed by a real server; this export
/// has no server and no JS at all — "click to load" would be a fake promise here, so this
/// stays "full text in the terminal", the one place that's actually true from a static file).
const TRUNC_TITLE: &str = "payload truncated — full text in the terminal";

/// Render a document to one self-contained HTML page: no external `<link>`/`<script src>`, no
/// webfont/CDN dependency (the design system is system-stack fonts and inline color tokens
/// only — verified true at #29 scoping time) — safe to open from disk with no network.
pub fn render(document: &Document) -> String {
    let view = &document.view;
    let mut body = String::new();
    write_header(&mut body, view, &document.schema_version);
    body.push_str("<div class=\"body\">");
    write_canvas(&mut body, view);
    write_attribution_pane(&mut body, view);
    body.push_str("</div>");

    let title = format!(
        "{} vs {} — amberfork",
        view.run_b.id.as_str(),
        view.run_a.id.as_str()
    );
    page_shell(&title, &body)
}

fn page_shell(title: &str, body: &str) -> String {
    let style = extract_style(UI_INDEX_HTML);
    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n\
         <style>{style}\n\
         .static-note {{ padding: 6px 16px; border-bottom: 1px solid var(--hair); \
         background: var(--surface); color: var(--muted); font-family: var(--mono); \
         font-size: 0.6875rem; }}\n\
         </style>\n\
         </head>\n\
         <body>\n\
         <p class=\"static-note\">{note}</p>\n\
         {body}\n\
         </body>\n\
         </html>\n",
        title = escape_html(title),
        note = escape_html(STATIC_NOTE),
    )
}

/// The `<style>...</style>` block's inner text, out of the real page the browser ships.
fn extract_style(index_html: &str) -> &str {
    index_html
        .split_once("<style>")
        .expect("ui/index.html always has a <style> block")
        .1
        .split_once("</style>")
        .expect("ui/index.html's <style> block is always closed")
        .0
}

/// Mirrors `ui/src/lib.rs`'s `Header` component: logo, pair identity, verdict, meta — minus the
/// `#fork` anchor's dependency on client-side scrolling, which a plain `href` already handles.
fn write_header(out: &mut String, view: &ViewModel, schema_version: &str) {
    let is_forked = matches!(view.verdict, Verdict::Forked);
    let headline = view.headline();
    let verdict_html = if is_forked {
        format!(
            "<a class=\"verdict verdict--fork\" href=\"#fork\">{}</a>",
            escape_html(&headline)
        )
    } else {
        format!("<span class=\"verdict\">{}</span>", escape_html(&headline))
    };
    let meta = format!(
        "{} vs {} steps · schema {schema_version}",
        view.run_b.n_steps, view.run_a.n_steps
    );
    let _ = write!(
        out,
        "<header class=\"hdr\" role=\"banner\">\
         <span class=\"logo\">amber<span class=\"logo-glyph\" aria-hidden=\"true\">⑂</span>fork</span>\
         <span class=\"pair\"><b>{bad_id}</b> <span class=\"role\">{bad_role}</span>\
         <span class=\"vs\" aria-hidden=\"true\"> vs </span>\
         <b>{good_id}</b> <span class=\"role\">{good_role}</span></span>\
         {verdict_html}\
         <span class=\"meta\">{meta}</span>\
         </header>",
        bad_id = escape_html(&view.run_b.id),
        bad_role = escape_html(view.run_b.role.label()),
        good_id = escape_html(&view.run_a.id),
        good_role = escape_html(view.run_a.role.label()),
        meta = escape_html(&meta),
    );
}

fn write_canvas(out: &mut String, view: &ViewModel) {
    out.push_str("<main class=\"canvas\" aria-label=\"alignment canvas\"><ol class=\"rows\">");
    for row in &view.rows {
        write_row(out, row, view.idx_width);
    }
    out.push_str("</ol></main>");
}

/// Mirrors `ui/src/canvas.rs`'s `row_view`, minus every interactive attribute (`tabindex`,
/// `role="option"`, `aria-selected`, click/keydown handlers) — there is no JS to back them here.
fn write_row(out: &mut String, row: &Row, idx_width: usize) {
    let step = row.step();
    let idx = idx_label(step, idx_width);
    match row {
        Row::Fork(fork) => {
            let aria_label = format!(
                "fork — reference and observed diverge at {idx}, {}",
                fork.confidence
            );
            let _ = write!(
                out,
                "<li class=\"row row--fork\" id=\"fork\" aria-label=\"{}\">",
                escape_html(&aria_label)
            );
            write_gutter(out, "⑂", &idx);
            write_cell(
                out,
                step.a.as_ref(),
                "cell cell--a",
                "cell cell--a cell--empty",
            );
            write_cell(
                out,
                step.b.as_ref(),
                "cell cell--b",
                "cell cell--b cell--empty",
            );
            let _ = write!(
                out,
                "<span class=\"tag\">[FORK · {}]</span>",
                escape_html(&fork.confidence)
            );
            out.push_str("</li>");
        }
        Row::Step(step_row) => {
            let (class, cue) = match step_row.role {
                RowRole::Spine => ("row row--spine", "·"),
                RowRole::Downstream => ("row row--down", "✗"),
            };
            let _ = write!(out, "<li class=\"{class}\">");
            write_gutter(out, cue, &idx);
            write_cell(
                out,
                step.a.as_ref(),
                "cell cell--a",
                "cell cell--a cell--empty",
            );
            write_cell(
                out,
                step.b.as_ref(),
                "cell cell--b",
                "cell cell--b cell--empty",
            );
            out.push_str("</li>");
        }
    }
}

fn idx_label(step: &AlignedStep, idx_width: usize) -> String {
    match step.display_idx() {
        Some(i) => format!("step {i:0idx_width$}"),
        None => format!("step {}", "·".repeat(idx_width)),
    }
}

fn write_gutter(out: &mut String, cue: &str, idx: &str) {
    let _ = write!(
        out,
        "<span class=\"gutter\"><span class=\"cue\" aria-hidden=\"true\">{}</span>\
         <span class=\"idx\">{}</span></span>",
        escape_html(cue),
        escape_html(idx),
    );
}

fn write_cell(out: &mut String, step: Option<&StepView>, full_class: &str, empty_class: &str) {
    match step {
        Some(view) => {
            let _ = write!(
                out,
                "<span class=\"{full_class}\"><span class=\"kind\">{kind}</span>\
                 <span class=\"name\">{name}</span><span class=\"sum\">{sum}</span></span>",
                kind = escape_html(kind_label(view.kind)),
                name = escape_html(&view.name),
                sum = slot_html(&view.summary),
            );
        }
        None => {
            let _ = write!(
                out,
                "<span class=\"{empty_class}\" aria-hidden=\"true\"></span>"
            );
        }
    }
}

fn slot_html(slot: &SlotText) -> String {
    let mut html = escape_html(&slot.text);
    if slot.truncated {
        let _ = write!(
            html,
            "<span class=\"slot-trunc\" title=\"{}\">…</span>",
            escape_html(TRUNC_TITLE)
        );
    }
    html
}

/// Mirrors `ui/src/attribution.rs`'s pane: the attribution answer (or its empty state), the
/// deltas subsection, then the fork's field-diff evidence — the terminal renders only the
/// fork's, so the export matches that scope rather than every row's (issue #27's per-row
/// evidence is a live-selection feature this static export doesn't have).
fn write_attribution_pane(out: &mut String, view: &ViewModel) {
    out.push_str("<aside class=\"attr\" aria-label=\"attribution\">");
    out.push_str("<h2 class=\"attr-title\">Attribution</h2>");
    match &view.attribution {
        Some(a) => write_attribution_rows(out, a),
        None => write_attribution_empty(out, view.verdict),
    }
    if let Some(deltas) = &view.deltas {
        write_deltas_section(out, deltas);
    }
    if let Some(fork) = fork_row(view) {
        write_field_diffs(out, &fork.step.field_diffs);
    }
    out.push_str("</aside>");
}

fn write_attribution_rows(out: &mut String, a: &AttributionView) {
    out.push_str("<dl class=\"attr-list\">");
    let _ = write!(
        out,
        "<div class=\"attr-row\"><dt>mode</dt><dd>{}</dd></div>\
         <div class=\"attr-row\"><dt>origin</dt><dd>{}</dd></div>\
         <div class=\"attr-row\"><dt>propagation</dt><dd>{}</dd></div>\
         <div class=\"attr-row\"><dt>confidence</dt><dd>{}</dd></div>",
        escape_html(&a.mode),
        escape_html(&a.origin),
        escape_html(&a.propagation),
        escape_html(&a.confidence),
    );
    // The web pane doesn't render this yet (its own slice); the terminal already does, so the
    // export follows the terminal here rather than waiting on the web slice to catch up.
    if let Some(verdict) = &a.verdict {
        let _ = write!(
            out,
            "<div class=\"attr-row\"><dt>verdict</dt><dd>{}</dd></div>",
            escape_html(verdict)
        );
    }
    out.push_str("</dl>");
}

fn write_attribution_empty(out: &mut String, verdict: Verdict) {
    let message = if matches!(verdict, Verdict::Forked) {
        "Fork found, but its origin couldn't be localized."
    } else {
        "The runs converged — no fork to attribute."
    };
    let _ = write!(out, "<p class=\"attr-empty\">{}</p>", escape_html(message));
}

fn write_deltas_section(out: &mut String, deltas: &DeltasView) {
    let mut rows = String::new();
    if let Some(total) = &deltas.total {
        let _ = write!(
            rows,
            "<div class=\"attr-row\"><dt>total</dt><dd>{}</dd></div>",
            escape_html(total)
        );
    }
    if let Some(at_fork) = &deltas.at_fork {
        let _ = write!(
            rows,
            "<div class=\"attr-row\"><dt>at fork</dt><dd>{}</dd></div>",
            escape_html(at_fork)
        );
    }
    if rows.is_empty() {
        return;
    }
    let _ = write!(
        out,
        "<div class=\"attr-section\"><h3 class=\"attr-title\">Deltas</h3>\
         <dl class=\"attr-list\">{rows}</dl></div>"
    );
}

fn fork_row(view: &ViewModel) -> Option<&ForkRow> {
    view.rows.iter().find_map(|row| match row {
        Row::Fork(fork) => Some(fork),
        Row::Step(_) => None,
    })
}

fn write_field_diffs(out: &mut String, diffs: &[FieldDiffView]) {
    if diffs.is_empty() {
        let _ = write!(
            out,
            "<p class=\"content-diff-empty\">{}</p>",
            escape_html(EMPTY_FIELD_DIFF)
        );
        return;
    }
    out.push_str("<section class=\"content-diff\" aria-label=\"field diff\">");
    for fd in diffs {
        let _ = write!(
            out,
            "<div class=\"content-diff-field\"><span class=\"content-diff-path\">{}</span>",
            escape_html(&fd.path)
        );
        if let Some(removed) = &fd.removed {
            write_diff_line(out, '-', "content-diff-del", "removed", removed);
        }
        if let Some(added) = &fd.added {
            write_diff_line(out, '+', "content-diff-add", "added", added);
        }
        out.push_str("</div>");
    }
    out.push_str("</section>");
}

fn write_diff_line(out: &mut String, sign: char, class: &str, label: &str, slot: &SlotText) {
    let aria = format!("{label} {}", slot.text);
    let _ = write!(
        out,
        "<div class=\"{class}\" aria-label=\"{}\">\
         <span class=\"content-diff-sign\" aria-hidden=\"true\">{sign}</span>\
         <span class=\"content-diff-val\">{}</span></div>",
        escape_html(&aria),
        slot_html(slot),
    );
}

/// Escapes text for both element content and quoted attribute values — the same rule set
/// (`&`/`<`/`>`/`"`/`'`) is safe and sufficient in both positions.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_layout::{RunHeader, RunRole, StepRow};

    fn header(id: &str, role: RunRole, n_steps: usize) -> RunHeader {
        RunHeader {
            id: id.to_string(),
            role,
            n_steps,
            outcome: None,
        }
    }

    fn minimal(rows: Vec<Row>, verdict: Verdict) -> ViewModel {
        ViewModel {
            run_a: header("good.json", RunRole::Reference, 3),
            run_b: header("bad.json", RunRole::Observed, 3),
            idx_width: 2,
            rows,
            verdict,
            attribution: None,
            deltas: None,
            warnings: vec![],
        }
    }

    fn spine_step(idx: usize) -> AlignedStep {
        AlignedStep {
            a_idx: Some(idx),
            b_idx: Some(idx),
            a: Some(StepView {
                kind: amberfork_model::StepKind::Tool,
                name: "search".to_string(),
                summary: SlotText::new("9 results"),
            }),
            b: Some(StepView {
                kind: amberfork_model::StepKind::Tool,
                name: "search".to_string(),
                summary: SlotText::new("9 results"),
            }),
            field_diffs: vec![],
        }
    }

    #[test]
    fn escapes_user_controlled_text() {
        assert_eq!(
            escape_html("<script>alert('hi')&\"there\"</script>"),
            "&lt;script&gt;alert(&#39;hi&#39;)&amp;&quot;there&quot;&lt;/script&gt;"
        );
    }

    #[test]
    fn extracts_the_real_style_block() {
        let style = extract_style(UI_INDEX_HTML);
        assert!(style.contains("--hair"), "a real design token is present");
        assert!(
            !style.contains("<style>"),
            "no leftover tag in the extracted text"
        );
    }

    #[test]
    fn output_is_self_contained_no_external_resources() {
        let view = minimal(
            vec![Row::Step(StepRow {
                role: RowRole::Spine,
                kind: amberfork_model::MoveKind::Sync,
                step: spine_step(0),
            })],
            Verdict::Identical { steps: 1 },
        );
        let document = Document::new(view);
        let html = render(&document);

        assert!(
            !html.contains("<link"),
            "no external stylesheet/font link: {html}"
        );
        assert!(!html.contains("<script"), "no script tag at all: {html}");
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "no network URL: {html}"
        );
    }

    #[test]
    fn converged_render_has_no_amber_role_hooks_and_states_no_fork() {
        let view = minimal(vec![], Verdict::Identical { steps: 5 });
        let html = render(&Document::new(view));

        assert!(html.contains("converged"), "converged answer given: {html}");
        assert!(
            !html.contains("<li class=\"row row--fork\""),
            "no fork row when converged (the CSS itself defines `.row--fork`, so this checks \
             for the actual element, not the stylesheet): {html}"
        );
        assert!(
            !html.contains("<dl class=\"attr-list\">"),
            "no attribution list when converged (the CSS itself mentions the class name, so \
             this checks for the actual element, not the stylesheet): {html}"
        );
    }

    #[test]
    fn forked_render_carries_the_fork_row_evidence_and_attribution() {
        let fork = ForkRow {
            step: AlignedStep {
                a_idx: Some(2),
                b_idx: Some(2),
                a: None,
                b: None,
                field_diffs: vec![FieldDiffView {
                    path: "outputs.arg".to_string(),
                    removed: Some(SlotText::new("\"8841\"")),
                    added: Some(SlotText::new("\"J. Smith\"")),
                }],
            },
            side_a: SlotText::new("A"),
            side_b: SlotText::new("B"),
            confidence: "conf 0.86".to_string(),
        };
        let mut view = minimal(vec![Row::Fork(fork)], Verdict::Forked);
        view.attribution = Some(AttributionView {
            mode: "static".to_string(),
            origin: "origin step 02".to_string(),
            propagation: "step 03".to_string(),
            confidence: "conf 0.86".to_string(),
            verdict: None,
        });
        view.deltas = Some(DeltasView {
            total: Some("+5.20s".to_string()),
            at_fork: None,
        });
        let html = render(&Document::new(view));

        assert!(html.contains("row--fork"), "fork row present: {html}");
        assert!(
            html.contains("id=\"fork\""),
            "fork anchor target present: {html}"
        );
        assert!(
            html.contains("origin step 02"),
            "attribution renders: {html}"
        );
        assert!(html.contains("+5.20s"), "deltas render: {html}");
        assert!(
            html.contains("content-diff-del") && html.contains("content-diff-add"),
            "the fork's field diff evidence renders: {html}"
        );
    }

    #[test]
    fn no_interactive_affordances_and_the_static_note_is_present() {
        let fork = ForkRow {
            step: AlignedStep {
                a_idx: Some(0),
                b_idx: Some(0),
                a: None,
                b: None,
                field_diffs: vec![FieldDiffView {
                    path: "outputs".to_string(),
                    removed: Some(SlotText::new("a")),
                    added: Some(SlotText::new("b")),
                }],
            },
            side_a: SlotText::new("A"),
            side_b: SlotText::new("B"),
            confidence: "conf 0.9".to_string(),
        };
        let html = render(&Document::new(minimal(
            vec![Row::Fork(fork)],
            Verdict::Forked,
        )));

        assert!(
            !html.contains("tabindex"),
            "no tabindex — nothing to focus-cycle: {html}"
        );
        assert!(
            !html.contains("role=\"option\""),
            "no fake selectability: {html}"
        );
        assert!(
            !html.contains("<button"),
            "no inert copy button — the CSS class name alone isn't proof, the shared \
             stylesheet defines `.content-diff-copy` whether or not a button exists: {html}"
        );
        assert!(
            html.contains("static-note"),
            "the honest note is present: {html}"
        );
        assert!(
            html.contains("amberfork serve"),
            "the note names the live-view command: {html}"
        );
    }
}
