//! A payload slot, plain or expandable (issue #30): the wire envelope's truncation marker
//! doubles as a click-to-expand affordance once a real [`SlotAddress`] exists to resolve it
//! against. Used by the content-diff pane's field values only — NOT the canvas's step
//! summaries, which stay the old inert mark (`canvas.rs::slot_view`). Verified live: the
//! canvas row's `.sum` cell clips overflow with CSS ellipsis for its one-line-gist layout,
//! which clips a real click target away with it (a pointer click on it timed out, unreachable
//! in a real browser) — the content-diff pane has no such clipping, so this component's home
//! is there.
//!
//! The fetch (the one impure edge here) is `csr`-gated exactly like the content-diff pane's
//! clipboard write: a plain function that no-ops under the `ssr` host build, so the SSR string
//! render — what every test in this crate pins — never depends on a browser or a live server.

#[cfg(feature = "csr")]
use amberfork_layout::PayloadResponse;
use amberfork_layout::{SlotAddress, SlotText};
use leptos::prelude::*;

const TITLE_COLLAPSED: &str = "payload truncated — click to load the full text";
const TITLE_LOADING: &str = "loading…";
const TITLE_FAILED: &str = "couldn't load — click to retry";

/// Mirrors `amberfork_server::PAYLOAD_ROUTE`. A one-line copy, not a dependency: the ui never
/// depends on the server crate (that would drag tokio/axum into a wasm build), the same reason
/// `main.rs` copies `DOCUMENT_ROUTE` rather than importing it.
#[cfg(feature = "csr")]
const PAYLOAD_ROUTE: &str = "/api/payload";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadState {
    Collapsed,
    Loading,
    Failed,
}

/// The button's title/disabled state for every non-terminal [`LoadState`]; `None` once the
/// text has actually loaded, which is the signal to stop rendering a button at all.
fn button_state(state: LoadState) -> (&'static str, bool) {
    match state {
        LoadState::Collapsed => (TITLE_COLLAPSED, false),
        LoadState::Loading => (TITLE_LOADING, true),
        LoadState::Failed => (TITLE_FAILED, false),
    }
}

/// One payload slot. Untruncated: plain reactive text, nothing else. Truncated with no
/// [`SlotAddress`] (malformed input, or a hand-built fixture that never ran through the real
/// envelope): the honest `…` mark stays, same as before this issue, but inert — there is
/// nothing to fetch. Truncated with an address: the same mark, now a real button that fetches
/// the full text on click and replaces itself with it — the fetched text is never shown
/// alongside a still-present button, since there is nothing left to expand once it has all
/// arrived.
#[component]
pub(crate) fn Slot(value: SlotText) -> impl IntoView {
    let text = RwSignal::new(value.text);
    if !value.truncated {
        return view! { <>{move || text.get()}</> }.into_any();
    }
    let Some(address) = value.address else {
        return view! {
            <>
                {move || text.get()}
                <span class="slot-trunc" title=TITLE_COLLAPSED>
                    "…"
                </span>
            </>
        }
        .into_any();
    };

    let state = RwSignal::new(LoadState::Collapsed);
    let loaded = RwSignal::new(false);

    view! {
        <>
            {move || text.get()}
            {move || {
                if loaded.get() {
                    return None;
                }
                let (title, disabled) = button_state(state.get());
                let address = address.clone();
                let on_click = move |_| {
                    if state.get_untracked() == LoadState::Loading {
                        return;
                    }
                    state.set(LoadState::Loading);
                    expand(
                        address.clone(),
                        move |result| match result {
                            Ok(full) => {
                                text.set(full);
                                loaded.set(true);
                            }
                            Err(()) => state.set(LoadState::Failed),
                        },
                    );
                };
                Some(
                    view! {
                        <button
                            type="button"
                            class="slot-trunc"
                            title=title
                            disabled=disabled
                            on:click=on_click
                        >
                            "…"
                        </button>
                    },
                )
            }}
        </>
    }
    .into_any()
}

/// Fetch one slot's full text and hand the result to `on_done`. Browser-only (a real network
/// call), so it is csr-gated; under `ssr` there is no click to trigger it in the first place.
#[cfg(feature = "csr")]
fn expand(address: SlotAddress, on_done: impl FnOnce(Result<String, ()>) + 'static) {
    leptos::task::spawn_local(async move {
        on_done(fetch_payload(&address).await);
    });
}

#[cfg(not(feature = "csr"))]
fn expand(_address: SlotAddress, _on_done: impl FnOnce(Result<String, ()>) + 'static) {}

/// `POST /api/payload` the address, get its full text back. Any failure — transport, a non-2xx
/// status, an unreadable body — collapses to `Err(())`: the button's retry affordance treats
/// them all the same, since none is actionable differently by a user who can only click again.
#[cfg(feature = "csr")]
async fn fetch_payload(address: &SlotAddress) -> Result<String, ()> {
    let response = gloo_net::http::Request::post(PAYLOAD_ROUTE)
        .json(address)
        .map_err(|_| ())?
        .send()
        .await
        .map_err(|_| ())?;
    if !response.ok() {
        return Err(());
    }
    let payload: PayloadResponse = response.json().await.map_err(|_| ())?;
    Ok(payload.text)
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use amberfork_layout::{Side, SlotKind};

    fn render(slot: SlotText) -> String {
        let owner = Owner::new();
        owner.with(|| view! { <Slot value=slot /> }.to_html())
    }

    #[test]
    fn an_untruncated_slot_renders_only_its_text_no_marker() {
        let html = render(SlotText::new("full text"));
        assert!(html.contains("full text"));
        assert!(!html.contains("slot-trunc"));
    }

    #[test]
    fn truncated_with_no_address_stays_the_old_inert_mark() {
        // A hand-built SlotText (as several other tests in this crate construct) that sets
        // `truncated` directly never gets an address — back-compat with those fixtures, and
        // the honest behavior when there is genuinely nothing to fetch.
        let mut slot = SlotText::new("cut ");
        slot.truncated = true;
        let html = render(slot);
        assert!(html.contains("cut "));
        assert!(html.contains("<span"));
        assert!(html.contains(r#"class="slot-trunc""#));
        assert!(
            !html.contains("<button"),
            "no address means nothing to click: {html}"
        );
    }

    #[test]
    fn truncated_with_an_address_renders_a_real_button() {
        let mut slot = SlotText::new("cut ");
        slot.truncated = true;
        slot.address = Some(SlotAddress {
            row: 0,
            kind: SlotKind::StepSummary { side: Side::A },
        });
        let html = render(slot);
        assert!(html.contains("cut "));
        assert!(
            html.contains("<button") && html.contains(r#"class="slot-trunc""#),
            "an addressed slot is a real, clickable affordance: {html}"
        );
        assert!(html.contains(TITLE_COLLAPSED));
        assert!(!html.contains("disabled"), "not yet clicked, not disabled");
    }
}
