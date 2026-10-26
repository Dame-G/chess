//! The two Lichess connections: **you** (your account, played through the Board API) and **bot** (Dameless's BOT
//! account). One client per token, shared by everything that talks to Lichess for that account (your games, imports,
//! the explorer), and an anonymous one for public reads. Tokens are files (`hub.secrets`), never rows; what else a
//! connection is (its account, settings, state) is its row of `connections` and the `connection.upserted` event.
//!
//! Logging in (yours only) is OAuth with PKCE: [`Connections::start_login`] gives the Lichess link, and the callback
//! hands the code to [`Connections::finish_login`], which stores the token. The explorer and the tablebase are asked
//! here, behind the fair-play policy.

use std::sync::Arc;

use dameless_core::{
    ConnectionKey, Error, Timestamp,
    events::AssistKind,
    lichess::{Account, ConnectionStatus},
};
use dameless_hub::Hub;
use dameless_lichess::{
    Lichess, User,
    explorer::ExplorerQuery,
    oauth::{LOGIN_FOR, Logins},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    bot::Bot,
    clients::{Clients, account_of},
    error,
    games::Games,
    you::{ChallengeBody, ChallengeSent, You},
};

/// The answer to `POST /api/connections/you/login`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoginStarted {
    /// The Lichess page to open.
    pub url: String,
    pub expires_in_seconds: u64,
}

/// What a login came to, for the callback to send the browser back with.
#[derive(Clone, Debug)]
pub struct LoginFinished {
    /// Where the browser goes back to.
    pub return_to: String,
    pub result: Result<ConnectionStatus, Error>,
}

impl LoginFinished {
    /// `return_to` with `?lichess=connected`, or `?lichess=error&message=…`.
    pub fn redirect(&self) -> String {
        let Ok(mut url) = reqwest::Url::parse(&self.return_to) else { return self.return_to.clone() };
        {
            let mut q = url.query_pairs_mut();
            match &self.result {
                Ok(_) => q.append_pair("lichess", "connected"),
                Err(e) => q.append_pair("lichess", "error").append_pair("message", &e.message()),
            };
        }
        url.to_string()
    }
}

struct Inner {
    hub: Arc<Hub>,
    clients: Arc<Clients>,
    you: You,
    bot: Bot,
    logins: Logins,
}

/// The Lichess connections. Cheap to clone.
#[derive(Clone)]
pub struct Connections {
    inner: Arc<Inner>,
}

impl Connections {
    /// The connections of the Lichess `config` points at (a mock in tests), playing their games through `games`.
    pub fn new(hub: Arc<Hub>, games: Games, config: dameless_lichess::Config) -> Self {
        let clients = Arc::new(Clients::new(config));
        let you = You::new(hub.clone(), clients.clone(), games.clone());
        let bot = Bot::new(hub.clone(), clients.clone(), games);
        Self { inner: Arc::new(Inner { hub, clients, you, bot, logins: Logins::new(LOGIN_FOR) }) }
    }

    /// Picks up the stored tokens: your account is followed again, and the bot comes back online if it was.
    pub async fn start(&self) -> Result<(), Error> {
        for key in [ConnectionKey::You, ConnectionKey::Bot] {
            let token = self
                .inner
                .hub
                .secrets
                .lichess_token(key)
                .map_err(|e| Error::internal(format!("read the {key} token: {e}")))?;
            if let Some(client) = self.inner.clients.set(key, token.as_deref())
                && let Some(account) = self.status(key).account
            {
                client.set_username(&account.username);
            }
        }
        self.inner.you.resume().await;
        self.inner.bot.resume().await;
        Ok(())
    }

    pub fn you(&self) -> &You {
        &self.inner.you
    }

    pub fn bot(&self) -> &Bot {
        &self.inner.bot
    }

    /// The account's client, while a token is stored.
    pub fn client(&self, key: ConnectionKey) -> Option<Lichess> {
        self.inner.clients.get(key)
    }

    /// A client without a token, for public reads.
    pub fn anonymous(&self) -> Lichess {
        self.inner.clients.anonymous()
    }

    /// Your client when you are logged in, else the anonymous one; never the bot's.
    pub fn yours_or_anonymous(&self) -> Lichess {
        self.client(ConnectionKey::You).unwrap_or_else(|| self.anonymous())
    }

    /// The Lichess games are on, for links.
    pub fn lichess_url(&self) -> &str {
        self.inner.clients.url()
    }

    pub fn status(&self, key: ConnectionKey) -> ConnectionStatus {
        match key {
            ConnectionKey::You => self.inner.you.status(),
            ConnectionKey::Bot => self.inner.bot.status(),
        }
    }

