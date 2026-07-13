//! Terminal painter over the semantic [`ViewModel`] — DESIGN.md "Terminal rendering
//! (first-class surface)".
//!
//! One seam, two painters (issue #21): `amberfork-layout` computes WHAT a diff says — rows
//! and their roles, summaries, the designed wording — and this module decides only how a
//! terminal shows it: column arithmetic, truncation and wrapping, gutter glyphs, ANSI. The
//! web painter draws its own geometry over the same view, and neither feeds styling back
//! into the seam (the full `DiffResult → ViewModel → two painters` diagram lives on
//! `amberfork_layout`'s crate doc).
//!
//! North star: *sameness recedes, divergence glows.* Spine rows are one dim line each; the
//! fork carries a `⑂` gutter glyph and amber; every step downstream of the fork keeps a `✗`
//! marker in the same single amber (DR4). Red/green appear only on the `-`/`+` field-diff
//! lines inside the fork block, nowhere else. Structure — glyphs, markers, tags — carries the
//! whole signal without color (DR2): a plain render is byte-identical to a colored one with
//! the escapes stripped, which is a unit-tested invariant here.
//!
//! Pure by design: [`render`] is a function of the view and [`RenderOpts`]; TTY/env sniffing
//! lives in [`resolve_color_mode`] so `main` decides once at the edge.

use amberfork_layout::{
    ForkRow, Row as ViewRow, RowRole, RunHeader, SlotText, StepRow, StepView, ViewModel,
    kind_label, move_label, outcome_label,
};
use std::fmt::Write;

/// How much color the output medium supports. Decided once at startup, then threaded through
/// the render as plain data — the renderer never inspects the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// No ANSI at all (`--no-color`, `NO_COLOR`, non-TTY, `TERM=dumb`).
    Plain,
    /// Bold as the last-resort divergence cue (no usable color support).
    Bold,
    /// ANSI-256: amber is color 208.
    Ansi256,
    /// 24-bit color: the design-system hexes verbatim.
    Truecolor,
}

/// Inputs the painter needs beyond the view itself.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub color: ColorMode,
    /// Target line width; content is truncated or hard-wrapped to fit (never horizontal
    /// scroll).
    pub width: usize,
}

/// Paint the human-readable diff from its semantic view. Returns the full output including
/// the trailing newline.
pub fn render(view: &ViewModel, opts: &RenderOpts) -> String {
    let cols = Columns::compute(view, opts.width);
    let mut rows = Vec::new();

    let id_width = view
        .run_a
        .id
        .chars()
        .count()
        .max(view.run_b.id.chars().count());
    let role_width = view
        .run_a
        .role
        .label()
        .chars()
        .count()
        .max(view.run_b.role.label().chars().count());
    rows.push(header_row('A', &view.run_a, id_width, role_width));
    rows.push(header_row('B', &view.run_b, id_width, role_width));
    rows.push(Row::blank());

    for vrow in &view.rows {
        match vrow {
            ViewRow::Step(step_row) => rows.push(move_row(step_row, &cols)),
            ViewRow::Fork(fork_row) => fork_block(&mut rows, fork_row, &cols),
        }
    }

    if let Some(text) = view.verdict.converged_text() {
        rows.push(Row::blank());
        rows.push(Row {
            role: Role::Footer,
            prefix: String::new(),
            body: format!("  {text}"),
        });
    }

    // The forked counterpart of the converged footer: every diff ends with a designed answer
    // line. Plain, not amber — it is a statement about the divergence, not the divergence.
    if let Some(attribution) = &view.attribution {
        rows.push(Row::blank());
        rows.push(Row {
            role: Role::Footer,
            prefix: String::new(),
            body: format!(
                "  attribution · {} · {} · propagation {} · {}",
                attribution.mode,
                attribution.origin,
                attribution.propagation,
                attribution.confidence
            ),
        });
    }

    let mut out = String::new();
    for row in &rows {
        let _ = writeln!(out, "{}", row.paint(opts.color, opts.width));
    }
    out
}

