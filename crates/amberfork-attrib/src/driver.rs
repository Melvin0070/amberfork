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
    use crate::testkit::{
        B2, G0, G1, G2, ScriptedAgent, body_of, cassette, good_cassette, response,
    };
    use amberfork_record::Cassette;
    use amberfork_replay::ScriptedUpstream;

    fn good_run() -> Run {
        normalize(&good_cassette())
    }

    /// The patched bad cassette: prefix matches good, turn 1 (the fork) serves the good response,
    /// turn 2 still holds the bad answer for the bad request. Origin (the patched step) is 1 — this
    /// is what `patch_cassette` produces from the good/bad pair, built directly here so the driver
    /// test does not depend on the patch builder.
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
