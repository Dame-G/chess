//! HTTP to Lichess with one token (or none): JSON calls, form posts and NDJSON streams, one request at a time, and a
//! minute's pause after a 429. The token goes in the `Authorization` header and nowhere else, never into a URL, a log
//! line or an error.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use reqwest::{Method, header};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Config, Error, Ndjson};

/// A client of Lichess for one token, or anonymous. Cheap to clone; clones share the request slot and the pause.
#[derive(Clone)]
pub struct Lichess {
    shared: Arc<Shared>,
}

struct Shared {
    config: Config,
    http: reqwest::Client,
    token: Option<String>,
    agent: Mutex<String>,
    /// Held for each plain request and while a stream opens: Lichess asks for one request at a time per token.
    slot: tokio::sync::Mutex<()>,
    /// After a 429, no request before this.
    paused_until: Mutex<Option<Instant>>,
    cache: Cache,
}

/// What a request carries.
pub(crate) enum Body {
    None,
    Form(Vec<(&'static str, String)>),
    Text(String),
}

impl Lichess {
    /// A client for `token` (none, or an empty one, makes an anonymous client).
    pub fn new(config: Config, token: Option<&str>) -> Self {
        let token = token.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned);
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build().unwrap_or_default();
        Self {
            shared: Arc::new(Shared {
                config,
                http,
                token,
                agent: Mutex::new(format!("Dameless/{}", env!("CARGO_PKG_VERSION"))),
                slot: tokio::sync::Mutex::new(()),
                paused_until: Mutex::new(None),
                cache: Cache::default(),
            }),
        }
    }

    /// A client without a token, for public reads (profiles, leaderboards, another player's games, the tablebase).
    pub fn anonymous(config: Config) -> Self {
        Self::new(config, None)
    }

    pub fn config(&self) -> &Config {
        &self.shared.config
    }

    pub fn has_token(&self) -> bool {
        self.shared.token.is_some()
    }

    /// Puts the account's name in the User-Agent (`Dameless/0.1.0 user:<name>`), as Lichess asks of bots.
    pub fn set_username(&self, name: &str) {
        *self.shared.agent.lock().unwrap() = format!("Dameless/{} user:{name}", env!("CARGO_PKG_VERSION"));
    }

    /// How long until this token may make requests again after a 429, or `None` when it may now. Requests made
    /// meanwhile wait.
    pub fn rate_limited_for(&self) -> Option<Duration> {
        let until = (*self.shared.paused_until.lock().unwrap())?;
        until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())
    }

    pub(crate) fn cache(&self) -> &Cache {
        &self.shared.cache
    }

    pub(crate) fn token(&self) -> Option<&str> {
        self.shared.token.as_deref()
    }

    pub(crate) fn need_token(&self) -> Result<(), Error> {
        if self.has_token() { Ok(()) } else { Err(Error::NoToken) }
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.shared.http
    }

    fn build(&self, method: Method, path: &str, body: Body, ndjson: bool) -> reqwest::RequestBuilder {
        let agent = self.shared.agent.lock().unwrap().clone();
        let mut req = self
            .shared
            .http
            .request(method, format!("{}{path}", self.shared.config.base))
            .header(header::USER_AGENT, agent);
        if let Some(token) = &self.shared.token {
            req = req.bearer_auth(token);
        }
        if ndjson {
            req = req.header(header::ACCEPT, "application/x-ndjson");
        }
        match body {
            Body::None => req,
            Body::Form(form) => req.form(&form),
            Body::Text(text) => req.header(header::CONTENT_TYPE, "text/plain").body(text),
        }
    }

    /// Waits out a pause after a 429. The caller holds the slot, so every other request waits behind it.
    async fn wait_out_pause(&self) {
        let until = *self.shared.paused_until.lock().unwrap();
        if let Some(until) = until {
            tokio::time::sleep_until(until.into()).await;
        }
    }

    /// Sends a request (the caller holds the slot) and turns an error answer into an [`Error`].
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
        self.wait_out_pause().await;
        let res = req.send().await.map_err(Error::transport)?;
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }
        let text = res.text().await.unwrap_or_default();
        if status.as_u16() == 429 {
            let pause = self.shared.config.timing.rate_limit_pause;
            *self.shared.paused_until.lock().unwrap() = Some(Instant::now() + pause);
            tracing::warn!("lichess: rate-limited, pausing this token for {} s", pause.as_secs());
        }
        Err(Error::from_answer(status.as_u16(), &text))
    }

    /// A plain request, answered whole, in the slot.
    async fn text(&self, method: Method, path: &str, body: Body, ndjson: bool) -> Result<String, Error> {
        let _slot = self.shared.slot.lock().await;
        let timeout = self.shared.config.timing.request_timeout;
        let res = self.send(self.build(method, path, body, ndjson).timeout(timeout)).await?;
        res.text().await.map_err(Error::transport)
    }

    /// A request answered with JSON (`null` when the answer is empty).
    pub(crate) async fn json<T: DeserializeOwned>(&self, method: Method, path: &str, body: Body) -> Result<T, Error> {
        let text = self.text(method, path, body, false).await?;
        let text = if text.trim().is_empty() { "null" } else { text.as_str() };
        serde_json::from_str(text).map_err(|e| {
            let start: String = text.chars().take(120).collect();
            Error::Unexpected(format!("{e} in {start}"))
        })
    }

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.json(Method::GET, path, Body::None).await
    }

    /// A POST whose answer does not matter beyond its status (`{"ok":true}` mostly).
    pub(crate) async fn post_ok(&self, path: &str, form: Vec<(&'static str, String)>) -> Result<(), Error> {
        let body = if form.is_empty() { Body::None } else { Body::Form(form) };
        self.text(Method::POST, path, body, false).await.map(drop)
    }

    pub(crate) async fn delete_ok(&self, path: &str) -> Result<(), Error> {
        self.text(Method::DELETE, path, Body::None, false).await.map(drop)
    }

    /// Opens a stream of JSON lines. The slot is held until Lichess has answered, not for the stream's life.
    pub(crate) async fn stream<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Body,
    ) -> Result<Ndjson<T>, Error> {
        let res = {
            let _slot = self.shared.slot.lock().await;
            self.send(self.build(method, path, body, true)).await?
        };
        Ok(Ndjson::new(res, self.shared.config.timing.stream_idle))
    }
}

