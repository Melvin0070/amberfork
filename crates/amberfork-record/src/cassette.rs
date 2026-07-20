//! The cassette: the record path's output contract.
//!
//! A cassette is the full-content capture of one agent run at the provider HTTP boundary —
//! every request the agent sent and every response it got back, in order. It is what makes
//! the record path's two claims true: content is *guaranteed* (rather than opt-in and often
//! absent, as on the passive path), and the run can be re-executed later, because a
//! boundary-level recording is replayable in a way a telemetry photo is not.
//!
//! The wire shape is documented in `docs/cassette-format.md`; these types are its source of
//! truth.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the cassette contract.
///
/// Deliberately NOT [`amberfork_model::SchemaVersion`]: the trace format and the cassette are
/// two independent contracts with two independent audiences, and sharing a version number
/// would make a change to either one falsely announce a break in the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CassetteVersion(pub String);

impl CassetteVersion {
    /// The version this build writes and reads natively.
    pub const CURRENT: &'static str = "0.1";

    /// The current contract version.
    #[must_use]
    pub fn current() -> Self {
        Self(Self::CURRENT.to_string())
    }

    /// Whether this cassette declares the version this build reads natively.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.0 == Self::CURRENT
    }
}

impl Default for CassetteVersion {
    fn default() -> Self {
        Self::current()
    }
}

/// Request headers preserved in a cassette.
///
/// This is an **allowlist, and must stay one**. A cassette is a shareable artifact — committed
/// as a fixture, attached to a bug report, pasted into an issue — and every provider spells
/// its credential differently (`authorization`, `x-api-key`, `x-goog-api-key`,
/// `anthropic-auth-token`, …). A denylist silently leaks the next provider's scheme the day it
/// ships; an allowlist drops an unrecognized header instead. Replay keys on method, path, and
/// body, so nothing here is load-bearing for correctness — which makes fail-closed free.
const KEPT_REQUEST_HEADERS: &[&str] = &["content-type", "accept"];

/// Response headers preserved in a cassette. Same allowlist reasoning: a response can carry
/// `set-cookie`, and organization/project identifiers are not ours to redistribute.
const KEPT_RESPONSE_HEADERS: &[&str] = &["content-type"];

/// What the recorder writes when a body is not valid JSON. The cassette stays a JSON document,
/// so a non-JSON body (an HTML error page from a proxy, a truncated stream) is preserved as
/// text rather than dropped or allowed to fail the capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Body {
    /// A parsed JSON body — the normal case for a provider API.
    Json(Value),
    /// A body that did not parse as JSON, preserved verbatim.
    Text(String),
}

impl Body {
    /// Parse captured bytes into a body, falling back to lossy text.
    ///
    /// Capture never fails on a malformed body: a recording that dies because the provider
    /// returned an HTML 502 is a recording you cannot debug, which is exactly when you needed
    /// it. The fidelity loss is reported by shape (`Text` rather than `Json`), not hidden.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).map_or_else(
            |_| Self::Text(String::from_utf8_lossy(bytes).into_owned()),
            Self::Json,
        )
    }
}

/// One captured request, with full inputs.
///
/// Full **input** capture is the point (design doc, "Replay fidelity ceiling"): output-only
/// logs leave ≥21% of cases unattributable, and capturing inputs buys +76% relative
/// step-level accuracy. It is also what a counterfactual re-run needs — you cannot re-ask a
/// question you did not record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturedRequest {
    pub method: String,
    /// Path and query as the agent sent them, upstream origin excluded — the origin belongs
    /// to the recording session, not the exchange.
    pub path: String,
    /// Headers surviving [`KEPT_REQUEST_HEADERS`].
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

/// One captured response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturedResponse {
    pub status: u16,
    /// Headers surviving [`KEPT_RESPONSE_HEADERS`].
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

/// One request/response round trip at the provider boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    /// 0-based capture order. Concurrent agent calls are ordered by *completion*, which is the
    /// only order the proxy can observe; it is a record of what happened, not a causal claim.
    pub idx: usize,
    pub request: CapturedRequest,
    pub response: CapturedResponse,
}

/// A full-content recording of one agent run at the provider boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cassette {
    pub cassette_version: CassetteVersion,
    /// Unique recording id (any string).
    pub id: String,
    /// The exchanges, in capture order.
    #[serde(default)]
    pub exchanges: Vec<Exchange>,
}

