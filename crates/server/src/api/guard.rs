//! Who may call the app.
//!
//! - **Host.** Bound to a loopback address, every request (the UI's pages too) must name a loopback host: a page on
//!   another website whose name was made to point at 127.0.0.1 (DNS rebinding) sends its own name, and is refused.
//! - **Origin.** A request a browser sends from a page carries its origin, which must be the app's own (the UI it
//!   serves) or the hosted UI (`https://chess.dame.gg`). Any other website is refused, token or not.
//! - **Token.** Every API request carries the bearer token, except the health check and the Lichess login's callback
//!   (a browser arriving from lichess.org has none; the login's single-use `state` is its proof). The UI the app serves
//!   gets the token in its page (see `web`); the CLI reads it from the data directory; the hosted UI is given it once,
//!   by hand. An `EventSource` cannot send headers, so the event stream also takes it as `?token=`.
//! - Bound to anything else (a box behind a tunnel): only the token counts.
//!
//! The token is compared in constant time.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dameless_core::Error;

use super::error::ApiError;
use crate::App;

/// The `Host` check, around everything the app serves.
pub async fn host(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    if app.hub.config.is_loopback() {
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default();
        if !is_loopback_host(host) {
            return ApiError(Error::Forbidden(format!("host {host:?} is not this machine"))).into_response();
        }
    }
    next.run(req).await
}

/// The token and `Origin` checks, around the API.
pub async fn guard(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let query_token = req.uri().query().and_then(token_in_query);
    match check(&app.hub.token, &app.hub.config.hosted_origin, app.hub.config.is_loopback(), req.headers(), query_token)
    {
        Ok(()) => next.run(req).await,
        Err(e) => ApiError(e).into_response(),
    }
}

/// The decision, apart from axum.
pub fn check(
    token: &str,
    hosted: &str,
    loopback: bool,
    headers: &HeaderMap,
    query_token: Option<String>,
) -> Result<(), Error> {
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let bearer = get(header::AUTHORIZATION).and_then(|v| v.strip_prefix("Bearer ")).map(str::to_owned);
    let token_ok = bearer.or(query_token).is_some_and(|given| same_secret(given.trim(), token));
    if loopback && let Some(origin) = get(header::ORIGIN) {
        let own = get(header::HOST).is_some_and(|host| origin.strip_prefix("http://") == Some(host));
        if !own && origin != hosted {
            return Err(Error::Forbidden(format!("requests from {origin} are not answered")));
        }
    }
    if token_ok { Ok(()) } else { Err(Error::Unauthorized("the API token is missing or wrong".into())) }
}

/// `token=<t>` in a query string.
fn token_in_query(query: &str) -> Option<String> {
    query.split('&').find_map(|pair| pair.strip_prefix("token=")).map(|t| t.replace("%20", " "))
}

/// Compares two secrets in time that depends only on their lengths, so response times do not reveal how much of a
/// guess was right.
fn same_secret(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// A loopback name, with or without a port: `127.0.0.1:7979`, `localhost`, `[::1]`.
pub fn is_loopback_host(host: &str) -> bool {
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

    const HOSTED: &str = "https://chess.dame.gg";

    fn ask(loopback: bool, headers: &[(&'static str, &str)], query: Option<&str>) -> Result<(), u16> {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(*name, value.parse().unwrap());
        }
        check("secret", HOSTED, loopback, &map, query.map(str::to_owned)).map_err(|e| e.status())
    }

    #[test]
    fn who_may_call() {
        let host = ("host", "127.0.0.1:7979");
        let bearer = ("authorization", "Bearer secret");
        // The CLI, with the token and without.
        assert_eq!(ask(true, &[host, bearer], None), Ok(()));
        assert_eq!(ask(true, &[host], None), Err(401));
        // The app's own UI, with the token from its page.
        assert_eq!(ask(true, &[host, ("origin", "http://127.0.0.1:7979"), bearer], None), Ok(()));
        // The hosted UI, with and without the token, and a wrong one.
        let hosted = ("origin", HOSTED);
        assert_eq!(ask(true, &[host, hosted, bearer], None), Ok(()));
        assert_eq!(ask(true, &[host, hosted], None), Err(401));
        assert_eq!(ask(true, &[host, hosted, ("authorization", "Bearer secreT")], None), Err(401));
        // Another website, even with a token it somehow has.
        assert_eq!(ask(true, &[host, ("origin", "https://evil.example"), bearer], None), Err(403));
        assert_eq!(ask(true, &[host, ("origin", "http://localhost:7979"), bearer], None), Err(403));
        // An event stream's token in the query.
        assert_eq!(ask(true, &[host], Some("secret")), Ok(()));
        assert_eq!(ask(true, &[host], Some("wrong")), Err(401));
        // Not on loopback, only the token counts.
        assert_eq!(
            ask(false, &[("host", "box.example"), ("origin", "https://elsewhere.example"), bearer], None),
            Ok(())
        );
        assert_eq!(ask(false, &[host], None), Err(401));
    }

    #[test]
    fn compares_secrets() {
        assert!(same_secret("abc123", "abc123"));
        assert!(!same_secret("abc124", "abc123") && !same_secret("abc12", "abc123") && !same_secret("", "abc123"));
    }

    #[test]
    fn loopback_hosts() {
        assert!(is_loopback_host("127.0.0.1:7979"));
        assert!(is_loopback_host("localhost:7979"));
        assert!(is_loopback_host("[::1]:7979"));
        assert!(!is_loopback_host("evil.example:7979"));
        assert!(!is_loopback_host("127.0.0.1.evil.example"));
        assert!(is_loopback_host("[::1]") && is_loopback_host("localhost"));
        assert!(!is_loopback_host("localhost:") && !is_loopback_host("[::2]:7979"));
    }

    #[test]
    fn reads_a_token_from_the_query() {
        assert_eq!(token_in_query("since=4&token=abc"), Some("abc".into()));
        assert_eq!(token_in_query("since=4"), None);
    }
}
