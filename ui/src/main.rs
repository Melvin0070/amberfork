//! The browser entry (`csr`): the one impure edge. It fetches the document the engine bound
//! into the server (issue #25) and hands it to the pure [`App`] view. Everything here is
//! `csr`-only; under `ssr` — the host test build — `main` is an empty stub so the crate
//! still compiles for the string-render tests.

#[cfg(feature = "csr")]
use amberfork_layout::Document;
#[cfg(feature = "csr")]
use amberfork_ui::{App, DisconnectBanner};
#[cfg(feature = "csr")]
use gloo_net::http::Request;
#[cfg(feature = "csr")]
use leptos::prelude::*;
#[cfg(feature = "csr")]
use leptos::task::spawn_local;
#[cfg(feature = "csr")]
use std::time::Duration;

/// The one content endpoint (D12). Mirrors `amberfork_server::DOCUMENT_ROUTE`; it is a URL
/// path, not the schema, so a one-line copy is the right seam — the ui never depends on the
/// server crate (that would drag tokio/axum into a wasm build).
#[cfg(feature = "csr")]
const DOCUMENT_ROUTE: &str = "/api/document";

/// How often the browser re-polls the content endpoint to notice a stopped server. Gentle: the
/// view is fully usable from the loaded snapshot after boot, so the poll only annunciates a
/// terminal state — it need not be tight, and a quiet cadence keeps the loopback idle.
#[cfg(feature = "csr")]
const POLL_INTERVAL: Duration = Duration::from_secs(5);

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
/// resolved view. On success the resolved view is [`Live`] — the app plus its liveness watch.
#[cfg(feature = "csr")]
#[component]
fn Root() -> impl IntoView {
    let document = LocalResource::new(|| async { fetch_document().await });

    view! {
        <Suspense fallback=|| view! { <BootLoading /> }>
            {move || Suspend::new(async move {
                match document.await {
                    Ok(loaded) => view! { <Live loaded /> }.into_any(),
                    Err(message) => view! { <BootError message /> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// The first fetch's result: the document plus the snapshot's ETag, which the liveness probe
/// echoes back as `If-None-Match` so a healthy server answers with a cheap 304 (no re-download).
#[cfg(feature = "csr")]
#[derive(Clone)]
struct Loaded {
    document: Document,
    etag: Option<String>,
}

/// The connected view plus its liveness watch. Renders the pure [`App`] over the loaded
/// document and, once the server stops answering, mounts the [`DisconnectBanner`]. This poll is
/// the app's only ongoing I/O, and it *latches*: a dead loopback fetch means the server process
/// is gone, and since the server serves an immutable snapshot, recovery is restart + reload —
/// never a silent reconnect to a possibly-different diff. So on first failure we show the banner
/// and stop polling (no retry spinner).
#[cfg(feature = "csr")]
#[component]
fn Live(loaded: Loaded) -> impl IntoView {
    let disconnected = RwSignal::new(false);
    let etag = loaded.etag;
    // The restart command names the real runs (bad, then `--against` good) so it is paste-ready.
    let bad = loaded.document.view.run_b.id.clone();
    let good = loaded.document.view.run_a.id.clone();

    let handle = set_interval_with_handle(
        move || {
            let etag = etag.clone();
            spawn_local(async move {
                if !server_is_up(&etag).await {
                    disconnected.set(true);
                }
            });
        },
        POLL_INTERVAL,
    )
    .expect("registering a browser interval timer never fails");

    // Latch: the moment we know the server is gone, stop polling — the banner stays until the
    // user restarts and reloads. `on_cleanup` also clears it if this view is ever torn down.
    Effect::new(move |_| {
        if disconnected.get() {
            handle.clear();
        }
    });
    on_cleanup(move || handle.clear());

    view! {
        <App document=loaded.document />
        {move || {
            disconnected
                .get()
                .then(|| view! { <DisconnectBanner bad=bad.clone() good=good.clone() /> })
        }}
    }
}

/// Fetch the one document. The server serves an immutable snapshot with a strong ETag; a plain
/// GET is the whole protocol, and we keep the ETag for the liveness probe.
#[cfg(feature = "csr")]
async fn fetch_document() -> Result<Loaded, String> {
    let response = Request::get(DOCUMENT_ROUTE)
        .send()
        .await
        .map_err(|err| format!("can't reach the local server: {err}"))?;
    if !response.ok() {
        return Err(format!("the local server responded {}", response.status()));
    }
    let etag = response.headers().get("etag");
    let document = response
        .json::<Document>()
        .await
        .map_err(|err| format!("unreadable document: {err}"))?;
    Ok(Loaded { document, etag })
}

/// Liveness probe: a conditional GET the server answers with a cheap 304 when the snapshot is
/// unchanged — the ETag/304 path it built for exactly this. ANY HTTP response means the server
/// is up; only a transport error — the loopback process gone — reads as stopped.
#[cfg(feature = "csr")]
async fn server_is_up(etag: &Option<String>) -> bool {
    let mut request = Request::get(DOCUMENT_ROUTE);
    if let Some(tag) = etag {
        request = request.header("If-None-Match", tag);
    }
    request.send().await.is_ok()
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
