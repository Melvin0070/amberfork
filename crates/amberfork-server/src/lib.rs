//! The serving edge: a loopback-only HTTP server over the layout [`Document`].
//!
//! This crate is where `tokio` lives (design doc: engine crates stay sync + pure; the
//! runtime is quarantined to I/O edges). It exposes exactly ONE content endpoint (D12) and
//! binds 127.0.0.1 only, guarded against DNS rebinding (D6). The document is a snapshot:
//! serialized once at bind time, then served as immutable bytes — re-polls are answered
//! with a strong `ETag`/304 pair, which is all the UI's disconnect detection needs.

use amberfork_layout::Document;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::{EmbeddedFile, RustEmbed};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpListener;

/// The one content endpoint (D12): the versioned view-model document, whole.
pub const DOCUMENT_ROUTE: &str = "/api/document";

/// The app shell every SPA route lands on; its presence is what makes a bundle a bundle.
const INDEX_HTML: &str = "index.html";

/// The web UI bundle (D7: one embed site, owned by this crate — D13). Empty in a dev
/// checkout — the contents are .gitignored, built by the release workflow (#26/#28), and
/// carried into the crates.io package by this crate's `include` list.
#[derive(RustEmbed)]
#[folder = "ui-dist"]
struct UiAssets;

/// The document snapshot on the wire: serialized once, hashed once, shared by every request.
struct Content {
    body: Bytes,
    etag: HeaderValue,
}

/// A bound-but-not-yet-serving server: bind and serve are split so the caller can print
/// the real URL (OS-assigned port included) before the accept loop starts.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    router: Router,
    local_addr: SocketAddr,
}

/// Everything that can go wrong at the serving edge, typed so the CLI can phrase each case.
#[derive(Debug)]
pub enum ServeError {
    /// Binding the loopback listener failed — in practice: the port is already taken.
    Bind { addr: SocketAddr, source: io::Error },
    /// The accept loop died after a successful bind.
    Serve(io::Error),
    /// The embedded web bundle has no `index.html` — a dev build without built UI assets.
    BundleMissing,
}

impl fmt::Display for ServeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind { addr, source } => write!(f, "cannot bind {addr}: {source}"),
            Self::Serve(source) => write!(f, "server stopped: {source}"),
            Self::BundleMissing => write!(
                f,
                "web UI bundle missing — this build has no index.html embedded; build the \
                 web UI into crates/amberfork-server/ui-dist/ first (release artifacts ship \
                 with it built in)"
            ),
        }
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind { source, .. } | Self::Serve(source) => Some(source),
            Self::BundleMissing => None,
        }
    }
}

impl Server {
    /// Bind `127.0.0.1:port` (`0` = OS-assigned) around a snapshot of `document`.
    ///
    /// Loopback is not a default, it is the only path: no widen flag exists in this crate
    /// (D6 — traces carry prompts, tool args, and whatever secrets leaked into them, so
    /// "local, no account" must be literally true).
    ///
    /// # Errors
    /// [`ServeError::Bind`] when the loopback bind fails (port in use).
    pub async fn bind(document: &Document, port: u16) -> Result<Self, ServeError> {
        Self::bind_with_assets::<UiAssets>(document, port).await
    }

    /// [`bind`](Self::bind), generic over the embedded bundle — the seam that lets tests
    /// (and any embedder) supply their own assets.
    ///
    /// # Errors
    /// [`ServeError::Bind`] when the loopback bind fails; [`ServeError::BundleMissing`]
    /// (checked BEFORE binding — a startup failure must never leave a port claimed) when
    /// the bundle has no `index.html`.
    pub async fn bind_with_assets<A: RustEmbed + 'static>(
        document: &Document,
        port: u16,
    ) -> Result<Self, ServeError> {
        if A::get(INDEX_HTML).is_none() {
            return Err(ServeError::BundleMissing);
        }
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|source| ServeError::Bind { addr, source })?;
        let local_addr = listener
            .local_addr()
            .map_err(|source| ServeError::Bind { addr, source })?;
        Ok(Self {
            listener,
            router: router::<A>(document),
            local_addr,
        })
    }

    /// The address actually bound — the one true source for the URL the CLI prints.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Run the accept loop until the future is dropped.
    ///
    /// # Errors
    /// [`ServeError::Serve`] if the accept loop fails after a successful bind.
    pub async fn serve(self) -> Result<(), ServeError> {
        axum::serve(self.listener, self.router)
            .await
            .map_err(ServeError::Serve)
    }
}