/// The color-support ladder from DESIGN.md, as a pure decision: explicit opt-outs first
/// (`--no-color` flag, non-empty `NO_COLOR`, piped stdout, `TERM=dumb`), then
/// `COLORTERM=truecolor|24bit`, then a 256-color `TERM`, then bold as last resort.
pub fn resolve_color_mode(
    no_color_flag: bool,
    stdout_is_tty: bool,
    no_color_env: Option<&str>,
    term: Option<&str>,
    colorterm: Option<&str>,
) -> ColorMode {
    let opted_out = no_color_flag
        || no_color_env.is_some_and(|v| !v.is_empty())
        || !stdout_is_tty
        || term == Some("dumb");
    if opted_out {
        return ColorMode::Plain;
    }
    match colorterm {
        Some("truecolor" | "24bit") => ColorMode::Truecolor,
        _ if term.is_some_and(|t| t.contains("256color")) => ColorMode::Ansi256,
        _ => ColorMode::Bold,
    }
}

impl ColorMode {
    fn sgr(self, code: &str, text: &str) -> String {
        if self == Self::Plain || code.is_empty() {
            return text.to_string();
        }
        format!("\x1b[{code}m{text}\x1b[0m")
    }

    /// Dim gray for the sync spine — the terminal's own `dim` attribute, so it respects the
    /// user's palette at every capability level. Crate-visible so `main` can style chrome
    /// (the demo hand-off line) without duplicating escape codes.
    pub(crate) fn dim(self, text: &str) -> String {
        self.sgr("2", text)
    }

    /// The divergence accent ladder: truecolor `#FF7A1A` → ANSI-256 `208` → bold.
    fn amber(self, text: &str) -> String {
        let code = match self {
            Self::Plain => "",
            Self::Bold => "1",
            Self::Ansi256 => "38;5;208",
            Self::Truecolor => "38;2;255;122;26",
        };
        self.sgr(code, text)
    }

    /// Diff-removed red (`#FF5C5C` in truecolor); allowed only on `-` lines in the fork block.
    fn removed(self, text: &str) -> String {
        let code = match self {
            Self::Plain | Self::Bold => "",
            Self::Ansi256 => "31",
            Self::Truecolor => "38;2;255;92;92",
        };
        self.sgr(code, text)
    }

    /// Diff-added green (`#46D39A` in truecolor); allowed only on `+` lines in the fork block.
    fn added(self, text: &str) -> String {
        let code = match self {
            Self::Plain | Self::Bold => "",
            Self::Ansi256 => "32",
            Self::Truecolor => "38;2;70;211;154",
        };
        self.sgr(code, text)
    }
}

/// What a line *is*, so styling is a pure function of role — the containment rules
/// (amber only at/after the fork, red/green only on field-diff lines) hold by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Run identity lines at the top. Dim.
    Header,
    /// A pre-fork move (sync or recovered blip). Dim — sameness recedes.
    Spine,
    /// The fork line and its continuation lines. Amber — divergence glows.
    Fork,
    /// A `-` field-diff line inside the fork block. Red.
    FieldRemoved,
    /// A `+` field-diff line inside the fork block. Green.
    FieldAdded,
    /// A move downstream of the fork: amber marker prefix, plain body (DR4 uniform amber).
    Downstream,
    /// The converged statement. Plain — it is the answer.
    Footer,
    Blank,
}

/// One output line, kept as plain text until paint time so structure is identical across
/// color modes. `prefix` is the gutter/step/marker span (what Downstream paints amber);
/// `body` is the rest.
struct Row {
    role: Role,
    prefix: String,
    body: String,
}

impl Row {
    fn blank() -> Self {
        Self {
            role: Role::Blank,
            prefix: String::new(),
            body: String::new(),
        }
    }

    fn paint(&self, color: ColorMode, width: usize) -> String {
        let line = truncate(&format!("{}{}", self.prefix, self.body), width);
        match self.role {
            Role::Header | Role::Spine => color.dim(&line),
            Role::Fork => color.amber(&line),
            Role::FieldRemoved => color.removed(&line),
            Role::FieldAdded => color.added(&line),
            Role::Downstream => {
                // Only the marker prefix glows; content stays plain (and undimmed — the
                // divergent path must not recede).
                let split = self.prefix.chars().count().min(line.chars().count());
                let (prefix, body) = split_at_char(&line, split);
                format!("{}{}", color.amber(prefix), body)
            }
            Role::Footer | Role::Blank => line,
        }
    }
}

