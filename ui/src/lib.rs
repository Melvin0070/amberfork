//! amberfork's web painter: the Leptos view over the layout [`Document`].
//!
//! Contracts first — this crate renders the SAME [`Document`] the server ships (issue #24),
//! never a re-declared schema. The render is a pure function of that document, which is what
//! lets the SAME components be tested with a host-side SSR string render (no browser, no
//! Node — issue #26 D16) and compiled to wasm for the browser (`csr`). The fetch that feeds
//! the view a document is the one impure edge, and it lives in the binary (`main.rs`,
//! `csr`-only), never here.
//!
//! Slice 0 (this file) renders the header frame for real against a bound document. The
//! canvas — the shared spine and the amber fork itself — arrives in the next slice; the
//! header carries ZERO amber on purpose ("sameness recedes"): amber is spent exactly twice,
//! both in the canvas (the fork node and its divergent path).

use amberfork_layout::{Document, FieldDiffView, Row, Verdict};
use leptos::prelude::*;

mod attribution;
mod canvas;
mod content_diff;
use attribution::Attribution;
use canvas::Canvas;

/// The whole view: the header frame over the two-pane body — the shared-spine canvas and the
/// attribution pane (the v0.5 composition, DESIGN.md decisions 2026-07-12).
#[component]
pub fn App(document: Document) -> impl IntoView {
    // Pull what each pane reads before the document moves into the header. The canvas renders the
    // rows; the attribution pane needs only the diff's answer and its verdict — so it never
    // reaches into the rows, and the header's `#fork` anchor still lands on the canvas fork row.
    let model = document.view.clone();
    let attribution = model.attribution.clone();
    let verdict = model.verdict;

    // Selection is lifted here so both panes read one source of truth: the canvas commits it
    // (click/Enter/arrows), the content-diff pane reflects it. Default = the fork, so the app
    // opens on the answer (DR5) AND its field diff, never a dead pane. A converged diff has no
    // fork, so nothing is selected and the content-diff shows no card.
    let selected = RwSignal::new(model.rows.iter().position(|r| matches!(r, Row::Fork(_))));
    // Each row's field-level evidence, indexed to match the canvas rows, so the content-diff
    // pane resolves the selected row without reaching back into the canvas. Cloned once here
    // before `model` moves into the canvas.
    let field_diffs: Vec<Vec<FieldDiffView>> = model
        .rows
        .iter()
        .map(|r| r.step().field_diffs.clone())
        .collect();

    view! {
        <Header document=document />
        <div class="body">
            <main class="canvas" aria-label="alignment canvas">
                <Canvas model=model selected=selected />
            </main>
            <Attribution
                attribution=attribution
                verdict=verdict
                selected=selected
                field_diffs=field_diffs
            />
        </div>
    }
}

/// The header bar: pair identity and the verdict — the protagonist. No amber lives here; the
/// header is the quiet frame, and the verdict earns its prominence through the `text` token,
/// mono type, and its place adjacent to the pair identity, not through color (DESIGN.md
/// decisions 2026-07-12).
#[component]
fn Header(document: Document) -> impl IntoView {
    let schema = document.schema_version;
    let view = document.view;
    let headline = view.headline();
    let is_forked = matches!(view.verdict, Verdict::Forked);
    let run_a = view.run_a;
    let run_b = view.run_b;
    let meta = format!(
        "{} vs {} steps · schema {schema}",
        run_b.n_steps, run_a.n_steps
    );

    view! {
        <header class="hdr" role="banner">
            <span class="logo">
                "amber"<span class="logo-glyph" aria-hidden="true">"⑂"</span>"fork"
            </span>
            <span class="pair">
                <b>{run_b.id}</b>" "<span class="role">{run_b.role.label()}</span>
                <span class="vs" aria-hidden="true">" vs "</span>
                <b>{run_a.id}</b>" "<span class="role">{run_a.role.label()}</span>
            </span>
            <VerdictLine headline=headline is_forked=is_forked />
            <span class="meta">{meta}</span>
        </header>
    }
}

/// The verdict line. When the runs forked it is a native anchor to the fork row — no JS, the
/// `#fork` target lands with the canvas — so the protagonist is one click from the answer.
/// When they converged there is nothing to jump to, so it is plain text.
#[component]
fn VerdictLine(headline: String, is_forked: bool) -> impl IntoView {
    if is_forked {
        view! { <a class="verdict verdict--fork" href="#fork">{headline}</a> }.into_any()
    } else {
        view! { <span class="verdict">{headline}</span> }.into_any()
    }
}

