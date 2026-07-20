//! The re-execution driver: run the patched cassette once and judge the result.
//!
//! Where [`patch_cassette`](crate::patch_cassette) builds the counterfactual and
//! [`classify_recovery`](crate::oracle::classify_recovery) judges a re-run, this stage is the I/O
//! edge between them: stand up a [`ReplayServer`] over the patched cassette, drive the agent
//! against its loopback URL, read the re-executed tape back, normalize it, and hand it to the
//! oracle. It is the one place in counterfactual attribution that reaches the network — and only
//! past the patch, where the re-run has branched off the tape.
//!
//! ## The agent is a seam, so the test stays offline
//!
//! Re-driving the agent is abstracted behind [`AgentDriver`]. Production spawns the recorded
//! `-- <cmd>` subprocess with its base-URL env var pointed at the listener — exactly how
//! `amberfork record` drove it the first time (that impl lands with the CLI). The test substitutes
//! an in-process agent that speaks HTTP to the same listener, so `cargo test` exercises the real
//! server, normalize, and oracle without a provider or a subprocess. This mirrors how
//! [`Upstream`]/`ScriptedUpstream` already keep the replay crate's tests offline.

use amberfork_align::DiffParams;
use amberfork_model::Run;
use amberfork_record::normalize;
use amberfork_replay::{ReplayError, ReplayServer, Upstream};
use std::fmt;
use std::future::Future;

use crate::oracle::{RunVerdict, classify_recovery};

/// Drives the agent under test once, pointed at a base URL, so a re-execution can capture what it
/// does.
///
/// The counterfactual analog of [`Upstream`]: a native async trait (return-position `impl Future`,
/// not the `async-trait` crate), so the driver is chosen at compile time — an in-process stub in
/// tests, a spawned subprocess in production — with no `dyn` and no per-run allocation.
pub trait AgentDriver: Send + Sync {
    /// Drive the agent once against `base_url`, returning when it has finished.
    ///
    /// The agent's own exit code is deliberately *not* a failure here: a run that exits non-zero
    /// is often exactly the run being re-executed, and its trajectory is what matters, not its
    /// verdict. [`AgentError`] is reserved for not being able to run the agent at all.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError`] when the agent cannot be launched (for the subprocess driver, the
    /// command could not be spawned).
    fn drive(&self, base_url: &str) -> impl Future<Output = Result<(), AgentError>> + Send;
}

/// Why the agent could not be driven.
///
/// `#[non_exhaustive]`: the subprocess driver may add variants (a spawn-vs-signal distinction)
/// without breaking callers.
#[derive(Debug)]
#[non_exhaustive]
pub enum AgentError {
    /// The agent command could not be launched — in practice, the binary is missing or not
    /// executable.
    Spawn(std::io::Error),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(source) => write!(f, "cannot run the agent: {source}"),
        }
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(source) => Some(source),
        }
    }
}

/// Why a re-execution could not be carried out at all.
///
/// Distinct from a [`RunVerdict`] of `Inconclusive`: that is a re-run that *ran* but produced no
/// usable evidence. `ReexecError` is the experiment failing to start — the listener would not bind,
/// or the agent could not be launched. Both abort a `verify`; an inconclusive run does not.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReexecError {
    /// The loopback replay listener could not be stood up.
    Bind(ReplayError),
    /// The agent could not be driven.
    Agent(AgentError),
}

impl fmt::Display for ReexecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind(source) => write!(f, "cannot start the replay listener: {source}"),
            Self::Agent(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for ReexecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind(source) => Some(source),
            Self::Agent(source) => Some(source),
        }
    }
}

