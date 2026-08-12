//! The LLM-judge baseline's providers (issue #46, registered in notebook 069).
//!
//! Three registered conditions need three places to ask: OpenAI for the frontier arm, Gemini
//! for the cross-provider check that keeps the result from being OpenAI-specific, and a local
//! Ollama for the no-API-key arm a reader can reproduce without a card on file.
//!
//! Blocking, via `ureq`, like every other networked path in this harness — the tokio
//! quarantine (CLAUDE.md) keeps async out of engine and harness code alike.
//!
//! Two seams, both for the same reason: `cargo test --workspace` must not be able to reach a
//! network even by accident. [`Post`] is the one function that touches it (mirroring
//! `fetch::Http`), so every provider's request shaping and response reading is exercised
//! against canned bytes; and [`Localizer`] is the seam above that, so the cassette layer and
//! the CLI can be driven by a scripted double that never constructs a client at all.
//!
//! Request shapes are written against each provider's documented API and are *unvalidated
//! against a live endpoint* until #46 slice A3 makes the first real call. That is deliberate
//! sequencing — no money is spent before the run is registered — and it is why the shapes are
//! pure functions with their own tests: when a live call disagrees, the fix is one function
//! and one test, not a debugging session inside a scoring run.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Decoding parameters, as *sent*. Registered: temperature 0, single sample, no tools.
///
/// `temperature` is optional because a reasoning model may reject the parameter outright; the
/// registration's instruction in that case is to use the most deterministic setting the model
/// accepts and record what that was. So this travels into the cassette rather than being
/// assumed — the published table can then say exactly how each arm was decoded instead of
/// quoting an intention.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Decoding {
    pub temperature: Option<f64>,
    pub max_output_tokens: u32,
}

impl Decoding {
    /// The registered default: temperature 0, one sample.
    #[must_use]
    pub fn registered(max_output_tokens: u32) -> Self {
        Self {
            temperature: Some(0.0),
            max_output_tokens,
        }
    }
}

/// The one seam that touches the network: a blocking `POST` returning the response body.
pub trait Post {
    /// Post `body` to `url` with `headers`, returning the response body on a 2xx status.
    ///
    /// # Errors
    /// [`PostError`], classified as retryable or not — the caller's retry policy depends on
    /// telling a rate limit apart from a malformed request.
    fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<Vec<u8>, PostError>;
}

/// A failed `POST`: what it was for, what went wrong, and whether trying again could help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostError {
    pub url: String,
    pub msg: String,
    /// Transport failures, 429s and 5xx are retryable; a 4xx is the request being wrong, and
    /// repeating it just spends the rate limit on the same mistake.
    pub retryable: bool,
}

impl fmt::Display for PostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "post {}: {}", self.url, self.msg)
    }
}

/// The real [`Post`], backed by `ureq`.
pub struct UreqPost;

impl Post for UreqPost {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<Vec<u8>, PostError> {
        let mut request = ureq::post(url).header("Content-Type", "application/json");
        for (name, value) in headers {
            request = request.header(*name, value);
        }
        match request.send(body) {
            Ok(mut response) => response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE_BYTES)
                .read_to_vec()
                .map_err(|err| PostError {
                    url: url.to_string(),
                    msg: err.to_string(),
                    retryable: true,
                }),
            Err(err) => {
                let retryable = match &err {
                    ureq::Error::StatusCode(code) => *code == 429 || *code >= 500,
                    _ => true,
                };
                Err(PostError {
                    url: url.to_string(),
                    msg: err.to_string(),
                    retryable,
                })
            }
        }
    }
}

/// Cap on a single provider response. Generous next to a judge's few-hundred-token answer;
/// it exists only so a misbehaving endpoint cannot balloon memory.
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

/// Asking one model to localize the decisive step.
///
/// Deliberately *not* `amberfork_judge::Judge`. That trait narrates a fork the aligner already
/// found and structurally cannot report a location; a baseline that cannot report a location
/// cannot be scored. The two must stay separate — the product's judge should remain unable to
/// invent a step index (notebook 069).
pub trait Localizer {
    /// Provider identity, as recorded in the cassette key and the published table.
    fn provider(&self) -> &'static str;
    /// Model identity, likewise.
    fn model(&self) -> &str;
    /// The decoding parameters this localizer sends.
    fn decoding(&self) -> Decoding;
    /// Ask the model, returning its raw answer text — unparsed, because the parse failure
    /// rate is itself a published number ([`crate::judge_answer`]).
    ///
    /// # Errors
    /// [`PostError`] on any transport or provider failure.
    fn ask(&self, prompt: &str) -> Result<String, PostError>;
}

