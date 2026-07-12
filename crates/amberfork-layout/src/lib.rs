//! The semantic view-model over a [`DiffResult`] — the one seam every painter reads
//! (issue #21, design doc Amendment 2026-07-11).
//!
//! ```text
//!            DiffResult  (+ the two Runs)
//!                     │
//!                     ▼
//!             amberfork-layout
//!                     │
//!                     ▼
//!                 ViewModel
//!        semantic rows (spine / fork / downstream), step summaries,
//!        designed wording (confidence, verdict, absence), attribution
//!        reading order, field-diff evidence
//!                     │
//!         ┌───────────┴───────────┐
//!         ▼                       ▼
//!   CLI painter (render.rs)   web painter (ui/, Leptos)
//!   columns · glyphs · ANSI   SVG/DOM geometry
//! ```
//!
//! What lives here is exactly what every surface must agree on: which alignment move plays
//! which role, the one-line gist of a step, the wording rules the design locked — confidence
//! as `conf 0.NN` with the explicit `marginal call` at zero (notebook 005), the converged
//! verdict that only claims "identical" when the alignment earned it (issue #19), the
//! `(no aligned step)` absence — plus the attribution reading order (DR5) and the
//! field-level evidence at the fork.
//!
//! What deliberately does NOT live here is any painter's own arithmetic: column widths,
//! truncation, wrapping, gutter glyphs, and ANSI are the CLI painter's business; pixel
//! geometry is the web painter's. This crate has zero terminal dependencies, and styling
//! decisions never feed back into it.
//!
//! For the web painter the view crosses a wire: [`Document`] is the serializable form the
//! server ships (issue #24) — the same [`ViewModel`] plus a [`DOCUMENT_VERSION`] stamp and
//! the payload envelope: building the document cuts every payload-derived slot at
//! [`SLOT_TEXT_LIMIT`] bytes with an explicit [`SlotText::truncated`] mark the UI must
//! render visibly.

use std::fmt;

use amberfork_model::{
    Attribution, AttributionMode, DiffResult, FieldDiffKind, MoveKind, Outcome, Payload, Run, Step,
    StepKind, Warning,
};
use serde::{Deserialize, Serialize};

/// The document version this build emits. A bare wire-hygiene marker (issue #24): the web UI
/// and the server ship in one binary, lockstep by construction, so there is no read-gate —
/// bump it when the document's shape changes so a stale payload is at least identifiable.
pub const DOCUMENT_VERSION: &str = "0.1";

/// The wire form of the seam: what `amberfork serve` ships to the web painter. The body is
/// the SAME [`ViewModel`] the terminal paints — the document only adds wire hygiene on top.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Always [`DOCUMENT_VERSION`] when built by this crate.
    pub schema_version: String,
    pub view: ViewModel,
}

impl Document {
    /// Wrap a view for the wire: stamp the current document version and enforce the payload
    /// envelope. There is deliberately no other way to build a [`Document`], so an
    /// over-limit payload can never reach the browser unmarked (issue #24).
    #[must_use]
    pub fn new(mut view: ViewModel) -> Self {
        envelope(&mut view);
        Self {
            schema_version: DOCUMENT_VERSION.to_string(),
            view,
        }
    }
}

/// The payload envelope's per-slot byte limit. The threat is multi-MB field-diff payloads
/// (eng review D12+D17): a slot is one display line, so 4 KiB loses nothing any pane shows
/// unexpanded (expand-on-demand is issue #30's job) while keeping a worst-case document of
/// hundreds of rows around a megabyte. Bytes, not chars, because the guard protects wire
/// size; the cut backs off to a UTF-8 char boundary.
pub const SLOT_TEXT_LIMIT: usize = 4096;

/// Cut every payload-derived slot in the view down to [`SLOT_TEXT_LIMIT`], marking the cut.
fn envelope(view: &mut ViewModel) {
    for row in &mut view.rows {
        match row {
            Row::Step(step_row) => envelope_step(&mut step_row.step),
            Row::Fork(fork) => {
                envelope_step(&mut fork.step);
                fork.side_a.truncate_to(SLOT_TEXT_LIMIT);
                fork.side_b.truncate_to(SLOT_TEXT_LIMIT);
                for fd in &mut fork.field_diffs {
                    if let Some(removed) = &mut fd.removed {
                        removed.truncate_to(SLOT_TEXT_LIMIT);
                    }
                    if let Some(added) = &mut fd.added {
                        added.truncate_to(SLOT_TEXT_LIMIT);
                    }
                }
            }
        }
    }
}

