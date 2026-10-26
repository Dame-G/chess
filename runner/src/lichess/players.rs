//! Other Lichess players, for comparison: the leaderboards and a player's profile (proxied and cached for ten
//! minutes), and importing a player's recent public games as matches with `account: "player"` and `player: <id>`.
//! None of it needs a token; your own is sent when you are logged in, which Lichess allows a faster export for. The
//! bot's token is never used here.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::{
    call_error,
    client::{CallError, Lichess},
    me::{explorer::Cache, import::to_match_as},
    quota,
};
use crate::{
    api::{ApiError, AppState},
    jobs::{self, Job, JobSpec},
    matches::{self, LichessAccount},
};

/// How long a leaderboard or a profile is kept.
const KEEP_FOR: Duration = Duration::from_secs(600);
/// Games a player import takes when the request gives no `max`, and at most.
const DEFAULT_MAX: u32 = 50;
const MAX: u32 = 1000;

/// The speeds Lichess keeps a leaderboard of.
pub const PERFS: [&str; 5] = ["ultraBullet", "bullet", "blitz", "rapid", "classical"];

pub struct Players {
    base: String,
    http: reqwest::Client,
    cache: Cache,
}

impl Players {
    pub fn new(base: &str) -> Self {
        Self { base: base.trim_end_matches('/').to_owned(), http: reqwest::Client::new(), cache: Cache::default() }
    }

    /// Your token's client when you are logged in, else an anonymous one.
    fn client(&self, state: &AppState) -> Lichess {
        state.me.client().unwrap_or_else(|| Lichess::new(self.http.clone(), &self.base, "", None))
    }
}

/// A Lichess username: 2 to 30 letters, digits, `_` or `-`.
pub fn check_username(name: &str) -> Result<String, ApiError> {
    let ok = (2..=30).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(name.to_lowercase()) } else { Err(ApiError::bad_request(&format!("{name} is not a Lichess username"))) }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct TopPlayer {
    pub id: String,
    pub username: String,
    pub title: Option<String>,
    pub rating: u32,
    /// Rating change over the last games, as Lichess reports it.
    pub progress: i32,
}

/// The best `nb` players (at most 200) at a speed. Cached ten minutes.
pub async fn top(state: &AppState, perf: &str, nb: u32) -> Result<Vec<TopPlayer>, ApiError> {
    if !PERFS.contains(&perf) {
        return Err(ApiError::bad_request(&format!("perf is one of {}", PERFS.join(", "))));
    }
    let nb = nb.clamp(1, 200);
    let key = format!("top/{perf}/{nb}");
    let v = match state.players.cache.get(&key) {
        Some(v) => v,
        None => {
            let v =
                state.players.client(state).get(&format!("/api/player/top/{nb}/{perf}")).await.map_err(call_error)?;
            state.players.cache.put(key, KEEP_FOR, v.clone());
            v
        }
    };
    Ok(read_top(&v, perf))
}

