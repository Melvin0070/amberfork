//! `diff --verify`: re-execute the fork to verify the cause, upgrading attribution from static to
//! counterfactual (issue #37).
//!
//! `amberfork-align` localizes *where* the runs diverge and labels it `Static` — a structural
//! claim no re-execution ever checked. `--verify` patches that fork step with the good run's
//! response, re-drives the recorded agent through `amberfork-replay`, and reports whether the run
//! *recovered*. The mechanism lives in [`amberfork_attrib`], entirely behind injected seams; this
//! module is the CLI composition root that supplies the two production seams — a subprocess
//! [`AgentDriver`] and a live [`LiveUpstream`] — and the current-thread runtime that wraps them,
//! keeping tokio at the I/O edge exactly as `serve` and `record` do.

use std::fmt;
use std::future::Future;

use amberfork_align::DiffParams;
use amberfork_attrib::{AgentDriver, AgentError, ReexecError};
use amberfork_model::{Attribution, DiffResult};
use amberfork_record::Cassette;
use amberfork_replay::LiveUpstream;

/// The validated `--verify` configuration, present only when `--verify` was given with the
/// companions re-execution needs. Building it is the single validation gate — everything
/// downstream takes a `VerifyConfig` and never re-checks the flags.
#[derive(Debug)]
pub struct VerifyConfig {
    /// The real provider origin post-branch cache-misses relay to.
    upstream: String,
    /// Env var(s) that point the re-driven agent's SDK at the replay listener.
    base_url_env: Vec<String>,
    /// The agent command to re-drive, e.g. `["python", "agent.py"]`.
    command: Vec<String>,
    /// How many times to re-execute; the recovery verdict is a majority over these.
    runs: u32,
}

impl VerifyConfig {
    /// Resolve the verify flags into a config, or `None` when `--verify` was not requested.
    ///
    /// This is the whole cross-flag contract, kept here (not in clap constraints) so it is
    /// unit-testable and gives one honest message per way it can be wrong.
    ///
    /// # Errors
    /// Returns the user-facing message when the flags are inconsistent: a companion flag given
    /// without `--verify`, or `--verify` missing a companion it cannot run without.
    pub fn resolve(
        verify: bool,
        upstream: Option<&str>,
        base_url_env: &[String],
        command: &[String],
        runs: u32,
    ) -> Result<Option<Self>, String> {
        let has_companions = upstream.is_some() || !base_url_env.is_empty() || !command.is_empty();
        if !verify {
            if has_companions {
                return Err(
                    "--upstream, --base-url-env, and a `-- <cmd>` re-drive only apply with --verify"
                        .to_string(),
                );
            }
            return Ok(None);
        }
        let upstream = upstream
            .ok_or(
                "--verify requires --upstream <URL> — the real provider to relay post-branch \
                 cache-misses to",
            )?
            .to_string();
        if base_url_env.is_empty() {
            return Err(
                "--verify requires at least one --base-url-env <NAME> to point the \
                        re-driven agent at the replay listener"
                    .to_string(),
            );
        }
        if command.is_empty() {
            return Err(
                "--verify requires an agent command after `--`, e.g. `-- python agent.py`"
                    .to_string(),
            );
        }
        Ok(Some(Self {
            upstream,
            base_url_env: base_url_env.to_vec(),
            command: command.to_vec(),
            runs,
        }))
    }
}

/// The production [`AgentDriver`]: spawn the recorded `-- <cmd>` with its base-URL env var(s)
/// pointed at the replay listener — the same way `amberfork record` drove it the first time.
struct SubprocessDriver {
    command: Vec<String>,
    base_url_env: Vec<String>,
}

impl AgentDriver for SubprocessDriver {
    fn drive(&self, base_url: &str) -> impl Future<Output = Result<(), AgentError>> + Send {
        // Own everything the child needs so the returned future is `'static`. It is re-driven once
        // per run, so a few small clones cost nothing against a subprocess launch.
        let base_url = base_url.to_owned();
        let command = self.command.clone();
        let base_url_env = self.base_url_env.clone();
        async move {
            // `VerifyConfig::resolve` guarantees a non-empty command.
            let (program, program_args) = command
                .split_first()
                .expect("verify config guarantees a non-empty agent command");
            let mut child = tokio::process::Command::new(program);
            child.args(program_args);
            // Inherit the environment (the agent's credential lives there); override only the
            // base-URL var(s) so its SDK talks to the listener instead of the provider directly.
            for name in &base_url_env {
                child.env(name, &base_url);
            }
            // The agent's own exit code is not our failure: a re-run that exits non-zero is often
            // exactly the run being re-executed. Only a failure to launch it is a driver error.
            child.status().await.map_err(AgentError::Spawn)?;
            Ok(())
        }
    }
}