fn envelope_step(step: &mut AlignedStep) {
    for side in [&mut step.a, &mut step.b].into_iter().flatten() {
        side.summary.truncate_to(SLOT_TEXT_LIMIT);
    }
}

/// One payload-derived text slot. [`ViewModel::compute`] always produces the full text;
/// only the payload envelope in [`Document::new`] sets `truncated` — so the CLI painter,
/// which reads the view directly, never sees a cut slot. Generated wording (verdicts,
/// confidence, attribution, warnings) is bounded by construction and stays plain `String`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotText {
    pub text: String,
    /// Whether the envelope cut this slot at [`SLOT_TEXT_LIMIT`]. A UI must render the
    /// mark visibly — a silently shortened payload reads as the payload.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

impl SlotText {
    /// A full, unmarked slot — the only form [`ViewModel::compute`] emits.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            truncated: false,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Cut to the largest char boundary within `limit` bytes and mark the cut.
    fn truncate_to(&mut self, limit: usize) {
        if self.text.len() <= limit {
            return;
        }
        let cut = (0..=limit)
            .rev()
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(0);
        self.text.truncate(cut);
        self.truncated = true;
    }
}

impl fmt::Display for SlotText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Content comparison — the mark is provenance, not content.
impl PartialEq<&str> for SlotText {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The full semantic view of one diff: everything a painter needs, nothing it must compute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewModel {
    pub run_a: RunHeader,
    pub run_b: RunHeader,
    /// Digits for zero-padded step indices (`step 06`), derived from the runs' largest step
    /// index — shared number-formatting voice, not a column width (those are data-derived,
    /// these are medium-derived; only the former belong in the seam).
    pub idx_width: usize,
    pub rows: Vec<Row>,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<AttributionView>,
    /// Pass-through of the result's warnings so a painter never reaches back into
    /// [`DiffResult`]: the CLI keeps warnings on stderr, the web UI surfaces them inline.
    pub warnings: Vec<Warning>,
}

/// A run's identity as every surface introduces it (terminal header lines, web header bar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunHeader {
    pub id: String,
    pub role: RunRole,
    pub n_steps: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
}

/// Which seat a run occupies in the diff — the `DiffResult` contract's side convention:
/// `a` is always the reference, `b` the observed/failing run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunRole {
    Reference,
    Observed,
}

impl RunRole {
    /// The designed label every surface prints.
    pub fn label(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Observed => "observed",
        }
    }
}

/// One alignment move as a display row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Row {
    /// A regular move: spine before the fork, divergent path after it.
    Step(StepRow),
    /// The fork itself, with both sides' content and the field-level evidence.
    Fork(ForkRow),
}

impl Row {
    /// The aligned pair behind the row, whichever variant it is.
    pub fn step(&self) -> &AlignedStep {
        match self {
            Self::Step(row) => &row.step,
            Self::Fork(row) => &row.step,
        }
    }
}

/// Where a non-fork row stands relative to the fork — the role that drives every painter's
/// styling (sameness recedes, divergence glows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowRole {
    /// Before the fork, or every row of a converged diff. The eye skates over it.
    Spine,
    /// After the fork: the divergent path stays marked.
    Downstream,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRow {
    pub role: RowRole,
    /// The move's alignment kind; painters frame it as the row tag (the CLI brackets it).
    pub kind: MoveKind,
    pub step: AlignedStep,
}

/// The fork row: the first move of the block the alignment never recovers from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForkRow {
    pub step: AlignedStep,
    /// Reference-side content at the fork, resolved to the designed absence wording
    /// (`(no aligned step)`) when the fork has no step on that side.
    pub side_a: SlotText,
    /// Observed-side counterpart of `side_a`.
    pub side_b: SlotText,
    /// Designed confidence wording: `conf 0.NN`, or `marginal call` at zero (notebook 005).
    pub confidence: String,
    /// The only red/green in any surface: field-level `-`/`+` evidence at the fork.
    pub field_diffs: Vec<FieldDiffView>,
}