pub fn read_top(v: &Value, perf: &str) -> Vec<TopPlayer> {
    v["users"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| TopPlayer {
            id: u["id"].as_str().unwrap_or_default().to_owned(),
            username: u["username"].as_str().unwrap_or_default().to_owned(),
            title: u["title"].as_str().map(str::to_owned),
            rating: u["perfs"][perf]["rating"].as_u64().unwrap_or(0) as u32,
            progress: u["perfs"][perf]["progress"].as_i64().unwrap_or(0) as i32,
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PerfRating {
    pub rating: u32,
    pub games: u32,
    /// Lichess's provisional flag: too few games for the rating to mean much.
    pub provisional: bool,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct PlayerRatings {
    pub bullet: Option<PerfRating>,
    pub blitz: Option<PerfRating>,
    pub rapid: Option<PerfRating>,
    pub classical: Option<PerfRating>,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct GameCounts {
    pub all: u32,
    pub rated: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PlayerProfile {
    pub id: String,
    pub username: String,
    pub title: Option<String>,
    pub url: String,
    pub ratings: PlayerRatings,
    /// Lichess's counts of all the player's games.
    pub counts: GameCounts,
    pub flag: Option<String>,
    pub created_at: Option<String>,
    pub seen_at: Option<String>,
    /// A closed account, or one marked for breaking Lichess's terms.
    pub disabled: bool,
    /// Games of the player stored here (imported), and how many of them are reviewed.
    pub imported: u32,
    pub reviewed: u32,
}

pub fn read_profile(v: &Value) -> PlayerProfile {
    let perf = |name: &str| {
        let p = &v["perfs"][name];
        p["rating"].as_u64().map(|r| PerfRating {
            rating: r as u32,
            games: p["games"].as_u64().unwrap_or(0) as u32,
            provisional: p["prov"].as_bool().unwrap_or(false),
        })
    };
    let n = |k: &str| v["count"][k].as_u64().unwrap_or(0) as u32;
    let id = v["id"].as_str().unwrap_or_default().to_owned();
    PlayerProfile {
        url: format!("https://lichess.org/@/{id}"),
        id,
        username: v["username"].as_str().unwrap_or_default().to_owned(),
        title: v["title"].as_str().map(str::to_owned),
        ratings: PlayerRatings {
            bullet: perf("bullet"),
            blitz: perf("blitz"),
            rapid: perf("rapid"),
            classical: perf("classical"),
        },
        counts: GameCounts { all: n("all"), rated: n("rated"), wins: n("win"), draws: n("draw"), losses: n("loss") },
        flag: v["profile"]["flag"].as_str().or(v["flag"].as_str()).map(str::to_owned),
        created_at: v["createdAt"].as_i64().map(quota::iso),
        seen_at: v["seenAt"].as_i64().map(quota::iso),
        disabled: v["disabled"].as_bool().unwrap_or(false) || v["tosViolation"].as_bool().unwrap_or(false),
        imported: 0,
        reviewed: 0,
    }
}

/// A player's profile (cached ten minutes) and how many of their games are here. 404 if Lichess knows no such player.
pub async fn profile(state: &AppState, username: &str) -> Result<PlayerProfile, ApiError> {
    let id = check_username(username)?;
    let key = format!("user/{id}");
    let v = match state.players.cache.get(&key) {
        Some(v) => v,
        None => {
            let v = match state.players.client(state).get(&format!("/api/user/{id}")).await {
                Ok(v) => v,
                Err(CallError::Status { status: 404, .. }) => {
                    return Err(ApiError::not_found(&format!("Lichess has no player {username}")));
                }
                Err(e) => return Err(call_error(e)),
            };
            state.players.cache.put(key, KEEP_FOR, v.clone());
            v
        }
    };
    let mut p = read_profile(&v);
    let who = id.clone();
    let (imported, reviewed) = state
        .store
        .call(move |c| {
            Ok(c.query_row(
                "SELECT count(*), ifnull(sum(EXISTS (SELECT 1 FROM reviews r WHERE r.match_id = m.id)), 0) FROM matches m
                 WHERE json_extract(m.data, '$.lichess.account') = 'player' AND json_extract(m.data, '$.lichess.player') = ?1",
                [who],
                |r| Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?)),
            )?)
        })
        .await?;
    p.imported = imported;
    p.reviewed = reviewed;
    Ok(p)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct PlayerImportRequest {
    /// The player's newest games, at most this many. Default 50, at most 1000.
    #[serde(default)]
    pub max: Option<u32>,
    /// Only these speeds (`bullet`, `blitz`, `rapid`, `classical`, ...). All when empty.
    #[serde(default)]
    pub speeds: Vec<String>,
    /// Only rated (true) or casual (false) games.
    #[serde(default)]
    pub rated: Option<bool>,
}

/// Starts importing a player's games: a job beside the queue (it uses the network). 409 while you play on Lichess.
pub async fn start_import(state: &Arc<AppState>, username: &str, req: PlayerImportRequest) -> Result<Job, ApiError> {
    super::me::check_fair_play(state)?;
    let username = check_username(username)?;
    crate::review::check_speeds(&req.speeds)?;
    Ok(jobs::run_now(state, JobSpec::LichessPlayerImport { username, options: req }).await?)
}

/// How many games to store at once.
const BATCH: usize = 100;

pub fn export_path(username: &str, req: &PlayerImportRequest) -> String {
    let mut path = format!(
        "/api/games/user/{username}?max={}&moves=true&clocks=true&evals=true&opening=true&finished=true&sort=dateDesc",
        req.max.unwrap_or(DEFAULT_MAX).clamp(1, MAX)
    );
    if !req.speeds.is_empty() {
        path += &format!("&perfType={}", req.speeds.join(","));
    }
    if let Some(r) = req.rated {
        path += &format!("&rated={r}");
    }
    path
}

/// The import job: streams the player's newest games and stores the ones not here yet.
pub async fn run_import(
    state: &Arc<AppState>,
    cancel: &CancellationToken,
    log: &(dyn Fn(&str) + Sync),
    username: &str,
    req: &PlayerImportRequest,
) -> Result<Value> {
    let client = state.players.client(state);
    log(&format!("importing {username}'s newest games (at most {})", req.max.unwrap_or(DEFAULT_MAX).clamp(1, MAX)));
    let mut stream = match client.stream(&export_path(username, req)).await {
        Ok(s) => s,
        Err(CallError::Status { status: 404, .. }) => bail!("Lichess has no player {username}"),
        Err(e) => return Err(e.into()),
    };
    let (mut stored, mut skipped, mut other, mut seen) = (0usize, 0usize, 0usize, 0usize);
    let mut batch = Vec::new();
    loop {
        let line = tokio::select! {
            _ = cancel.cancelled() => bail!("cancelled"),
            line = stream.next() => line,
        };
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                // Keep what was read: a second run fetches the same newest games and skips those.
                if !batch.is_empty() {
                    let (s, _) = matches::import(state, std::mem::take(&mut batch)).await?;
                    stored += s;
                }
                return Err(e).with_context(|| format!("{seen} games read, {stored} stored"));
            }
        };
        let done = line.is_none();
        if let Some(g) = line {
            seen += 1;
            match to_match_as(&g, username, LichessAccount::Player) {
                Ok(Some(m)) => batch.push(m),
                Ok(None) => other += 1,
                Err(e) => {
                    log(&format!("skipped: {e:#}"));
                    other += 1;
                }
            }
        }
        if batch.len() >= BATCH || (done && !batch.is_empty()) {
            let (s, k) = matches::import(state, std::mem::take(&mut batch)).await?;
            stored += s;
            skipped += k;
            log(&format!("{seen} games read, {stored} stored"));
        }
        if done {
            break;
        }
    }
    log(&format!("done: {stored} new, {skipped} already here, {other} not standard chess or unreadable"));
    Ok(json!({ "player": username, "imported": stored, "skipped": skipped, "other": other }))
}
