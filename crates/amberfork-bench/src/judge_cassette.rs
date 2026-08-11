//! Cassettes: the judge baseline's answers, cached so a published table replays offline
//! forever (issue #46, registered in notebook 069).
//!
//! **The cassette never stores the prompt.** That is not a size optimisation. The registered
//! corpus is the TRAIL↔HAL natural pairs, whose prompts embed verbatim GAIA questions, and
//! GAIA is gated upstream — BENCHMARK.md's data rules bar that text from this repo, which is
//! why the committed chimera fixtures are sanitized in the first place. Keying on the
//! *rendered prompt's* sha256 keeps the cache byte-exact while keeping the questions out: a
//! cached answer can only ever be replayed for the identical question that produced it, and
//! a reader who wants to see that question renders it locally with `judge-prompt`.
//!
//! The key covers everything that could change an answer — provider, model, arm, prompt
//! revision, rendered prompt, and the decoding actually sent. Anything outside that set is
//! either not ours (the model's own nondeterminism) or would be a protocol violation to vary
//! silently.
//!
//! Default posture is [`Mode::ReplayOnly`]: a missing cassette is an error naming the key, not
//! a quiet trip to the network. Recording is opt-in, at the CLI, with a key present in the
//! environment — so `cargo test --workspace` cannot spend money or reach a provider even by
//! accident.

use crate::judge_prompt::JudgeArm;
use crate::judge_provider::{Decoding, Localizer, PostError, ask_with_retries};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Cassette document version, so a later shape change is legible rather than silent.
pub const CASSETTE_SCHEMA_VERSION: &str = "0.1";

/// Retries for one prompt (registered: three, then the pair is an exclusion for that arm).
pub const ATTEMPTS: u32 = 3;

/// Whether a missing cassette may reach the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The default and the only mode any test runs in.
    ReplayOnly,
    /// Opt-in, CLI-only: call the provider on a miss and record the answer.
    Record,
}

/// One cached answer. Field order is the document's order on disk; `response` sits last so a
/// human reading a committed cassette meets the provenance before the payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cassette {
    pub cassette_schema_version: String,
    pub key: String,
    pub provider: String,
    pub model: String,
    pub arm: String,
    /// The frozen template's sha256 — which prompt *revision* was asked.
    pub prompt_sha256: String,
    /// The rendered prompt's sha256 — which *question*. The prompt text itself is deliberately
    /// absent; see the module docs.
    pub rendered_prompt_sha256: String,
    /// The decoding actually sent, not the decoding intended.
    pub decoding: Decoding,
    /// Unix seconds. A plain integer rather than a formatted date: this workspace has no time
    /// crate, and inventing a date format for a provenance field is not worth a dependency.
    pub recorded_unix: u64,
    pub response: String,
}

/// The identity a cassette is filed under: everything that could change the answer.
#[derive(Debug, Clone, Copy, Serialize)]
struct KeyMaterial<'a> {
    provider: &'a str,
    model: &'a str,
    arm: &'a str,
    prompt_sha256: &'a str,
    rendered_prompt_sha256: &'a str,
    decoding: Decoding,
}

/// The cassette key for one question.
///
/// Serialized through a typed struct, so field order is fixed by the declaration rather than
/// by a map's iteration order — the key must not drift with a serde or hashmap detail.
#[must_use]
pub fn key(
    provider: &str,
    model: &str,
    arm: JudgeArm,
    prompt_sha256: &str,
    rendered_prompt_sha256: &str,
    decoding: Decoding,
) -> String {
    let material = KeyMaterial {
        provider,
        model,
        arm: arm.name(),
        prompt_sha256,
        rendered_prompt_sha256,
        decoding,
    };
    let canonical = serde_json::to_vec(&material).expect("key material serializes");
    format!("{:x}", Sha256::digest(&canonical))
}

/// Why a cassette could not be read or written.
#[derive(Debug)]
pub enum CassetteError {
    Io {
        file: PathBuf,
        source: std::io::Error,
    },
    Malformed {
        file: PathBuf,
        source: serde_json::Error,
    },
    /// The file on disk is filed under a different key than it claims — a copied or
    /// hand-edited cassette, which would silently answer a question it never heard.
    KeyMismatch {
        file: PathBuf,
        expected: String,
        found: String,
    },
    /// [`Mode::ReplayOnly`] and nothing cached. Not a fallback to the network: an arm that
    /// would have to pay to answer must say so.
    Missing { file: PathBuf, key: String },
}

