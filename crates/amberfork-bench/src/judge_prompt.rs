//! The LLM-judge baseline's frozen prompt surface (issue #46, registered in notebook 069).
//!
//! Everything a judge arm is *asked* lives here: the three frozen templates under
//! `bench/judge_prompts/`, the sha256 pin that makes "frozen" mean something, and the step
//! rendering that turns a [`Run`] into the text a model reads. Nothing here calls a network;
//! providers and the cassette cache are the next slice.
//!
//! Three properties this module exists to guarantee, all of them registered in advance:
//!
//! - **The prompt is a pinned artifact, like `bench/params.toml`.** [`PromptSet::load`] hashes
//!   each template's exact bytes and refuses any file that is not the registered revision. A
//!   reworded prompt cannot reach a published table by accident; changing one means editing
//!   the pin here, which is the moment BENCHMARK.md rule 3 (report old and new, never swap)
//!   becomes unavoidable. Rewording a baseline's prompt after seeing its score is the
//!   baseline-shaped way to tune on test, and it is the specific thing rule 10 forbids.
//! - **What the judge sees is bounded and stated.** Payloads are capped at
//!   [`PAYLOAD_CAP_CHARS`], head plus tail with the elided count spelled out, because a tool
//!   result's error usually sits at its end and the call's shape at its start. The cap is a
//!   thumb on the scale in the product's favour — the cost model reads full payloads — which
//!   is why it is registered, published, and named as an alternative explanation if a judge
//!   arm loses.
//! - **A template can never render half-substituted.** [`substitute`] fails loudly on any
//!   surviving `{{TOKEN}}`, so a future revision that renames a placeholder produces an error
//!   rather than a prompt with literal braces in it that a model then answers anyway.
//!
//! This is deliberately NOT `amberfork-judge`. That crate's `Judge` trait is a *narration*
//! interface whose guardrail is that the model structurally cannot report a step index; the
//! baseline needs exactly the opposite. Keeping the localizer in the harness, next to the
//! fixtures and the protocol that hashes it, is what stops the two from being merged later
//! (notebook 069).

use amberfork_model::{Payload, Run};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

/// Leading characters kept from a payload preview.
pub const PAYLOAD_HEAD_CHARS: usize = 400;
/// Trailing characters kept from a payload preview.
pub const PAYLOAD_TAIL_CHARS: usize = 200;
/// Total characters of a payload a judge is shown. Measured (notebook 069): this holds the
/// largest `judge-paired` prompt on the registered corpus to roughly 64k tokens, so no
/// provider silently truncates a prompt out from under the protocol.
pub const PAYLOAD_CAP_CHARS: usize = PAYLOAD_HEAD_CHARS + PAYLOAD_TAIL_CHARS;

/// The three registered baseline conditions (notebook 069).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeArm {
    /// Failing trace only — replicates the Who&When all-in-one method.
    Single,
    /// Both runs in the prompt: the same information the aligner gets. The headline.
    Paired,
    /// The Who&When step-by-step method: one call per candidate step.
    Stepwise,
}

impl JudgeArm {
    /// Every arm, in registration order.
    pub const ALL: [Self; 3] = [Self::Single, Self::Paired, Self::Stepwise];

    /// The arm's name in tables and results JSON.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "judge-single",
            Self::Paired => "judge-paired",
            Self::Stepwise => "judge-stepwise",
        }
    }

    /// Position in [`Self::ALL`] — the arm's slot in a [`PromptSet`].
    fn slot(self) -> usize {
        match self {
            Self::Single => 0,
            Self::Paired => 1,
            Self::Stepwise => 2,
        }
    }

    /// The template's file name inside the prompts directory.
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Single => "judge_single.md",
            Self::Paired => "judge_paired.md",
            Self::Stepwise => "judge_stepwise.md",
        }
    }

    /// The registered sha256 of that template's exact bytes — notebook 069 and
    /// `bench/judge_prompts/README.md` publish these same values, and a reviewer recomputes
    /// them with `shasum -a 256 bench/judge_prompts/*.md`.
    #[must_use]
    pub fn registered_sha256(self) -> &'static str {
        match self {
            Self::Single => "e622edfd84bc2e15974b9e2ac94474fe047f41385a2d4bef64732fafaaec6e61",
            Self::Paired => "ce7515e888ffde2c54e61b0d5fcd90a9c27c29776532bdb4479e2fb1d1e9d942",
            Self::Stepwise => "d0c13e46c41e225510f0c6c23d1dfba9bdbd95530758a1ef2c83cc8a44bbc209",
        }
    }
}

