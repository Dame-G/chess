//! Logging in to a Lichess account: OAuth2's authorization code flow with PKCE (S256), which Lichess offers without
//! registering an app (`client_id` is free-form). The app keeps the verifier and the state of each login under way for
//! ten minutes, single use; Lichess sends the browser back to a redirect URI the caller chooses, with a code that is
//! exchanged for a token (it lasts a year). Which pages a login may return to is the caller's rule.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use reqwest::Method;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{Error, Lichess, client::Body};

/// What Lichess shows as the app asking for access.
pub const CLIENT_ID: &str = "dameless";

/// The scopes asked for: playing with the Board API, and reading and sending challenges. Your games and the opening
/// explorer need a login but no scope.
pub const SCOPES: &[&str] = &["board:play", "challenge:read", "challenge:write"];

/// How long a login may take, from the link to the callback.
pub const LOGIN_FOR: Duration = Duration::from_secs(600);

/// A login under way, kept by its `state` until the callback takes it.
#[derive(Clone, Debug)]
pub struct PendingLogin {
    pub verifier: String,
    pub redirect_uri: String,
    /// Where the caller sends the browser once the login is done.
    pub return_to: String,
    created: Instant,
}

/// The link that starts a login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginLink {
    /// The Lichess page to open, where the person logs in and allows access.
    pub url: String,
    /// The single-use key of this login, also in `url`.
    pub state: String,
    pub expires_in: Duration,
}

/// A token Lichess granted.
#[derive(Clone, Deserialize)]
pub struct Grant {
    pub access_token: String,
    /// Seconds.
    #[serde(default)]
    pub expires_in: Option<u64>,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grant").field("access_token", &"…").field("expires_in", &self.expires_in).finish()
    }
}

/// The logins under way, in memory: a restart forgets them, and the person starts again.
pub struct Logins {
    pending: Mutex<HashMap<String, PendingLogin>>,
    ttl: Duration,
}

impl Default for Logins {
    fn default() -> Self {
        Self::new(LOGIN_FOR)
    }
}

impl Logins {
    pub fn new(ttl: Duration) -> Self {
        Self { pending: Mutex::new(HashMap::new()), ttl }
    }

    /// Starts a login at the Lichess at `lichess_base`: a fresh verifier and state, kept here, and the link to open.
    pub fn start(&self, lichess_base: &str, redirect_uri: &str, return_to: &str) -> Result<LoginLink, Error> {
        let verifier = random(32)?;
        let state = random(16)?;
        let url = reqwest::Url::parse_with_params(
            &format!("{}/oauth", lichess_base.trim_end_matches('/')),
            &[
                ("response_type", "code"),
                ("client_id", CLIENT_ID),
                ("redirect_uri", redirect_uri),
                ("code_challenge_method", "S256"),
                ("code_challenge", challenge_of(&verifier).as_str()),
                ("scope", SCOPES.join(" ").as_str()),
                ("state", state.as_str()),
            ],
        )
        .map_err(|e| Error::Unexpected(format!("the login address: {e}")))?;
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, l| l.created.elapsed() < self.ttl);
        let login = PendingLogin {
            verifier,
            redirect_uri: redirect_uri.to_owned(),
            return_to: return_to.to_owned(),
            created: Instant::now(),
        };
        pending.insert(state.clone(), login);
        Ok(LoginLink { url: url.to_string(), state, expires_in: self.ttl })
    }

    /// Takes the login of `state`, once: `None` when it is unknown, used or expired.
    pub fn take(&self, state: &str) -> Option<PendingLogin> {
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, l| l.created.elapsed() < self.ttl);
        pending.remove(state)
    }
}

impl Lichess {
    /// Exchanges the code Lichess sent back for a token (`POST /api/token`). Any client will do; an anonymous one
    /// is usual.
    pub async fn exchange_code(&self, login: &PendingLogin, code: &str) -> Result<Grant, Error> {
        let form = vec![
            ("grant_type", "authorization_code".to_owned()),
            ("code", code.to_owned()),
            ("code_verifier", login.verifier.clone()),
            ("redirect_uri", login.redirect_uri.clone()),
            ("client_id", CLIENT_ID.to_owned()),
        ];
        self.json(Method::POST, "/api/token", Body::Form(form)).await
    }