/// The disconnect banner: the server that fed this view stopped. Pure markup so it is
/// SSR-testable (D16) — the impure re-poll loop that decides *when* to mount it lives in the
/// `csr` binary (`main.rs`), the one I/O edge. It speaks in `warning`, never amber: a
/// system-status message is not a divergence, and amber is spent only twice, both in the canvas.
/// It emits a paste-ready restart command with the real run names (the evidence-out rule), and
/// carries no spinner — the state is terminal until the user restarts the server and reloads.
#[component]
pub fn DisconnectBanner(bad: String, good: String) -> impl IntoView {
    let command = format!("amberfork serve {bad} --against {good}");
    view! {
        <div class="banner banner--disconnect" role="alert">
            "server stopped — restart: "<code>{command}</code>
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use amberfork_layout::{
        AlignedStep, AttributionView, FieldDiffView, ForkRow, Row, RunHeader, RunRole, SlotText,
        ViewModel,
    };

    /// Render a component to an HTML string exactly as the browser's SSR peer would — inside
    /// a reactive owner, no DOM required. This is the seam issue #26 tests through (D16).
    fn render(document: Document) -> String {
        let owner = Owner::new();
        owner.with(|| view! { <App document=document /> }.to_html())
    }

    /// Render the disconnect banner in isolation, exactly as the `csr` binary mounts it.
    fn render_banner(bad: &str, good: &str) -> String {
        let owner = Owner::new();
        owner.with(|| {
            view! { <DisconnectBanner bad=bad.to_string() good=good.to_string() /> }.to_html()
        })
    }

    fn run(id: &str, role: RunRole, n_steps: usize) -> RunHeader {
        RunHeader {
            id: id.to_string(),
            role,
            n_steps,
            outcome: None,
        }
    }

    /// A forked view built through the layout crate's public API — the same shape
    /// `ViewModel::compute` emits, so `headline()` runs the real wording logic.
    fn forked_doc() -> Document {
        let fork = ForkRow {
            step: AlignedStep {
                a_idx: Some(11),
                b_idx: Some(11),
                a: None,
                b: None,
                field_diffs: vec![FieldDiffView {
                    path: "outputs.arg".to_string(),
                    removed: Some(SlotText::new("\"8841\"")),
                    added: Some(SlotText::new("\"J. Smith\"")),
                }],
            },
            side_a: SlotText::new("A: order_id=\"8841\""),
            side_b: SlotText::new("B: name=\"J. Smith\""),
            confidence: "conf 0.86".to_string(),
        };
        Document::new(ViewModel {
            run_a: run("good.json", RunRole::Reference, 25),
            run_b: run("bad.json", RunRole::Observed, 27),
            idx_width: 2,
            rows: vec![Row::Fork(fork)],
            verdict: Verdict::Forked,
            attribution: Some(AttributionView {
                mode: "static".to_string(),
                origin: "origin step 11".to_string(),
                propagation: "step 12".to_string(),
                confidence: "conf 0.86".to_string(),
            }),
            warnings: vec![],
        })
    }

    fn converged_doc() -> Document {
        Document::new(ViewModel {
            run_a: run("good.json", RunRole::Reference, 25),
            run_b: run("good-again.json", RunRole::Observed, 25),
            idx_width: 2,
            rows: vec![],
            verdict: Verdict::Identical { steps: 25 },
            attribution: None,
            warnings: vec![],
        })
    }

    #[test]
    fn forked_header_is_the_verdict_as_a_fork_anchor() {
        let doc = forked_doc();
        // Tie the render to the real wording, not a copy of the string, so a change to the
        // designed headline surfaces here instead of drifting silently.
        let headline = doc.view.headline();
        assert_eq!(headline, "⑂ forked at step 11 · conf 0.86");
        let html = render(doc);

        assert!(html.contains(&headline), "verdict text is rendered: {html}");
        assert!(
            html.contains("verdict--fork"),
            "forked verdict is the fork anchor: {html}"
        );
        assert!(
            html.contains("href=\"#fork\""),
            "anchor points at the fork row: {html}"
        );
        assert!(
            html.contains("bad.json") && html.contains("good.json"),
            "both runs named"
        );
        assert!(
            html.contains("observed") && html.contains("reference"),
            "roles labelled"
        );
        assert!(html.contains("27 vs 25 steps"), "step counts in the meta");
    }

    #[test]
    fn converged_header_is_plain_verdict_text() {
        let doc = converged_doc();
        let headline = doc.view.headline();
        assert_eq!(headline, "converged — identical through 25 steps");
        let html = render(doc);

        assert!(
            html.contains(&headline),
            "converged wording rendered: {html}"
        );
        assert!(
            !html.contains("verdict--fork"),
            "converged verdict is plain text, never a fork anchor: {html}"
        );
    }

    #[test]
    fn header_and_canvas_are_landmarks() {
        let html = render(forked_doc());
        assert!(
            html.contains("role=\"banner\""),
            "header is a banner landmark: {html}"
        );
        assert!(
            html.contains("aria-label=\"alignment canvas\""),
            "canvas region is a labelled landmark from frame one: {html}"
        );
        assert!(
            html.contains("aria-label=\"attribution\""),
            "attribution pane is a labelled landmark: {html}"
        );
    }

    #[test]
    fn app_opens_on_the_attribution_answer() {
        // The two-pane body renders the diff's answer beside the canvas, so the app is never a
        // canvas with a dead pane (the fork is selected by default in the canvas).
        let html = render(forked_doc());
        assert!(
            html.contains("origin step 11") && html.contains("static"),
            "attribution parts render in the pane: {html}"
        );
        assert!(
            html.contains("row row--fork row--selected"),
            "the fork opens selected: {html}"
        );
    }

    #[test]
    fn the_pane_opens_on_the_forks_field_diff() {
        // The fork is selected by default, so the content-diff pane is never a dead zone: it
        // opens showing the fork's field-level evidence (the amendment's default state).
        let html = render(forked_doc());
        assert!(
            html.contains("content-diff-del") && html.contains("content-diff-add"),
            "the fork's red/green field diff renders on load: {html}"
        );
        assert!(
            html.contains("8841") && html.contains("J. Smith"),
            "both field values are real selectable text: {html}"
        );
    }

    #[test]
    fn red_green_is_confined_to_the_content_diff_pane() {
        // DESIGN.md's hard containment rule: red/green live ONLY in the content-diff card, which
        // sits inside the attribution aside. Everything the canvas paints (before the aside)
        // must carry neither diff class — the canvas spends amber, never red/green.
        let html = render(forked_doc());
        let aside = html
            .find("class=\"attr\"")
            .expect("attribution aside present");
        let canvas = &html[..aside];
        assert!(
            !canvas.contains("content-diff-del") && !canvas.contains("content-diff-add"),
            "no red/green anywhere in the canvas: {canvas}"
        );
        assert!(
            html.contains("content-diff-del"),
            "the red/green does render — inside the pane: {html}"
        );
    }

    #[test]
    fn disconnect_banner_says_stopped_and_how_to_restart() {
        // The banner names the failure and the exact recovery, in the interface's voice — and
        // the restart command carries the REAL run names so it is paste-ready, not a template.
        let html = render_banner("bad.json", "good.json");
        assert!(html.contains("server stopped"), "names the failure: {html}");
        assert!(
            html.contains("amberfork serve bad.json --against good.json"),
            "restart command names the real runs (bad, then --against good): {html}"
        );
        assert!(
            html.contains("banner--disconnect"),
            "carries the disconnect banner class the stylesheet keys `warning` to: {html}"
        );
        assert!(
            html.contains("role=\"alert\""),
            "announced assertively to assistive tech: {html}"
        );
    }

    #[test]
    fn disconnect_banner_is_warning_never_amber() {
        // Amber is spent exactly twice, both in the canvas (fork + divergent path). A
        // system-status message is not a divergence, so the banner must carry none of the
        // canvas's amber-role hooks — it speaks in `warning` via `banner--disconnect` alone.
        let html = render_banner("bad.json", "good.json");
        for amber_hook in [
            "row--fork",
            "row--down",
            "spine-path",
            "fork-node",
            "verdict--fork",
        ] {
            assert!(
                !html.contains(amber_hook),
                "banner carries no amber hook `{amber_hook}`: {html}"
            );
        }
    }

    #[test]
    fn the_loaded_app_never_shows_the_banner() {
        // The banner is mounted only by the `csr` re-poll loop on disconnect; the pure App
        // render — the connected state — must never carry it, or a live server would look dead.
        let html = render(forked_doc());
        assert!(
            !html.contains("banner--disconnect"),
            "the connected view carries no disconnect banner: {html}"
        );
    }

    #[test]
    fn app_shell_never_blanks() {
        // The pre-wasm shell lives in index.html (static) so the page is never blank before
        // wasm mounts (D20); assert all three states it must carry are present.
        let shell = include_str!("../index.html");
        assert!(shell.contains("Loading the fork"), "loading state present");
        assert!(shell.contains("<noscript"), "noscript fallback present");
        assert!(
            shell.contains("boot--error"),
            "wasm-load error state present"
        );
    }
}