/// Column widths, computed once from the view so every row aligns (tabular discipline).
/// This arithmetic is exactly what does NOT belong in the seam: it exists only because a
/// terminal is a character grid.
struct Columns {
    /// Digits used for zero-padded step indices (the view's shared formatting voice).
    idx_width: usize,
    /// `⑂ step NN  ✗  ` — everything before the kind column.
    prefix_width: usize,
    kind_width: usize,
    name_width: usize,
    content_width: usize,
}

impl Columns {
    fn compute(view: &ViewModel, width: usize) -> Self {
        let idx_width = view.idx_width;
        // "⑂ " + "step " + idx + "  " + marker + "  "
        let prefix_width = 2 + 5 + idx_width + 2 + 1 + 2;
        let kind_width = 5; // the longest canonical kind, "agent"/"other"
        let name_width = view
            .rows
            .iter()
            .flat_map(|row| {
                let step = row.step();
                step.a.iter().chain(step.b.iter())
            })
            .map(|s| s.name.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(4, 16);
        let tag_width = view
            .rows
            .iter()
            .map(|row| tag(row).chars().count())
            .max()
            .unwrap_or(0);
        let fixed = prefix_width + kind_width + 2 + name_width + 2 + tag_width + 2;
        let content_width = width.saturating_sub(fixed).max(20);
        Self {
            idx_width,
            prefix_width,
            kind_width,
            name_width,
            content_width,
        }
    }

    /// `⑂ step 06  ✗  ` / `  step 03  ·  ` — the gutter-to-marker span of a move row.
    fn prefix(&self, gutter: char, idx: Option<usize>, marker: char) -> String {
        let idx = match idx {
            Some(i) => format!("{i:0w$}", w = self.idx_width),
            None => "·".repeat(self.idx_width),
        };
        format!("{gutter} step {idx}  {marker}  ")
    }

    /// kind + name + content columns, padded; tag appended after the content column.
    fn columns(&self, step: Option<&StepView>, content: &str, tag: &str) -> String {
        let kind = step.map_or("", |s| kind_label(s.kind));
        let name = step.map_or(String::new(), |s| truncate(&s.name, self.name_width));
        format!(
            "{kind:<kw$}  {name:<nw$}  {content:<cw$}  {tag}",
            kw = self.kind_width,
            nw = self.name_width,
            cw = self.content_width,
        )
    }
}

/// The row tag in this painter's framing: bracketed move labels, and the fork's verdict tag.
fn tag(row: &ViewRow) -> String {
    match row {
        ViewRow::Step(step_row) => format!("[{}]", move_label(step_row.kind)),
        ViewRow::Fork(fork_row) => format!("[FORK · {}]", fork_row.confidence),
    }
}

fn header_row(side: char, run: &RunHeader, id_width: usize, role_width: usize) -> Row {
    let outcome = run
        .outcome
        .map_or(String::new(), |o| format!(" · {}", outcome_label(o)));
    Row {
        role: Role::Header,
        prefix: String::new(),
        body: format!(
            "  {side}  {id:<id_width$}  ·  {role:<role_width$} · {n} steps{outcome}",
            id = run.id,
            role = run.role.label(),
            n = run.n_steps,
        ),
    }
}

/// A regular (non-fork) move row: dim spine upstream, amber-marked downstream.
fn move_row(step_row: &StepRow, cols: &Columns) -> Row {
    let step = step_row.step.front();
    let (role, marker) = match step_row.role {
        RowRole::Spine => (Role::Spine, '·'),
        RowRole::Downstream => (Role::Downstream, '✗'),
    };
    Row {
        role,
        prefix: cols.prefix(' ', step_row.step.display_idx(), marker),
        body: cols.columns(
            step,
            &truncate(step.map_or("", |s| s.summary.as_str()), cols.content_width),
            &format!("[{}]", move_label(step_row.kind)),
        ),
    }
}

/// The fork block: `⑂` gutter line with the A side, a continuation line with the B side, then
/// any field-level `-`/`+` diffs — the only red/green in the whole output.
fn fork_block(rows: &mut Vec<Row>, fork: &ForkRow, cols: &Columns) {
    let tag = format!("[FORK · {}]", fork.confidence);
    let a_lines = wrap(&format!("A: {}", fork.side_a), cols.content_width);
    let b_lines = wrap(&format!("B: {}", fork.side_b), cols.content_width);

    let step = fork.step.front();
    rows.push(Row {
        role: Role::Fork,
        prefix: cols.prefix('⑂', fork.step.display_idx(), '✗'),
        body: cols.columns(step, &a_lines[0], &tag),
    });
    // Continuation lines land in the content column.
    let cont_indent = cols.prefix_width + cols.kind_width + 2 + cols.name_width + 2;
    for line in a_lines[1..].iter().chain(&b_lines) {
        rows.push(Row {
            role: Role::Fork,
            prefix: " ".repeat(cont_indent),
            body: line.clone(),
        });
    }

    // Field-level diff, indented to the kind column, hard-wrapped.
    let field_width = |prefix_len: usize| cols.content_width.max(20).saturating_sub(prefix_len);
    for fd in &fork.step.field_diffs {
        let mut push_side = |sign: char, role: Role, value: &Option<SlotText>| {
            let Some(value) = value else { return };
            let text = format!("{sign} {}: {value}", fd.path);
            for line in wrap(&text, field_width(2) + cols.name_width) {
                rows.push(Row {
                    role,
                    prefix: " ".repeat(cols.prefix_width),
                    body: line,
                });
            }
        };
        push_side('-', Role::FieldRemoved, &fd.removed);
        push_side('+', Role::FieldAdded, &fd.added);
    }
}

/// Truncate to `width` chars, ellipsis-terminated. Char-count based (documented
/// approximation; East-Asian double-width is out of scope for v1).
fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Greedy word wrap at `width` chars (DESIGN.md: never rely on horizontal scroll). Words
/// longer than a line are hard-chunked. Always returns at least one line.
fn wrap(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in s.split_whitespace() {
        let fits_appended =
            !current.is_empty() && current.chars().count() + 1 + word.chars().count() <= width;
        if fits_appended {
            current.push(' ');
            current.push_str(word);
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        // Start a fresh line; a word wider than the line itself gets hard-chunked.
        let chars: Vec<char> = word.chars().collect();
        let mut chunks = chars.chunks(width).peekable();
        while let Some(chunk) = chunks.next() {
            let piece: String = chunk.iter().collect();
            if chunks.peek().is_some() {
                lines.push(piece);
            } else {
                current = piece;
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Split at a char boundary (not a byte offset — prefixes contain `⑂`).
fn split_at_char(s: &str, chars: usize) -> (&str, &str) {
    let byte = s
        .char_indices()
        .nth(chars)
        .map_or(s.len(), |(byte, _)| byte);
    s.split_at(byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{
        Attribution, AttributionMode, DiffResult, FieldDiff, FieldDiffKind, Fork, Meta, Move,
        Outcome, Run, RunPair, RunRef, Source, Step, test_support,
    };
    use serde_json::json;

    const WIDTH: usize = 100;

    /// The design-system escape codes under test.
    const SGR_AMBER_TRUECOLOR: &str = "\x1b[38;2;255;122;26m"; // #FF7A1A
    const SGR_RED_TRUECOLOR: &str = "\x1b[38;2;255;92;92m"; // #FF5C5C
    const SGR_GREEN_TRUECOLOR: &str = "\x1b[38;2;70;211;154m"; // #46D39A

    fn opts(color: ColorMode) -> RenderOpts {
        RenderOpts {
            color,
            width: WIDTH,
        }
    }

    /// The full pipeline under test: view computed from the diff, then painted.
    fn paint(result: &DiffResult, reference: &Run, observed: &Run, opts: &RenderOpts) -> String {
        render(&ViewModel::compute(result, reference, observed), opts)
    }

    // Field lists live in amberfork-model's test-support builders (issue #22); these one-line
    // adapters keep call sites in the shape the assertions read.

    fn step(idx: usize, name: &str, out: &str) -> Step {
        test_support::step(idx, name).text_output(out).build()
    }

    fn run(id: &str, outcome: Outcome, steps: Vec<Step>) -> Run {
        test_support::run(id, steps).outcome(outcome).build()
    }

    fn result(a: &Run, b: &Run, alignment: Vec<Move>, fork: Option<Fork>) -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: RunRef {
                    id: a.id.clone(),
                    task: None,
                    outcome: a.outcome,
                    n_steps: a.steps.len(),
                },
                b: RunRef {
                    id: b.id.clone(),
                    task: None,
                    outcome: b.outcome,
                    n_steps: b.steps.len(),
                },
            },
            alignment,
            fork,
            field_diffs: Vec::new(),
            attribution: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Passive),
        }
    }

    /// Converged pair: three identical steps, all-sync alignment, no fork.
    fn converged() -> (Run, Run, DiffResult) {
        let steps = || {
            vec![
                step(0, "plan", "search for census data"),
                step(1, "web.search", "9 results, top census.gov"),
                step(2, "answer", "population is 8,443,000"),
            ]
        };
        let a = run("good", Outcome::Pass, steps());
        let b = run("good_again", Outcome::Pass, steps());
        let alignment = vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::sync(1, 1, 0.0, 1.0),
            Move::sync(2, 2, 0.0, 1.0),
        ];
        let res = result(&a, &b, alignment, None);
        (a, b, res)
    }

    /// Forked pair: two clean syncs, a high-cost sync (the fork), one log move downstream.
    /// Field diffs attached at the fork index so the red/green containment rule is exercised.
    fn forked(confidence: f64) -> (Run, Run, DiffResult) {
        let a = run(
            "good",
            Outcome::Pass,
            vec![
                step(0, "plan", "search for census data"),
                step(1, "web.search", "9 results, top census.gov"),
                step(2, "web.fetch", "census.gov page: population 8,443,000"),
            ],
        );
        let b = run(
            "bad",
            Outcome::Fail,
            vec![
                step(0, "plan", "search for census data"),
                step(1, "web.search", "9 results, top census.gov"),
                step(
                    2,
                    "web.fetch",
                    "blogspot page: the city has grown to 9,100,000",
                ),
                step(3, "reader", "blog says about 9,100,000 people"),
            ],
        );
        let alignment = vec![
            Move::sync(0, 0, 0.02, 0.98),
            Move::sync(1, 1, 0.05, 0.95),
            Move::sync(2, 2, 0.82, 0.18),
            Move::log(3, 0.6, 0.9),
        ];
        let fork = Fork {
            index: 2,
            a_step: Some(2),
            b_step: Some(2),
            confidence,
        };
        let mut res = result(&a, &b, alignment, Some(fork));
        res.field_diffs = vec![FieldDiff {
            step: 2,
            path: "outputs".to_string(),
            before: Some(json!("census.gov page")),
            after: Some(json!("blogspot page")),
            kind: FieldDiffKind::Changed,
        }];
        // What the engine's static mode emits for this fork (amberfork-align issue #12).
        res.attribution = Some(Attribution {
            mode: AttributionMode::Static,
            origin_step: Some(2),
            propagation: vec![3],
            counterfactual: None,
            cause_label: None,
            confidence,
        });
        (a, b, res)
    }

    /// Converged-but-not-identical pair (issue #19): no fork, yet the alignment absorbed a
    /// costly sync and a model move — the runs are not identical, and the one line everyone
    /// reads must keep the two states apart.
    fn absorbed() -> (Run, Run, DiffResult) {
        let a = run(
            "good",
            Outcome::Pass,
            vec![
                step(0, "plan", "search for census data"),
                step(1, "web.search", "9 results, top census.gov"),
                step(2, "answer", "population is 8,443,000"),
                step(3, "log", "cache the answer"),
            ],
        );
        let b = run(
            "good_retry",
            Outcome::Pass,
            vec![
                step(0, "plan", "search for census data"),
                step(1, "web.search", "9 results, top census.gov"),
                step(2, "answer", "population is about 8,443,000"),
            ],
        );
        let alignment = vec![
            Move::sync(0, 0, 0.0, 1.0),
            Move::sync(1, 1, 0.0, 1.0),
            Move::sync(2, 2, 0.55, 0.45),
            Move::model(3, 0.6, 0.0),
        ];
        let res = result(&a, &b, alignment, None);
        (a, b, res)
    }

    /// Remove ANSI SGR sequences (`ESC [ ... m`).
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                for esc in chars.by_ref() {
                    if esc == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn converged_renders_the_designed_state() {
        let (a, b, res) = converged();
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));

        assert!(
            out.contains("converged — identical through 3 steps"),
            "DR1 designed converged state, got:\n{out}"
        );
        assert!(!out.contains('⑂'), "no fork glyph on a converged diff");
        assert!(
            !out.contains('✗'),
            "no divergence markers on a converged diff"
        );
        assert!(
            !out.contains("attribution"),
            "nothing to attribute on a converged diff"
        );
        assert!(!out.contains('\x1b'), "Plain mode must emit no ANSI");
    }

    #[test]
    fn converged_with_absorbed_divergence_does_not_claim_identical() {
        let (a, b, res) = absorbed();
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));

