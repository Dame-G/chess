//! The web UI (`apps/web/dist`): read from disk in debug builds (rust-embed reads the folder at run time, so a fresh
//! `pnpm build` shows without rebuilding the app) and embedded in release builds. Paths that are not files are routes
//! of the single-page app and get `index.html`.
//!
//! The page carries the API token in `<meta name="dameless-token">`, so the UI the app serves can call it. Only this
//! machine can read the page: the `Host` check refuses other names (DNS rebinding), and the same-origin policy keeps
//! other websites from reading the answer.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;

use crate::App;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../apps/web/dist"]
#[allow_missing = true]
struct Assets;

pub async fn web(State(app): State<Arc<App>>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = Assets::get(path).filter(|_| !path.is_empty() && path != "index.html") {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite fingerprints everything under assets/, so those files never change.
        let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        return ([(header::CONTENT_TYPE, mime.as_ref()), (header::CACHE_CONTROL, cache)], file.data).into_response();
    }
    // A path that looks like a file and is not one is a 404. Anything else is a route of the single-page app.
    if path.rsplit('/').next().is_some_and(|last| last.contains('.') && last != "index.html")
        || path.starts_with("api/")
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get("index.html") {
        Some(index) => {
            let page = with_token(&String::from_utf8_lossy(&index.data), &app.hub.token);
            ([(header::CACHE_CONTROL, "no-store")], Html(page)).into_response()
        }
        None => Html(
            "<!doctype html><title>Dameless</title><p>The UI is not built. Run <code>corepack pnpm build</code> in the \
             repository (and rebuild the app for a release build).</p>",
        )
        .into_response(),
    }
}

/// The page with the token in a `<meta>` at the top of its head.
pub fn with_token(page: &str, token: &str) -> String {
    let meta = format!("<meta name=\"dameless-token\" content=\"{token}\">");
    match page.find("<head>") {
        Some(i) => format!("{}{meta}{}", &page[..i + 6], &page[i + 6..]),
        None => format!("{meta}{page}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn puts_the_token_in_the_head() {
        assert_eq!(
            super::with_token("<html><head><title>x</title></head></html>", "t0k"),
            "<html><head><meta name=\"dameless-token\" content=\"t0k\"><title>x</title></head></html>"
        );
    }
}
