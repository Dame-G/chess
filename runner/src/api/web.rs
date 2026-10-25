//! The UI, embedded in the binary: the same build that chess.dame.gg serves. Served from the runner's own origin, it
//! needs no token and no local-network permission.

use axum::{
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../web/dist"]
struct Assets;

pub async fn web(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = Assets::get(path).filter(|_| !path.is_empty()) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite fingerprints everything under assets/, so those files never change.
        let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        return ([(header::CONTENT_TYPE, mime.as_ref()), (header::CACHE_CONTROL, cache)], file.data).into_response();
    }
    // A path that looks like a file and is not one is a 404. Anything else is a route of the single-page app.
    if path.rsplit('/').next().is_some_and(|last| last.contains('.')) || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get("index.html") {
        Some(index) => ([(header::CACHE_CONTROL, "no-cache")], Html(index.data)).into_response(),
        None => {
            Html("<p>The UI is not built. Run <code>pnpm build</code> in the repository, then rebuild the runner.</p>")
                .into_response()
        }
    }
}
