//! The browser entry (`csr`): the one impure edge. It fetches the document the engine bound
//! into the server (issue #25) and hands it to the pure [`App`] view. Everything here is
//! `csr`-only; under `ssr` — the host test build — `main` is an empty stub so the crate
//! still compiles for the string-render tests.

#[cfg(feature = "csr")]
use amberfork_layout::Document;
#[cfg(feature = "csr")]
use amberfork_ui::App;
#[cfg(feature = "csr")]
use leptos::prelude::*;

/// The one content endpoint (D12). Mirrors `amberfork_server::DOCUMENT_ROUTE`; it is a URL
/// path, not the schema, so a one-line copy is the right seam — the ui never depends on the
/// server crate (that would drag tokio/axum into a wasm build).
#[cfg(feature = "csr")]
const DOCUMENT_ROUTE: &str = "/api/document";

#[cfg(feature = "csr")]
fn main() {
    console_error_panic_hook::set_once();

    // The pre-wasm boot shell (index.html) has done its job now that wasm is alive; drop it
    // so the mounted app owns the page. If boot fails, `main` never runs and index.html's
    // error handler reveals the failure state instead — never a blank page (D20).
    if let Some(boot) = document().get_element_by_id("boot") {
        boot.remove();
    }

    mount_to_body(|| view! { <Root /> });
}

/// The fetch edge as a component: pull the document, then render the loading, error, or
/// resolved view. Disconnect re-polling and the designed banner arrive in a later slice
/// (issue #26); slice 0 covers the first fetch honestly.
#[cfg(feature = "csr")]
#[component]
fn Root() -> impl IntoView {
    let document = LocalResource::new(|| async { fetch_document().await });

    view! {
        <Suspense fallback=|| view! { <BootLoading /> }>
            {move || Suspend::new(async move {
                match document.await {
                    Ok(doc) => view! { <App document=doc /> }.into_any(),
                    Err(message) => view! { <BootError message /> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// Fetch the one document. The server serves an immutable snapshot with a strong ETag, so a
/// plain GET is the whole protocol.
#[cfg(feature = "csr")]
async fn fetch_document() -> Result<Document, String> {
    let response = gloo_net::http::Request::get(DOCUMENT_ROUTE)
        .send()
        .await
        .map_err(|err| format!("can't reach the local server: {err}"))?;
    if !response.ok() {
        return Err(format!("the local server responded {}", response.status()));
    }
    response
        .json::<Document>()
        .await
        .map_err(|err| format!("unreadable document: {err}"))
}

/// The in-app loading state — the same treatment as index.html's pre-wasm shell, so the two
/// loading moments read as one. Shown while the first fetch is in flight.
#[cfg(feature = "csr")]
#[component]
fn BootLoading() -> impl IntoView {
    view! {
        <div class="boot" role="status" aria-live="polite">
            <span class="boot-glyph" aria-hidden="true">"⑂"</span>
            <p>"Loading the fork…"</p>
        </div>
    }
}

/// The fetch-failure state. It says what broke and how to recover, in the interface's voice —
/// no apology, no dead spinner.
#[cfg(feature = "csr")]
#[component]
fn BootError(message: String) -> impl IntoView {
    view! {
        <div class="boot boot--error" role="alert">
            <span class="boot-glyph" aria-hidden="true">"⑂"</span>
            <p>"amberfork couldn't load the diff."</p>
            <p class="boot-hint">{message}</p>
        </div>
    }
}

#[cfg(not(feature = "csr"))]
fn main() {}