/// The two sides of one alignment move, resolved for display. Indices are kept separate
/// from the resolved views: a gap move has an index on one side only, and a painter still
/// shows the index even where (malformed hand-built input) it resolves to no step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignedStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_idx: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_idx: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<StepView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<StepView>,
}

impl AlignedStep {
    /// The step fronting a single-line rendering: the observed side where it exists — that
    /// is the run being debugged — the reference side for model-only moves.
    pub fn front(&self) -> Option<&StepView> {
        self.b.as_ref().or(self.a.as_ref())
    }

    /// The index shown in the gutter, matching [`Self::front`]'s side priority.
    pub fn display_idx(&self) -> Option<usize> {
        self.b_idx.or(self.a_idx)
    }
}

/// One side's step, resolved to what surfaces display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepView {
    pub kind: StepKind,
    pub name: String,
    /// One-line gist: first line of the step's output (else input) text, compact JSON for
    /// structured payloads, or the designed `(no content captured)`.
    pub summary: SlotText,
}

/// One field-level difference at the fork, values already in display form (compact JSON).
/// `removed`/`added` are what a painter shows on the `-`/`+` side; an added field has no
/// `removed`, a removed field no `added`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDiffView {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<SlotText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<SlotText>,
}

/// The one-line answer every diff ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Every move a sync at cost 0 — a claim the alignment must earn (issue #19).
    Identical {
        steps: usize,
    },
    /// No fork, but the alignment absorbed divergence (gap moves, costly syncs) on the way.
    Absorbed {
        absorbed: usize,
        a_steps: usize,
        b_steps: usize,
    },
    Forked,
}

impl Verdict {
    /// The designed converged statement; `None` when forked — the fork row and attribution
    /// carry the answer instead.
    pub fn converged_text(&self) -> Option<String> {
        match *self {
            Self::Identical { steps } => {
                Some(format!("converged — identical through {steps} steps"))
            }
            Self::Absorbed {
                absorbed,
                a_steps,
                b_steps,
            } => Some(format!(
                "converged — no fork ({absorbed} absorbed divergence{} across {a_steps}⇄{b_steps} steps)",
                if absorbed == 1 { "" } else { "s" },
            )),
            Self::Forked => None,
        }
    }
}

/// The attribution answer in DR5's reading order — mode, origin, propagation, confidence —
/// each part already in its designed wording. The terminal flattens the parts to one line;
/// the web attribution pane renders them as separate elements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionView {
    /// `static` or `counterfactual` — owned, not `&'static str`, because the view must
    /// deserialize on the web side of the wire.
    pub mode: String,
    /// `origin step 02` (gutter-padded) or `origin unlocalized`.
    pub origin: String,
    /// `step 03`, a contiguous `steps 03–07`, a comma list, or `none`.
    pub propagation: String,
    pub confidence: String,
}

impl ViewModel {
    /// Build the semantic view. Pure and total: any `DiffResult` over its two runs yields a
    /// view, and nothing here inspects the output medium.
    pub fn compute(result: &DiffResult, reference: &Run, observed: &Run) -> Self {
        let max_idx = reference
            .steps
            .iter()
            .chain(&observed.steps)
            .map(|s| s.idx)
            .max()
            .unwrap_or(0);
        let idx_width = decimal_digits(max_idx).max(2);

        let rows = result
            .alignment
            .iter()
            .enumerate()
            .map(|(i, mv)| {
                let step = AlignedStep {
                    a_idx: mv.a_idx,
                    b_idx: mv.b_idx,
                    a: step_view(reference, mv.a_idx),
                    b: step_view(observed, mv.b_idx),
                };
                match result.fork {
                    Some(fork) if i == fork.index => Row::Fork(ForkRow {
                        step,
                        side_a: side_content(reference, fork.a_step),
                        side_b: side_content(observed, fork.b_step),
                        confidence: confidence_text(fork.confidence),
                        field_diffs: field_diff_views(result, i),
                    }),
                    Some(fork) if i > fork.index => Row::Step(StepRow {
                        role: RowRole::Downstream,
                        kind: mv.kind,
                        step,
                    }),
                    _ => Row::Step(StepRow {
                        role: RowRole::Spine,
                        kind: mv.kind,
                        step,
                    }),
                }
            })
            .collect();

        let verdict = match result.fork {
            Some(_) => Verdict::Forked,
            None => {
                // "identical" is a claim the alignment must earn: every move a sync at cost
                // 0. Anything the resync rule merely absorbed (gap moves, costly syncs)
                // converged without being identical (issue #19).
                let absorbed = result
                    .alignment
                    .iter()
                    .filter(|mv| mv.kind != MoveKind::Sync || mv.cost > 0.0)
                    .count();
                if absorbed == 0 {
                    Verdict::Identical {
                        steps: result.alignment.len(),
                    }
                } else {
                    Verdict::Absorbed {
                        absorbed,
                        a_steps: reference.steps.len(),
                        b_steps: observed.steps.len(),
                    }
                }
            }
        };

        Self {
            run_a: RunHeader {
                id: result.runs.a.id.clone(),
                role: RunRole::Reference,
                n_steps: reference.steps.len(),
                outcome: reference.outcome,
            },
            run_b: RunHeader {
                id: result.runs.b.id.clone(),
                role: RunRole::Observed,
                n_steps: observed.steps.len(),
                outcome: observed.outcome,
            },
            idx_width,
            rows,
            verdict,
            attribution: result
                .attribution
                .as_ref()
                .map(|a| attribution_view(a, idx_width)),
            warnings: result.warnings.clone(),
        }
    }
}

