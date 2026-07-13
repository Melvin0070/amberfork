//! The alignment canvas: side-by-side runs on the shared spine (issue #26 slice 1).
//!
//! Two run tracks (A reference, B observed) are laid out on ONE vertical timeline so identical
//! steps sit at the same y and a divergence visibly breaks the alignment (DESIGN.md 2026-07-12).
//! The design's north star governs every choice here: **sameness recedes, divergence glows** —
//! the sync spine is `muted`, the fork and every downstream row glow `amber`, and amber is spent
//! on nothing else.
//!
//! Rendering is split so text stays selectable/accessible (a hard requirement — DOM/SVG, never
//! canvas): the step content is real DOM in a CSS grid; the spine, the amber divergent-path
//! segment, and the fork node are a narrow SVG overlay. The two are keyed to ONE geometry
//! constant ([`ROW_H`]) so they line up without either measuring the other — which is why the
//! geometry is a pure function ([`spine_geometry`]) with invariants a plain `cargo test` pins,
//! independent of anything the browser paints (issue #26 D16).

use amberfork_layout::{AlignedStep, Row, RowRole, SlotText, StepView, ViewModel, kind_label};
use leptos::prelude::*;

/// Vertical pitch between adjacent rows, in px. The single geometry constant the DOM grid
/// (`.row` height) and the SVG overlay (this module) both read, so they stay aligned by
/// construction rather than by measurement.
const ROW_H: f64 = 30.0;
/// Canvas top padding before the first row's center resolves — mirrored by `.rows` padding-top.
const TOP_PAD: f64 = 18.0;
/// Width of the SVG spine strip; `.rows` clears it with an equal left margin.
const SPINE_W: f64 = 28.0;
/// x of the spine rail within its strip (its center).
const SPINE_X: f64 = 14.0;
/// Radius of the fork node marker.
const SPINE_DOT: f64 = 4.0;

const TRUNC_TITLE: &str = "payload truncated — full text in the terminal";

// Cell class pairs, spelled out so the render never allocates a class string per row.
const CELL_A: &str = "cell cell--a";
const CELL_B: &str = "cell cell--b";
const CELL_A_EMPTY: &str = "cell cell--a cell--empty";
const CELL_B_EMPTY: &str = "cell cell--b cell--empty";

/// The center-y of row `i` on the shared timeline. Linear, so the ys are strictly increasing
/// and evenly spaced by [`ROW_H`] — the "y monotone" invariant holds by construction.
fn row_y(i: usize) -> f64 {
    TOP_PAD + (i as f64) * ROW_H + ROW_H / 2.0
}

/// The SVG overlay's coordinates, resolved from the semantic rows alone. Converged diffs have
/// no `fork_y`; forked diffs put it exactly on the fork row's center, and the amber path runs
/// from there to the last row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpineGeometry {
    /// Total SVG height: the last row's center plus a half-row and the bottom pad.
    pub height: f64,
    /// Center-y of the first row (rail top). Equals `y_last` when there is a single row.
    pub y_first: f64,
    /// Center-y of the last row (rail bottom, and the amber path's end).
    pub y_last: f64,
    /// Center-y of the fork row, or `None` when the runs converged (no amber is drawn).
    pub fork_y: Option<f64>,
}

/// Map the semantic rows to spine geometry. Pure and total: the fork is found by its variant,
/// never recomputed, so `fork_y` can only ever land on the row the engine marked.
pub(crate) fn spine_geometry(rows: &[Row]) -> SpineGeometry {
    let n = rows.len();
    if n == 0 {
        return SpineGeometry {
            height: TOP_PAD * 2.0,
            y_first: TOP_PAD,
            y_last: TOP_PAD,
            fork_y: None,
        };
    }
    let fork_y = rows
        .iter()
        .position(|row| matches!(row, Row::Fork(_)))
        .map(row_y);
    SpineGeometry {
        height: row_y(n - 1) + ROW_H / 2.0 + TOP_PAD,
        y_first: row_y(0),
        y_last: row_y(n - 1),
        fork_y,
    }
}

/// The alignment canvas over one diff's [`ViewModel`].
#[component]
pub(crate) fn Canvas(model: ViewModel) -> impl IntoView {
    let geom = spine_geometry(&model.rows);
    let idx_width = model.idx_width;
    let rows: Vec<AnyView> = model
        .rows
        .iter()
        .map(|row| row_view(row, idx_width))
        .collect();

    view! {
        <div class="track" style=format!("min-height:{}px", geom.height)>
            <Spine geom=geom />
            <ol class="rows" role="list">{rows}</ol>
        </div>
    }
}