    /// Checks a token with Lichess and stores it as the account's. Yours must be a person's account (not a BOT, not
    /// the bot's) allowed to play (`board:play`); the bot's must be a BOT account (and not yours).
    pub async fn set_token(&self, key: ConnectionKey, token: &str) -> Result<ConnectionStatus, Error> {
        let token = token.trim();
        if token.is_empty() {
            return Err(Error::bad_request("the token is empty"));
        }
        let probe = Lichess::new(self.inner.clients.config.clone(), Some(token));
        let account = match probe.account().await {
            Ok(u) => account_of(&u),
            Err(e) if e.is_unauthorized() => {
                return Err(Error::bad_request(match key {
                    ConnectionKey::You => "Lichess rejected that token",
                    ConnectionKey::Bot => {
                        "Lichess rejected that token: make one with the bot:play scope on lichess.org"
                    }
                }));
            }
            Err(e) => return Err(error::lichess(e)),
        };
        let (scopes, expires_at) = match probe.test_token().await {
            Ok(Some(info)) => (info.scopes, info.expires.map(Timestamp::from_unix_ms)),
            _ => (Vec::new(), None),
        };
        match key {
            ConnectionKey::You => {
                if account.is_bot() {
                    return Err(Error::bad_request(format!(
                        "{} is a BOT account: connect it as the bot, not as yours",
                        account.username
                    )));
                }
                if self.inner.bot.account().is_some_and(|b| b.id == account.id) {
                    return Err(Error::bad_request("that is the bot's account: your own account must be another one"));
                }
                if !scopes.is_empty() && !scopes.iter().any(|s| s == "board:play") {
                    return Err(Error::bad_request(
                        "that token cannot play (no board:play scope): log in again, or make a token with board:play, \
                         challenge:read and challenge:write",
                    ));
                }
            }
            ConnectionKey::Bot => {
                if !account.is_bot() {
                    return Err(Error::bad_request(format!(
                        "{} is not a BOT account: upgrade it on Lichess first (it must not have played)",
                        account.username
                    )));
                }
                if self.inner.you.account().is_some_and(|y| y.id == account.id) {
                    return Err(Error::bad_request("that is your own account: the bot's must be another one"));
                }
            }
        }
        self.store(key, token, account, scopes, expires_at).await
    }

    async fn store(
        &self,
        key: ConnectionKey,
        token: &str,
        account: Account,
        scopes: Vec<String>,
        expires_at: Option<Timestamp>,
    ) -> Result<ConnectionStatus, Error> {
        self.inner
            .hub
            .secrets
            .set_lichess_token(key, token)
            .map_err(|e| Error::internal(format!("store the {key} token: {e}")))?;
        if let Some(client) = self.inner.clients.set(key, Some(token)) {
            client.set_username(&account.username);
        }
        match key {
            ConnectionKey::You => self.inner.you.connected(account, scopes, expires_at).await,
            ConnectionKey::Bot => self.inner.bot.connected(account).await,
        }
        Ok(self.status(key))
    }

    /// Forgets the account's token (revoking yours at Lichess, as far as Lichess can be reached) and stops following
    /// it. Games in progress go on on Lichess.
    pub async fn logout(&self, key: ConnectionKey) -> Result<ConnectionStatus, Error> {
        if key == ConnectionKey::You
            && let Some(client) = self.client(key)
            && let Err(e) = client.revoke_token().await
        {
            tracing::info!("lichess (you): could not revoke the token: {e}");
        }
        self.inner.clients.set(key, None);
        self.inner
            .hub
            .secrets
            .delete_lichess_token(key)
            .map_err(|e| Error::internal(format!("forget the {key} token: {e}")))?;
        match key {
            ConnectionKey::You => self.inner.you.disconnected().await,
            ConnectionKey::Bot => self.inner.bot.disconnected().await,
        }
        Ok(self.status(key))
    }

