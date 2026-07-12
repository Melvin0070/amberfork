//! `Step`/`Run` fixture builders for tests across the workspace (issue #22), replacing the
//! per-crate hand-rolled copies of the full field list.
//!
//! Gated behind the `test-support` feature (plus this crate's own `cfg(test)`), which no crate
//! enables outside `[dev-dependencies]` — the builders never ship in a normal build. When a
//! field lands on [`Step`] or [`Run`], this file — the builders plus the exhaustive literals in
//! its tests — is the one fixture site that has to change.

use crate::{Outcome, Payload, Run, SchemaVersion, Step, StepKind};
use serde_json::Map;

/// Start a minimal valid step: `idx` + `name`, kind [`StepKind::Tool`], everything else empty.
/// Chain setters for what the test cares about, then [`StepBuilder::build`].
pub fn step(idx: usize, name: impl Into<String>) -> StepBuilder {
    StepBuilder {
        step: Step {
            idx,
            kind: StepKind::Tool,
            name: name.into(),
            inputs: None,
            outputs: None,
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        },
    }
}

/// Start a minimal run: current schema, `id`, the given trajectory, no task/outcome/edges.
pub fn run(id: impl Into<String>, steps: Vec<Step>) -> RunBuilder {
    RunBuilder {
        run: Run {
            schema_version: SchemaVersion::current(),
            id: id.into(),
            task: None,
            outcome: None,
            steps,
            edges: None,
        },
    }
}

/// Fluent [`Step`] fixture builder; start one with [`step`].
#[derive(Debug, Clone)]
pub struct StepBuilder {
    step: Step,
}

impl StepBuilder {
    /// Override the default [`StepKind::Tool`].
    #[must_use]
    pub fn kind(mut self, kind: StepKind) -> Self {
        self.step.kind = kind;
        self
    }

    /// Set the step's inputs. Accepts a bare [`Payload`] or an `Option` (proptest strategies
    /// hand over an `Option` directly).
    #[must_use]
    pub fn inputs(mut self, inputs: impl Into<Option<Payload>>) -> Self {
        self.step.inputs = inputs.into();
        self
    }

    /// Set the step's outputs. Accepts a bare [`Payload`] or an `Option`.
    #[must_use]
    pub fn outputs(mut self, outputs: impl Into<Option<Payload>>) -> Self {
        self.step.outputs = outputs.into();
        self
    }

    /// The dominant fixture shape: a text payload as the step's outputs.
    #[must_use]
    pub fn text_output(self, out: impl Into<String>) -> Self {
        self.outputs(Payload::Text(out.into()))
    }

    #[must_use]
    pub fn build(self) -> Step {
        self.step
    }
}

/// Fluent [`Run`] fixture builder; start one with [`run`].
#[derive(Debug, Clone)]
pub struct RunBuilder {
    run: Run,
}

impl RunBuilder {
    /// Set the human task label.
    #[must_use]
    pub fn task(mut self, task: impl Into<String>) -> Self {
        self.run.task = Some(task.into());
        self
    }

    /// Set the run-level verdict.
    #[must_use]
    pub fn outcome(mut self, outcome: Outcome) -> Self {
        self.run.outcome = Some(outcome);
        self
    }

    #[must_use]
    pub fn build(self) -> Run {
        self.run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The literals below are exhaustive on purpose — no `..Default::default()`, no builder.
    // When a field is added to `Step` or `Run`, these are where the workspace's fixtures
    // break first, and the only place that has to be fixed.

    #[test]
    fn minimal_step_is_a_bare_tool_step() {
        let expected = Step {
            idx: 3,
            kind: StepKind::Tool,
            name: "web.search".to_string(),
            inputs: None,
            outputs: None,
            attrs: Map::new(),
            t_start: None,
            t_end: None,
            parent_idx: None,
        };
        assert_eq!(step(3, "web.search").build(), expected);
    }

    #[test]
    fn step_setters_reach_their_fields() {
        let s = step(0, "planner")
            .kind(StepKind::Llm)
            .inputs(Payload::Text("question".to_string()))
            .text_output("plan: search then verify")
            .build();
        assert_eq!(s.kind, StepKind::Llm);
        assert_eq!(s.inputs, Some(Payload::Text("question".to_string())));
        assert_eq!(
            s.outputs,
            Some(Payload::Text("plan: search then verify".to_string()))
        );
    }

    #[test]
    fn outputs_accepts_an_option_directly() {
        let s = step(0, "fetch").outputs(None).build();
        assert_eq!(s.outputs, None);
    }

    #[test]
    fn minimal_run_is_current_schema_with_no_verdict() {
        let steps = vec![step(0, "plan").build()];
        let expected = Run {
            schema_version: SchemaVersion::current(),
            id: "r1".to_string(),
            task: None,
            outcome: None,
            steps: steps.clone(),
            edges: None,
        };
        assert_eq!(run("r1", steps).build(), expected);
    }

    #[test]
    fn run_setters_reach_their_fields() {
        let r = run("r2", Vec::new())
            .task("find the census figure")
            .outcome(Outcome::Fail)
            .build();
        assert_eq!(r.task.as_deref(), Some("find the census figure"));
        assert_eq!(r.outcome, Some(Outcome::Fail));
    }
}