    /// Revokes this client's token (`DELETE /api/token`).
    pub async fn revoke_token(&self) -> Result<(), Error> {
        self.need_token()?;
        self.delete_ok("/api/token").await
    }
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

/// `BASE64URL(SHA256(verifier))`.
pub fn challenge_of(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

fn random(n: usize) -> Result<String, Error> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| Error::Unexpected(format!("random bytes: {e}")))?;
    Ok(base64url(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MockAccount, MockLichess};

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
    fn a_login_is_single_use_and_expires() {
        let logins = Logins::default();
        let link = logins.start("http://lichess.test/", "http://127.0.0.1:7979/cb", "/you").unwrap();
        let url = reqwest::Url::parse(&link.url).unwrap();
        assert_eq!(url.path(), "/oauth");
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["client_id"], "dameless");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["scope"], "board:play challenge:read challenge:write");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:7979/cb");
        assert_eq!(q["state"], link.state);
        let login = logins.take(&link.state).unwrap();
        assert_eq!(challenge_of(&login.verifier), q["code_challenge"]);
        assert_eq!(login.return_to, "/you");
        assert!(logins.take(&link.state).is_none());

        let short = Logins::new(Duration::ZERO);
        let link = short.start("http://lichess.test", "http://127.0.0.1/cb", "/").unwrap();
        assert!(short.take(&link.state).is_none());
    }

    #[tokio::test]
    async fn exchanges_the_code_and_revokes_the_token() {
        let mock = MockLichess::start().await;
        mock.add_account("lio_granted", MockAccount::user("Dame"));
        let logins = Logins::default();
        let link = logins.start(mock.url(), "http://127.0.0.1:7979/api/connections/you/callback", "/you").unwrap();
        // The person logs in and allows access; Lichess sends the browser back with a code.
        let back = mock.authorize(&link.url, "lio_granted").unwrap();
        assert_eq!(back.state, link.state);
        let login = logins.take(&back.state).unwrap();
        let anonymous = mock.client(None);
        // A wrong verifier is refused.
        let mut forged = login.clone();
        forged.verifier = "not-the-verifier".into();
        let e = anonymous.exchange_code(&forged, &back.code).await.unwrap_err();
        assert_eq!(e.status(), Some(400));
        let grant = anonymous.exchange_code(&login, &back.code).await.unwrap();
        assert_eq!(grant.access_token, "lio_granted");
        assert_eq!(grant.expires_in, Some(31_536_000));
        assert!(!format!("{grant:?}").contains("lio_granted"));
        // The code is single-use.
        assert!(anonymous.exchange_code(&login, &back.code).await.is_err());

        let me = mock.client(Some(&grant.access_token));
        assert_eq!(me.account().await.unwrap().username, "Dame");
        me.revoke_token().await.unwrap();
        assert!(me.account().await.unwrap_err().is_unauthorized());
    }

    #[tokio::test]
    async fn the_browser_flow_goes_through_the_mock_s_oauth_page() {
        let mock = MockLichess::start().await;
        mock.add_account("lio_web", MockAccount::user("Dame"));
        mock.grant_logins_as("lio_web");
        let logins = Logins::default();
        let link = logins.start(mock.url(), "http://127.0.0.1:7979/callback", "/you").unwrap();
        let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let res = http.get(&link.url).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 303);
        let location = reqwest::Url::parse(res.headers()["location"].to_str().unwrap()).unwrap();
        assert!(location.as_str().starts_with("http://127.0.0.1:7979/callback?"));
        let q: HashMap<String, String> = location.query_pairs().into_owned().collect();
        assert_eq!(q["state"], link.state);
        let login = logins.take(&q["state"]).unwrap();
        let grant = mock.client(None).exchange_code(&login, &q["code"]).await.unwrap();
        assert_eq!(grant.access_token, "lio_web");
    }
}