/// `path?k=v&…`, the values encoded; `path` alone when there are none.
pub(crate) fn with_query(path: &str, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return path.to_owned();
    }
    let mut url = reqwest::Url::parse("http://x/").expect("a valid URL");
    url.query_pairs_mut().extend_pairs(params);
    format!("{path}?{}", url.query().unwrap_or_default())
}

/// One path segment (a username, a game or challenge id, a move), encoded so it stays one segment.
pub(crate) fn seg(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Answers kept for a while by key (a URL), so a profile or an explorer position is not asked twice in a row.
#[derive(Default)]
pub(crate) struct Cache {
    entries: Mutex<HashMap<String, (Instant, Duration, Value)>>,
}

/// Answers kept at most; the cache starts over beyond.
const KEEP: usize = 512;

impl Cache {
    pub(crate) fn get(&self, key: &str) -> Option<Value> {
        let entries = self.entries.lock().unwrap();
        entries.get(key).filter(|(at, ttl, _)| at.elapsed() < *ttl).map(|(_, _, v)| v.clone())
    }

    pub(crate) fn put(&self, key: String, ttl: Duration, value: Value) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= KEEP {
            entries.retain(|_, (at, ttl, _)| at.elapsed() < *ttl);
            if entries.len() >= KEEP {
                entries.clear();
            }
        }
        entries.insert(key, (Instant::now(), ttl, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MockAccount, MockLichess};

    #[test]
    fn builds_queries_and_segments() {
        assert_eq!(with_query("/a", &[]), "/a");
        assert_eq!(with_query("/a", &[("x", "1 2".into()), ("y", "a&b".into())]), "/a?x=1+2&y=a%26b");
        assert_eq!(seg("e2e4"), "e2e4");
        assert_eq!(seg("a/b c"), "a%2Fb%20c");
    }

    #[test]
    fn the_cache_forgets_after_its_time() {
        let cache = Cache::default();
        cache.put("a".into(), Duration::from_secs(60), Value::from(1));
        cache.put("b".into(), Duration::ZERO, Value::from(2));
        assert_eq!(cache.get("a"), Some(Value::from(1)));
        assert_eq!(cache.get("b"), None);
    }

    #[tokio::test]
    async fn one_request_at_a_time_per_token() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        mock.delay("/api/account", Duration::from_millis(80));
        let client = mock.client(Some("tok"));
        let calls = (0..4).map(|_| {
            let c = client.clone();
            async move { c.account().await }
        });
        for r in futures::future::join_all(calls).await {
            assert_eq!(r.unwrap().username, "Dame");
        }
        assert_eq!(mock.max_concurrent("tok"), 1);
        assert_eq!(mock.requests_to("/api/account").len(), 4);
        // The token is in the header, never in the address.
        assert!(mock.requests().iter().all(|r| !r.path.contains("tok") && r.token.as_deref() == Some("tok")));
        assert!(mock.requests()[0].user_agent.starts_with("Dameless/"));
    }

    #[tokio::test]
    async fn a_429_pauses_the_token_before_its_next_request() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        let client = mock.client(Some("tok"));
        mock.rate_limit_next("/api/account");
        let e = client.account().await.unwrap_err();
        assert!(matches!(e, Error::RateLimited { .. }), "{e:?}");
        let pause = client.rate_limited_for().expect("paused");
        assert!(pause <= mock.config().timing.rate_limit_pause);
        let started = Instant::now();
        assert_eq!(client.account().await.unwrap().id, "dame");
        assert!(started.elapsed() >= pause.saturating_sub(Duration::from_millis(20)), "{:?}", started.elapsed());
        assert_eq!(client.rate_limited_for(), None);
    }

    #[tokio::test]
    async fn the_user_agent_names_the_account_once_known() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::bot("RecklessBot"));
        let client = mock.client(Some("tok"));
        client.set_username("RecklessBot");
        client.account().await.unwrap();
        assert!(mock.requests()[0].user_agent.ends_with(" user:RecklessBot"), "{}", mock.requests()[0].user_agent);
    }

    #[tokio::test]
    async fn errors_carry_lichess_s_message_and_never_the_token() {
        let mock = MockLichess::start().await;
        let client = mock.client(Some("lip_secret"));
        let e = client.account().await.unwrap_err();
        assert!(e.is_unauthorized(), "{e:?}");
        assert!(!format!("{e} {e:?}").contains("lip_secret"));
        let anonymous = mock.client(None);
        assert!(matches!(anonymous.account().await, Err(Error::NoToken)));
        let gone = Lichess::anonymous(Config::new("http://127.0.0.1:9"));
        assert!(matches!(gone.user("dame").await, Err(Error::Unreachable(_))));
    }
}