/// Why `--verify` could not be carried out. Distinct from a re-run being inconclusive (that folds
/// into the verdict as `Recovery::Unverified`): these are the experiment failing to start at all.
#[derive(Debug)]
pub enum VerifyError {
    /// The async runtime could not be built.
    Runtime(std::io::Error),
    /// The HTTP client for the live provider could not be built.
    HttpClient(reqwest::Error),
    /// A re-execution could not be carried out (the listener would not bind, or the agent could
    /// not be launched).
    Reexec(ReexecError),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(source) => write!(f, "cannot start the async runtime: {source}"),
            Self::HttpClient(source) => write!(
                f,
                "cannot build the HTTP client for the live provider: {source}"
            ),
            Self::Reexec(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for VerifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Runtime(source) => Some(source),
            Self::HttpClient(source) => Some(source),
            Self::Reexec(source) => Some(source),
        }
    }
}

/// Re-execute the diff's fork and return the upgraded attribution, or `None` when the diff
/// converged (there is no fork to verify). The one async edge in `diff`: a current-thread runtime
/// wraps the whole experiment, so the engine and reporting around it stay sync — the same tokio
/// quarantine `serve` and `record` observe.
///
/// # Errors
/// [`VerifyError`] when the experiment cannot run (runtime, HTTP client, or a re-execution that
/// could not start). A re-run that runs but yields no usable trajectory is not an error — it folds
/// into a `Recovery::Unverified` verdict inside [`amberfork_attrib`].
pub fn verify_attribution(
    diff: &DiffResult,
    good: &Cassette,
    bad: &Cassette,
    config: &VerifyConfig,
    params: &DiffParams,
) -> Result<Option<Attribution>, VerifyError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .map_err(VerifyError::Runtime)?;
    let client = reqwest::Client::builder()
        .build()
        .map_err(VerifyError::HttpClient)?;
    let driver = SubprocessDriver {
        command: config.command.clone(),
        base_url_env: config.base_url_env.clone(),
    };
    runtime.block_on(async {
        amberfork_attrib::verify(
            diff,
            good,
            bad,
            &driver,
            || LiveUpstream::new(client.clone(), config.upstream.clone()),
            config.runs,
            params,
        )
        .await
        .map_err(VerifyError::Reexec)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_verify_and_no_companions_is_the_untouched_default() {
        assert!(
            VerifyConfig::resolve(false, None, &[], &[], 3)
                .expect("no flags is valid")
                .is_none()
        );
    }

    #[test]
    fn a_companion_flag_without_verify_is_rejected() {
        for (upstream, env, command) in [
            (Some("http://api"), &[][..], &[][..]),
            (None, &["OPENAI_BASE_URL".to_string()][..], &[][..]),
            (None, &[][..], &["python".to_string()][..]),
        ] {
            let err = VerifyConfig::resolve(false, upstream, env, command, 3)
                .expect_err("a companion without --verify is an error");
            assert!(err.contains("--verify"), "message names the gate: {err}");
        }
    }

    #[test]
    fn verify_missing_a_companion_names_the_one_it_needs() {
        let no_upstream =
            VerifyConfig::resolve(true, None, &strings(&["ENV"]), &strings(&["cmd"]), 3)
                .expect_err("--verify needs --upstream");
        assert!(no_upstream.contains("--upstream"));

        let no_env = VerifyConfig::resolve(true, Some("http://api"), &[], &strings(&["cmd"]), 3)
            .expect_err("--verify needs --base-url-env");
        assert!(no_env.contains("--base-url-env"));

        let no_command =
            VerifyConfig::resolve(true, Some("http://api"), &strings(&["ENV"]), &[], 3)
                .expect_err("--verify needs a command");
        assert!(no_command.contains("`--`"));
    }

    #[test]
    fn a_complete_verify_request_resolves_to_a_config() {
        let config = VerifyConfig::resolve(
            true,
            Some("http://api"),
            &strings(&["OPENAI_BASE_URL"]),
            &strings(&["python", "agent.py"]),
            4,
        )
        .expect("a complete request is valid")
        .expect("and yields a config");
        assert_eq!(config.upstream, "http://api");
        assert_eq!(config.base_url_env, ["OPENAI_BASE_URL"]);
        assert_eq!(config.command, ["python", "agent.py"]);
        assert_eq!(config.runs, 4);
    }
}