impl Cassette {
    /// An empty cassette stamped with the current contract version.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            cassette_version: CassetteVersion::current(),
            id: id.into(),
            exchanges: Vec::new(),
        }
    }
}

/// Filter headers down to an allowlist, lowercasing names so the contract does not inherit the
/// caller's casing.
pub(crate) fn retain_headers<'a>(
    headers: impl Iterator<Item = (&'a str, &'a [u8])>,
    allowed: &[&str],
) -> Vec<(String, String)> {
    headers
        .filter_map(|(name, value)| {
            let name = name.to_ascii_lowercase();
            if !allowed.contains(&name.as_str()) {
                return None;
            }
            // A kept header with a non-UTF-8 value is dropped rather than lossily mangled:
            // the allowlist is small and textual, so this is a malformed-input case.
            let value = std::str::from_utf8(value).ok()?;
            Some((name, value.to_string()))
        })
        .collect()
}

/// Filter request headers to the recordable allowlist.
///
/// Public because the record path is not the only writer of a cassette: `amberfork-replay`'s
/// loopback listener re-records the agent's requests at the *same* provider boundary, and its
/// re-executed cassette is just as shareable as a recorded one. It reuses this function so there
/// stays exactly one allowlist — the module doc's whole point is that a second, forked one would
/// silently leak the next provider's credential scheme.
pub fn retain_request_headers<'a>(
    headers: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Vec<(String, String)> {
    retain_headers(headers, KEPT_REQUEST_HEADERS)
}

/// Filter response headers to the recordable allowlist.
///
/// Public for the same reason as [`retain_request_headers`]: `amberfork-replay`'s live relay builds
/// a [`CapturedResponse`] from the provider's answer, and that response lands on the re-executed
/// cassette — just as shareable as a recorded one. It reuses this function so there stays exactly
/// one response allowlist, never a second that could leak a `set-cookie` or an org identifier.
pub fn retain_response_headers<'a>(
    headers: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Vec<(String, String)> {
    retain_headers(headers, KEPT_RESPONSE_HEADERS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_headers_never_survive_capture() {
        // The load-bearing test of this module. Every one of these is a real provider's
        // credential scheme; the allowlist must drop all of them without naming any.
        let headers: Vec<(&str, &[u8])> = vec![
            ("content-type", b"application/json"),
            ("authorization", b"Bearer sk-secret"),
            ("x-api-key", b"sk-secret"),
            ("x-goog-api-key", b"sk-secret"),
            ("anthropic-auth-token", b"sk-secret"),
            ("cookie", b"session=sk-secret"),
        ];
        let kept = retain_request_headers(headers.into_iter());
        assert_eq!(
            kept,
            vec![("content-type".to_string(), "application/json".to_string())]
        );
    }

    #[test]
    fn unknown_headers_are_dropped_not_kept() {
        // Fail-closed: the allowlist's whole purpose is that a header nobody anticipated —
        // including the next provider's auth scheme — does not reach the cassette.
        let headers: Vec<(&str, &[u8])> = vec![("x-some-future-credential", b"sk-secret")];
        assert!(retain_request_headers(headers.into_iter()).is_empty());
    }

    #[test]
    fn header_names_are_lowercased() {
        let headers: Vec<(&str, &[u8])> = vec![("Content-Type", b"application/json")];
        let kept = retain_request_headers(headers.into_iter());
        assert_eq!(kept[0].0, "content-type");
    }

    #[test]
    fn non_json_body_is_preserved_as_text() {
        // A provider 502 is an HTML page. Capture must survive it — that is precisely the run
        // you wanted recorded.
        let body = Body::from_bytes(b"<html>502 Bad Gateway</html>");
        assert_eq!(body, Body::Text("<html>502 Bad Gateway</html>".to_string()));
    }

    #[test]
    fn json_body_round_trips() {
        let body = Body::from_bytes(br#"{"model":"claude-sonnet-5"}"#);
        let Body::Json(value) = &body else {
            panic!("expected a JSON body, got {body:?}");
        };
        assert_eq!(value["model"], "claude-sonnet-5");
    }

    #[test]
    fn cassette_version_serializes_transparently() {
        let json = serde_json::to_string(&CassetteVersion::current()).unwrap();
        assert_eq!(json, "\"0.1\"");
    }
}
