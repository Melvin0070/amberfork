//! The replay core: a cassette prepared to answer a re-issued request from the tape.

use amberfork_record::{CapturedRequest, CapturedResponse, Cassette};

/// A recorded run, ready to serve its responses back to a re-execution.
///
/// `Replay` is the pure, offline half of the replay path: given the cassette and a request the
/// re-driven agent just issued, it answers with the recorded response (a VCR hit) or reports
/// that the run has branched off the recording (a miss). Turning a miss into a live relay, and
/// binding the loopback listener the agent talks to, are the I/O-edge halves layered on top in
/// later slices — this type stays sync and pure.
#[derive(Debug)]
pub struct Replay {
    cassette: Cassette,
}

impl Replay {
    /// Prepare a cassette for replay.
    #[must_use]
    pub fn new(cassette: Cassette) -> Self {
        Self { cassette }
    }

    /// The recorded response for a re-issued `request`, or `None` when no recorded exchange
    /// matches — the point at which the re-execution has diverged from the tape.
    ///
    /// Matching is on `(method, path, body)` and nothing else: request headers carry the
    /// credential and other session-specific noise the cassette deliberately does not treat as
    /// load-bearing, so keying on them would turn every re-run into a miss. JSON bodies compare
    /// by value, so a semantically identical body with reordered object keys still matches.
    #[must_use]
    pub fn lookup(&self, request: &CapturedRequest) -> Option<&CapturedResponse> {
        self.cassette
            .exchanges
            .iter()
            .find(|exchange| request_matches(&exchange.request, request))
            .map(|exchange| &exchange.response)
    }
}

/// Whether a re-issued request addresses the same recorded exchange, keyed on the fields the
/// cassette contract declares load-bearing: method, path, and body.
fn request_matches(recorded: &CapturedRequest, incoming: &CapturedRequest) -> bool {
    recorded.method == incoming.method
        && recorded.path == incoming.path
        && recorded.body == incoming.body
}

#[cfg(test)]
mod tests {
    use super::*;
    use amberfork_record::{Body, Exchange};
    use serde_json::{Value, json};

    fn request(body: Value) -> CapturedRequest {
        CapturedRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: Vec::new(),
            body: Body::Json(body),
        }
    }

    fn response(body: Value) -> CapturedResponse {
        CapturedResponse {
            status: 200,
            headers: Vec::new(),
            body: Body::Json(body),
        }
    }

    fn one_exchange_cassette(req: CapturedRequest, resp: CapturedResponse) -> Cassette {
        let mut cassette = Cassette::new("test");
        cassette.exchanges.push(Exchange {
            idx: 0,
            request: req,
            response: resp,
        });
        cassette
    }

    #[test]
    fn recorded_request_returns_recorded_response() {
        let recorded_request = request(json!({"messages": [{"role": "user", "content": "hi"}]}));
        let recorded_response = response(json!({"choices": [{"message": {"content": "hello"}}]}));
        let replay = Replay::new(one_exchange_cassette(
            recorded_request.clone(),
            recorded_response.clone(),
        ));

        assert_eq!(replay.lookup(&recorded_request), Some(&recorded_response));
    }

    #[test]
    fn body_object_key_order_does_not_defeat_a_match() {
        // "Canonical JSON body": a semantically identical body with its object keys reordered is
        // the same request. The agent's SDK re-serializes on each turn and need not preserve key
        // order, so matching by value rather than by bytes is what keeps a hit a hit.
        let replay = Replay::new(one_exchange_cassette(
            request(json!({"model": "claude-sonnet-5", "stream": false})),
            response(json!({"ok": true})),
        ));

        let reordered = request(json!({"stream": false, "model": "claude-sonnet-5"}));
        assert!(replay.lookup(&reordered).is_some());
    }

    #[test]
    fn a_branched_request_is_a_miss() {
        let replay = Replay::new(one_exchange_cassette(
            request(json!({"messages": [{"role": "user", "content": "hi"}]})),
            response(json!({"choices": []})),
        ));

        let branched = request(json!({"messages": [{"role": "user", "content": "different"}]}));
        assert_eq!(replay.lookup(&branched), None);
    }
}