/// The SVG spine overlay: a faint timeline rail always, plus — only when forked — the amber
/// divergent-path segment and the fork node. Decorative to assistive tech (the rows carry the
/// signal); it is the drawn timeline the ignition beat will animate in a later slice.
#[component]
fn Spine(geom: SpineGeometry) -> impl IntoView {
    let SpineGeometry {
        height,
        y_first,
        y_last,
        fork_y,
    } = geom;
    let has_rail = y_last > y_first;

    view! {
        <svg
            class="spine"
            width=SPINE_W.to_string()
            height=height.to_string()
            aria-hidden="true"
        >
            {has_rail.then(|| view! {
                <line
                    class="spine-rail"
                    x1=SPINE_X.to_string()
                    y1=y_first.to_string()
                    x2=SPINE_X.to_string()
                    y2=y_last.to_string()
                />
            })}
            {fork_y.map(|fy| view! {
                <line
                    class="spine-path"
                    x1=SPINE_X.to_string()
                    y1=fy.to_string()
                    x2=SPINE_X.to_string()
                    y2=y_last.to_string()
                />
            })}
            {fork_y.map(|fy| view! {
                <circle
                    class="fork-node"
                    cx=SPINE_X.to_string()
                    cy=fy.to_string()
                    r=SPINE_DOT.to_string()
                />
            })}
        </svg>
    }
}

/// One aligned move as a canvas row. The role decides everything the eye reads: the gutter cue
/// (`·` sync / `⑂` fork / `✗` downstream), the amber class, and — on the fork alone — the
/// `[FORK · conf]` tag, the `#fork` anchor target, and the accessible name that carries the
/// divergence without relying on color or the decorative glyph.
fn row_view(row: &Row, idx_width: usize) -> AnyView {
    let step = row.step();
    let idx = idx_label(step, idx_width);
    let cell_a = cell_view(step.a.as_ref(), CELL_A, CELL_A_EMPTY);
    let cell_b = cell_view(step.b.as_ref(), CELL_B, CELL_B_EMPTY);

    match row {
        Row::Fork(fork) => {
            let tag = format!("[FORK · {}]", fork.confidence);
            let aria = format!(
                "fork — reference and observed diverge at {idx}, {}",
                fork.confidence
            );
            view! {
                <li class="row row--fork" id="fork" aria-label=aria>
                    <span class="gutter">
                        <span class="cue" aria-hidden="true">"⑂"</span>
                        <span class="idx">{idx}</span>
                    </span>
                    {cell_a}
                    {cell_b}
                    <span class="tag">{tag}</span>
                </li>
            }
            .into_any()
        }
        Row::Step(step_row) => {
            let (class, cue) = match step_row.role {
                RowRole::Spine => ("row row--spine", "·"),
                RowRole::Downstream => ("row row--down", "✗"),
            };
            view! {
                <li class=class>
                    <span class="gutter">
                        <span class="cue" aria-hidden="true">{cue}</span>
                        <span class="idx">{idx}</span>
                    </span>
                    {cell_a}
                    {cell_b}
                </li>
            }
            .into_any()
        }
    }
}

/// The timeline gutter label: `step NN`, zero-padded to the view's shared width. A fork with no
/// step on either side (malformed hand-built input) shows the gutter's dot convention.
fn idx_label(step: &AlignedStep, idx_width: usize) -> String {
    match step.display_idx() {
        Some(i) => format!("step {i:0idx_width$}"),
        None => format!("step {}", "·".repeat(idx_width)),
    }
}

/// One side's cell. An absent side renders empty — that gap IS the visible break in the
/// alignment, not a thing to fill with prose.
fn cell_view(step: Option<&StepView>, full: &'static str, empty: &'static str) -> AnyView {
    match step {
        Some(view) => {
            let kind = kind_label(view.kind);
            let name = view.name.clone();
            let summary = slot_view(&view.summary);
            view! {
                <span class=full>
                    <span class="kind">{kind}</span>
                    <span class="name">{name}</span>
                    <span class="sum">{summary}</span>
                </span>
            }
            .into_any()
        }
        None => view! { <span class=empty aria-hidden="true"></span> }.into_any(),
    }
}