/// Call `localizer`, retrying a *retryable* failure up to `attempts` times total.
///
/// Registered distinction (notebook 069): a transport failure that survives its retries makes
/// the pair an exclusion for that arm under rule 4, while a parse failure is scored as a miss.
/// One is our infrastructure failing, the other is the method failing, and collapsing them
/// would let a flaky network flatter a baseline.
///
/// # Errors
/// The last [`PostError`] seen, after `attempts` tries or on the first non-retryable failure.
pub fn ask_with_retries(
    localizer: &dyn Localizer,
    prompt: &str,
    attempts: u32,
    backoff: Duration,
) -> Result<String, PostError> {
    let mut last = None;
    for attempt in 0..attempts.max(1) {
        match localizer.ask(prompt) {
            Ok(text) => return Ok(text),
            Err(err) => {
                if !err.retryable {
                    return Err(err);
                }
                last = Some(err);
                if attempt + 1 < attempts && !backoff.is_zero() {
                    // Linear, not exponential: three attempts never wait long enough for the
                    // difference to matter, and a predictable wait is easier to reason about
                    // when a run of 23 pairs is sitting behind it.
                    std::thread::sleep(backoff * (attempt + 1));
                }
            }
        }
    }
    Err(last.expect("at least one attempt ran"))
}

/// The frontier arm's provider (`gpt-5.6-sol`, `gpt-5.6-luna` for stepwise).
pub struct OpenAi<P: Post> {
    post: P,
    base_url: String,
    model: String,
    api_key: String,
    decoding: Decoding,
}

impl<P: Post> OpenAi<P> {
    pub fn new(
        post: P,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        decoding: Decoding,
    ) -> Self {
        Self {
            post,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api_key: api_key.into(),
            decoding,
        }
    }
}

impl<P: Post> Localizer for OpenAi<P> {
    fn provider(&self) -> &'static str {
        "openai"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn decoding(&self) -> Decoding {
        self.decoding
    }
    fn ask(&self, prompt: &str) -> Result<String, PostError> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = openai_body(&self.model, prompt, self.decoding);
        let raw = self.post.post(
            &url,
            &[("Authorization", format!("Bearer {}", self.api_key))],
            body.to_string().as_bytes(),
        )?;
        openai_text(&raw).map_err(|msg| PostError {
            url,
            msg,
            retryable: false,
        })
    }
}

/// `max_completion_tokens`, not `max_tokens`: the reasoning-era parameter name. `n` is left
/// unset rather than sent as 1 — the default is a single sample, and sending a parameter a
/// model may reject buys nothing.
fn openai_body(model: &str, prompt: &str, decoding: Decoding) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_completion_tokens": decoding.max_output_tokens,
    });
    if let Some(temperature) = decoding.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    body
}

fn openai_text(raw: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|err| format!("response is not JSON: {err}"))?;
    value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("no /choices/0/message/content in response: {value}"))
}

/// The cross-provider check (`gemini-3.6-flash`, free tier).
pub struct Gemini<P: Post> {
    post: P,
    base_url: String,
    model: String,
    api_key: String,
    decoding: Decoding,
}

impl<P: Post> Gemini<P> {
    pub fn new(
        post: P,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        decoding: Decoding,
    ) -> Self {
        Self {
            post,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api_key: api_key.into(),
            decoding,
        }
    }
}

impl<P: Post> Localizer for Gemini<P> {
    fn provider(&self) -> &'static str {
        "gemini"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn decoding(&self) -> Decoding {
        self.decoding
    }
    fn ask(&self, prompt: &str) -> Result<String, PostError> {
        // The key rides in a header, never the query string: a URL carrying a secret leaks
        // into shell history, proxy logs, and any error message that echoes the URL — and
        // this module's own `PostError` echoes the URL.
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );
        let body = gemini_body(prompt, self.decoding);
        let raw = self.post.post(
            &url,
            &[("x-goog-api-key", self.api_key.clone())],
            body.to_string().as_bytes(),
        )?;
        gemini_text(&raw).map_err(|msg| PostError {
            url,
            msg,
            retryable: false,
        })
    }
}