impl fmt::Display for JudgeArm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One loaded template with its provenance: the file it came from and the hash that proves it
/// is the registered revision.
#[derive(Debug)]
pub struct FrozenPrompt {
    pub source: String,
    pub sha256: String,
    template: String,
}

/// A rendered prompt: the exact text a provider will send, plus its own hash — the cassette
/// key's other half, so a cached response can only ever be replayed for the byte-identical
/// question that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub arm: JudgeArm,
    pub text: String,
    pub sha256: String,
}

/// Why a prompt file was rejected. Every variant is fatal: a judge arm that cannot establish
/// which prompt revision it is running must not produce a number.
#[derive(Debug)]
pub enum PromptError {
    Read {
        file: PathBuf,
        source: std::io::Error,
    },
    NotUtf8 {
        file: PathBuf,
        source: std::str::Utf8Error,
    },
    /// The bytes hash to something other than the registered revision.
    Unregistered {
        file: PathBuf,
        expected: &'static str,
        actual: String,
    },
}

impl fmt::Display for PromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { file, source } => write!(f, "read prompt {}: {source}", file.display()),
            Self::NotUtf8 { file, source } => write!(f, "read prompt {}: {source}", file.display()),
            Self::Unregistered {
                file,
                expected,
                actual,
            } => write!(
                f,
                "prompt {} is not the registered revision: expected sha256 {expected}, found \
                 {actual} — a reworded baseline prompt is a new revision (BENCHMARK.md rules 3 \
                 and 10, notebook 069), so update the pin and publish both numbers",
                file.display()
            ),
        }
    }
}

impl std::error::Error for PromptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::NotUtf8 { source, .. } => Some(source),
            Self::Unregistered { .. } => None,
        }
    }
}

/// Why a prompt could not be rendered for a given pair.
#[derive(Debug, PartialEq, Eq)]
pub enum RenderError {
    /// A run with no steps has no step to ask about. `load_pairs` already excludes these, so
    /// reaching here means a caller bypassed the pair loader.
    EmptyRun { which: &'static str },
    /// `judge-stepwise` was asked about a step the failing run does not have.
    CandidateOutOfRange { candidate: usize, len: usize },
    /// A `{{TOKEN}}` survived substitution — the template and this code disagree about the
    /// placeholder set, which must never reach a model as literal braces.
    UnsubstitutedPlaceholder { token: String },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRun { which } => write!(f, "{which} run has no steps"),
            Self::CandidateOutOfRange { candidate, len } => write!(
                f,
                "stepwise candidate {candidate} is outside the failing run's {len} steps"
            ),
            Self::UnsubstitutedPlaceholder { token } => {
                write!(f, "template placeholder {token} was never substituted")
            }
        }
    }
}

impl std::error::Error for RenderError {}

/// All three frozen templates, loaded and pinned together.
///
/// Loaded as a set rather than one at a time on purpose: a run that can only establish two of
/// three revisions is not a run under this protocol, and finding that out at load time beats
/// finding it out after the first arm has already cost money.
#[derive(Debug)]
pub struct PromptSet {
    prompts: [FrozenPrompt; JudgeArm::ALL.len()],
}

impl PromptSet {
    /// Load and pin every template in `dir` (canonically `bench/judge_prompts`).
    ///
    /// Driven off [`JudgeArm::ALL`] rather than three named loads, so a fourth registered
    /// condition cannot be added and then silently left unpinned.
    ///
    /// # Errors
    /// A [`PromptError`] naming the first file that is unreadable, not UTF-8, or not the
    /// registered revision.
    pub fn load(dir: &Path) -> Result<Self, PromptError> {
        let mut prompts = Vec::with_capacity(JudgeArm::ALL.len());
        for arm in JudgeArm::ALL {
            prompts.push(load_one(dir, arm)?);
        }
        Ok(Self {
            prompts: prompts.try_into().expect("one prompt per registered arm"),
        })
    }

    /// The loaded template for one arm — its hash is what a published table names.
    #[must_use]
    pub fn prompt(&self, arm: JudgeArm) -> &FrozenPrompt {
        &self.prompts[arm.slot()]
    }