impl fmt::Display for CassetteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { file, source } => write!(f, "cassette {}: {source}", file.display()),
            Self::Malformed { file, source } => {
                write!(f, "cassette {} is malformed: {source}", file.display())
            }
            Self::KeyMismatch {
                file,
                expected,
                found,
            } => write!(
                f,
                "cassette {} is filed under {expected} but claims {found} — a cassette must \
                 answer only the question it recorded",
                file.display()
            ),
            Self::Missing { file, key } => write!(
                f,
                "no cassette for {key} at {} — replay-only mode does not call a provider; \
                 re-run with --live and an API key to record it",
                file.display()
            ),
        }
    }
}

impl std::error::Error for CassetteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Malformed { source, .. } => Some(source),
            Self::KeyMismatch { .. } | Self::Missing { .. } => None,
        }
    }
}

/// A directory of cassettes, one file per question, grouped by arm.
#[derive(Debug)]
pub struct Cassettes {
    root: PathBuf,
}

impl Cassettes {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where one question's answer lives: `<root>/<arm>/<key>.json`.
    #[must_use]
    pub fn path(&self, arm: JudgeArm, key: &str) -> PathBuf {
        self.root.join(arm.name()).join(format!("{key}.json"))
    }

    /// Read a cached answer, if one exists.
    ///
    /// # Errors
    /// [`CassetteError`] if the file exists but cannot be trusted — unreadable, malformed, or
    /// filed under a key it does not claim.
    pub fn read(&self, arm: JudgeArm, key: &str) -> Result<Option<Cassette>, CassetteError> {
        let file = self.path(arm, key);
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(CassetteError::Io { file, source }),
        };
        let cassette: Cassette =
            serde_json::from_slice(&bytes).map_err(|source| CassetteError::Malformed {
                file: file.clone(),
                source,
            })?;
        if cassette.key != key {
            return Err(CassetteError::KeyMismatch {
                file,
                expected: key.to_string(),
                found: cassette.key,
            });
        }
        Ok(Some(cassette))
    }

    /// Write a cassette, creating its arm directory.
    ///
    /// # Errors
    /// [`CassetteError::Io`] if the directory or file cannot be written.
    pub fn write(&self, arm: JudgeArm, cassette: &Cassette) -> Result<PathBuf, CassetteError> {
        let file = self.path(arm, &cassette.key);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CassetteError::Io {
                file: parent.to_path_buf(),
                source,
            })?;
        }
        // Pretty-printed with a trailing newline: cassettes are committed artifacts a reviewer
        // reads in a diff, not an opaque cache.
        let mut bytes = serde_json::to_vec_pretty(cassette).expect("cassette serializes");
        bytes.push(b'\n');
        std::fs::write(&file, bytes).map_err(|source| CassetteError::Io {
            file: file.clone(),
            source,
        })?;
        Ok(file)
    }
}

/// What answering one question did.
#[derive(Debug)]
pub struct Answer {
    pub text: String,
    pub key: String,
    /// True when the answer came off disk. The distinction publishes: a table whose rows were
    /// all replayed is reproducible, and one that quietly re-asked is not the same run.
    pub replayed: bool,
}

/// Why a question could not be answered.
#[derive(Debug)]
pub enum ObtainError {
    Cassette(CassetteError),
    /// Transport failure that survived its retries. Registered: the pair becomes an exclusion
    /// for that arm under rule 4 — distinct from a parse failure, which is scored as a miss.
    Transport {
        attempts: u32,
        source: PostError,
    },
}

impl fmt::Display for ObtainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cassette(err) => write!(f, "{err}"),
            Self::Transport { attempts, source } => {
                write!(f, "{source} (after {attempts} attempts)")
            }
        }
    }
}

impl std::error::Error for ObtainError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cassette(err) => Some(err),
            Self::Transport { .. } => None,
        }
    }
}

