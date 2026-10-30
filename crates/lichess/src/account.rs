//! Accounts and users: whose a token is (`GET /api/account`), what a token may do (`POST /api/token/test`), turning
//! an account into a BOT, and the user shape Lichess uses for profiles, online bots and leaderboards alike.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Error, Lichess,
    client::Body,
};

/// A Lichess user as `/api/account`, `/api/user/{name}`, `/api/bot/online` and the leaderboards send it. Each sends
/// some of these fields; the rest are empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// The lower-case id, as games name players.
    pub id: String,
    #[serde(default)]
    pub username: String,
    /// `GM`, `FM`, …, or `BOT`.
    #[serde(default)]
    pub title: Option<String>,
    /// Ratings by perf key (`bullet`, `blitz`, `rapid`, `classical`, `correspondence`, `puzzle`, …).
    #[serde(default)]
    pub perfs: BTreeMap<String, Perf>,
    #[serde(default)]
    pub count: Option<Counts>,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub profile: Option<Profile>,
    /// When the account was made, ms since 1970.
    #[serde(default)]
    pub created_at: Option<i64>,
    /// When it was last seen, ms since 1970.
    #[serde(default)]
    pub seen_at: Option<i64>,
    /// A closed account.
    #[serde(default)]
    pub disabled: bool,
    /// Marked for breaking Lichess's terms.
    #[serde(default)]
    pub tos_violation: bool,
    #[serde(default)]
    pub url: Option<String>,
}

/// Your own account: the same shape as any user.
pub type Account = User;

impl User {
    pub fn is_bot(&self) -> bool {
        self.title.as_deref() == Some("BOT")
    }

    /// The rating at a perf, if the user has one.
    pub fn rating(&self, perf: &str) -> Option<u32> {
        self.perfs.get(perf).and_then(|p| p.rating)
    }

    /// The country flag, wherever Lichess put it.
    pub fn flag(&self) -> Option<&str> {
        self.flag.as_deref().or(self.profile.as_ref().and_then(|p| p.flag.as_deref()))
    }
}

/// A rating at one perf.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Perf {
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub games: Option<u32>,
    /// Rating deviation.
    #[serde(default)]
    pub rd: Option<u32>,
    /// Progress over the last games (profiles).
    #[serde(default)]
    pub prog: Option<i32>,
    /// Progress (leaderboards).
    #[serde(default)]
    pub progress: Option<i32>,
    /// Lichess's provisional flag: too few games for the rating to mean much.
    #[serde(default)]
    pub prov: bool,
}

/// A user's game counts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    #[serde(default)]
    pub all: u32,
    #[serde(default)]
    pub rated: u32,
    #[serde(default)]
    pub win: u32,
    #[serde(default)]
    pub draw: u32,
    #[serde(default)]
    pub loss: u32,
    #[serde(default)]
    pub playing: u32,
    #[serde(default)]
    pub import: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
}

/// What Lichess says of a token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenInfo {
    /// The scopes it was granted (`board:play`, `challenge:read`, …).
    pub scopes: Vec<String>,
    pub user_id: Option<String>,
    /// When it expires, ms since 1970 (a login lasts a year; personal tokens may never expire).
    pub expires: Option<i64>,
}

impl Lichess {
    /// Whose the token is.
    pub async fn account(&self) -> Result<Account, Error> {
        self.need_token()?;
        self.get("/api/account").await
    }

    /// The token's scopes and expiry (`POST /api/token/test`), or `None` when Lichess does not know the token.
    pub async fn test_token(&self) -> Result<Option<TokenInfo>, Error> {
        let token = self.token().ok_or(Error::NoToken)?.to_owned();
        let v: Value = self.json(Method::POST, "/api/token/test", Body::Text(token.clone())).await?;
        let info = &v[token.as_str()];
        if info.is_null() {
            return Ok(None);
        }
        let scopes = info["scopes"].as_str().unwrap_or_default();
        Ok(Some(TokenInfo {
            scopes: scopes.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect(),
            user_id: info["userId"].as_str().map(str::to_owned),
            expires: info["expires"].as_i64(),
        }))
    }

}

#[cfg(test)]
mod tests {

    use crate::testing::{MockAccount, MockLichess};

    #[tokio::test]
    async fn reads_the_account_and_the_token_s_scopes() {
        let mock = MockLichess::start().await;
        mock.add_account(
            "tok",
            MockAccount::user("Dame").rating("blitz", 1800).rating("rapid", 1850).scopes(&["board:play"]),
        );
        let client = mock.client(Some("tok"));
        let me = client.account().await.unwrap();
        assert_eq!((me.id.as_str(), me.username.as_str()), ("dame", "Dame"));
        assert!(!me.is_bot());
        assert_eq!(me.rating("rapid"), Some(1850));
        assert_eq!(me.rating("bullet"), None);
        let info = client.test_token().await.unwrap().unwrap();
        assert_eq!(info.scopes, vec!["board:play"]);
        assert_eq!(info.user_id.as_deref(), Some("dame"));
        assert!(info.expires.is_some());
        assert_eq!(mock.client(Some("unknown")).test_token().await.unwrap(), None);
    }

}
