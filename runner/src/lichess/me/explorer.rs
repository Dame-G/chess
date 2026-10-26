//! The opening explorer (explorer.lichess.org: Lichess games, masters' games or one player's) and the tablebase
//! (tablebase.lichess.org), proxied for the board with a small in-memory cache. The explorer needs a Lichess login,
//! so it is asked with your token; the tablebase needs none. Both are refused while you play (fair play).

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::Value;
use shakmaty::{CastlingMode, Chess, Position, fen::Fen};
use utoipa::{IntoParams, ToSchema};

use super::fair_play_error;
use crate::api::{ApiError, AppState};

/// How long an answer is kept: explorer statistics move slowly, tablebase answers never change.
const EXPLORER_FOR: Duration = Duration::from_secs(600);
const TABLEBASE_FOR: Duration = Duration::from_secs(24 * 3600);
/// Answers kept at most; the cache starts over beyond.
const KEEP: usize = 512;

#[derive(Default)]
pub struct Cache {
    entries: Mutex<HashMap<String, (Instant, Duration, Value)>>,
}

impl Cache {
    fn get(&self, key: &str) -> Option<Value> {
        let entries = self.entries.lock().unwrap();
        entries.get(key).filter(|(at, ttl, _)| at.elapsed() < *ttl).map(|(_, _, v)| v.clone())
    }

    fn put(&self, key: String, ttl: Duration, value: Value) {
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

/// Which games the explorer counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExplorerDb {
    /// Games played on Lichess, filtered by speed and rating.
    #[default]
    Lichess,
    /// Over-the-board games between masters.
    Masters,
    /// One player's games (`player` and `color` needed).
    Player,
}

#[derive(Clone, Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExplorerQuery {
    /// `lichess` (default), `masters` or `player`.
    #[param(inline)]
    pub db: Option<ExplorerDb>,
    /// The position as FEN. Default the start position.
    pub fen: Option<String>,
    /// Moves played from `fen`, in UCI notation, separated by commas.
    pub play: Option<String>,
    /// `lichess` and `player`: speeds, separated by commas (`blitz,rapid`).
    pub speeds: Option<String>,
    /// `lichess`: rating groups, separated by commas (`1600,1800,2000`).
    pub ratings: Option<String>,
    /// `player`: `rated` and/or `casual`, separated by commas.
    pub modes: Option<String>,
    /// From this month (`2020-01`), or year for `masters`.
    pub since: Option<String>,
    /// Until this month (`2024-12`), or year for `masters`.
    pub until: Option<String>,
    /// How many moves to list. Default 12.
    pub moves: Option<u32>,
    /// How many top games to list (at most 4 for `lichess`, 15 for `masters`).
    pub top_games: Option<u32>,
    /// How many recent games to list (at most 4, or 8 for `player`).
    pub recent_games: Option<u32>,
    /// `player`: whose games, a Lichess username.
    pub player: Option<String>,
    /// `player`: `white` or `black`.
    pub color: Option<String>,
}

#[derive(Clone, Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TablebaseQuery {
    /// The position as FEN: standard chess with at most 7 pieces.
    pub fen: String,
}

fn check_fen(fen: &str) -> Result<Chess, ApiError> {
    fen.parse::<Fen>()
        .map_err(|e| ApiError::bad_request(&format!("that FEN does not parse: {e}")))?
        .into_position(CastlingMode::Standard)
        .map_err(|e| ApiError::bad_request(&format!("that is not a legal position: {e}")))
}

fn unavailable(what: &str, e: reqwest::Error) -> ApiError {
    ApiError::bad_gateway(&format!("the {what} is unavailable: {}", e.without_url()))
}