fn router<A: RustEmbed + 'static>(document: &Document) -> Router {
    let json = serde_json::to_string(document)
        .expect("Document serialization is infallible (no non-string map keys)");
    let etag = HeaderValue::from_str(&format!("\"{:x}\"", Sha256::digest(json.as_bytes())))
        .expect("a quoted hex digest is valid header ASCII");
    let content = Arc::new(Content {
        body: Bytes::from(json),
        etag,
    });
    // The guard is a `.layer` on the whole router so it also wraps the fallback — every
    // route, the SPA fallback included, is born behind it.
    Router::new()
        .route(DOCUMENT_ROUTE, get(serve_document))
        .fallback(get(serve_ui::<A>))
        .layer(middleware::from_fn(require_local_host))
        .with_state(content)
}

/// The SPA handler over the embedded bundle: an exact asset is served as itself; anything
/// else is a client-side route and lands on `index.html` (`#step-N` anchors never reach the
/// server at all). A custom handler because `ServeDir` reads the filesystem, not an embed.
async fn serve_ui<A: RustEmbed + 'static>(uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") {
        // A typo'd endpoint must fail loud — the fallback handing fetch() an HTML page to
        // parse is the silent version of that bug.
        return (
            StatusCode::NOT_FOUND,
            "amberfork: no such endpoint (the content endpoint is /api/document)\n",
        )
            .into_response();
    }
    let asset_path = path.trim_start_matches('/');
    match A::get(asset_path) {
        Some(asset) => asset_response(asset_path, asset),
        None => match A::get(INDEX_HTML) {
            Some(index) => asset_response(INDEX_HTML, index),
            // Unreachable via bind (checked up front), but the library path never panics —
            // and in a debug build the folder is re-read from disk on every request, so the
            // bundle genuinely can vanish under a running server.
            None => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "amberfork: UI bundle lost index.html\n",
            )
                .into_response(),
        },
    }
}

fn asset_response(path: &str, asset: EmbeddedFile) -> Response {
    // Cow discriminates release (borrowed from the binary) from debug (read off disk);
    // `from_static` keeps the release path zero-copy.
    let body = match asset.data {
        Cow::Borrowed(bytes) => Bytes::from_static(bytes),
        Cow::Owned(bytes) => Bytes::from(bytes),
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mime = HeaderValue::from_str(mime.as_ref())
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    ([(header::CONTENT_TYPE, mime)], body).into_response()
}

async fn serve_document(State(content): State<Arc<Content>>, headers: HeaderMap) -> Response {
    // `no-cache` means "revalidate, don't guess": the browser re-polls with If-None-Match
    // instead of heuristically caching a stale document.
    let revalidate = [
        (header::ETAG, content.etag.clone()),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
    ];
    if headers.get(header::IF_NONE_MATCH) == Some(&content.etag) {
        return (StatusCode::NOT_MODIFIED, revalidate).into_response();
    }
    (
        revalidate,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        content.body.clone(),
    )
        .into_response()
}

/// DNS-rebinding defense (D6, the vite/Jupyter CVE class): a hostile page can point its own
/// domain at 127.0.0.1 and read traces cross-origin — the browser's only tell is the `Host`
/// header, so anything that isn't a literal localhost form is refused before routing.
async fn require_local_host(request: Request, next: Next) -> Response {
    let allowed = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(host_is_local);
    if !allowed {
        return (
            StatusCode::FORBIDDEN,
            "amberfork: forbidden — non-local Host header\n",
        )
            .into_response();
    }
    next.run(request).await
}

/// Whether an HTTP `Host` authority names this machine's loopback: `localhost`, `127.0.0.1`,
/// or `[::1]`, case-insensitively, with or without a port. Exact names only — suffix tricks
/// like `localhost.evil.example` must not pass.
fn host_is_local(host: &str) -> bool {
    let name = if host.starts_with('[') {
        // Bracketed IPv6 authority: the name is the bracketed span, `:port` may follow.
        let Some(end) = host.find(']') else {
            return false;
        };
        match &host[end + 1..] {
            "" => &host[..=end],
            port if port.starts_with(':') => &host[..=end],
            _ => return false,
        }
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let name = name.to_ascii_lowercase();
    name == "localhost" || name == "127.0.0.1" || name == "[::1]"
}

#[cfg(test)]
mod tests {
    use super::host_is_local;

    #[test]
    fn localhost_forms_pass() {
        for host in [
            "localhost",
            "LOCALHOST",
            "localhost:7777",
            "127.0.0.1",
            "127.0.0.1:7777",
            "[::1]",
            "[::1]:7777",
        ] {
            assert!(host_is_local(host), "{host:?} names loopback");
        }
    }

    #[test]
    fn everything_else_is_refused() {
        for host in [
            "",
            "evil.example",
            "evil.example:7777",
            "localhost.evil.example",
            "127.0.0.1.evil.example",
            "[::2]",
            "[::1",
            "[::1]evil",
            "0.0.0.0",
            "192.168.1.10:7777",
        ] {
            assert!(!host_is_local(host), "{host:?} must not pass the guard");
        }
    }
}
