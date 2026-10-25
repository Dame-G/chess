//! Who may call the API.
//!
//! - Bound to loopback: the CLI and agents (no `Origin`) and the runner's own UI (its own origin) need nothing more,
//!   since they already run on this machine. The `Host` must be a loopback name, which stops DNS rebinding.
//! - The hosted UI (`https://chess.dame.gg`) needs the token, so no other website can drive the runner.
//! - Bound to anything else (a rented box behind a tunnel): every request needs the token.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::AppState;

pub async fn guard(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    match check(&state, req.headers()) {
        Ok(()) => next.run(req).await,
        Err((status, error)) => (status, Json(serde_json::json!({ "error": error }))).into_response(),
    }
}

fn check(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let token_ok =
        get(header::AUTHORIZATION).and_then(|v| v.strip_prefix("Bearer ")) == Some(state.config.token.as_str());
    if !state.config.is_loopback() {
        return if token_ok { Ok(()) } else { Err((StatusCode::UNAUTHORIZED, "token required")) };
    }

    let host = get(header::HOST).unwrap_or_default();
    if !is_loopback_host(host) {
        return Err((StatusCode::FORBIDDEN, "host not allowed"));
    }
    match get(header::ORIGIN) {
        None => Ok(()),
        Some(origin) if origin.strip_prefix("http://") == Some(host) => Ok(()),
        Some(_) if token_ok => Ok(()),
        Some(_) => Err((StatusCode::UNAUTHORIZED, "token required")),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts() {
        assert!(is_loopback_host("127.0.0.1:7878"));
        assert!(is_loopback_host("localhost:7878"));
        assert!(is_loopback_host("[::1]:7878"));
        assert!(!is_loopback_host("evil.example:7878"));
        assert!(!is_loopback_host("127.0.0.1.evil.example"));
    }
}
