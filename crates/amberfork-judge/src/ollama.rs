//! The live provider: a local Ollama server, over the exact `/api/generate` endpoint/shape
//! `crates/amberfork/tests/verify_cli.rs` (#44) already established as this workspace's
//! local-provider convention.

use crate::context::ExplainContext;
use crate::judge::{Explanation, Judge, JudgeError};
use crate::prompt;
use serde::Deserialize;
use std::future::Future;

/// The production [`Judge`]. Never asks the model to report *where* the fork is —
/// [`Explanation::fork_index`] is set from the [`ExplainContext`] itself, not parsed from model
/// output, so the model structurally cannot mis-localize (design guardrail #1); it only ever
/// narrates the window it's handed.
#[derive(Debug, Clone)]
pub struct OllamaJudge {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaJudge {
    /// A judge that asks `model` at `base_url` (e.g. `http://127.0.0.1:11434`). The client is
    /// injected — a caller shares one connection pool / sets one timeout, the same reasoning
    /// `amberfork_replay::LiveUpstream` uses.
    #[must_use]
    pub fn new(
        client: reqwest::Client,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
        }
    }
}

impl Judge for OllamaJudge {
    fn explain(
        &self,
        context: &ExplainContext<'_>,
    ) -> impl Future<Output = Result<Explanation, JudgeError>> + Send {
        let fork_index = context.result.fork.map(|fork| fork.index);
        // Build the request synchronously so the future borrows neither `self` nor `context` —
        // the same reasoning `LiveUpstream::send` documents. Converged: `request` stays `None`
        // and the model is never called — never fabricate a fork, never ask it to confirm one
        // that doesn't exist (issue #10 edge case).
        let request = fork_index.map(|_| {
            let body = serde_json::json!({
                "model": self.model,
                "prompt": prompt::build(&context.window),
                "stream": false,
            });
            (
                format!("{}/api/generate", self.base_url),
                self.client.clone(),
                body,
            )
        });

        async move {
            let Some((url, client, body)) = request else {
                return Ok(Explanation {
                    fork_index: None,
                    narrative: "no divergence to explain".to_string(),
                    speculative_fix: None,
                });
            };
            let response = client
                .post(url)
                .json(&body)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|err| JudgeError::Unreachable(err.to_string()))?;
            let payload: OllamaResponse = response
                .json()
                .await
                .map_err(|err| JudgeError::Unreachable(err.to_string()))?;
            Ok(Explanation {
                fork_index,
                narrative: payload.response.trim().to_string(),
                speculative_fix: None,
            })
        }
    }
}

/// Ollama's non-streaming `/api/generate` response. Only `response` (the generated text) is
/// read; every other field it returns (`done`, timing, token counts) is not this slice's job.
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_model::test_support::{run, step};
    use amberfork_model::{DiffResult, Fork, Meta, Outcome, RunPair, RunRef, Source, StepKind};

    /// Deliberately not 11434: nothing listens on port 1 without root, so this is a reliable
    /// "connection refused" target regardless of whether a real Ollama server happens to be
    /// running elsewhere on the test machine (unlike the real default port, which #44's fixtures
    /// deliberately do run against).
    const UNREACHABLE_URL: &str = "http://127.0.0.1:1";

    fn forked_result() -> DiffResult {
        DiffResult {
            runs: RunPair {
                a: RunRef {
                    id: "a".into(),
                    task: None,
                    outcome: Some(Outcome::Pass),
                    n_steps: 3,
                },
                b: RunRef {
                    id: "b".into(),
                    task: None,
                    outcome: Some(Outcome::Fail),
                    n_steps: 3,
                },
            },
            alignment: Vec::new(),
            fork: Some(Fork {
                index: 1,
                a_step: Some(1),
                b_step: Some(1),
                confidence: 0.9,
            }),
            field_diffs: Vec::new(),
            attribution: None,
            warnings: Vec::new(),
            meta: Meta::current(Source::Passive),
        }
    }

    fn small_run(id: &str) -> amberfork_model::Run {
        let steps = (0..3)
            .map(|i| step(i, "step").kind(StepKind::Llm).build())
            .collect();
        run(id, steps).build()
    }

    #[tokio::test]
    async fn a_converged_context_never_dials_out() {
        // No server at all — if this reached the network it would fail. It shouldn't: fork is
        // `None`, so `OllamaJudge` must answer without building a request.
        let judge = OllamaJudge::new(reqwest::Client::new(), UNREACHABLE_URL, "irrelevant");
        let result = DiffResult {
            fork: None,
            ..forked_result()
        };
        let (a, b) = (small_run("a"), small_run("b"));
        let ctx = ExplainContext::windowed(&result, &a, &b, 1);

        let explanation = judge
            .explain(&ctx)
            .await
            .expect("converged never dials out");

        assert_eq!(explanation.fork_index, None);
        assert_eq!(explanation.narrative, "no divergence to explain");
    }

    #[tokio::test]
    async fn an_unreachable_provider_reports_unreachable() {
        let judge = OllamaJudge::new(reqwest::Client::new(), UNREACHABLE_URL, "irrelevant");
        let result = forked_result();
        let (a, b) = (small_run("a"), small_run("b"));
        let ctx = ExplainContext::windowed(&result, &a, &b, 1);

        let err = judge
            .explain(&ctx)
            .await
            .expect_err("nothing listens on port 1");

        assert!(matches!(err, JudgeError::Unreachable(_)));
    }
}