/// A payload slot as selectable text. A slot the envelope cut ([`SlotText::truncated`]) keeps
/// its honest mark — a silently shortened payload would read as the payload. The web UI is the
/// first surface to see a cut slot; it reuses the project's `…` truncation glyph.
fn slot_view(slot: &SlotText) -> AnyView {
    let text = slot.text.clone();
    if slot.truncated {
        view! {
            <>
                {text}
                <span class="slot-trunc" title=TRUNC_TITLE>"…"</span>
            </>
        }
        .into_any()
    } else {
        view! { <>{text}</> }.into_any()
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use amberfork_layout::{ForkRow, MoveKind, StepKind, StepRow, Verdict};

    /// Render a component to HTML exactly as the browser's SSR peer would (issue #26 D16).
    fn render(model: ViewModel) -> String {
        let owner = Owner::new();
        owner.with(|| view! { <Canvas model=model /> }.to_html())
    }

    fn stepview(kind: StepKind, name: &str, summary: &str) -> StepView {
        StepView {
            kind,
            name: name.to_string(),
            summary: SlotText::new(summary),
        }
    }

    fn synced(idx: usize, kind: StepKind, name: &str, summary: &str) -> AlignedStep {
        AlignedStep {
            a_idx: Some(idx),
            b_idx: Some(idx),
            a: Some(stepview(kind, name, summary)),
            b: Some(stepview(kind, name, summary)),
        }
    }

    fn spine(idx: usize, name: &str, summary: &str) -> Row {
        Row::Step(StepRow {
            role: RowRole::Spine,
            kind: MoveKind::Sync,
            step: synced(idx, StepKind::Llm, name, summary),
        })
    }

    fn downstream(idx: usize, name: &str, summary: &str) -> Row {
        Row::Step(StepRow {
            role: RowRole::Downstream,
            kind: MoveKind::Model,
            step: synced(idx, StepKind::Tool, name, summary),
        })
    }

    fn fork(idx: usize) -> Row {
        Row::Fork(ForkRow {
            step: AlignedStep {
                a_idx: Some(idx),
                b_idx: Some(idx),
                a: Some(stepview(
                    StepKind::Tool,
                    "lookup_order",
                    "order_id=\"8841\"",
                )),
                b: Some(stepview(
                    StepKind::Tool,
                    "lookup_order",
                    "name=\"J. Smith\"",
                )),
            },
            side_a: SlotText::new("A: order_id=\"8841\""),
            side_b: SlotText::new("B: name=\"J. Smith\""),
            confidence: "conf 0.86".to_string(),
            field_diffs: vec![],
        })
    }

    fn model(rows: Vec<Row>, verdict: Verdict) -> ViewModel {
        ViewModel {
            run_a: header("good.json"),
            run_b: header("bad.json"),
            idx_width: 2,
            rows,
            verdict,
            attribution: None,
            warnings: vec![],
        }
    }

    fn header(id: &str) -> amberfork_layout::RunHeader {
        amberfork_layout::RunHeader {
            id: id.to_string(),
            role: amberfork_layout::RunRole::Reference,
            n_steps: 3,
            outcome: None,
        }
    }

    /// The canonical forked shape: sync spine, the fork, then a divergent path.
    fn forked() -> ViewModel {
        model(
            vec![
                spine(9, "planner", "\"summarize findings\""),
                spine(10, "web.search", "q=\"Q2 refunds policy\""),
                fork(11),
                downstream(12, "planner", "paths diverge downstream"),
                downstream(13, "send_email", "A only — absorbed retry in B"),
            ],
            Verdict::Forked,
        )
    }

    // --- geometry: the pure seam ---------------------------------------------------------

    #[test]
    fn row_ys_are_monotone_and_evenly_spaced() {
        for n in 1..8usize {
            let ys: Vec<f64> = (0..n).map(row_y).collect();
            for pair in ys.windows(2) {
                assert!(pair[1] > pair[0], "y strictly increases: {ys:?}");
                assert!(
                    (pair[1] - pair[0] - ROW_H).abs() < 1e-9,
                    "rows are evenly spaced by ROW_H: {ys:?}"
                );
            }
        }
    }

    #[test]
    fn fork_y_lands_exactly_on_the_fork_row() {
        // Fork is the 3rd row (index 2) in `forked()`.
        let geom = spine_geometry(&forked().rows);
        assert_eq!(geom.fork_y, Some(row_y(2)));
    }

    #[test]
    fn converged_geometry_draws_no_fork() {
        let rows = vec![spine(0, "a", "x"), spine(1, "b", "y")];
        assert!(spine_geometry(&rows).fork_y.is_none());
    }

    #[test]
    fn empty_geometry_is_total() {
        let geom = spine_geometry(&[]);
        assert!(geom.fork_y.is_none());
        assert_eq!(geom.y_first, geom.y_last);
    }

    // --- render: divergence glows, sameness recedes --------------------------------------

    #[test]
    fn fork_row_glows_amber_with_its_non_color_cues() {
        let html = render(forked());
        // The amber class the stylesheet keys the glow + dashed stroke to.
        assert!(
            html.contains("row--fork"),
            "fork row carries the amber class: {html}"
        );
        // Non-color cue #1: the fork glyph. #2 (dashed stroke) is CSS keyed to `row--fork`.
        assert!(html.contains('⑂'), "fork glyph present: {html}");
        // The designed tag, with the real confidence wording from the view.
        assert!(
            html.contains("[FORK · conf 0.86]"),
            "fork tag with confidence: {html}"
        );
    }

    #[test]
    fn every_divergent_row_carries_the_cross() {
        let html = render(forked());
        // Two downstream rows in the fixture; each keeps the ✗ (DR2 covers the whole path).
        assert_eq!(
            html.matches('✗').count(),
            2,
            "one ✗ per divergent row: {html}"
        );
        assert_eq!(
            html.matches("row--down").count(),
            2,
            "both downstream rows marked: {html}"
        );
    }

    #[test]
    fn amber_never_touches_the_spine() {
        let html = render(forked());
        // Two sync rows recede: they are spine-classed and never fork/downstream.
        assert_eq!(
            html.matches("row--spine").count(),
            2,
            "sync rows are spine: {html}"
        );
        // The sync spine never carries a divergence glyph.
        let spine_glyph_free = !html.split("row--fork").next().unwrap_or("").contains('✗');
        assert!(spine_glyph_free, "no ✗ upstream of the fork: {html}");
    }

    #[test]
    fn converged_canvas_shows_no_divergence() {
        for verdict in [
            Verdict::Identical { steps: 2 },
            Verdict::Absorbed {
                absorbed: 1,
                a_steps: 2,
                b_steps: 2,
            },
        ] {
            let html = render(model(
                vec![
                    spine(0, "planner", "\"plan\""),
                    spine(1, "web.search", "q=\"x\""),
                ],
                verdict,
            ));
            assert!(!html.contains('⑂'), "no fork glyph when converged: {html}");
            assert!(
                !html.contains('✗'),
                "no divergence glyph when converged: {html}"
            );
            assert!(
                !html.contains("row--fork"),
                "no fork row when converged: {html}"
            );
            assert!(
                !html.contains("spine-path"),
                "no amber path when converged: {html}"
            );
            assert!(
                !html.contains("fork-node"),
                "no fork node when converged: {html}"
            );
        }
    }

    #[test]
    fn step_text_is_real_selectable_dom() {
        let html = render(forked());
        // Both sides of the fork render their own content, side-by-side.
        assert!(
            html.contains("order_id=\"8841\""),
            "reference side text present: {html}"
        );
        assert!(
            html.contains("name=\"J. Smith\""),
            "observed side text present: {html}"
        );
        // A recede-row's summary is real text too, not an image or canvas draw.
        assert!(
            html.contains("summarize findings"),
            "sync summary is real text: {html}"
        );
    }

    #[test]
    fn absent_side_renders_an_empty_break() {
        // A model-move present only on the observed side: the reference cell is the visible gap.
        let one_sided = Row::Step(StepRow {
            role: RowRole::Downstream,
            kind: MoveKind::Model,
            step: AlignedStep {
                a_idx: None,
                b_idx: Some(12),
                a: None,
                b: Some(stepview(StepKind::Llm, "planner", "diverged")),
            },
        });
        let html = render(model(vec![fork(11), one_sided], Verdict::Forked));
        assert!(
            html.contains("cell--empty"),
            "the missing side is an empty break: {html}"
        );
    }

    #[test]
    fn truncated_slot_keeps_its_honest_mark() {
        let mut step = synced(4, StepKind::Tool, "big_tool", "a very long payload");
        if let Some(view) = step.b.as_mut() {
            view.summary.truncated = true;
        }
        let html = render(model(
            vec![Row::Step(StepRow {
                role: RowRole::Spine,
                kind: MoveKind::Sync,
                step,
            })],
            Verdict::Identical { steps: 1 },
        ));
        assert!(
            html.contains("slot-trunc"),
            "truncation mark rendered: {html}"
        );
    }

    #[test]
    fn fork_row_is_the_anchor_target_and_is_labelled() {
        let html = render(forked());
        assert!(
            html.contains("id=\"fork\""),
            "the header's #fork anchor lands: {html}"
        );
        assert!(
            html.contains("aria-label=\"fork"),
            "fork carries an accessible name (the third redundancy leg): {html}"
        );
    }
}