/// Asks a side server and reads its answer: one JSON document, or (the player explorer, which keeps sending updates
/// while it indexes) the last line that came within a few seconds.
async fn fetch(what: &str, req: reqwest::RequestBuilder, lines: bool) -> Result<Value, ApiError> {
    let res =
        req.timeout(Duration::from_secs(if lines { 30 } else { 15 })).send().await.map_err(|e| unavailable(what, e))?;
    let status = res.status();
    if status.as_u16() == 429 {
        return Err(ApiError::too_many_requests(&format!("Lichess is rate-limiting the {what}: wait a minute")));
    }
    if status.as_u16() == 401 {
        return Err(ApiError::bad_gateway(&format!(
            "Lichess refused the {what}: your login may have expired, log in again"
        )));
    }
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        let message: String = text.chars().take(200).collect();
        return Err(if status.is_client_error() {
            ApiError::bad_request(&format!("the {what} answered {status}: {message}"))
        } else {
            ApiError::bad_gateway(&format!("the {what} answered {status}"))
        });
    }
    if !lines {
        return res.json().await.map_err(|e| unavailable(what, e));
    }
    let mut res = res;
    let mut buffer = Vec::new();
    let mut last = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let chunk = tokio::select! {
            c = res.chunk() => c.map_err(|e| unavailable(what, e))?,
            _ = tokio::time::sleep_until(deadline) => None,
        };
        let Some(chunk) = chunk else { break };
        buffer.extend_from_slice(&chunk);
        while let Some(line) = super::super::client::take_line(&mut buffer) {
            if let Ok(Some(v)) = super::super::client::parse_line(&line) {
                last = Some(v);
            }
        }
    }
    if let Ok(Some(v)) = super::super::client::parse_line(&String::from_utf8_lossy(&buffer)) {
        last = Some(v);
    }
    last.ok_or_else(|| ApiError::bad_gateway(&format!("the {what} sent nothing")))
}

/// Looks a position up in the opening explorer. 409 without a login (the explorer needs one), or while you play a
/// game other than correspondence.
pub async fn explorer(state: &AppState, q: ExplorerQuery) -> Result<Value, ApiError> {
    if state.me.explorer_locked() {
        return Err(fair_play_error());
    }
    let token = state
        .me
        .token()
        .ok_or(ApiError::conflict("Lichess's opening explorer needs a login: connect your Lichess account first"))?;
    let db = q.db.unwrap_or_default();
    if let Some(fen) = &q.fen {
        check_fen(fen)?;
    }
    let mut params: Vec<(&str, String)> = vec![("variant", "standard".into())];
    let mut add = |key: &'static str, v: &Option<String>| {
        if let Some(v) = v.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            params.push((key, v.to_owned()));
        }
    };
    add("fen", &q.fen);
    add("play", &q.play);
    add("since", &q.since);
    add("until", &q.until);
    match db {
        ExplorerDb::Lichess => {
            add("speeds", &q.speeds);
            add("ratings", &q.ratings);
        }
        ExplorerDb::Masters => {}
        ExplorerDb::Player => {
            let player = q.player.as_deref().map(str::trim).filter(|p| !p.is_empty());
            let color = q.color.as_deref();
            if player.is_none() || !matches!(color, Some("white" | "black")) {
                return Err(ApiError::bad_request("the player explorer needs `player` and `color` (white or black)"));
            }
            add("player", &q.player);
            add("color", &q.color);
            add("speeds", &q.speeds);
            add("modes", &q.modes);
        }
    }
    for (key, v) in [("moves", q.moves), ("topGames", q.top_games), ("recentGames", q.recent_games)] {
        if let Some(n) = v {
            params.push((key, n.min(50).to_string()));
        }
    }
    let path = match db {
        ExplorerDb::Lichess => "lichess",
        ExplorerDb::Masters => "masters",
        ExplorerDb::Player => "player",
    };
    let url = reqwest::Url::parse_with_params(&format!("{}/{path}", state.me.explorer), &params)
        .map_err(|e| anyhow::anyhow!("the explorer URL: {e}"))?;
    let key = url.to_string();
    if let Some(v) = state.me.cache.get(&key) {
        return Ok(v);
    }
    let req = state.me.http.get(url).bearer_auth(token);
    let v = fetch("opening explorer", req, db == ExplorerDb::Player).await?;
    state.me.cache.put(key, EXPLORER_FOR, v.clone());
    Ok(v)
}

/// Looks a position of at most seven pieces up in the tablebase. 409 while you play.
pub async fn tablebase(state: &AppState, q: TablebaseQuery) -> Result<Value, ApiError> {
    if state.me.locked() {
        return Err(fair_play_error());
    }
    let pos = check_fen(&q.fen)?;
    if pos.board().occupied().count() > 7 {
        return Err(ApiError::bad_request("the tablebase knows positions of at most 7 pieces"));
    }
    let url = reqwest::Url::parse_with_params(&format!("{}/standard", state.me.tablebase), &[("fen", q.fen.trim())])
        .map_err(|e| anyhow::anyhow!("the tablebase URL: {e}"))?;
    let key = url.to_string();
    if let Some(v) = state.me.cache.get(&key) {
        return Ok(v);
    }
    let v = fetch("tablebase", state.me.http.get(url), false).await?;
    state.me.cache.put(key, TABLEBASE_FOR, v.clone());
    Ok(v)
}