/// The canonical step-kind vocabulary every surface prints.
pub fn kind_label(kind: StepKind) -> &'static str {
    match kind {
        StepKind::Llm => "llm",
        StepKind::Tool => "tool",
        StepKind::Agent => "agent",
        StepKind::Other => "other",
    }
}

/// The canonical move-kind vocabulary; painters add their own framing (the CLI brackets it
/// into `[sync]`, the web renders a chip).
pub fn move_label(kind: MoveKind) -> &'static str {
    match kind {
        MoveKind::Sync => "sync",
        MoveKind::Log => "log-move",
        MoveKind::Model => "model-move",
    }
}

/// The canonical outcome vocabulary.
pub fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Fail => "fail",
        Outcome::Unknown => "unknown",
    }
}

/// Confidence per notebook 005: zero — the designed weak-call state (evidence ≤ τ) — is
/// stated in words, never rendered as a small number.
pub fn confidence_text(confidence: f64) -> String {
    if confidence <= f64::EPSILON {
        "marginal call".to_string()
    } else {
        format!("conf {confidence:.2}")
    }
}

/// One side of an alignment move, where its index resolves to a real step.
fn step_view(run: &Run, idx: Option<usize>) -> Option<StepView> {
    idx.and_then(|i| run.steps.get(i)).map(|s| StepView {
        kind: s.kind,
        name: s.name.clone(),
        summary: SlotText::new(summarize(s)),
    })
}

/// Fork-side content: the step's summary, empty where an index resolves to no step, the
/// designed absence wording where the fork has no step on that side at all.
fn side_content(run: &Run, step: Option<usize>) -> SlotText {
    SlotText::new(match step {
        Some(idx) => run.steps.get(idx).map_or_else(String::new, summarize),
        None => "(no aligned step)".to_string(),
    })
}

/// The field-level evidence attached to the fork's alignment index, values in display form.
fn field_diff_views(result: &DiffResult, index: usize) -> Vec<FieldDiffView> {
    result
        .field_diffs
        .iter()
        .filter(|fd| fd.step == index)
        .map(|fd| FieldDiffView {
            path: fd.path.clone(),
            removed: match fd.kind {
                FieldDiffKind::Added => None,
                _ => fd.before.as_ref().map(|v| SlotText::new(compact_json(v))),
            },
            added: match fd.kind {
                FieldDiffKind::Removed => None,
                _ => fd.after.as_ref().map(|v| SlotText::new(compact_json(v))),
            },
        })
        .collect()
}