/// Re-execute the patched cassette once and return whether the run recovered.
///
/// Stands up a [`ReplayServer`] over `patched`, drives `driver` against its loopback URL, then
/// normalizes the re-executed tape and asks the oracle whether the fork at `origin` is gone. The
/// re-run answers recorded turns from the tape and relays through `upstream` once it branches off
/// — so the patched step is served the good run's response, and everything after runs live.
///
/// `Ok(verdict)` is the experiment's result, `Inconclusive` included; `Err` means the experiment
/// could not run (see [`ReexecError`]).
///
/// # Errors
///
/// [`ReexecError::Bind`] if the listener cannot bind; [`ReexecError::Agent`] if the agent cannot be
/// launched.
// Consumed by the multi-run consensus in `verify` (slice 4 of #37); until then it is exercised only
// by this module's tests. The allow goes the moment `verify` calls it.
#[allow(dead_code)]
pub(crate) async fn reexecute_once<U, D>(
    patched: amberfork_record::Cassette,
    good: &Run,
    origin: usize,
    driver: &D,
    upstream: U,
    reexecuted_id: &str,
    params: &DiffParams,
) -> Result<RunVerdict, ReexecError>
where
    U: Upstream + 'static,
    D: AgentDriver,
{
    let server = ReplayServer::bind(patched, upstream, reexecuted_id)
        .await
        .map_err(ReexecError::Bind)?;
    // A completed agent run — any exit code — leaves its tape; only a failure to launch it aborts.
    driver
        .drive(&server.base_url())
        .await
        .map_err(ReexecError::Agent)?;
    // The re-executed tape is itself a cassette (recorded turns + relayed misses, in order), so it
    // normalizes into a `Run` through the very same path a first-party recording does.
    let reexecuted = normalize(&server.reexecuted().await);
    Ok(classify_recovery(good, &reexecuted, origin, params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_record::{Body, CapturedRequest, CapturedResponse, Cassette, Exchange};
    use amberfork_replay::ScriptedUpstream;
    use serde_json::{Value, json};

    /// An in-process agent that POSTs a fixed script of request bodies at the loopback listener,
    /// exactly as an SDK pointed at the base URL would. It reacts to nothing it is served: the
    /// re-executed tape is fully determined by the script and what the server answers, which is
    /// what makes each scenario's recovered/not-recovered outcome deterministic.
    struct ScriptedAgent {
        requests: Vec<Value>,
    }

    impl AgentDriver for ScriptedAgent {
        fn drive(&self, base_url: &str) -> impl Future<Output = Result<(), AgentError>> + Send {
            let url = format!("{base_url}/v1/chat/completions");
            let bodies = self.requests.clone();
            async move {
                let client = reqwest::Client::new();
                for body in bodies {
                    client
                        .post(&url)
                        .json(&body)
                        .send()
                        .await
                        .expect("the scripted agent reaches the replay listener");
                }
                Ok(())
            }
        }
    }

    fn request(content: &str) -> CapturedRequest {
        CapturedRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Body::Json(json!({
                "model": "claude-sonnet-5",
                "messages": [{ "role": "user", "content": content }],
            })),
        }
    }

    fn response(content: &str) -> CapturedResponse {
        CapturedResponse {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Body::Json(json!({ "choices": [{ "message": { "content": content } }] })),
        }
    }

    /// The request body of a turn, as an SDK would re-serialize it — what the scripted agent sends.
    fn body_of(content: &str) -> Value {
        let Body::Json(body) = request(content).body else {
            unreachable!("request bodies are JSON")
        };
        body
    }

    fn cassette(id: &str, turns: &[(&str, &str)]) -> Cassette {
        let mut cassette = Cassette::new(id);
        for (idx, (question, answer)) in turns.iter().enumerate() {
            cassette.exchanges.push(Exchange {
                idx,
                request: request(question),
                response: response(answer),
            });
        }
        cassette
    }

    // The shared fixture across both scenarios. The good run answers three turns cleanly; the
    // patched cassette is the bad run with its fork step (turn 1) already carrying the good
    // response, so re-execution starts from the counterfactual "what if turn 1 had gone right".
    // `q2_good` is a turn the bad cassette never recorded, so it cache-misses onto the upstream.
    const G0: &str = "acknowledged, looking up order 8841";
    const G1: &str = "order 8841 found, refund eligible";
    const G2: &str = "refund of 42 dollars issued";
    const B2: &str = "escalation ticket opened pending manual review"; // disjoint tokens from G2

    fn good_run() -> Run {
        normalize(&cassette("good", &[("q0", G0), ("q1", G1), ("q2", G2)]))
    }

    /// The patched bad cassette: prefix matches good, turn 1 (the fork) serves the good response,
    /// turn 2 still holds the bad answer for the bad request. Origin (the patched step) is 1.
    fn patched_cassette() -> Cassette {
        cassette("bad", &[("q0", G0), ("q1", G1), ("q2_bad", B2)])
    }

    #[tokio::test]
    async fn a_rerun_that_follows_the_good_path_is_recovered() {
        // The scripted agent, having been served the good turn-1 response, now asks the good
        // turn-2 question — which the bad cassette never recorded, so it cache-misses and the
        // upstream serves the good turn-2 answer. The re-executed outputs are [G0, G1, G2], which
        // aligns against good with no fork.
        let verdict = reexecute_once(
            patched_cassette(),
            &good_run(),
            1,
            &ScriptedAgent {
                requests: vec![body_of("q0"), body_of("q1"), body_of("q2_good")],
            },
            ScriptedUpstream::new([response(G2)]),
            "reexec",
            &DiffParams::default(),
        )
        .await
        .expect("the experiment runs");

        assert_eq!(verdict, RunVerdict::Recovered);
    }

    #[tokio::test]
    async fn a_rerun_that_still_diverges_is_not_recovered() {
        // Even served the good turn-1 response, this agent asks the bad turn-2 question, which the
        // patched cassette still answers with the bad response. The re-executed outputs are
        // [G0, G1, B2]; B2 is disjoint from good's G2, so the fork survives at the tail.
        let verdict = reexecute_once(
            patched_cassette(),
            &good_run(),
            1,
            &ScriptedAgent {
                requests: vec![body_of("q0"), body_of("q1"), body_of("q2_bad")],
            },
            ScriptedUpstream::new([]), // every turn hits the tape; no relay expected
            "reexec",
            &DiffParams::default(),
        )
        .await
        .expect("the experiment runs");

        assert_eq!(verdict, RunVerdict::NotRecovered);
    }
}