    /// Starts a login to your account: the Lichess link to open. Lichess sends the browser to `redirect_uri` (the
    /// app's callback) with a code; the login then sends it on to `return_to`: a page of the app on a loopback
    /// address or of the hosted UI (default the app's `/you`).
    pub fn start_login(&self, return_to: Option<&str>, redirect_uri: &str) -> Result<LoginStarted, Error> {
        let return_to = match return_to.map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) if allowed_return(r, &self.inner.hub.config.hosted_origin) => r.to_owned(),
            Some(_) => {
                return Err(Error::bad_request(format!(
                    "return_to must be a page of this app or of {}",
                    self.inner.hub.config.hosted_origin
                )));
            }
            None => {
                let base =
                    reqwest::Url::parse(redirect_uri).map_err(|_| Error::bad_request("a bad redirect address"))?;
                format!("{}/you", base.origin().ascii_serialization())
            }
        };
        let link = self.inner.logins.start(self.lichess_url(), redirect_uri, &return_to).map_err(error::lichess)?;
        Ok(LoginStarted { url: link.url, expires_in_seconds: link.expires_in.as_secs() })
    }

    /// Finishes a login: Lichess sent the browser back with `code` (or `error`) for the login `state`. The token is
    /// exchanged, checked and stored. 400 when the login is unknown, used or expired (nowhere to send the browser).
    pub async fn finish_login(
        &self,
        code: Option<&str>,
        state: &str,
        error: Option<&str>,
    ) -> Result<LoginFinished, Error> {
        let Some(login) = self.inner.logins.take(state) else {
            return Err(Error::bad_request("that login is unknown, used or expired: log in again"));
        };
        let return_to = login.return_to.clone();
        let result = match (code, error) {
            (_, Some(why)) => Err(Error::bad_request(format!("Lichess did not grant access: {why}"))),
            (None, None) => Err(Error::bad_request("Lichess sent no code")),
            (Some(code), None) => match self.anonymous().exchange_code(&login, code).await {
                Ok(grant) => self.set_token(ConnectionKey::You, &grant.access_token).await,
                Err(e) => Err(error::lichess(e)),
            },
        };
        Ok(LoginFinished { return_to, result })
    }

    /// Lichess's opening explorer, asked with your token (it answers 401 without a login). Refused while you play on
    /// Lichess, unless every game of yours is correspondence.
    pub async fn explorer(&self, query: &ExplorerQuery) -> Result<serde_json::Value, Error> {
        self.inner.hub.policy.check(AssistKind::Explorer)?;
        let client =
            self.client(ConnectionKey::You).ok_or_else(|| Error::conflict("the explorer needs your Lichess login"))?;
        client.explorer(query).await.map_err(error::lichess)
    }

    /// The tablebase, for at most seven pieces. Refused while you play on Lichess.
    pub async fn tablebase(&self, fen: &str) -> Result<serde_json::Value, Error> {
        self.inner.hub.policy.check(AssistKind::Tablebase)?;
        self.anonymous().tablebase(fen).await.map_err(error::lichess)
    }

    /// A player's public profile (with your token when you are logged in). 404 when Lichess has no such player.
    pub async fn user(&self, username: &str) -> Result<User, Error> {
        if !dameless_core::ids::valid_lichess_name(username.trim()) {
            return Err(Error::bad_request(format!("{username} is not a Lichess username")));
        }
        self.yours_or_anonymous().user(username.trim()).await.map_err(error::lichess)
    }

    /// Lichess's leaderboard at a perf (`bullet`, `blitz`, `rapid`, `classical`, `ultraBullet`).
    pub async fn leaderboard(&self, perf: &str, nb: u32) -> Result<Vec<User>, Error> {
        if !dameless_lichess::players::LEADERBOARD_PERFS.contains(&perf) {
            return Err(Error::bad_request(format!(
                "perf is one of {}",
                dameless_lichess::players::LEADERBOARD_PERFS.join(", ")
            )));
        }
        self.yours_or_anonymous().leaderboard(nb, perf).await.map_err(error::lichess)
    }

    /// A challenge from a connection: yours (a player, Stockfish or your bot) or the bot's (a player).
    pub async fn challenge(&self, key: ConnectionKey, body: ChallengeBody) -> Result<ChallengeSent, Error> {
        match key {
            ConnectionKey::You => {
                let bot = self.inner.bot.account().map(|a| a.username);
                self.inner.you.challenge(body, bot).await
            }
            ConnectionKey::Bot => self.inner.bot.challenge(body).await,
        }
    }
}

/// Whether `url` is a page a login may return to: the app on a loopback address (http), or the hosted UI.
pub(crate) fn allowed_return(url: &str, hosted: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    if let Ok(h) = reqwest::Url::parse(hosted)
        && h.scheme() == u.scheme()
        && h.host_str() == u.host_str()
        && h.port_or_known_default() == u.port_or_known_default()
    {
        return true;
    }
    let loopback = match u.host_str() {
        Some("localhost") => true,
        Some(h) => h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()),
        None => false,
    };
    u.scheme() == "http" && loopback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_only_to_the_app_or_the_hosted_ui() {
        let hosted = "https://chess.dame.gg";
        assert!(allowed_return("http://127.0.0.1:7979/you", hosted));
        assert!(allowed_return("http://localhost:5173/you?x=1", hosted));
        assert!(allowed_return("http://[::1]:7979/you", hosted));
        assert!(allowed_return("https://chess.dame.gg/you", hosted));
        assert!(!allowed_return("https://chess.dame.gg.evil.example/", hosted));
        assert!(!allowed_return("http://chess.dame.gg/you", hosted));
        assert!(!allowed_return("https://chess.dame.gg:8443/you", hosted));
        assert!(!allowed_return("https://evil.example/", hosted));
        assert!(!allowed_return("javascript:alert(1)", hosted));
    }
}