    fn template(&self, arm: JudgeArm) -> &str {
        &self.prompt(arm).template
    }

    /// `judge-single`: the failing run alone.
    ///
    /// # Errors
    /// [`RenderError::EmptyRun`] if the failing run has no steps.
    pub fn render_single(&self, failing: &Run) -> Result<Rendered, RenderError> {
        let last = last_index(failing, "failing")?;
        finish(
            JudgeArm::Single,
            substitute(
                self.template(JudgeArm::Single),
                &[
                    ("{{FAILING_STEPS}}", &render_steps(&failing.steps)),
                    ("{{FAILING_LAST_INDEX}}", &last.to_string()),
                ],
            )?,
        )
    }

    /// `judge-paired`: both runs, the same information the aligner gets.
    ///
    /// # Errors
    /// [`RenderError::EmptyRun`] if either run has no steps.
    pub fn render_paired(&self, reference: &Run, failing: &Run) -> Result<Rendered, RenderError> {
        let last = last_index(failing, "failing")?;
        last_index(reference, "reference")?;
        finish(
            JudgeArm::Paired,
            substitute(
                self.template(JudgeArm::Paired),
                &[
                    ("{{REFERENCE_STEPS}}", &render_steps(&reference.steps)),
                    ("{{FAILING_STEPS}}", &render_steps(&failing.steps)),
                    ("{{FAILING_LAST_INDEX}}", &last.to_string()),
                ],
            )?,
        )
    }

    /// `judge-stepwise`: the failing run truncated after `candidate`, asking whether that last
    /// shown step is the decisive error. Every later step is withheld — the method's whole
    /// point is that the judge decides without hindsight.
    ///
    /// # Errors
    /// [`RenderError::EmptyRun`] if the failing run has no steps, or
    /// [`RenderError::CandidateOutOfRange`] if `candidate` is past its last step.
    pub fn render_stepwise(
        &self,
        failing: &Run,
        candidate: usize,
    ) -> Result<Rendered, RenderError> {
        let last = last_index(failing, "failing")?;
        if candidate > last {
            return Err(RenderError::CandidateOutOfRange {
                candidate,
                len: failing.steps.len(),
            });
        }
        finish(
            JudgeArm::Stepwise,
            substitute(
                self.template(JudgeArm::Stepwise),
                &[
                    (
                        "{{PREFIX_STEPS}}",
                        &render_steps(&failing.steps[..=candidate]),
                    ),
                    ("{{CANDIDATE_INDEX}}", &candidate.to_string()),
                ],
            )?,
        )
    }
}

fn load_one(dir: &Path, arm: JudgeArm) -> Result<FrozenPrompt, PromptError> {
    let file = dir.join(arm.file_name());
    let bytes = std::fs::read(&file).map_err(|source| PromptError::Read {
        file: file.clone(),
        source,
    })?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    if sha256 != arm.registered_sha256() {
        return Err(PromptError::Unregistered {
            file,
            expected: arm.registered_sha256(),
            actual: sha256,
        });
    }
    let template = std::str::from_utf8(&bytes)
        .map_err(|source| PromptError::NotUtf8 {
            file: file.clone(),
            source,
        })?
        .to_string();
    Ok(FrozenPrompt {
        source: file.display().to_string(),
        sha256,
        template,
    })
}

fn finish(arm: JudgeArm, text: String) -> Result<Rendered, RenderError> {
    let sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    Ok(Rendered { arm, text, sha256 })
}

fn last_index(run: &Run, which: &'static str) -> Result<usize, RenderError> {
    run.steps
        .len()
        .checked_sub(1)
        .ok_or(RenderError::EmptyRun { which })
}

/// Replace every `{{TOKEN}}` and refuse to return a string that still holds one.
///
/// Substitution is literal, single-pass over the token list, and deliberately unaware of
/// escaping: trace content is *data* here, so a payload that happens to contain `{{FOO}}`
/// stays as it is rather than being re-scanned — the residual check runs on the template's
/// own unreplaced tokens, which is why substitution walks the declared token list rather than
/// hunting for brace pairs in the finished text.
fn substitute(template: &str, vars: &[(&str, &str)]) -> Result<String, RenderError> {
    let mut out = template.to_string();
    for (token, value) in vars {
        out = out.replace(token, value);
    }
    for (token, _) in vars {
        debug_assert!(!out.contains(token), "{token} survived its own replacement");
    }
    if let Some(token) = residual_placeholder(template, vars) {
        return Err(RenderError::UnsubstitutedPlaceholder { token });
    }
    Ok(out)
}