fn gemini_body(prompt: &str, decoding: Decoding) -> serde_json::Value {
    let mut config = serde_json::json!({ "maxOutputTokens": decoding.max_output_tokens });
    if let Some(temperature) = decoding.temperature {
        config["temperature"] = serde_json::json!(temperature);
    }
    serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": config,
    })
}

/// Concatenates every text part rather than reading only the first: a response split across
/// parts would otherwise lose its tail, and the answer contract puts the JSON object *last*.
///
/// An answered-but-empty completion returns `Ok("")`, not an error. A thinking model can spend
/// its whole budget on reasoning tokens and emit no text — `finishReason` `STOP` or
/// `MALFORMED_RESPONSE` over an empty `content`, with the request itself perfectly successful.
/// That is the judge failing its own output contract, which 069 scores as a *miss*: "a judge
/// that cannot obey its own output contract is worse at the task, not un-evaluable." Raising it
/// as a transport error would make it a rule-4 exclusion instead, dropping a failure out of the
/// denominator and flattering the arm. A malformed *envelope* — no candidate at all — is still
/// an error, because that is the provider failing rather than the model.
fn gemini_text(raw: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|err| format!("response is not JSON: {err}"))?;
    let candidate = value
        .pointer("/candidates/0")
        .ok_or_else(|| format!("no /candidates/0 in response: {value}"))?;
    let Some(parts) = candidate
        .pointer("/content/parts")
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(String::new());
    };
    Ok(parts
        .iter()
        .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
        .collect())
}

/// The no-API-key arm: a local Ollama server, over the same `/api/generate` shape the rest of
/// this workspace already speaks (`amberfork-judge::OllamaJudge`, `verify_cli.rs`).
pub struct Ollama<P: Post> {
    post: P,
    base_url: String,
    model: String,
    decoding: Decoding,
    num_ctx: Option<u32>,
}

impl<P: Post> Ollama<P> {
    pub fn new(
        post: P,
        base_url: impl Into<String>,
        model: impl Into<String>,
        decoding: Decoding,
    ) -> Self {
        Self {
            post,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            decoding,
            num_ctx: None,
        }
    }

    /// Set the context window Ollama loads the model with, in tokens.
    ///
    /// Load-bearing for this baseline, not a tuning knob. Ollama's default window is 4096 and
    /// it silently drops whatever does not fit — on the dev split 19 of 23 `judge-paired`
    /// prompts are larger than that, so an unset window answers a question the judge only
    /// partly saw and reports it as an ordinary miss. 069 froze the payload cap precisely so
    /// no provider could truncate a prompt out from under the protocol.
    pub fn with_num_ctx(mut self, num_ctx: u32) -> Self {
        self.num_ctx = Some(num_ctx);
        self
    }
}

impl<P: Post> Localizer for Ollama<P> {
    fn provider(&self) -> &'static str {
        "ollama"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn decoding(&self) -> Decoding {
        self.decoding
    }
    fn ask(&self, prompt: &str) -> Result<String, PostError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = ollama_body(&self.model, prompt, self.decoding, self.num_ctx);
        let raw = self.post.post(&url, &[], body.to_string().as_bytes())?;
        ollama_text(&raw, self.num_ctx).map_err(|msg| PostError {
            url,
            msg,
            retryable: false,
        })
    }
}

fn ollama_body(
    model: &str,
    prompt: &str,
    decoding: Decoding,
    num_ctx: Option<u32>,
) -> serde_json::Value {
    let mut options = serde_json::json!({ "num_predict": decoding.max_output_tokens });
    if let Some(num_ctx) = num_ctx {
        options["num_ctx"] = serde_json::json!(num_ctx);
    }
    if let Some(temperature) = decoding.temperature {
        options["temperature"] = serde_json::json!(temperature);
    }
    serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "options": options,
    })
}

