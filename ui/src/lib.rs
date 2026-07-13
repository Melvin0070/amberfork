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

use amberfork_layout::{Document, Verdict};
use leptos::prelude::*;

mod attribution;
mod canvas;
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
    view! {
        <Header document=document />
        <div class="body">
            <main class="canvas" aria-label="alignment canvas">
                <Canvas model=model />
            </main>
            <Attribution attribution=attribution verdict=verdict />
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use amberfork_layout::{
        AlignedStep, AttributionView, ForkRow, Row, RunHeader, RunRole, SlotText, ViewModel,
    };

    /// Render a component to an HTML string exactly as the browser's SSR peer would — inside
    /// a reactive owner, no DOM required. This is the seam issue #26 tests through (D16).
    fn render(document: Document) -> String {
        let owner = Owner::new();
        owner.with(|| view! { <App document=document /> }.to_html())
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
            },
            side_a: SlotText::new("A: order_id=\"8841\""),
            side_b: SlotText::new("B: name=\"J. Smith\""),
            confidence: "conf 0.86".to_string(),
            field_diffs: vec![],
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