        assert!(
            out.contains("converged — no fork (2 absorbed divergences across 4⇄3 steps)"),
            "absorbed-divergence footer, got:\n{out}"
        );
        assert!(
            !out.contains("identical"),
            "'identical' is reserved for all-sync, all-cost-0 alignments, got:\n{out}"
        );
    }

    #[test]
    fn forked_render_closes_with_the_attribution_line() {
        let (a, b, res) = forked(0.47);
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));

        let last = out.lines().last().expect("non-empty render");
        assert_eq!(
            last, "  attribution · static · origin step 02 · propagation step 03 · conf 0.47",
            "every forked diff ends with a designed answer line, like converged does"
        );
    }

    #[test]
    fn fork_line_carries_glyph_confidence_and_downstream_markers() {
        let (a, b, res) = forked(0.47);
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));
        let lines: Vec<&str> = out.lines().collect();

        let fork_at = lines
            .iter()
            .position(|l| l.contains('⑂'))
            .expect("fork line carries the ⑂ gutter glyph");
        assert!(
            lines[fork_at].contains("[FORK · conf 0.47]"),
            "fork tag with confidence, got: {}",
            lines[fork_at]
        );
        assert!(
            lines[fork_at].contains('✗'),
            "fork line carries the non-color divergence marker"
        );

        // The fork block shows both sides' content.
        assert!(out.contains("A: "), "fork block shows the reference side");
        assert!(out.contains("B: "), "fork block shows the observed side");

        // Downstream of the fork: ✗ markers (the log move on b step 3).
        let downstream = lines
            .iter()
            .skip(fork_at + 1)
            .find(|l| l.contains("[log-move]"))
            .expect("downstream move is rendered");
        assert!(downstream.contains('✗'), "downstream keeps the ✗ marker");

        // Upstream sync lines skate by: · marker, never ✗, never ⑂.
        for l in &lines[..fork_at] {
            assert!(!l.contains('✗') && !l.contains('⑂'), "pre-fork line: {l}");
        }
    }

    #[test]
    fn zero_confidence_renders_an_explicit_marginal_call() {
        let (a, b, res) = forked(0.0);
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));

        assert!(
            out.contains("[FORK · marginal call]"),
            "notebook 005: conf 0 is a designed weak-call state, got:\n{out}"
        );
        assert!(
            out.contains("propagation step 03 · marginal call"),
            "the attribution line follows the same weak-call rule, got:\n{out}"
        );
        assert!(
            !out.contains("conf 0.0"),
            "never render the marginal state as a small number"
        );
    }

    #[test]
    fn red_green_confined_to_field_diff_lines_and_amber_to_the_fork() {
        let (a, b, res) = forked(0.47);
        let out = paint(&res, &a, &b, &opts(ColorMode::Truecolor));
        let fork_at = out
            .lines()
            .position(|l| l.contains('⑂'))
            .expect("fork line present");

        for (i, line) in out.lines().enumerate() {
            let plain = strip_ansi(line);
            let body = plain.trim_start();
            if line.contains(SGR_RED_TRUECOLOR) {
                assert!(body.starts_with('-'), "red outside a `-` line: {plain}");
            }
            if line.contains(SGR_GREEN_TRUECOLOR) {
                assert!(body.starts_with('+'), "green outside a `+` line: {plain}");
            }
            if line.contains(SGR_AMBER_TRUECOLOR) {
                assert!(
                    i >= fork_at,
                    "amber before the fork (sameness must recede): {plain}"
                );
            }
        }
        assert!(
            out.contains(SGR_RED_TRUECOLOR) && out.contains(SGR_GREEN_TRUECOLOR),
            "field diff at the fork renders in red/green"
        );
        assert!(out.contains(SGR_AMBER_TRUECOLOR), "the fork glows amber");
    }

    #[test]
    fn structure_is_identical_across_color_modes() {
        let (a, b, res) = forked(0.47);
        let colored = paint(&res, &a, &b, &opts(ColorMode::Truecolor));
        let plain = paint(&res, &a, &b, &opts(ColorMode::Plain));
        assert_eq!(
            strip_ansi(&colored),
            plain,
            "color is styling only; structure is the contract"
        );
    }

    #[test]
    fn every_line_fits_the_requested_width() {
        let (a, b, res) = forked(0.47);
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));
        for line in out.lines() {
            assert!(
                line.chars().count() <= WIDTH,
                "line exceeds width {WIDTH}: {line}"
            );
        }
    }

    // ── Slice-0 snapshot net (issue #21) ─────────────────────────────────────────
    // Four file snapshots that byte-lock the render ahead of the amberfork-layout
    // extraction: the rewrite is output-locked, so it must land with zero churn in
    // these files. They live at the unit level because two of the axes cannot be
    // reached through the binary — piped stdout always resolves to Plain (no ANSI
    // ever leaves a non-TTY), and width comes from the terminal, not a flag.

    /// ESC made visible (`␛`, U+241B) so the truecolor snapshot stays readable in a
    /// git diff. The character never appears in render output, so the substitution
    /// is bijective — the snapshot still pins the exact escape bytes.
    fn visible_ansi(s: &str) -> String {
        s.replace('\x1b', "␛")
    }

    #[test]
    fn snapshot_forked_truecolor() {
        let (a, b, res) = forked(0.47);
        let out = paint(&res, &a, &b, &opts(ColorMode::Truecolor));
        insta::assert_snapshot!("forked_truecolor", visible_ansi(&out));
    }

    #[test]
    fn snapshot_forked_narrow_width() {
        // 60 is the floor `main` enforces on detected terminal widths.
        let (a, b, res) = forked(0.47);
        let narrow = RenderOpts {
            color: ColorMode::Plain,
            width: 60,
        };
        insta::assert_snapshot!("forked_narrow_width_60", paint(&res, &a, &b, &narrow));
    }

    #[test]
    fn snapshot_converged_identical() {
        let (a, b, res) = converged();
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));
        insta::assert_snapshot!("converged_identical", out);
    }

    #[test]
    fn snapshot_converged_absorbed_divergence() {
        let (a, b, res) = absorbed();
        let out = paint(&res, &a, &b, &opts(ColorMode::Plain));
        insta::assert_snapshot!("converged_absorbed_divergence", out);
    }

    #[test]
    fn color_ladder_resolves_per_design() {
        use ColorMode::{Ansi256, Bold, Plain, Truecolor};
        let resolve = resolve_color_mode;

        // Opt-outs win over everything.
        assert_eq!(
            resolve(true, true, None, Some("xterm"), Some("truecolor")),
            Plain
        );
        assert_eq!(
            resolve(false, true, Some("1"), Some("xterm"), Some("truecolor")),
            Plain
        );
        assert_eq!(
            resolve(false, false, None, Some("xterm"), Some("truecolor")),
            Plain
        );
        assert_eq!(
            resolve(false, true, None, Some("dumb"), Some("truecolor")),
            Plain
        );
        // An empty NO_COLOR is "not set" per no-color.org.
        assert_eq!(
            resolve(false, true, Some(""), None, Some("truecolor")),
            Truecolor
        );

        // The ladder: truecolor → 256 → bold.
        assert_eq!(
            resolve(false, true, None, Some("xterm-256color"), Some("truecolor")),
            Truecolor
        );
        assert_eq!(
            resolve(false, true, None, Some("xterm-256color"), Some("24bit")),
            Truecolor
        );
        assert_eq!(
            resolve(false, true, None, Some("xterm-256color"), None),
            Ansi256
        );
        assert_eq!(resolve(false, true, None, Some("xterm"), None), Bold);
    }
}