/// The first `{{TOKEN}}` the template declares that the caller did not supply a value for.
/// Scans the *template*, never the rendered output, so trace content can never be mistaken
/// for a placeholder.
fn residual_placeholder(template: &str, vars: &[(&str, &str)]) -> Option<String> {
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open..];
        let close = after.find("}}")?;
        let token = &after[..close + 2];
        if !vars.iter().any(|(name, _)| *name == token) {
            return Some(token.to_string());
        }
        rest = &after[close + 2..];
    }
    None
}

/// One line per step, in run order, per the contract in `bench/judge_prompts/README.md`.
///
/// The index rendered is the step's *position* in the slice, not its `idx` field. Position is
/// the index space gold steps and arm predictions both live in (`pairs::Pair::gold_step`,
/// `DiffResult::fork_step_observed`), and a trimmed or synthesized run can carry an `idx` that
/// disagrees with it — a judge answering in a different index space than it is scored in would
/// be a silent, systematic miss.
fn render_steps(steps: &[amberfork_model::Step]) -> String {
    // Joined, not line-terminated: the templates already put the closing tag on its own line,
    // and a trailing newline would render a blank line into every trace block.
    steps
        .iter()
        .enumerate()
        .map(|(pos, step)| {
            format!(
                "#{pos} [{:?} · {}] inputs: {} | outputs: {}",
                step.kind,
                step.name,
                preview(step.inputs.as_ref()),
                preview(step.outputs.as_ref()),
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// A payload as the judge sees it: head, an exact elision count, tail. An absent payload is
/// `(none)` rather than empty, so a step with no content reads as a fact instead of a gap.
fn preview(payload: Option<&Payload>) -> String {
    let Some(payload) = payload else {
        return "(none)".to_string();
    };
    let raw = match payload {
        Payload::Text(text) => text.clone(),
        Payload::Object(map) => serde_json::to_string(map).unwrap_or_default(),
        Payload::Other(value) => value.to_string(),
    };
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= PAYLOAD_CAP_CHARS {
        return raw;
    }
    let elided = chars.len() - PAYLOAD_CAP_CHARS;
    let head: String = chars[..PAYLOAD_HEAD_CHARS].iter().collect();
    let tail: String = chars[chars.len() - PAYLOAD_TAIL_CHARS..].iter().collect();
    format!("{head}…[{elided} chars elided]…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::{Run, SchemaVersion, Step, StepKind};

    fn committed_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/judge_prompts")
    }

    fn step(name: &str, outputs: Option<&str>) -> Step {
        Step {
            idx: 0,
            kind: StepKind::Tool,
            name: name.to_string(),
            inputs: None,
            outputs: outputs.map(|text| Payload::Text(text.to_string())),
            attrs: serde_json::Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        }
    }

    fn run(id: &str, steps: Vec<Step>) -> Run {
        Run {
            schema_version: SchemaVersion::current(),
            id: id.to_string(),
            task: None,
            outcome: None,
            steps,
            edges: None,
        }
    }

    fn three_steps() -> Run {
        run(
            "r",
            vec![
                step("search", Some("first")),
                step("fetch", Some("second")),
                step("answer", Some("third")),
            ],
        )
    }

    #[test]
    fn the_committed_prompts_are_the_registered_revisions() {
        // The pin notebook 069 registered and slice A1 deliberately deferred to here. If a
        // template is reworded without updating `registered_sha256`, this is the tripwire —
        // which is the point: rule 3 says a new revision publishes alongside the old number,
        // and that decision should cost a red test, not go unnoticed.
        let set = PromptSet::load(&committed_dir()).expect("committed prompts load and pin");
        for arm in JudgeArm::ALL {
            let prompt = set.prompt(arm);
            assert_eq!(prompt.sha256, arm.registered_sha256());
            assert_eq!(prompt.sha256.len(), 64, "full sha256 hex, not a prefix");
        }
    }

    #[test]
    fn every_arm_indexes_its_own_slot() {
        // `slot` and `ALL` are two hand-written orderings of the same list; if they drift, a
        // `PromptSet` hands out the wrong template under the right name and every arm's
        // published prompt hash becomes a lie.
        for (position, arm) in JudgeArm::ALL.into_iter().enumerate() {
            assert_eq!(arm.slot(), position, "{arm} sits in the wrong slot");
        }
    }

    #[test]
    fn a_reworded_prompt_is_rejected_not_quietly_used() {
        let dir = tempfile::tempdir().expect("tempdir");
        for arm in JudgeArm::ALL {
            let mut bytes = std::fs::read(committed_dir().join(arm.file_name())).expect("read");
            if arm == JudgeArm::Paired {
                // The smallest edit that still reads as the same instruction — exactly the
                // kind of "harmless" tweak that must not reach a published table.
                bytes.extend_from_slice(b"\nBe concise.\n");
            }
            std::fs::write(dir.path().join(arm.file_name()), bytes).expect("write");
        }

        let err = PromptSet::load(dir.path()).expect_err("an edited template must not load");

        assert!(
            matches!(err, PromptError::Unregistered { .. }),
            "got: {err}"
        );
        assert!(err.to_string().contains("judge_paired.md"), "got: {err}");
    }

    #[test]
    fn the_hash_is_the_standard_sha256_a_reviewer_computes() {
        // Independent known answer (`printf 'judge\n' | shasum -a 256`, coreutils — NOT the
        // sha2 crate, which would make the check circular).
        let rendered = finish(JudgeArm::Single, "judge\n".to_string()).expect("finish");
        assert_eq!(
            rendered.sha256,
            "b24ce7fc9a08741864447134e4a08517bef51a97ad668ac2d006efc6ce075e33"
        );
    }

    #[test]
    fn a_missing_prompt_file_is_an_error_naming_the_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = PromptSet::load(dir.path()).expect_err("empty prompts dir");
        assert!(matches!(err, PromptError::Read { .. }), "got: {err}");
        assert!(err.to_string().contains("judge_single.md"), "got: {err}");
    }

    #[test]
    fn a_step_line_carries_position_kind_name_and_both_payloads() {
        let text = render_steps(&three_steps().steps);
        assert_eq!(
            text,
            "#0 [Tool · search] inputs: (none) | outputs: first\n\
             #1 [Tool · fetch] inputs: (none) | outputs: second\n\
             #2 [Tool · answer] inputs: (none) | outputs: third"
        );
    }

    #[test]
    fn the_rendered_index_is_the_position_not_the_idx_field() {
        // A trimmed or synthesized run can carry an `idx` that disagrees with its position
        // (`build_trail`'s leading-content-free trim re-indexes; other adapters may not).
        // Gold and predictions live in position space, so the prompt must too.
        let mut run = three_steps();
        for (offset, step) in run.steps.iter_mut().enumerate() {
            step.idx = 100 + offset;
        }

        let text = render_steps(&run.steps);

        assert!(text.starts_with("#0 "), "got: {text}");
        assert!(!text.contains("#100"), "got: {text}");
    }

    #[test]
    fn a_payload_at_the_cap_is_shown_whole() {
        let exact = "x".repeat(PAYLOAD_CAP_CHARS);
        let text = render_steps(&[step("t", Some(&exact))]);
        assert!(
            text.contains(&exact),
            "a payload at the cap must not be cut"
        );
        assert!(!text.contains("elided"), "got: {text}");
    }

    #[test]
    fn an_oversized_payload_keeps_head_and_tail_and_counts_what_it_dropped() {
        let head = "H".repeat(PAYLOAD_HEAD_CHARS);
        let middle = "M".repeat(37);
        let tail = "T".repeat(PAYLOAD_TAIL_CHARS);
        let payload = format!("{head}{middle}{tail}");

        let text = render_steps(&[step("t", Some(&payload))]);

        // The tail is where a tool result's error lands; head-only truncation would drop it.
        assert!(
            text.contains(&format!("{head}…[37 chars elided]…{tail}")),
            "got: {text}"
        );
    }

    #[test]
    fn the_cap_counts_characters_not_bytes() {
        // A multibyte payload one char over the cap elides exactly one char — byte-based
        // slicing would either panic on a char boundary or silently drop more than it says.
        let payload = "é".repeat(PAYLOAD_CAP_CHARS + 1);
        let text = render_steps(&[step("t", Some(&payload))]);
        assert!(text.contains("…[1 chars elided]…"), "got: {text}");
    }

    #[test]
    fn substitution_refuses_to_leave_a_placeholder_behind() {
        let err = substitute(
            "ask about {{FAILING_STEPS}} at {{MISSING}}",
            &[("{{FAILING_STEPS}}", "a")],
        )
        .expect_err("an unsupplied placeholder must not render as literal braces");

        assert_eq!(
            err,
            RenderError::UnsubstitutedPlaceholder {
                token: "{{MISSING}}".to_string()
            }
        );
    }

    #[test]
    fn trace_content_that_looks_like_a_placeholder_is_left_alone() {
        // A step whose payload contains `{{FAILING_STEPS}}` must not trip the residual check
        // — the scan reads the template, never the substituted text.
        let rendered = substitute(
            "steps:\n{{FAILING_STEPS}}",
            &[(
                "{{FAILING_STEPS}}",
                "#0 [Tool · t] inputs: (none) | outputs: {{NOT_A_TOKEN}}",
            )],
        )
        .expect("payload braces are data, not template syntax");

        assert!(rendered.contains("{{NOT_A_TOKEN}}"), "got: {rendered}");
    }

    #[test]
    fn single_names_the_last_index_and_substitutes_everything() {
        let set = PromptSet::load(&committed_dir()).expect("prompts");
        let rendered = set.render_single(&three_steps()).expect("render");

        assert_eq!(rendered.arm, JudgeArm::Single);
        assert!(!rendered.text.contains("{{"), "got: {}", rendered.text);
        assert!(
            rendered.text.contains("from 0 to 2"),
            "got: {}",
            rendered.text
        );
        assert!(rendered.text.contains("#2 [Tool · answer]"));
        assert_eq!(rendered.sha256.len(), 64);
    }

    #[test]
    fn paired_renders_both_runs_each_numbered_from_zero() {
        let set = PromptSet::load(&committed_dir()).expect("prompts");
        let reference = run(
            "ref",
            vec![step("plan", Some("good")), step("done", Some("ok"))],
        );

        let rendered = set
            .render_paired(&reference, &three_steps())
            .expect("render");

        assert!(!rendered.text.contains("{{"), "got: {}", rendered.text);
        // Per-run 0-based numbering: both blocks start at #0, which is exactly the thing the
        // paired template warns the model about in words.
        assert!(rendered.text.contains("#0 [Tool · plan]"));
        assert!(rendered.text.contains("#0 [Tool · search]"));
        assert!(
            rendered.text.contains("from 0 to 2"),
            "the failing run's last index"
        );
    }

    #[test]
    fn stepwise_withholds_every_step_after_the_candidate() {
        let set = PromptSet::load(&committed_dir()).expect("prompts");

        let rendered = set.render_stepwise(&three_steps(), 1).expect("render");

        assert!(!rendered.text.contains("{{"), "got: {}", rendered.text);
        assert!(rendered.text.contains("#1 [Tool · fetch]"));
        assert!(
            !rendered.text.contains("#2 [Tool · answer]"),
            "hindsight would defeat the step-by-step method: {}",
            rendered.text
        );
    }

    #[test]
    fn stepwise_refuses_a_candidate_the_run_does_not_have() {
        let set = PromptSet::load(&committed_dir()).expect("prompts");
        let err = set
            .render_stepwise(&three_steps(), 3)
            .expect_err("candidate past the last step");
        assert_eq!(
            err,
            RenderError::CandidateOutOfRange {
                candidate: 3,
                len: 3
            }
        );
    }

    #[test]
    fn an_empty_run_is_an_error_not_an_underflow() {
        let set = PromptSet::load(&committed_dir()).expect("prompts");
        let empty = run("empty", vec![]);

        assert_eq!(
            set.render_single(&empty)
                .expect_err("no steps to ask about"),
            RenderError::EmptyRun { which: "failing" }
        );
        assert_eq!(
            set.render_paired(&empty, &three_steps())
                .expect_err("no reference steps"),
            RenderError::EmptyRun { which: "reference" }
        );
    }

    #[test]
    fn the_same_input_renders_the_same_bytes() {
        // The cassette key leans on this: a replayed response must belong to the
        // byte-identical question that produced it (rule 5, determinism).
        let set = PromptSet::load(&committed_dir()).expect("prompts");
        let first = set
            .render_paired(&three_steps(), &three_steps())
            .expect("a");
        let second = set
            .render_paired(&three_steps(), &three_steps())
            .expect("b");
        assert_eq!(first, second);
    }
}
