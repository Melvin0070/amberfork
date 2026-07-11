//! Terminal render of a [`DiffResult`] — DESIGN.md "Terminal rendering (first-class surface)".
//!
//! North star: *sameness recedes, divergence glows.* Sync steps are one dim line each; the
//! fork carries a `⑂` gutter glyph and amber; every step downstream of the fork keeps a `✗`
//! marker in the same single amber (DR4). Red/green appear only on the `-`/`+` field-diff
//! lines inside the fork block, nowhere else. Structure — glyphs, markers, tags — carries the
//! whole signal without color (DR2): a plain render is byte-identical to a colored one with
//! the escapes stripped, which is a unit-tested invariant here.
//!
//! Confidence follows notebook 005: rendered as `conf 0.NN` in the fork tag, except
//! `confidence == 0` — the designed weak-call state (evidence ≤ τ) — which renders as an
//! explicit `marginal call`, never a small number.
//!
//! Pure by design: [`render`] is a function of the diff, the two runs, and [`RenderOpts`];
//! TTY/env sniffing lives in [`resolve_color_mode`] so `main` decides once at the edge.

use amberfork_model::{DiffResult, FieldDiffKind, MoveKind, Payload, Run, Step};
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

/// Inputs the renderer needs beyond the data itself.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub color: ColorMode,
    /// Target line width; content is truncated or hard-wrapped to fit (never horizontal
    /// scroll).
    pub width: usize,
}

