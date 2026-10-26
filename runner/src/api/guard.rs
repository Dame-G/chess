//! Who may call the API.
//!
//! - Bound to loopback: the CLI and agents (no `Origin`) and the runner's own UI (its own origin) need nothing more,
//!   since they already run on this machine. The `Host` must be a loopback name, which stops DNS rebinding.
//! - The hosted UI (`https://chess.dame.gg`) needs the token, so no other website can drive the runner.
//! - A request a browser marks as cross-site (`Sec-Fetch-Site`) needs the token even without an `Origin`: a plain
//!   `<img>` or link on another website sends none, and must not pass on the strength of the loopback rule.
//! - Bound to anything else (a rented box behind a tunnel): every request needs the token.
//!
//! The token is compared in constant time.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::AppState;

pub async fn guard(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    match check(&state.config.token, state.config.is_loopback(), req.headers()) {
        Ok(()) => next.run(req).await,
        Err((status, error)) => (status, Json(serde_json::json!({ "error": error }))).into_response(),
    }
}

fn check(token: &str, loopback: bool, headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let token_ok = get(header::AUTHORIZATION)
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|given| same_secret(given, token));
    if !loopback {
        return if token_ok { Ok(()) } else { Err((StatusCode::UNAUTHORIZED, "token required")) };
    }

    let host = get(header::HOST).unwrap_or_default();
    if !is_loopback_host(host) {
        return Err((StatusCode::FORBIDDEN, "host not allowed"));
    }
    // Sent by every current browser: `same-origin` from the runner's own UI, `none` when the user typed the address.
    let cross_site =
        get(HeaderName::from_static("sec-fetch-site")).is_some_and(|site| !matches!(site, "same-origin" | "none"));
    match get(header::ORIGIN) {
        None if cross_site && !token_ok => Err((StatusCode::UNAUTHORIZED, "token required")),
        None => Ok(()),
        Some(origin) if origin.strip_prefix("http://") == Some(host) => Ok(()),
        Some(_) if token_ok => Ok(()),
        Some(_) => Err((StatusCode::UNAUTHORIZED, "token required")),
    }
}

/// Compares two secrets in time that depends only on their lengths, so response times do not reveal how much of a
/// guess was right.
fn same_secret(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// A loopback name, with or without a port: `127.0.0.1:7878`, `localhost`, `[::1]`.
fn is_loopback_host(host: &str) -> bool {
    // The last colon starts the port, unless it is inside an IPv6 address's brackets.
    let (name, port) = match host.rfind(':') {
        Some(i) if !host[i..].contains(']') => (&host[..i], Some(&host[i + 1..])),
        _ => (host, None),
    };
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    port_ok && matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(loopback: bool, headers: &[(&'static str, &str)]) -> Result<(), StatusCode> {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(*name, value.parse().unwrap());
        }
        check("secret", loopback, &map).map_err(|(status, _)| status)
    }

    #[test]
    fn who_may_call() {
        let host = ("host", "127.0.0.1:7878");
        let bearer = ("authorization", "Bearer secret");
        // The CLI, and the runner's own UI.
        assert_eq!(ask(true, &[host]), Ok(()));
        assert_eq!(ask(true, &[host, ("origin", "http://127.0.0.1:7878"), ("sec-fetch-site", "same-origin")]), Ok(()));
        assert_eq!(ask(true, &[host, ("sec-fetch-site", "none")]), Ok(()));
        // The hosted UI, with and without the token, and a wrong one.
        let hosted = ("origin", "https://chess.dame.gg");
        assert_eq!(ask(true, &[host, hosted, bearer]), Ok(()));
        assert_eq!(ask(true, &[host, hosted]), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ask(true, &[host, hosted, ("authorization", "Bearer secreT")]), Err(StatusCode::UNAUTHORIZED));
        // Another website's `<img>`: no Origin, but the browser says it is cross-site.
        assert_eq!(ask(true, &[host, ("sec-fetch-site", "cross-site")]), Err(StatusCode::UNAUTHORIZED));
        // DNS rebinding: the attacker's name in Host.
        assert_eq!(ask(true, &[("host", "evil.example:7878"), bearer]), Err(StatusCode::FORBIDDEN));
        // Not on loopback, only the token counts.
        assert_eq!(ask(false, &[host]), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ask(false, &[("host", "box.example"), bearer]), Ok(()));
    }

    #[test]
    fn compares_secrets() {
        assert!(same_secret("abc123", "abc123"));
        assert!(!same_secret("abc124", "abc123") && !same_secret("abc12", "abc123") && !same_secret("", "abc123"));
    }

    #[test]
    fn loopback_hosts() {
        assert!(is_loopback_host("127.0.0.1:7878"));
        assert!(is_loopback_host("localhost:7878"));
        assert!(is_loopback_host("[::1]:7878"));
        assert!(!is_loopback_host("evil.example:7878"));
        assert!(!is_loopback_host("127.0.0.1.evil.example"));
        assert!(is_loopback_host("[::1]") && is_loopback_host("localhost"));
        assert!(!is_loopback_host("localhost:") && !is_loopback_host("[::2]:7878"));
    }
}