fn attribution_view(attribution: &Attribution, idx_width: usize) -> AttributionView {
    let mode = match attribution.mode {
        AttributionMode::Static => "static",
        AttributionMode::Counterfactual => "counterfactual",
    }
    .to_string();
    let origin = attribution.origin_step.map_or_else(
        || "origin unlocalized".to_string(),
        |s| format!("origin step {s:0w$}", w = idx_width),
    );
    AttributionView {
        mode,
        origin,
        propagation: steps_text(&attribution.propagation, idx_width),
        confidence: confidence_text(attribution.confidence),
    }
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

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn decimal_digits(n: usize) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{
        Attribution, DiffResult, FieldDiff, Fork, Meta, Move, RunPair, RunRef, Source, Warning,
        WarningCode, test_support,
    };
    use serde_json::json;

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

    /// Forked pair: two clean syncs, a high-cost sync (the fork), one log move downstream,
    /// with field diffs and a static attribution attached at the fork.
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

    #[test]
    fn forked_rows_carry_roles_in_alignment_order() {
        let (a, b, res) = forked(0.47);
        let view = ViewModel::compute(&res, &a, &b);

        let roles: Vec<&str> = view
            .rows
            .iter()
            .map(|row| match row {
                Row::Step(s) if s.role == RowRole::Spine => "spine",
                Row::Step(_) => "downstream",
                Row::Fork(_) => "fork",
            })
            .collect();
        assert_eq!(roles, ["spine", "spine", "fork", "downstream"]);
        assert_eq!(view.verdict, Verdict::Forked);
        assert_eq!(view.verdict.converged_text(), None);
    }

    #[test]
    fn fork_row_resolves_sides_confidence_and_evidence() {
        let (a, b, res) = forked(0.47);
        let view = ViewModel::compute(&res, &a, &b);

        let Some(Row::Fork(fork)) = view.rows.get(2) else {
            panic!("row 2 is the fork");
        };
        assert_eq!(fork.side_a, "census.gov page: population 8,443,000");
        assert_eq!(
            fork.side_b,
            "blogspot page: the city has grown to 9,100,000"
        );
        assert_eq!(fork.confidence, "conf 0.47");
        assert_eq!(
            fork.field_diffs,
            [FieldDiffView {
                path: "outputs".to_string(),
                removed: Some(SlotText::new("\"census.gov page\"")),
                added: Some(SlotText::new("\"blogspot page\"")),
            }]
        );
        // The observed run fronts the row; both sides stay available for the web columns.
        assert_eq!(fork.step.front().unwrap().name, "web.fetch");
        assert_eq!(fork.step.display_idx(), Some(2));
        assert!(fork.step.a.is_some() && fork.step.b.is_some());
    }

    #[test]
    fn attribution_parts_follow_the_reading_order() {
        let (a, b, res) = forked(0.47);
        let view = ViewModel::compute(&res, &a, &b);

        assert_eq!(
            view.attribution,
            Some(AttributionView {
                mode: "static".to_string(),
                origin: "origin step 02".to_string(),
                propagation: "step 03".to_string(),
                confidence: "conf 0.47".to_string(),
            })
        );
    }

    #[test]
    fn zero_confidence_is_an_explicit_marginal_call() {
        let (a, b, res) = forked(0.0);
        let view = ViewModel::compute(&res, &a, &b);

        let Some(Row::Fork(fork)) = view.rows.get(2) else {
            panic!("row 2 is the fork");
        };
        assert_eq!(fork.confidence, "marginal call");
        assert_eq!(view.attribution.unwrap().confidence, "marginal call");
        assert_eq!(confidence_text(0.47), "conf 0.47");
    }

    #[test]
    fn model_move_fronts_the_reference_side() {
        // A model move exists only on the reference (a) side; the front and the gutter
        // index must follow it there.
        let a = run("good", Outcome::Pass, vec![step(0, "plan", "x")]);
        let b = run("bad", Outcome::Fail, vec![]);
        let res = result(&a, &b, vec![Move::model(0, 0.6, 0.0)], None);
        let view = ViewModel::compute(&res, &a, &b);

        let Some(Row::Step(row)) = view.rows.first() else {
            panic!("one row");
        };
        assert_eq!(row.kind, MoveKind::Model);
        assert_eq!(row.step.front().unwrap().name, "plan");
        assert_eq!(row.step.display_idx(), Some(0));
        assert!(row.step.b.is_none(), "no observed side on a model move");
    }

    #[test]
    fn converged_identical_is_earned_by_all_zero_cost_syncs() {
        let steps = || vec![step(0, "plan", "x"), step(1, "answer", "y")];
        let a = run("good", Outcome::Pass, steps());
        let b = run("good_again", Outcome::Pass, steps());
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::sync(1, 1, 0.0, 1.0)];
        let view = ViewModel::compute(&result(&a, &b, alignment, None), &a, &b);

        assert_eq!(view.verdict, Verdict::Identical { steps: 2 });
        assert_eq!(
            view.verdict.converged_text().unwrap(),
            "converged — identical through 2 steps"
        );
        assert!(view.attribution.is_none());
    }

    #[test]
    fn absorbed_divergence_never_claims_identical() {
        let a = run(
            "good",
            Outcome::Pass,
            vec![step(0, "plan", "x"), step(1, "answer", "y")],
        );
        let b = run("good_retry", Outcome::Pass, vec![step(0, "plan", "x")]);
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::model(1, 0.6, 0.0)];
        let view = ViewModel::compute(&result(&a, &b, alignment, None), &a, &b);

        assert_eq!(
            view.verdict,
            Verdict::Absorbed {
                absorbed: 1,
                a_steps: 2,
                b_steps: 1,
            }
        );
        let text = view.verdict.converged_text().unwrap();
        assert_eq!(
            text,
            "converged — no fork (1 absorbed divergence across 2⇄1 steps)"
        );
        assert!(!text.contains("identical"));
    }

    #[test]
    fn missing_fork_side_resolves_to_the_designed_absence() {
        let a = run("good", Outcome::Pass, vec![step(0, "plan", "x")]);
        let b = run("bad", Outcome::Fail, vec![step(0, "other", "y")]);
        let fork = Fork {
            index: 0,
            a_step: None,
            b_step: Some(0),
            confidence: 0.9,
        };
        let res = result(&a, &b, vec![Move::sync(0, 0, 0.9, 0.1)], Some(fork));
        let view = ViewModel::compute(&res, &a, &b);

        let Some(Row::Fork(fork)) = view.rows.first() else {
            panic!("one fork row");
        };
        assert_eq!(fork.side_a, "(no aligned step)");
        assert_eq!(fork.side_b, "y");
    }

    #[test]
    fn contiguous_propagation_collapses_to_a_padded_range() {
        let (a, b, mut res) = forked(0.47);
        res.attribution.as_mut().unwrap().propagation = vec![3, 4, 5];
        let view = ViewModel::compute(&res, &a, &b);
        assert_eq!(view.attribution.unwrap().propagation, "steps 03–05");
    }

    #[test]
    fn document_roundtrips_through_json() {
        let (a, b, res) = forked(0.47);
        let doc = Document::new(ViewModel::compute(&res, &a, &b));
        assert_eq!(doc.schema_version, DOCUMENT_VERSION);

        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn roundtripped_roles_match_the_fork() {
        let (a, b, res) = forked(0.47);
        let doc = Document::new(ViewModel::compute(&res, &a, &b));
        let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();

        let fork_index = res.fork.unwrap().index;
        for (i, row) in back.view.rows.iter().enumerate() {
            match row {
                Row::Fork(_) => assert_eq!(i, fork_index),
                Row::Step(s) if i < fork_index => assert_eq!(s.role, RowRole::Spine),
                Row::Step(s) => assert_eq!(s.role, RowRole::Downstream),
            }
        }
    }

    #[test]
    fn converged_verdicts_survive_the_wire() {
        let steps = || vec![step(0, "plan", "x"), step(1, "answer", "y")];
        let a = run("good", Outcome::Pass, steps());
        let b = run("good_again", Outcome::Pass, steps());
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::sync(1, 1, 0.0, 1.0)];
        let doc = Document::new(ViewModel::compute(&result(&a, &b, alignment, None), &a, &b));
        let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(back.view.verdict, Verdict::Identical { steps: 2 });
        assert_eq!(
            back.view.verdict.converged_text().unwrap(),
            "converged — identical through 2 steps"
        );

        // The absorbed distinction (issue #19) must survive the wire too — the web surface
        // may never flatten "converged with absorbed divergence" into "identical".
        let b = run("good_retry", Outcome::Pass, vec![step(0, "plan", "x")]);
        let alignment = vec![Move::sync(0, 0, 0.0, 1.0), Move::model(1, 0.6, 0.0)];
        let doc = Document::new(ViewModel::compute(&result(&a, &b, alignment, None), &a, &b));
        let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(
            back.view.verdict,
            Verdict::Absorbed {
                absorbed: 1,
                a_steps: 2,
                b_steps: 1,
            }
        );
        assert!(
            !back
                .view
                .verdict
                .converged_text()
                .unwrap()
                .contains("identical")
        );
    }

    #[test]
    fn envelope_truncates_and_marks_over_limit_slots() {
        let huge = "x".repeat(SLOT_TEXT_LIMIT + 100);
        let a = run("good", Outcome::Pass, vec![step(0, "fetch", &huge)]);
        let b = run("bad", Outcome::Fail, vec![step(0, "fetch", "small")]);
        let fork = Fork {
            index: 0,
            a_step: Some(0),
            b_step: Some(0),
            confidence: 0.9,
        };
        let mut res = result(&a, &b, vec![Move::sync(0, 0, 0.9, 0.1)], Some(fork));
        res.field_diffs = vec![FieldDiff {
            step: 0,
            path: "outputs".to_string(),
            before: Some(json!(huge.clone())),
            after: Some(json!("small")),
            kind: FieldDiffKind::Changed,
        }];
        let doc = Document::new(ViewModel::compute(&res, &a, &b));

        let Some(Row::Fork(fork)) = doc.view.rows.first() else {
            panic!("one fork row");
        };
        // The same over-limit payload feeds side_a and the a-side summary; both arrive cut.
        for slot in [&fork.side_a, &fork.step.a.as_ref().unwrap().summary] {
            assert!(slot.truncated);
            assert_eq!(slot.text.len(), SLOT_TEXT_LIMIT);
            assert!(huge.starts_with(&slot.text));
        }
        assert!(!fork.side_b.truncated);
        let removed = fork.field_diffs[0].removed.as_ref().unwrap();
        assert!(removed.truncated);
        assert!(removed.text.len() <= SLOT_TEXT_LIMIT);
        assert!(!fork.field_diffs[0].added.as_ref().unwrap().truncated);

        // The marker itself survives the wire.
        let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn envelope_backs_off_to_a_char_boundary() {
        // Two ASCII bytes then 4-byte chars: byte SLOT_TEXT_LIMIT lands mid-char, so the
        // cut must back off to the last boundary below it instead of panicking.
        let text = format!("ab{}", "🌲".repeat(SLOT_TEXT_LIMIT / 4));
        let a = run("good", Outcome::Pass, vec![step(0, "fetch", &text)]);
        let b = run("bad", Outcome::Fail, vec![step(0, "fetch", "small")]);
        let fork = Fork {
            index: 0,
            a_step: Some(0),
            b_step: Some(0),
            confidence: 0.9,
        };
        let res = result(&a, &b, vec![Move::sync(0, 0, 0.9, 0.1)], Some(fork));
        let doc = Document::new(ViewModel::compute(&res, &a, &b));

        let Some(Row::Fork(fork)) = doc.view.rows.first() else {
            panic!("one fork row");
        };
        assert!(fork.side_a.truncated);
        assert_eq!(fork.side_a.text.len(), SLOT_TEXT_LIMIT - 2);
        assert!(text.starts_with(&fork.side_a.text));
    }

    #[test]
    fn under_limit_slots_pass_through_unmarked() {
        let (a, b, res) = forked(0.47);
        let doc = Document::new(ViewModel::compute(&res, &a, &b));

        // Nothing in the standard fixture is near the limit: no slot is marked, and the
        // lean wire never mentions the marker at all.
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("truncated"));
        let Some(Row::Fork(fork)) = doc.view.rows.get(2) else {
            panic!("row 2 is the fork");
        };
        assert_eq!(fork.side_a, "census.gov page: population 8,443,000");
    }

    #[test]
    fn headers_idx_width_and_warnings_come_through() {
        let (a, b, mut res) = forked(0.47);
        res.warnings = vec![Warning {
            code: WarningCode::ContentAbsent,
            msg: "run b: step 3 carried no content".to_string(),
        }];
        let view = ViewModel::compute(&res, &a, &b);

        assert_eq!(view.run_a.role.label(), "reference");
        assert_eq!(view.run_b.role.label(), "observed");
        assert_eq!((view.run_a.n_steps, view.run_b.n_steps), (3, 4));
        assert_eq!(view.run_a.outcome, Some(Outcome::Pass));
        assert_eq!(view.idx_width, 2, "indices are at least two digits");
        assert_eq!(view.warnings.len(), 1);
    }
}