/// Render the human-readable diff. `reference` is side `a` (`--against`), `observed` is side
/// `b` (the failing run). Returns the full output including the trailing newline.
pub fn render(result: &DiffResult, reference: &Run, observed: &Run, opts: &RenderOpts) -> String {
    let layout = Layout::compute(result, reference, observed, opts.width);
    let mut rows = Vec::new();

    let id_width = result
        .runs
        .a
        .id
        .chars()
        .count()
        .max(result.runs.b.id.chars().count());
    rows.push(header_row(
        'A',
        &result.runs.a.id,
        id_width,
        "reference",
        reference,
    ));
    rows.push(header_row(
        'B',
        &result.runs.b.id,
        id_width,
        "observed ",
        observed,
    ));
    rows.push(Row::blank());

    for (i, mv) in result.alignment.iter().enumerate() {
        match result.fork {
            Some(fork) if i == fork.index => {
                fork_block(&mut rows, result, reference, observed, &layout, i)
            }
            Some(fork) if i > fork.index => {
                rows.push(move_row(mv, reference, observed, &layout, Role::Downstream))
            }
            _ => rows.push(move_row(mv, reference, observed, &layout, Role::Spine)),
        }
    }

    if result.fork.is_none() {
        rows.push(Row::blank());
        // "identical" is a claim the alignment must earn: every move a sync at cost 0.
        // Anything the resync rule merely absorbed (gap moves, costly syncs) converged
        // without being identical, and the one line everyone reads must keep the two
        // states apart (issue #19).
        let absorbed = result
            .alignment
            .iter()
            .filter(|mv| mv.kind != MoveKind::Sync || mv.cost > 0.0)
            .count();
        let body = if absorbed == 0 {
            format!(
                "  converged — identical through {} steps",
                result.alignment.len()
            )
        } else {
            format!(
                "  converged — no fork ({absorbed} absorbed divergence{} across {}⇄{} steps)",
                if absorbed == 1 { "" } else { "s" },
                reference.steps.len(),
                observed.steps.len(),
            )
        };
        rows.push(Row {
            role: Role::Footer,
            prefix: String::new(),
            body,
        });
    }

    // The forked counterpart of the converged footer: every diff ends with a designed answer
    // line. Plain, not amber — it is a statement about the divergence, not the divergence.
    if let Some(attribution) = &result.attribution {
        rows.push(Row::blank());
        rows.push(Row {
            role: Role::Footer,
            prefix: String::new(),
            body: attribution_line(attribution, &layout),
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

/// Column widths, computed once from the data so every row aligns (tabular discipline).
struct Layout {
    /// Digits used for zero-padded step indices.
    idx_width: usize,
    /// `⑂ step NN  ✗  ` — everything before the kind column.
    prefix_width: usize,
    kind_width: usize,
    name_width: usize,
    content_width: usize,
}

impl Layout {
    fn compute(result: &DiffResult, reference: &Run, observed: &Run, width: usize) -> Self {
        let max_idx = reference
            .steps
            .iter()
            .chain(&observed.steps)
            .map(|s| s.idx)
            .max()
            .unwrap_or(0);
        let idx_width = decimal_digits(max_idx).max(2);
        // "⑂ " + "step " + idx + "  " + marker + "  "
        let prefix_width = 2 + 5 + idx_width + 2 + 1 + 2;
        let kind_width = 5; // the longest canonical kind, "agent"/"other"
        let name_width = reference
            .steps
            .iter()
            .chain(&observed.steps)
            .map(|s| s.name.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(4, 16);
        let tag_width = result
            .alignment
            .iter()
            .enumerate()
            .map(|(i, mv)| tag_text(result, i, mv).chars().count())
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
    fn columns(&self, step: Option<&Step>, content: &str, tag: &str) -> String {
        let kind = step.map_or("", |s| kind_str(s.kind));
        let name = step.map_or(String::new(), |s| truncate(&s.name, self.name_width));
        format!(
            "{kind:<kw$}  {name:<nw$}  {content:<cw$}  {tag}",
            kw = self.kind_width,
            nw = self.name_width,
            cw = self.content_width,
        )
    }
}

fn header_row(side: char, id: &str, id_width: usize, role_label: &str, run: &Run) -> Row {
    let outcome = run
        .outcome
        .map_or(String::new(), |o| format!(" · {}", outcome_str(o)));
    Row {
        role: Role::Header,
        prefix: String::new(),
        body: format!(
            "  {side}  {id:<id_width$}  ·  {role_label} · {n} steps{outcome}",
            n = run.steps.len()
        ),
    }
}

/// A regular (non-fork) move row: dim spine upstream, amber-marked downstream.
fn move_row(
    mv: &amberfork_model::Move,
    reference: &Run,
    observed: &Run,
    layout: &Layout,
    role: Role,
) -> Row {
    // The observed (failing) run is the one being debugged, so its step fronts the line;
    // a model move only exists on the reference side.
    let step = pick_step(mv, reference, observed);
    let marker = if role == Role::Downstream {
        '✗'
    } else {
        '·'
    };
    Row {
        role,
        prefix: layout.prefix(' ', display_idx(mv), marker),
        body: layout.columns(
            step,
            &truncate(&step.map_or(String::new(), summarize), layout.content_width),
            &tag_str(mv.kind),
        ),
    }
}

/// The fork block: `⑂` gutter line with the A side, a continuation line with the B side, then
/// any field-level `-`/`+` diffs — the only red/green in the whole output.
fn fork_block(
    rows: &mut Vec<Row>,
    result: &DiffResult,
    reference: &Run,
    observed: &Run,
    layout: &Layout,
    index: usize,
) {
    let mv = &result.alignment[index];
    let fork = result.fork.expect("fork_block is only called with a fork");

    let side_content = |step: Option<usize>, run: &Run, label: char| match step {
        Some(idx) => format!(
            "{label}: {}",
            run.steps.get(idx).map_or_else(String::new, summarize)
        ),
        None => format!("{label}: (no aligned step)"),
    };

    let tag = format!("[FORK · {}]", conf_text(fork.confidence));

    let a_lines = wrap(
        &side_content(fork.a_step, reference, 'A'),
        layout.content_width,
    );
    let b_lines = wrap(
        &side_content(fork.b_step, observed, 'B'),
        layout.content_width,
    );

    let step = pick_step(mv, reference, observed);
    rows.push(Row {
        role: Role::Fork,
        prefix: layout.prefix('⑂', display_idx(mv), '✗'),
        body: layout.columns(step, &a_lines[0], &tag),
    });
    // Continuation lines land in the content column.
    let cont_indent = layout.prefix_width + layout.kind_width + 2 + layout.name_width + 2;
    for line in a_lines[1..].iter().chain(&b_lines) {
        rows.push(Row {
            role: Role::Fork,
            prefix: " ".repeat(cont_indent),
            body: line.clone(),
        });
    }

    // Field-level diff, indented to the kind column, hard-wrapped.
    let field_width = |prefix_len: usize| layout.content_width.max(20).saturating_sub(prefix_len);
    for fd in result.field_diffs.iter().filter(|fd| fd.step == index) {
        let mut push_side = |sign: char, role: Role, value: &Option<serde_json::Value>| {
            let Some(value) = value else { return };
            let text = format!("{sign} {}: {}", fd.path, compact_json(value));
            for line in wrap(&text, field_width(2) + layout.name_width) {
                rows.push(Row {
                    role,
                    prefix: " ".repeat(layout.prefix_width),
                    body: line,
                });
            }
        };
        if fd.kind != FieldDiffKind::Added {
            push_side('-', Role::FieldRemoved, &fd.before);
        }
        if fd.kind != FieldDiffKind::Removed {
            push_side('+', Role::FieldAdded, &fd.after);
        }
    }
}

/// The step fronting a move's line: the observed side where it exists (that's the run being
/// debugged), the reference side for model-only moves.
fn pick_step<'r>(
    mv: &amberfork_model::Move,
    reference: &'r Run,
    observed: &'r Run,
) -> Option<&'r Step> {
    match mv.kind {
        MoveKind::Sync | MoveKind::Log => mv.b_idx.and_then(|i| observed.steps.get(i)),
        MoveKind::Model => mv.a_idx.and_then(|i| reference.steps.get(i)),
    }
}

/// The step index shown in the gutter, matching [`pick_step`]'s side choice.
fn display_idx(mv: &amberfork_model::Move) -> Option<usize> {
    mv.b_idx.or(mv.a_idx)
}

/// One-line gist of a step: its output (else input) text, first line only.
fn summarize(step: &Step) -> String {
    let payload = step.outputs.as_ref().or(step.inputs.as_ref());
    match payload {
        None => "(no content captured)".to_string(),
        Some(Payload::Text(t)) => t.lines().next().unwrap_or("").to_string(),
        Some(Payload::Object(map)) => compact_json(&serde_json::Value::Object(map.clone())),
        Some(Payload::Other(v)) => compact_json(v),
    }
}

fn tag_text(result: &DiffResult, index: usize, mv: &amberfork_model::Move) -> String {
    match result.fork {
        Some(fork) if fork.index == index => format!("[FORK · {}]", conf_text(fork.confidence)),
        _ => tag_str(mv.kind),
    }
}

/// Confidence per notebook 005: zero — the designed weak-call state (evidence ≤ τ) — is
/// stated in words, never rendered as a small number.
fn conf_text(confidence: f64) -> String {
    if confidence <= f64::EPSILON {
        "marginal call".to_string()
    } else {
        format!("conf {confidence:.2}")
    }
}

/// The attribution footer: mode, origin, propagation, confidence — the reading order the
/// attribution pane uses (DESIGN.md decisions log, DR5), flattened to one line.
fn attribution_line(attribution: &amberfork_model::Attribution, layout: &Layout) -> String {
    use amberfork_model::AttributionMode;
    let mode = match attribution.mode {
        AttributionMode::Static => "static",
        AttributionMode::Counterfactual => "counterfactual",
    };
    let origin = attribution.origin_step.map_or_else(
        || "origin unlocalized".to_string(),
        |s| format!("origin step {s:0w$}", w = layout.idx_width),
    );
    format!(
        "  attribution · {mode} · {origin} · propagation {} · {}",
        steps_text(&attribution.propagation, layout.idx_width),
        conf_text(attribution.confidence)
    )
}

/// A step list in the gutter's zero-padded style: `none`, `step 03`, a contiguous
/// `steps 03–07`, or a comma list when a future mode emits gaps.
fn steps_text(steps: &[usize], idx_width: usize) -> String {
    let pad = |s: &usize| format!("{s:0idx_width$}");
    let contiguous = steps.windows(2).all(|w| w[1] == w[0] + 1);
    match steps {
        [] => "none".to_string(),
        [only] => format!("step {}", pad(only)),
        [first, .., last] if contiguous => format!("steps {}–{}", pad(first), pad(last)),
        _ => format!(
            "steps {}",
            steps.iter().map(pad).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn tag_str(kind: MoveKind) -> String {
    match kind {
        MoveKind::Sync => "[sync]",
        MoveKind::Log => "[log-move]",
        MoveKind::Model => "[model-move]",
    }
    .to_string()
}

fn kind_str(kind: amberfork_model::StepKind) -> &'static str {
    use amberfork_model::StepKind;
    match kind {
        StepKind::Llm => "llm",
        StepKind::Tool => "tool",
        StepKind::Agent => "agent",
        StepKind::Other => "other",
    }
}

fn outcome_str(outcome: amberfork_model::Outcome) -> &'static str {
    use amberfork_model::Outcome;
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Fail => "fail",
        Outcome::Unknown => "unknown",
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn decimal_digits(n: usize) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
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
        Outcome, Payload, Run, RunPair, RunRef, SchemaVersion, Source, Step, StepKind,
    };
    use serde_json::{Map, json};

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

    fn step(idx: usize, name: &str, out: &str) -> Step {
        Step {
            idx,
            kind: StepKind::Tool,
            name: name.to_string(),
            inputs: None,
            outputs: Some(Payload::Text(out.to_string())),
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        }
    }

    fn run(id: &str, outcome: Outcome, steps: Vec<Step>) -> Run {
        Run {
            schema_version: SchemaVersion::current(),
            id: id.to_string(),
            task: None,
            outcome: Some(outcome),
            steps,
            edges: None,
        }
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
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));

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
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));

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
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));

        let last = out.lines().last().expect("non-empty render");
        assert_eq!(
            last, "  attribution · static · origin step 02 · propagation step 03 · conf 0.47",
            "every forked diff ends with a designed answer line, like converged does"
        );
    }

    #[test]
    fn fork_line_carries_glyph_confidence_and_downstream_markers() {
        let (a, b, res) = forked(0.47);
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));
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
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));

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
        let out = render(&res, &a, &b, &opts(ColorMode::Truecolor));
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
        let colored = render(&res, &a, &b, &opts(ColorMode::Truecolor));
        let plain = render(&res, &a, &b, &opts(ColorMode::Plain));
        assert_eq!(
            strip_ansi(&colored),
            plain,
            "color is styling only; structure is the contract"
        );
    }

    #[test]
    fn every_line_fits_the_requested_width() {
        let (a, b, res) = forked(0.47);
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));
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
        let out = render(&res, &a, &b, &opts(ColorMode::Truecolor));
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
        insta::assert_snapshot!("forked_narrow_width_60", render(&res, &a, &b, &narrow));
    }

    #[test]
    fn snapshot_converged_identical() {
        let (a, b, res) = converged();
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));
        insta::assert_snapshot!("converged_identical", out);
    }

    #[test]
    fn snapshot_converged_absorbed_divergence() {
        let (a, b, res) = absorbed();
        let out = render(&res, &a, &b, &opts(ColorMode::Plain));
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