/// One question, ready to ask or replay: which arm, which prompt revision, which rendered
/// question, and the text itself. Grouped because these four always travel together and only
/// the last of them is ever allowed to touch disk.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    pub arm: JudgeArm,
    pub prompt_sha256: &'a str,
    pub rendered_prompt_sha256: &'a str,
    pub prompt: &'a str,
}

/// Answer one question: from the cassette if it is there, from the provider only if `mode`
/// allows it — and record what came back.
///
/// # Errors
/// [`ObtainError`] on a cassette problem, a replay-only miss, or a transport failure that
/// survived [`ATTEMPTS`] attempts.
pub fn obtain(
    cassettes: &Cassettes,
    mode: Mode,
    localizer: &dyn Localizer,
    question: Question<'_>,
    backoff: Duration,
) -> Result<Answer, ObtainError> {
    let Question {
        arm,
        prompt_sha256,
        rendered_prompt_sha256,
        prompt,
    } = question;
    let decoding = localizer.decoding();
    let key = key(
        localizer.provider(),
        localizer.model(),
        arm,
        prompt_sha256,
        rendered_prompt_sha256,
        decoding,
    );

    if let Some(cassette) = cassettes.read(arm, &key).map_err(ObtainError::Cassette)? {
        return Ok(Answer {
            text: cassette.response,
            key,
            replayed: true,
        });
    }
    if mode == Mode::ReplayOnly {
        return Err(ObtainError::Cassette(CassetteError::Missing {
            file: cassettes.path(arm, &key),
            key,
        }));
    }

    let text = ask_with_retries(localizer, prompt, ATTEMPTS, backoff).map_err(|source| {
        ObtainError::Transport {
            attempts: ATTEMPTS,
            source,
        }
    })?;
    let cassette = Cassette {
        cassette_schema_version: CASSETTE_SCHEMA_VERSION.to_string(),
        key: key.clone(),
        provider: localizer.provider().to_string(),
        model: localizer.model().to_string(),
        arm: arm.name().to_string(),
        prompt_sha256: prompt_sha256.to_string(),
        rendered_prompt_sha256: rendered_prompt_sha256.to_string(),
        decoding,
        recorded_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default(),
        response: text.clone(),
    };
    cassettes
        .write(arm, &cassette)
        .map_err(ObtainError::Cassette)?;
    Ok(Answer {
        text,
        key,
        replayed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A [`Localizer`] that answers from a script and counts its calls — no client, no
    /// network, and a panic if a replay path reaches it.
    struct ScriptedLocalizer {
        replies: RefCell<Vec<Result<String, PostError>>>,
        prompts: RefCell<Vec<String>>,
        decoding: Decoding,
    }

    impl ScriptedLocalizer {
        fn answering(text: &str) -> Self {
            Self {
                replies: RefCell::new(vec![Ok(text.to_string())]),
                prompts: RefCell::new(Vec::new()),
                decoding: Decoding::registered(2000),
            }
        }

        fn failing() -> Self {
            Self {
                replies: RefCell::new(vec![
                    Err(PostError {
                        url: "u".to_string(),
                        msg: "timeout".to_string(),
                        retryable: true,
                    });
                    ATTEMPTS as usize
                ]),
                prompts: RefCell::new(Vec::new()),
                decoding: Decoding::registered(2000),
            }
        }

        fn calls(&self) -> usize {
            self.prompts.borrow().len()
        }
    }

    impl Localizer for ScriptedLocalizer {
        fn provider(&self) -> &'static str {
            "scripted"
        }
        fn model(&self) -> &str {
            "scripted-1"
        }
        fn decoding(&self) -> Decoding {
            self.decoding
        }
        fn ask(&self, prompt: &str) -> Result<String, PostError> {
            self.prompts.borrow_mut().push(prompt.to_string());
            let mut replies = self.replies.borrow_mut();
            assert!(!replies.is_empty(), "localizer called beyond its script");
            replies.remove(0)
        }
    }

    fn store() -> (tempfile::TempDir, Cassettes) {
        let dir = tempfile::tempdir().expect("tempdir");
        let cassettes = Cassettes::new(dir.path());
        (dir, cassettes)
    }

    const PROMPT_SHA: &str = "aa";
    const RENDERED_SHA: &str = "bb";

    #[test]
    fn the_key_changes_when_anything_that_could_change_the_answer_changes() {
        let base = key(
            "openai",
            "gpt-5.6-sol",
            JudgeArm::Paired,
            PROMPT_SHA,
            RENDERED_SHA,
            Decoding::registered(2000),
        );
        let variants = [
            key(
                "gemini",
                "gpt-5.6-sol",
                JudgeArm::Paired,
                PROMPT_SHA,
                RENDERED_SHA,
                Decoding::registered(2000),
            ),
            key(
                "openai",
                "gpt-5.6-luna",
                JudgeArm::Paired,
                PROMPT_SHA,
                RENDERED_SHA,
                Decoding::registered(2000),
            ),
            key(
                "openai",
                "gpt-5.6-sol",
                JudgeArm::Single,
                PROMPT_SHA,
                RENDERED_SHA,
                Decoding::registered(2000),
            ),
            key(
                "openai",
                "gpt-5.6-sol",
                JudgeArm::Paired,
                "cc",
                RENDERED_SHA,
                Decoding::registered(2000),
            ),
            key(
                "openai",
                "gpt-5.6-sol",
                JudgeArm::Paired,
                PROMPT_SHA,
                "dd",
                Decoding::registered(2000),
            ),
            key(
                "openai",
                "gpt-5.6-sol",
                JudgeArm::Paired,
                PROMPT_SHA,
                RENDERED_SHA,
                Decoding::registered(4000),
            ),
            key(
                "openai",
                "gpt-5.6-sol",
                JudgeArm::Paired,
                PROMPT_SHA,
                RENDERED_SHA,
                Decoding {
                    temperature: None,
                    max_output_tokens: 2000,
                },
            ),
        ];

        for variant in variants {
            assert_ne!(base, variant, "a component of the key is not being hashed");
        }
    }

    #[test]
    fn the_same_question_keys_the_same_way_every_time() {
        let once = key(
            "openai",
            "m",
            JudgeArm::Single,
            PROMPT_SHA,
            RENDERED_SHA,
            Decoding::registered(2000),
        );
        let twice = key(
            "openai",
            "m",
            JudgeArm::Single,
            PROMPT_SHA,
            RENDERED_SHA,
            Decoding::registered(2000),
        );
        assert_eq!(once, twice);
        assert_eq!(once.len(), 64);
    }

    #[test]
    fn recording_then_replaying_returns_the_same_answer_without_asking_twice() {
        let (_dir, cassettes) = store();
        let judge = ScriptedLocalizer::answering("{\"step\": 4}");

        let first = obtain(
            &cassettes,
            Mode::Record,
            &judge,
            Question {
                arm: JudgeArm::Paired,
                prompt_sha256: PROMPT_SHA,
                rendered_prompt_sha256: RENDERED_SHA,
                prompt: "PROMPT",
            },
            Duration::ZERO,
        )
        .expect("records");
        let second = obtain(
            &cassettes,
            // Replay-only on the second pass: the answer must come off disk, and the scripted
            // localizer would panic if it were asked again.
            Mode::ReplayOnly,
            &judge,
            Question {
                arm: JudgeArm::Paired,
                prompt_sha256: PROMPT_SHA,
                rendered_prompt_sha256: RENDERED_SHA,
                prompt: "PROMPT",
            },
            Duration::ZERO,
        )
        .expect("replays");

        assert!(!first.replayed);
        assert!(second.replayed);
        assert_eq!(first.text, second.text);
        assert_eq!(first.key, second.key);
        assert_eq!(judge.calls(), 1, "the provider was asked once");
    }

    #[test]
    fn a_recorded_cassette_never_contains_the_prompt() {
        // The rule that keeps gated GAIA question text out of this repo. Structural, not
        // hopeful: the prompt is simply not a field — and this test fails loudly if that
        // ever changes.
        let (_dir, cassettes) = store();
        let judge = ScriptedLocalizer::answering("ok");
        let prompt = "What is the surname of the equine veterinarian mentioned in 1.E Exercises?";

        let answer = obtain(
            &cassettes,
            Mode::Record,
            &judge,
            Question {
                arm: JudgeArm::Single,
                prompt_sha256: PROMPT_SHA,
                rendered_prompt_sha256: RENDERED_SHA,
                prompt,
            },
            Duration::ZERO,
        )
        .expect("records");

        let bytes = std::fs::read_to_string(cassettes.path(JudgeArm::Single, &answer.key))
            .expect("cassette file");
        assert!(
            !bytes.contains("equine veterinarian"),
            "a cassette must never carry the question: {bytes}"
        );
        assert!(bytes.contains(RENDERED_SHA), "but it must carry its hash");
    }

    #[test]
    fn replay_only_never_calls_a_provider_on_a_miss() {
        let (_dir, cassettes) = store();
        let judge = ScriptedLocalizer::answering("unreachable");

        let err = obtain(
            &cassettes,
            Mode::ReplayOnly,
            &judge,
            Question {
                arm: JudgeArm::Paired,
                prompt_sha256: PROMPT_SHA,
                rendered_prompt_sha256: RENDERED_SHA,
                prompt: "PROMPT",
            },
            Duration::ZERO,
        )
        .expect_err("nothing cached");

        assert!(
            matches!(err, ObtainError::Cassette(CassetteError::Missing { .. })),
            "got: {err}"
        );
        assert!(err.to_string().contains("--live"), "got: {err}");
        assert_eq!(judge.calls(), 0, "replay-only must not reach a provider");
    }

    #[test]
    fn a_transport_failure_that_survives_its_retries_is_reported_as_such() {
        let (_dir, cassettes) = store();
        let judge = ScriptedLocalizer::failing();

        let err = obtain(
            &cassettes,
            Mode::Record,
            &judge,
            Question {
                arm: JudgeArm::Paired,
                prompt_sha256: PROMPT_SHA,
                rendered_prompt_sha256: RENDERED_SHA,
                prompt: "PROMPT",
            },
            Duration::ZERO,
        )
        .expect_err("all attempts failed");

        assert!(matches!(err, ObtainError::Transport { .. }), "got: {err}");
        assert_eq!(judge.calls(), ATTEMPTS as usize);
        // Nothing was written: a failed call must not leave a cassette that would later
        // replay as though it had succeeded.
        assert!(!cassettes.root.join(JudgeArm::Paired.name()).exists());
    }

    #[test]
    fn a_cassette_filed_under_the_wrong_key_is_refused() {
        let (_dir, cassettes) = store();
        let judge = ScriptedLocalizer::answering("ok");
        let answer = obtain(
            &cassettes,
            Mode::Record,
            &judge,
            Question {
                arm: JudgeArm::Single,
                prompt_sha256: PROMPT_SHA,
                rendered_prompt_sha256: RENDERED_SHA,
                prompt: "PROMPT",
            },
            Duration::ZERO,
        )
        .expect("records");

        // Copy one question's answer onto another's filename — the shape a careless
        // hand-edit or a rebased cassette takes.
        let source = cassettes.path(JudgeArm::Single, &answer.key);
        let forged = cassettes.path(JudgeArm::Single, &"f".repeat(64));
        std::fs::copy(&source, &forged).expect("copy");

        let err = cassettes
            .read(JudgeArm::Single, &"f".repeat(64))
            .expect_err("a cassette must answer only its own question");

        assert!(
            matches!(err, CassetteError::KeyMismatch { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn a_malformed_cassette_is_an_error_not_a_miss() {
        // Treating unreadable JSON as "not cached" would silently re-ask a question that was
        // already paid for — and, in replay-only mode, hide a corrupt committed artifact.
        let (_dir, cassettes) = store();
        let path = cassettes.path(JudgeArm::Paired, "deadbeef");
        std::fs::create_dir_all(path.parent().expect("arm dir")).expect("mkdir");
        std::fs::write(&path, b"{ not json").expect("write");

        let err = cassettes
            .read(JudgeArm::Paired, "deadbeef")
            .expect_err("malformed");

        assert!(matches!(err, CassetteError::Malformed { .. }), "got: {err}");
    }
}
