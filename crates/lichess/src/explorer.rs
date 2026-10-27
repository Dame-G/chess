//! The opening explorer (explorer.lichess.org: Lichess games, masters' games or one player's) and the tablebase
//! (tablebase.lichess.org), answered as Lichess sends them and kept a while in memory. The explorer needs a login
//! (it answers 401 without one); the tablebase needs none. Neither goes through the token's request slot: they are
//! other servers.

use std::time::Duration;

use serde_json::Value;

use crate::{
    Error, Lichess,
    ndjson::{parse_line, take_line},
};

/// How long an answer is kept: explorer statistics move slowly, tablebase answers never change.
const EXPLORER_FOR: Duration = Duration::from_secs(600);
const TABLEBASE_FOR: Duration = Duration::from_secs(24 * 3600);

/// Which games the explorer counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExplorerDb {
    /// Games played on Lichess, filtered by speed and rating.
    #[default]
    Lichess,
    /// Over-the-board games between masters.
    Masters,
    /// One player's games (`player` and `color` needed).
    Player,
}

impl ExplorerDb {
    fn key(self) -> &'static str {
        match self {
            Self::Lichess => "lichess",
            Self::Masters => "masters",
            Self::Player => "player",
        }
    }
}

/// A position to look up, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExplorerQuery {
    pub db: ExplorerDb,
    /// The position as FEN. Default the start position.
    pub fen: Option<String>,
    /// Moves played from `fen`, in UCI notation.
    pub play: Vec<String>,
    /// `lichess` and `player`: speeds (`blitz`, `rapid`, …).
    pub speeds: Vec<String>,
    /// `lichess`: rating groups (`1600`, `1800`, …).
    pub ratings: Vec<u32>,
    /// `player`: `rated` and/or `casual`.
    pub modes: Vec<String>,
    /// From this month (`2020-01`), or year for `masters`.
    pub since: Option<String>,
    /// Until this month (`2024-12`), or year for `masters`.
    pub until: Option<String>,
    /// How many moves to list.
    pub moves: Option<u32>,
    pub top_games: Option<u32>,
    pub recent_games: Option<u32>,
    /// `player`: whose games.
    pub player: Option<String>,
    /// `player`: `white` or `black`.
    pub color: Option<String>,
}

impl ExplorerQuery {
    fn params(&self) -> Vec<(&'static str, String)> {
        let join = |v: &[String]| v.join(",");
        let mut params = vec![("variant", "standard".to_owned())];
        let mut add = |key: &'static str, v: Option<String>| {
            if let Some(v) = v.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
                params.push((key, v));
            }
        };
        add("fen", self.fen.clone());
        add("play", Some(join(&self.play)));
        add("since", self.since.clone());
        add("until", self.until.clone());
        match self.db {
            ExplorerDb::Lichess => {
                add("speeds", Some(join(&self.speeds)));
                add("ratings", Some(self.ratings.iter().map(u32::to_string).collect::<Vec<_>>().join(",")));
            }
            ExplorerDb::Masters => {}
            ExplorerDb::Player => {
                add("player", self.player.clone());
                add("color", self.color.clone());
                add("speeds", Some(join(&self.speeds)));
                add("modes", Some(join(&self.modes)));
            }
        }
        add("moves", self.moves.map(|n| n.to_string()));
        add("topGames", self.top_games.map(|n| n.to_string()));
        add("recentGames", self.recent_games.map(|n| n.to_string()));
        params
    }
}

impl Lichess {
    /// Looks a position up in the opening explorer. Needs the token (`Error::NoToken` without one).
    pub async fn explorer(&self, query: &ExplorerQuery) -> Result<Value, Error> {
        let token = self.token().ok_or(Error::NoToken)?.to_owned();
        let base = format!("{}/{}", self.config().explorer, query.db.key());
        let url = reqwest::Url::parse_with_params(&base, query.params())
            .map_err(|e| Error::Unexpected(format!("the explorer address: {e}")))?;
        let key = url.to_string();
        if let Some(v) = self.cache().get(&key) {
            return Ok(v);
        }
        let req = self.http().get(url).bearer_auth(token);
        let v = self.side_server(req, query.db == ExplorerDb::Player).await?;
        self.cache().put(key, EXPLORER_FOR, v.clone());
        Ok(v)
    }

