//! Logging in to your Lichess account: OAuth2's authorization code flow with PKCE, which Lichess offers without
//! registering an app (`client_id` is free-form). The runner keeps the verifier and the state, and Lichess sends the
//! browser back to the runner's own `/api/lichess/me/callback`, which exchanges the code for a token (it lasts a year)
//! and sends the browser on to the page that started the login.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use super::{SCOPES, set_token};
use crate::{
    api::{ApiError, AppState},
    config::HOSTED_ORIGIN,
};

/// What Lichess shows as the app asking for access.
pub const CLIENT_ID: &str = "dameless";

/// How long a login may take, from the link to the callback.
const LOGIN_FOR: Duration = Duration::from_secs(600);

/// A login under way, by its `state`.
pub struct Login {
    verifier: String,
    redirect_uri: String,
    return_to: String,
    created: Instant,
}

#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Where to send the browser once Lichess has answered: a page of this runner (any loopback address) or of
    /// https://chess.dame.gg. Default the runner's `/lichess` page. It gets `?lichess=connected`, or
    /// `?lichess=error&message=...`.
    #[serde(default)]
    pub return_to: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct LoginStarted {
    /// The Lichess page to open, where you log in and allow access.
    pub url: String,
    /// How long the link works.
    pub expires_in_seconds: u64,
}

/// Base64url without padding, as PKCE wants.
pub fn base64url(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..=chunk.len() {
            out.push(ABC[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn random(n: usize) -> Result<String, ApiError> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("random bytes: {e}"))?;
    Ok(base64url(&bytes))
}

/// `BASE64URL(SHA256(verifier))`.
pub fn challenge_of(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

/// Whether `url` is a page a login may return to: this runner on a loopback address, or the hosted UI.
fn allowed_return(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    let hosted = reqwest::Url::parse(HOSTED_ORIGIN).ok();
    if hosted.is_some_and(|h| h.scheme() == u.scheme() && h.host_str() == u.host_str() && u.port().is_none()) {
        return true;
    }
    let loopback = match u.host_str() {
        Some("localhost") => true,
        Some(h) => h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()),
        None => false,
    };
    u.scheme() == "http" && loopback
}

/// The runner's address as the browser reaches it: the request's `Host` when it is a loopback name, else the
/// listening address (with 127.0.0.1 for an unspecified one).
fn runner_host(state: &AppState, host: Option<&str>) -> String {
    if let Some(h) = host {
        let name = h.rsplit_once(':').map_or(h, |(n, _)| n);
        if matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
            return h.to_owned();
        }
    }
    let listen = state.config.listen;
    if listen.ip().is_unspecified() { format!("127.0.0.1:{}", listen.port()) } else { listen.to_string() }
}

/// Starts a login: the link to open, with a fresh verifier and state kept here for ten minutes.
pub fn start(state: &AppState, host: Option<&str>, req: LoginRequest) -> Result<LoginStarted, ApiError> {
    let host = runner_host(state, host);
    let redirect_uri = format!("http://{host}/api/lichess/me/callback");
    let return_to = match req.return_to.map(|r| r.trim().to_owned()).filter(|r| !r.is_empty()) {
        Some(r) if allowed_return(&r) => r,
        Some(_) => {
            return Err(ApiError::bad_request(&format!(
                "return_to must be a page of this runner or of {HOSTED_ORIGIN}"
            )));
        }
        None => format!("http://{host}/lichess"),
    };
    let verifier = random(32)?;
    let key = random(16)?;
    let url = reqwest::Url::parse_with_params(
        &format!("{}/oauth", state.me.base),
        &[
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge_of(&verifier).as_str()),
            ("scope", SCOPES.join(" ").as_str()),
            ("state", key.as_str()),
        ],
    )
    .map_err(|e| anyhow::anyhow!("the Lichess URL: {e}"))?;
    let mut inner = state.me.inner.lock().unwrap();
    inner.logins.retain(|_, l| l.created.elapsed() < LOGIN_FOR);
    inner.logins.insert(key, Login { verifier, redirect_uri, return_to, created: Instant::now() });
    Ok(LoginStarted { url: url.to_string(), expires_in_seconds: LOGIN_FOR.as_secs() })
}

#[derive(Debug, Default, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `return_to` with the outcome added to its query.
fn back(return_to: &str, outcome: Result<(), String>) -> Response {
    let mut url = match reqwest::Url::parse(return_to) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "bad return address").into_response(),
    };
    {
        let mut q = url.query_pairs_mut();
        match &outcome {
            Ok(()) => q.append_pair("lichess", "connected"),
            Err(message) => q.append_pair("lichess", "error").append_pair("message", message),
        };
    }
    Redirect::to(url.as_str()).into_response()
}

/// Swaps the code for a token at Lichess.
async fn exchange(state: &AppState, login: &Login, code: &str) -> Result<String, String> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", login.verifier.as_str()),
        ("redirect_uri", login.redirect_uri.as_str()),
        ("client_id", CLIENT_ID),
    ];
    let res = state
        .me
        .http
        .post(format!("{}/api/token", state.me.base))
        .form(&form)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("could not reach Lichess: {}", e.without_url()))?;
    let status = res.status();
    let body: serde_json::Value = res.json().await.unwrap_or_default();
    if !status.is_success() {
        let why = body["error_description"].as_str().or(body["error"].as_str()).unwrap_or("no reason given");
        return Err(format!("Lichess refused the login ({status}): {why}"));
    }
    body["access_token"].as_str().map(str::to_owned).ok_or_else(|| "Lichess sent no token".to_owned())
}

/// Where Lichess sends the browser after the login. Not behind the API's guard (a browser arriving from lichess.org
/// is cross-site and carries no token); the single-use `state` is what proves the login is ours.
pub async fn callback(State(state): State<Arc<AppState>>, Query(q): Query<CallbackQuery>) -> Response {
    let login = q.state.as_deref().and_then(|key| {
        let mut inner = state.me.inner.lock().unwrap();
        inner.logins.retain(|_, l| l.created.elapsed() < LOGIN_FOR);
        inner.logins.remove(key)
    });
    let Some(login) = login else {
        let page = "This Lichess login has expired or was already used. Start again from the app.";
        return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], page).into_response();
    };
    if let Some(error) = q.error {
        let why = q.error_description.unwrap_or(error);
        return back(&login.return_to, Err(format!("Lichess did not grant access: {why}")));
    }
    let Some(code) = q.code else {
        return back(&login.return_to, Err("Lichess sent no code".into()));
    };
    let outcome = match exchange(&state, &login, &code).await {
        Ok(token) => set_token(&state, &token).await.map(drop).map_err(|e| e.message),
        Err(e) => Err(e),
    };
    back(&login.return_to, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_and_the_pkce_challenge() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        // RFC 7636, appendix B.
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn returns_only_to_the_runner_or_the_hosted_ui() {
        assert!(allowed_return("http://127.0.0.1:7878/lichess"));
        assert!(allowed_return("http://localhost:5173/lichess?x=1"));
        assert!(allowed_return("https://chess.dame.gg/lichess"));
        assert!(!allowed_return("https://chess.dame.gg.evil.example/"));
        assert!(!allowed_return("http://chess.dame.gg/lichess"));
        assert!(!allowed_return("https://evil.example/"));
        assert!(!allowed_return("javascript:alert(1)"));
    }
}