/// Reads the answer, and refuses one the model never fully saw.
///
/// Ollama reports how many prompt tokens it actually evaluated. A count that reaches the
/// window means the prompt was cut to fit, and the reply answers a truncated question. That is
/// our infrastructure failing, not the method failing, so it must surface as an error the run
/// tabulates as an exclusion — scoring it as a miss would credit the arm's loss to the judge.
fn ollama_text(raw: &[u8], num_ctx: Option<u32>) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|err| format!("response is not JSON: {err}"))?;
    if let (Some(num_ctx), Some(evaluated)) = (
        num_ctx,
        value
            .get("prompt_eval_count")
            .and_then(serde_json::Value::as_u64),
    ) {
        if evaluated >= u64::from(num_ctx) {
            return Err(format!(
                "prompt filled the {num_ctx}-token context window ({evaluated} evaluated): \
                 the model saw a truncated prompt"
            ));
        }
    }
    value
        .get("response")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("no `response` field in reply: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// One recorded request: URL, headers, and the JSON body as sent.
    type RecordedCall = (String, Vec<(String, String)>, serde_json::Value);

    /// A [`Post`] that answers from a script and records what it was asked — the offline
    /// stand-in that keeps every provider test network-free.
    struct FakePost {
        replies: RefCell<Vec<Result<Vec<u8>, PostError>>>,
        calls: RefCell<Vec<RecordedCall>>,
    }

    impl FakePost {
        fn ok(body: &str) -> Self {
            Self::scripted(vec![Ok(body.as_bytes().to_vec())])
        }

        fn scripted(replies: Vec<Result<Vec<u8>, PostError>>) -> Self {
            Self {
                replies: RefCell::new(replies),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn call(&self, n: usize) -> RecordedCall {
            self.calls.borrow()[n].clone()
        }

        fn call_count(&self) -> usize {
            self.calls.borrow().len()
        }
    }

    impl Post for &FakePost {
        fn post(
            &self,
            url: &str,
            headers: &[(&str, String)],
            body: &[u8],
        ) -> Result<Vec<u8>, PostError> {
            self.calls.borrow_mut().push((
                url.to_string(),
                headers
                    .iter()
                    .map(|(name, value)| ((*name).to_string(), value.clone()))
                    .collect(),
                serde_json::from_slice(body).expect("request body is JSON"),
            ));
            let mut replies = self.replies.borrow_mut();
            if replies.is_empty() {
                panic!("provider called more times than scripted");
            }
            replies.remove(0)
        }
    }

    fn retryable(msg: &str) -> PostError {
        PostError {
            url: "u".to_string(),
            msg: msg.to_string(),
            retryable: true,
        }
    }

    #[test]
    fn openai_sends_the_frozen_decoding_and_reads_the_message_content() {
        let post = FakePost::ok(r#"{"choices":[{"message":{"content":"{\"step\": 3}"}}]}"#);
        let judge = OpenAi::new(
            &post,
            "https://api.openai.test/",
            "gpt-5.6-sol",
            "sk-test",
            Decoding::registered(2000),
        );

        let answer = judge.ask("PROMPT").expect("scripted reply");

        assert_eq!(answer, "{\"step\": 3}");
        let (url, headers, body) = post.call(0);
        assert_eq!(url, "https://api.openai.test/v1/chat/completions");
        assert_eq!(
            headers,
            vec![("Authorization".to_string(), "Bearer sk-test".to_string())]
        );
        assert_eq!(body["model"], "gpt-5.6-sol");
        assert_eq!(body["messages"][0]["content"], "PROMPT");
        assert_eq!(body["temperature"], 0.0);
        // The reasoning-era parameter name; `max_tokens` is rejected by these models.
        assert_eq!(body["max_completion_tokens"], 2000);
    }

    #[test]
    fn a_model_that_rejects_temperature_sends_none_at_all() {
        // Registered fallback (notebook 069): where a model refuses the parameter, use the
        // most deterministic setting it accepts and record what that was. Sending `null`
        // would be a different request than sending nothing.
        let post = FakePost::ok(r#"{"choices":[{"message":{"content":"ok"}}]}"#);
        let judge = OpenAi::new(
            &post,
            "https://api.openai.test",
            "gpt-5.6-sol",
            "sk-test",
            Decoding {
                temperature: None,
                max_output_tokens: 2000,
            },
        );

        judge.ask("PROMPT").expect("scripted reply");

        let (_, _, body) = post.call(0);
        assert!(body.get("temperature").is_none(), "got: {body}");
    }

    #[test]
    fn a_response_missing_the_content_path_is_not_retried() {
        // A well-formed HTTP 200 carrying a shape we do not understand is a bug in our
        // reading, not a blip; retrying it burns budget on the identical answer.
        let post = FakePost::ok(r#"{"error":{"message":"model not found"}}"#);
        let judge = OpenAi::new(
            &post,
            "https://api.openai.test",
            "nope",
            "sk-test",
            Decoding::registered(2000),
        );

        let err = judge.ask("PROMPT").expect_err("unreadable response");

        assert!(!err.retryable, "got: {err:?}");
        assert!(err.msg.contains("model not found"), "got: {}", err.msg);
    }

    #[test]
    fn gemini_puts_the_key_in_a_header_not_the_url() {
        let post = FakePost::ok(r#"{"candidates":[{"content":{"parts":[{"text":"hi"}]}}]}"#);
        let judge = Gemini::new(
            &post,
            "https://gemini.test",
            "gemini-3.6-flash",
            "secret-key",
            Decoding::registered(2000),
        );

        let answer = judge.ask("PROMPT").expect("scripted reply");

        assert_eq!(answer, "hi");
        let (url, headers, body) = post.call(0);
        assert_eq!(
            url,
            "https://gemini.test/v1beta/models/gemini-3.6-flash:generateContent"
        );
        assert!(
            !url.contains("secret-key"),
            "a URL carrying a secret leaks into logs and error messages: {url}"
        );
        assert_eq!(
            headers,
            vec![("x-goog-api-key".to_string(), "secret-key".to_string())]
        );
        assert_eq!(body["contents"][0]["parts"][0]["text"], "PROMPT");
        assert_eq!(body["generationConfig"]["temperature"], 0.0);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 2000);
    }

    #[test]
    fn gemini_concatenates_every_text_part() {
        // The answer contract puts the JSON object on the final line; reading only part 0
        // would drop exactly the part that carries the answer.
        let post = FakePost::ok(
            r#"{"candidates":[{"content":{"parts":[{"text":"because…\n"},{"text":"{\"step\": 2}"}]}}]}"#,
        );
        let judge = Gemini::new(
            &post,
            "https://gemini.test",
            "gemini-3.6-flash",
            "k",
            Decoding::registered(2000),
        );

        assert_eq!(
            judge.ask("PROMPT").expect("scripted reply"),
            "because…\n{\"step\": 2}"
        );
    }

    /// Observed live on `pair_50`, reproducibly and across two different API keys: a successful
    /// request whose candidate carries no content at all. The model answered nothing; it did
    /// not fail to be reached. Scoring it as a miss keeps the failure in the denominator.
    #[test]
    fn an_empty_completion_is_an_unparseable_answer_not_a_transport_failure() {
        let post = FakePost::ok(
            r#"{"candidates":[{"content":{},"finishReason":"MALFORMED_RESPONSE","index":0}],"usageMetadata":{"thoughtsTokenCount":62}}"#,
        );
        let judge = Gemini::new(
            &post,
            "https://gemini.test",
            "gemini-3.6-flash",
            "k",
            Decoding::registered(2000),
        );

        assert_eq!(
            judge
                .ask("PROMPT")
                .expect("an empty answer is still an answer"),
            ""
        );
    }

    /// The other side of that line: a reply with no candidate at all is the provider failing,
    /// which stays an error and so stays a rule-4 exclusion.
    #[test]
    fn a_reply_with_no_candidate_is_still_an_error() {
        let post = FakePost::ok(r#"{"promptFeedback":{"blockReason":"OTHER"}}"#);
        let judge = Gemini::new(
            &post,
            "https://gemini.test",
            "gemini-3.6-flash",
            "k",
            Decoding::registered(2000),
        );

        let err = judge.ask("PROMPT").expect_err("no candidate");

        assert!(err.msg.contains("no /candidates/0"), "got: {}", err.msg);
    }

    #[test]
    fn ollama_speaks_the_workspace_local_provider_shape() {
        let post = FakePost::ok(r#"{"response":"{\"step\": 1}","done":true}"#);
        let judge = Ollama::new(
            &post,
            "http://127.0.0.1:11434",
            "qwen3:8b",
            Decoding::registered(2000),
        );

        let answer = judge.ask("PROMPT").expect("scripted reply");

        assert_eq!(answer, "{\"step\": 1}");
        let (url, headers, body) = post.call(0);
        assert_eq!(url, "http://127.0.0.1:11434/api/generate");
        assert!(
            headers.is_empty(),
            "the local arm needs no key: {headers:?}"
        );
        assert_eq!(body["stream"], false);
        assert_eq!(body["options"]["temperature"], 0.0);
        assert_eq!(body["options"]["num_predict"], 2000);
        assert!(
            body["options"].get("num_ctx").is_none(),
            "an unset window stays unset: {body}"
        );
    }

    #[test]
    fn ollama_sends_the_context_window_it_was_given() {
        let post = FakePost::ok(r#"{"response":"{\"step\": 1}","prompt_eval_count":27595}"#);
        let judge = Ollama::new(
            &post,
            "http://local",
            "qwen3:8b",
            Decoding::registered(2000),
        )
        .with_num_ctx(40960);

        judge.ask("PROMPT").expect("scripted reply");

        assert_eq!(post.call(0).2["options"]["num_ctx"], 40960);
    }

    /// The whole point of setting a window: a prompt that overflows it must be a loud failure.
    /// Ollama answers a truncated prompt with a perfectly well-formed reply, so without this
    /// check the arm would publish a plausible number for a question the model never saw.
    #[test]
    fn a_prompt_that_fills_the_window_fails_instead_of_answering_a_truncated_question() {
        let post = FakePost::ok(r#"{"response":"{\"step\": 3}","prompt_eval_count":40960}"#);
        let judge = Ollama::new(
            &post,
            "http://local",
            "qwen3:8b",
            Decoding::registered(2000),
        )
        .with_num_ctx(40960);

        let err = judge.ask("A VERY LONG PROMPT").expect_err("truncated");

        assert!(err.msg.contains("truncated prompt"), "got: {}", err.msg);
        assert!(!err.retryable, "a re-ask cannot make the prompt shorter");
    }

    #[test]
    fn a_retryable_failure_is_retried_up_to_the_attempt_limit() {
        let post = FakePost::scripted(vec![
            Err(retryable("429 rate limited")),
            Err(retryable("503")),
            Ok(br#"{"response":"ok"}"#.to_vec()),
        ]);
        let judge = Ollama::new(&post, "http://local", "qwen3:8b", Decoding::registered(64));

        let answer = ask_with_retries(&judge, "PROMPT", 3, Duration::ZERO).expect("third try");

        assert_eq!(answer, "ok");
        assert_eq!(post.call_count(), 3);
    }

    #[test]
    fn retries_stop_at_the_limit_and_surface_the_last_failure() {
        let post = FakePost::scripted(vec![
            Err(retryable("a")),
            Err(retryable("b")),
            Err(retryable("c")),
        ]);
        let judge = Ollama::new(&post, "http://local", "qwen3:8b", Decoding::registered(64));

        let err = ask_with_retries(&judge, "PROMPT", 3, Duration::ZERO).expect_err("all failed");

        assert_eq!(err.msg, "c");
        assert_eq!(post.call_count(), 3, "no fourth attempt");
    }

    #[test]
    fn a_non_retryable_failure_is_not_retried() {
        let post = FakePost::scripted(vec![Err(PostError {
            url: "u".to_string(),
            msg: "400 bad request".to_string(),
            retryable: false,
        })]);
        let judge = Ollama::new(&post, "http://local", "qwen3:8b", Decoding::registered(64));

        let err = ask_with_retries(&judge, "PROMPT", 3, Duration::ZERO).expect_err("400");

        assert_eq!(err.msg, "400 bad request");
        assert_eq!(
            post.call_count(),
            1,
            "repeating a bad request spends the quota"
        );
    }
}