    /// Looks a position (FEN, standard chess, at most 7 pieces) up in the tablebase.
    pub async fn tablebase(&self, fen: &str) -> Result<Value, Error> {
        let url =
            reqwest::Url::parse_with_params(&format!("{}/standard", self.config().tablebase), [("fen", fen.trim())])
                .map_err(|e| Error::Unexpected(format!("the tablebase address: {e}")))?;
        let key = url.to_string();
        if let Some(v) = self.cache().get(&key) {
            return Ok(v);
        }
        let v = self.side_server(self.http().get(url), false).await?;
        self.cache().put(key, TABLEBASE_FOR, v.clone());
        Ok(v)
    }

    /// Asks a side server and reads its answer: one JSON document, or (the player explorer, which keeps sending
    /// updates while it indexes) the last line that came within the explorer's wait.
    async fn side_server(&self, req: reqwest::RequestBuilder, lines: bool) -> Result<Value, Error> {
        let timing = &self.config().timing;
        let req = if lines { req } else { req.timeout(timing.request_timeout) };
        let mut res = req.send().await.map_err(Error::transport)?;
        let status = res.status();
        if !status.is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(Error::from_answer(status.as_u16(), &text));
        }
        if !lines {
            let text = res.text().await.map_err(Error::transport)?;
            return serde_json::from_str(&text).map_err(|e| Error::Unexpected(e.to_string()));
        }
        let mut buffer = Vec::new();
        let mut last = None;
        let deadline = tokio::time::Instant::now() + timing.explorer_wait;
        loop {
            let chunk = tokio::select! {
                c = res.chunk() => c.map_err(Error::transport)?,
                _ = tokio::time::sleep_until(deadline) => None,
            };
            let Some(chunk) = chunk else { break };
            buffer.extend_from_slice(&chunk);
            while let Some(line) = take_line(&mut buffer) {
                if let Ok(Some(v)) = parse_line::<Value>(&line) {
                    last = Some(v);
                }
            }
        }
        if let Ok(Some(v)) = parse_line::<Value>(&String::from_utf8_lossy(&buffer)) {
            last = Some(v);
        }
        last.ok_or_else(|| Error::Unreachable("the explorer sent nothing".into()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testing::{MockAccount, MockLichess};

    #[tokio::test]
    async fn asks_the_explorer_with_the_token_and_keeps_the_answer() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        mock.set_explorer("lichess", vec![json!({ "white": 10, "draws": 2, "black": 7, "moves": [] })]);
        let client = mock.client(Some("me"));
        let query = ExplorerQuery {
            play: vec!["e2e4".into(), "e7e5".into()],
            speeds: vec!["blitz".into(), "rapid".into()],
            ratings: vec![1800, 2000],
            ..ExplorerQuery::default()
        };
        assert_eq!(client.explorer(&query).await.unwrap()["white"], 10);
        assert_eq!(client.explorer(&query).await.unwrap()["draws"], 2);
        let asked = mock.requests_to("/explorer/lichess");
        assert_eq!(asked.len(), 1, "the second answer came from the cache");
        assert_eq!(asked[0].token.as_deref(), Some("me"));
        assert_eq!(asked[0].query["play"], "e2e4,e7e5");
        assert_eq!(asked[0].query["speeds"], "blitz,rapid");
        assert_eq!(asked[0].query["ratings"], "1800,2000");
        assert!(matches!(mock.client(None).explorer(&query).await, Err(Error::NoToken)));
        // A token Lichess does not know: 401.
        let e = mock.client(Some("stale")).explorer(&query).await.unwrap_err();
        assert!(e.is_unauthorized());
    }

    #[tokio::test]
    async fn the_player_explorer_answers_its_last_update() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        mock.set_explorer("player", vec![json!({ "white": 1, "queuePosition": 0 }), json!({ "white": 5 })]);
        let query = ExplorerQuery {
            db: ExplorerDb::Player,
            player: Some("dame".into()),
            color: Some("white".into()),
            ..ExplorerQuery::default()
        };
        let v = mock.client(Some("me")).explorer(&query).await.unwrap();
        assert_eq!(v["white"], 5);
        assert_eq!(mock.requests_to("/explorer/player")[0].query["color"], "white");
    }

    #[tokio::test]
    async fn asks_the_tablebase_anonymously_once_a_day() {
        let mock = MockLichess::start().await;
        mock.set_tablebase(json!({ "category": "win", "dtz": 1 }));
        let client = mock.client(None);
        let fen = "8/8/8/8/8/2k5/2q5/K7 b - - 0 1";
        assert_eq!(client.tablebase(fen).await.unwrap()["category"], "win");
        client.tablebase(fen).await.unwrap();
        let asked = mock.requests_to("/tablebase/standard");
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].query["fen"], fen);
        assert_eq!(asked[0].token, None);
    }
}
