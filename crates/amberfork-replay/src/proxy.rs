//! The replay driver: answer a re-driven agent's requests from the tape, live-relay on a branch.
//!
//! [`Replay`] is the pure lookup; [`Upstream`] is the live seam. `ReplayProxy` is the stateful
//! thing that ties them together over a whole re-execution: for each request the re-driven agent
//! issues, it serves the recorded response on a hit and relays through the provider on a miss —
//! and, either way, records the turn onto a growing re-executed cassette.

use amberfork_record::{CapturedRequest, CapturedResponse, Cassette, Exchange};

use crate::{Replay, Upstream, UpstreamError};

/// Drives one re-execution of a recorded run, serving its requests from the tape and relaying
/// live once it branches off.
///
/// # Why it records every turn, not only the branched ones
///
/// The proxy appends an [`Exchange`] for **both** a recorded hit and a live miss, so the
/// re-executed cassette is a *complete* tape — the same shape [`amberfork_record::normalize`]
/// consumes. Counterfactual attribution (epic #35) normalizes this tape into a `Run` and aligns
/// it against the good run's `Run`; a tape that kept only the post-branch turns would normalize
/// into a stump starting at the branch point, and aligning that against a full trajectory is
/// meaningless. Each appended exchange records what *this* re-run actually did: the request the
/// agent issued this time, paired with the response it was served — recorded on a hit, live on a
/// miss.
///
/// # Why it is generic over `U`, not `dyn Upstream`
///
/// [`Upstream`] is a native async trait, so the relay implementation is chosen at compile time —
/// a scripted stub in tests, a live `reqwest` client in production — with no `dyn`, no vtable,
/// and no per-relay allocation. The bound lives on the impl block; the struct itself is unbounded.
#[derive(Debug)]
pub struct ReplayProxy<U> {
    replay: Replay,
    upstream: U,
    reexecuted: Cassette,
}

impl<U: Upstream> ReplayProxy<U> {
    /// A proxy that replays `cassette`, relays misses through `upstream`, and writes the
    /// re-executed run onto a fresh cassette stamped `reexecuted_id`.
    #[must_use]
    pub fn new(cassette: Cassette, upstream: U, reexecuted_id: impl Into<String>) -> Self {
        Self {
            replay: Replay::new(cassette),
            upstream,
            reexecuted: Cassette::new(reexecuted_id),
        }
    }

    /// Answer one request the re-driven agent issued, and record the turn.
    ///
    /// On a recorded hit the tape's response is served and the provider is never touched; on a
    /// miss the request is relayed through [`Upstream`] and the live exchange is appended. The
    /// request is taken by value because the appended exchange *is* the re-run's own record of
    /// what it sent — there is no reason to keep a second copy.
    ///
    /// # Errors
    ///
    /// Propagates [`UpstreamError`] from a live relay on a cache miss; a recorded hit never errors.
    pub async fn answer(
        &mut self,
        request: CapturedRequest,
    ) -> Result<CapturedResponse, UpstreamError> {
        let response = match self.replay.lookup(&request) {
            Some(recorded) => recorded.clone(),
            None => self.upstream.send(&request).await?,
        };
        self.reexecuted.exchanges.push(Exchange {
            idx: self.reexecuted.exchanges.len(),
            request,
            response: response.clone(),
        });
        Ok(response)
    }

    /// The tape accumulated so far — every turn this re-execution took, in order.
    #[must_use]
    pub fn reexecuted(&self) -> &Cassette {
        &self.reexecuted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScriptedUpstream;
    use amberfork_record::Body;
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

    fn cassette_with(
        exchanges: impl IntoIterator<Item = (CapturedRequest, CapturedResponse)>,
    ) -> Cassette {
        let mut cassette = Cassette::new("recorded");
        for (idx, (req, resp)) in exchanges.into_iter().enumerate() {
            cassette.exchanges.push(Exchange {
                idx,
                request: req,
                response: resp,
            });
        }
        cassette
    }

    #[tokio::test]
    async fn a_recorded_hit_serves_the_tape_and_records_the_turn() {
        let req = request(json!({"messages": [{"role": "user", "content": "hi"}]}));
        let resp = response(json!({"choices": [{"message": {"content": "hello"}}]}));
        // No scripted responses: if a hit wrongly relayed, `Upstream` would be exhausted and the
        // unwrap below would panic — so this also asserts a hit never touches the provider.
        let mut proxy = ReplayProxy::new(
            cassette_with([(req.clone(), resp.clone())]),
            ScriptedUpstream::new(Vec::<CapturedResponse>::new()),
            "reexec",
        );

        let served = proxy.answer(req.clone()).await.unwrap();

        assert_eq!(served, resp);
        assert_eq!(
            proxy.reexecuted().exchanges,
            vec![Exchange {
                idx: 0,
                request: req,
                response: resp,
            }]
        );
    }

    #[tokio::test]
    async fn a_branch_relays_upstream_and_appends_the_live_turn() {
        let recorded_req = request(json!({"messages": [{"role": "user", "content": "hi"}]}));
        let recorded_resp = response(json!({"choices": []}));
        let branched_req =
            request(json!({"messages": [{"role": "user", "content": "off the tape"}]}));
        let live_resp = response(json!({"choices": [{"message": {"content": "live answer"}}]}));

        let mut proxy = ReplayProxy::new(
            cassette_with([(recorded_req, recorded_resp)]),
            ScriptedUpstream::new([live_resp.clone()]),
            "reexec",
        );

        let served = proxy.answer(branched_req.clone()).await.unwrap();

        assert_eq!(served, live_resp);
        assert_eq!(
            proxy.reexecuted().exchanges,
            vec![Exchange {
                idx: 0,
                request: branched_req,
                response: live_resp,
            }]
        );
    }

    #[tokio::test]
    async fn the_reexecuted_tape_keeps_replayed_then_live_turns_in_order() {
        let a_req = request(json!({"messages": [{"role": "user", "content": "turn A"}]}));
        let a_resp = response(json!({"choices": [{"message": {"content": "A"}}]}));
        let b_req = request(json!({"messages": [{"role": "user", "content": "turn B off-tape"}]}));
        let b_resp = response(json!({"choices": [{"message": {"content": "B live"}}]}));

        let mut proxy = ReplayProxy::new(
            cassette_with([(a_req.clone(), a_resp.clone())]),
            ScriptedUpstream::new([b_resp.clone()]),
            "reexec",
        );

        proxy.answer(a_req.clone()).await.unwrap(); // hit → served from tape
        proxy.answer(b_req.clone()).await.unwrap(); // miss → relayed live

        assert_eq!(
            proxy.reexecuted().exchanges,
            vec![
                Exchange {
                    idx: 0,
                    request: a_req,
                    response: a_resp,
                },
                Exchange {
                    idx: 1,
                    request: b_req,
                    response: b_resp,
                },
            ]
        );
    }
}
