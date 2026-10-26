//! Your own Lichess account, beside the bot: you log in (OAuth2 with PKCE, or a personal token), and the runner
//! becomes a Lichess client for you. It follows the account's event stream, so the games you play show up as matches
//! (through the Board API, <https://lichess.org/api#tag/Board>), where the board's moves, draw offers, takebacks and
//! resignations go to Lichess. It also finds games (seeks and challenges), imports your history, and proxies the
//! opening explorer and the tablebase.
//!
//! Fair play is a hard rule here: Lichess forbids engines, books, tablebases and the explorer during a live game (the
//! explorer is allowed in correspondence only). While you have a game running, the runner refuses engine analysis,
//! the tablebase and (outside correspondence) the explorer, and says so in [`MeStatus::fair_play_lock`].
//!
//! The token is kept in the data directory in the file `lichess-me-token`, apart from the bot's, and no route ever
//! returns it. The account can never be the bot's: a BOT account is refused, as is the bot's own account.

pub mod explorer;
mod game;
pub mod import;
pub mod oauth;
#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::{
    BotState, ChallengeColor, ChallengeSent, Ratings, call_error,
    client::{CallError, Lichess},
};
use crate::api::{ApiError, AppState};

/// The scopes the runner asks for: playing with the Board API, and reading and sending challenges. Your games and the
/// opening explorer need a login but no scope.
pub const SCOPES: &[&str] = &["board:play", "challenge:read", "challenge:write"];

/// Your Lichess account.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct MeAccount {
    /// The lower-case id, as games name players.
    pub id: String,
    pub username: String,
    /// A Lichess title such as `FM`; never `BOT`, which is refused.
    pub title: Option<String>,
    /// Your rating at each speed you have played.
    pub ratings: Ratings,
}

/// Who a challenge is with.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct Opponent {
    pub name: String,
    pub title: Option<String>,
    pub rating: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Someone challenges you.
    Incoming,
    /// You challenge someone.
    Outgoing,
}

/// A clock, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct Clock {
    pub initial: u32,
    pub increment: u32,
}

/// A challenge waiting for an answer, yours or someone's.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct ChallengeInfo {
    /// The challenge id, which becomes the game id if it is accepted.
    pub id: String,
    pub url: String,
    pub direction: Direction,
    pub opponent: Opponent,
    pub rated: bool,
    /// `blitz`, `rapid`, `classical`, `correspondence`, ... as Lichess names it.
    pub speed: String,
    pub variant: String,
    /// The clock, or none for correspondence and unlimited games.
    pub clock: Option<Clock>,
    /// Days per move, for correspondence.
    pub days: Option<u32>,
    /// The colour you would play.
    pub color: ChallengeColor,
    /// Whether the game could be played here, through the Board API: blitz and slower. A bullet challenge has to be
    /// played on lichess.org.
    pub playable: bool,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct Challenges {
    pub incoming: Vec<ChallengeInfo>,
    pub outgoing: Vec<ChallengeInfo>,
}

/// A seek you have open in the lobby.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MeSeek {
    pub clock_seconds: u32,
    pub increment: u32,
    pub rated: bool,
    pub color: ChallengeColor,
    pub rating_range: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct MeSeekStatus {
    /// The seek open now, until someone takes it (the game then starts) or it is cancelled.
    pub seeking: Option<MeSeek>,
    /// How the last seek went, for people.
    pub note: Option<String>,
}

/// A game of yours that cannot be played here (a bullet game, a variant, started on lichess.org): it still locks
/// analysis.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct ElsewhereGame {
    pub id: String,
    pub url: String,
    pub speed: String,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MeStatus {
    /// Whether a token is stored.
    pub configured: bool,
    pub account: Option<MeAccount>,
    /// The scopes the token has, as Lichess reports them.
    pub scopes: Vec<String>,
    /// Scopes the runner asks for that the token lacks (a personal token made without them).
    pub missing_scopes: Vec<String>,
    /// When the token expires (ISO 8601, UTC), if Lichess said. A login lasts a year.
    pub expires_at: Option<String>,
    /// The event stream: `off` without a token, else `connecting`, `online` or `error` (with `error`).
    pub state: BotState,
    pub error: Option<String>,
    /// The matches of the games you are playing here now.
    pub playing: Vec<i64>,
    /// Games you are playing that cannot be played here.
    pub elsewhere: Vec<ElsewhereGame>,
    pub challenges: Challenges,
    pub seek: MeSeekStatus,
    /// True while you have a game running on Lichess: engine analysis, the tablebase and the explorer are refused.
    pub fair_play_lock: bool,
}

/// A game of yours running on Lichess.
struct Running {
    /// The record, once the game's task has one (games played here).
    match_id: Option<i64>,
    /// Whether it is played here, through the Board API.
    board: bool,
    speed: String,
}

struct Inner {
    token: Option<String>,
    scopes: Vec<String>,
    expires_at: Option<String>,
    account: Option<MeAccount>,
    state: BotState,
    error: Option<String>,
    run: Option<CancellationToken>,
    games: HashMap<String, Running>,
    challenges: Vec<ChallengeInfo>,
    seek: Option<(MeSeek, CancellationToken)>,
    /// Counts seeks, so the end of an old one does not clear a newer one.
    seek_id: u64,
    seek_note: Option<String>,
    logins: HashMap<String, oauth::Login>,
}

pub struct Me {
    base: String,
    explorer: String,
    tablebase: String,
    http: reqwest::Client,
    inner: Mutex<Inner>,
    cache: explorer::Cache,
}

/// The explorer's and the tablebase's servers: Lichess's own beside lichess.org, else (a mock) under its address.
/// `LICHESS_EXPLORER_URL` and `LICHESS_TABLEBASE_URL` override them.
fn side_servers(base: &str) -> (String, String) {
    let (explorer, tablebase) = if base == crate::config::LICHESS_URL {
        ("https://explorer.lichess.org".to_owned(), "https://tablebase.lichess.org".to_owned())
    } else {
        (format!("{base}/explorer"), format!("{base}/tablebase"))
    };
    let env = |key: &str, default: String| {
        std::env::var(key).ok().map(|v| v.trim_end_matches('/').to_owned()).filter(|v| !v.is_empty()).unwrap_or(default)
    };
    (env("LICHESS_EXPLORER_URL", explorer), env("LICHESS_TABLEBASE_URL", tablebase))
}

impl Me {
    pub fn new(base: &str) -> Self {
        let base = base.trim_end_matches('/').to_owned();
        let (explorer, tablebase) = side_servers(&base);
        Self {
            base,
            explorer,
            tablebase,
            http: reqwest::Client::new(),
            inner: Mutex::new(Inner {
                token: None,
                scopes: Vec::new(),
                expires_at: None,
                account: None,
                state: BotState::Off,
                error: None,
                run: None,
                games: HashMap::new(),
                challenges: Vec::new(),
                seek: None,
                seek_id: 0,
                seek_note: None,
                logins: HashMap::new(),
            }),
            cache: explorer::Cache::default(),
        }
    }

    fn client(&self) -> Option<Lichess> {
        let inner = self.inner.lock().unwrap();
        let token = inner.token.as_ref()?;
        Some(Lichess::new(self.http.clone(), &self.base, token, inner.account.as_ref().map(|a| a.username.as_str())))
    }

    fn token(&self) -> Option<String> {
        self.inner.lock().unwrap().token.clone()
    }

    fn account(&self) -> Option<MeAccount> {
        self.inner.lock().unwrap().account.clone()
    }

    pub fn status(&self) -> MeStatus {
        let inner = self.inner.lock().unwrap();
        let mut playing: Vec<i64> = inner.games.values().filter_map(|g| g.match_id).collect();
        playing.sort_unstable();
        let mut elsewhere: Vec<ElsewhereGame> = inner
            .games
            .iter()
            .filter(|(_, g)| !g.board)
            .map(|(id, g)| ElsewhereGame {
                id: id.clone(),
                url: format!("https://lichess.org/{id}"),
                speed: g.speed.clone(),
            })
            .collect();
        elsewhere.sort_by(|a, b| a.id.cmp(&b.id));
        let mut challenges = Challenges::default();
        for c in &inner.challenges {
            match c.direction {
                Direction::Incoming => challenges.incoming.push(c.clone()),
                Direction::Outgoing => challenges.outgoing.push(c.clone()),
            }
        }
        let missing_scopes = if inner.token.is_some() {
            SCOPES.iter().filter(|s| !inner.scopes.iter().any(|t| t == *s)).map(|s| (*s).to_owned()).collect()
        } else {
            Vec::new()
        };
        MeStatus {
            configured: inner.token.is_some(),
            account: inner.account.clone(),
            scopes: inner.scopes.clone(),
            missing_scopes,
            expires_at: inner.expires_at.clone(),
            state: inner.state,
            error: inner.error.clone(),
            playing,
            elsewhere,
            challenges,
            seek: MeSeekStatus { seeking: inner.seek.as_ref().map(|(s, _)| s.clone()), note: inner.seek_note.clone() },
            fair_play_lock: !inner.games.is_empty(),
        }
    }

    /// Whether you have a game running on Lichess: engine analysis and the tablebase are off.
    pub fn locked(&self) -> bool {
        !self.inner.lock().unwrap().games.is_empty()
    }

    /// Whether the explorer is off: during any game but correspondence, where Lichess allows it.
    fn explorer_locked(&self) -> bool {
        self.inner.lock().unwrap().games.values().any(|g| g.speed != "correspondence")
    }

    fn announce(&self, state: &AppState) {
        state.events.emit("lichess.me.status", serde_json::to_value(self.status()).unwrap_or_default());
    }

    fn set_state(&self, state: &AppState, to: BotState, error: Option<String>) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let changed = inner.state != to || inner.error != error;
            inner.state = to;
            inner.error = error;
            changed
        };
        if changed {
            self.announce(state);
        }
    }

    /// Changes the running games with `f`, and when that turns the fair-play lock on or off, says so and stops any
    /// live analysis.
    fn with_games(&self, state: &AppState, f: impl FnOnce(&mut HashMap<String, Running>)) {
        let (before, after) = {
            let mut inner = self.inner.lock().unwrap();
            let before = !inner.games.is_empty();
            f(&mut inner.games);
            (before, !inner.games.is_empty())
        };
        if after && !before {
            crate::analysis::stop_live(state);
        }
        if after != before {
            state.events.emit("lichess.me.fair_play", json!({ "locked": after }));
        }
        self.announce(state);
    }

    fn game_recorded(&self, state: &AppState, id: &str, match_id: i64) {
        if let Some(g) = self.inner.lock().unwrap().games.get_mut(id) {
            g.match_id = Some(match_id);
        }
        self.announce(state);
    }

    fn game_over(&self, state: &AppState, id: &str) {
        self.with_games(state, |games| {
            games.remove(id);
        });
    }
}

/// The 409 every helper answers while you play: Lichess forbids outside help during a game.
pub fn fair_play_error() -> ApiError {
    ApiError::conflict("Analysis is off while you're playing on Lichess")
}

/// Refuses engine analysis while you have a game running on Lichess.
pub fn check_fair_play(state: &AppState) -> Result<(), ApiError> {
    if state.me.locked() { Err(fair_play_error()) } else { Ok(()) }
}

// ---------------------------------------------------------------------------------------------------------------------
// The token
// ---------------------------------------------------------------------------------------------------------------------

pub fn token_path(data: &Path) -> PathBuf {
    data.join("lichess-me-token")
}

fn read_me(v: &Value) -> MeAccount {
    let rating = |speed: &str| v["perfs"][speed]["rating"].as_u64().map(|r| r as u32);
    MeAccount {
        id: v["id"].as_str().unwrap_or_default().to_owned(),
        username: v["username"].as_str().unwrap_or_default().to_owned(),
        title: v["title"].as_str().map(str::to_owned),
        ratings: Ratings {
            bullet: rating("bullet"),
            blitz: rating("blitz"),
            rapid: rating("rapid"),
            classical: rating("classical"),
        },
    }
}

/// What Lichess says of a token: its scopes and when it expires (`POST /api/token/test`).
async fn token_info(me: &Me, token: &str) -> Option<(Vec<String>, Option<String>)> {
    let res = me
        .http
        .post(format!("{}/api/token/test", me.base))
        .body(token.to_owned())
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .ok()?;
    let v: Value = res.json().await.ok()?;
    let info = &v[token];
    let scopes = info["scopes"].as_str()?.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    let expires = info["expires"].as_i64().map(super::quota::iso);
    Some((scopes.collect(), expires))
}

/// Checks a token with Lichess and stores it: it must be a person's account, not a BOT and not the bot's, and it must
/// be allowed to play (`board:play`). `expires_in` is what the OAuth exchange said, when it was one.
pub async fn set_token(state: &Arc<AppState>, token: &str) -> Result<MeStatus, ApiError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ApiError::bad_request("the token is empty"));
    }
    let me = &state.me;
    let client = Lichess::new(me.http.clone(), &me.base, token, None);
    let account = match client.get("/api/account").await {
        Ok(v) => read_me(&v),
        Err(CallError::Unauthorized) => return Err(ApiError::bad_request("Lichess rejected that token")),
        Err(e) => return Err(call_error(e)),
    };
    if account.title.as_deref() == Some("BOT") {
        return Err(ApiError::bad_request(&format!(
            "{} is a BOT account: connect it as the bot (`chess lichess token`), not as yours",
            account.username
        )));
    }
    if state.lichess.account().is_some_and(|bot| bot.id == account.id) {
        return Err(ApiError::bad_request("that is the bot's account: your own account must be another one"));
    }
    let (scopes, expires_at) = token_info(me, token).await.unwrap_or_default();
    if !scopes.is_empty() && !scopes.iter().any(|s| s == "board:play") {
        return Err(ApiError::bad_request(
            "that token cannot play (no board:play scope): log in again, or make a token with board:play, \
             challenge:read and challenge:write",
        ));
    }
    crate::config::write_private(&token_path(&state.config.data), token).context("store the Lichess token")?;
    {
        let mut inner = me.inner.lock().unwrap();
        inner.token = Some(token.to_owned());
        inner.scopes = scopes;
        inner.expires_at = expires_at;
        inner.account = Some(account);
        if let Some(run) = inner.run.take() {
            run.cancel();
        }
    }
    spawn_loop(state);
    me.announce(state);
    Ok(me.status())
}

/// Revokes the token at Lichess (as far as Lichess can be reached) and forgets it. Games in progress stop being
/// followed here; on Lichess they go on.
pub async fn logout(state: &Arc<AppState>) -> Result<MeStatus, ApiError> {
    let me = &state.me;
    if let Some(client) = me.client()
        && let Err(e) = client.delete("/api/token").await
    {
        tracing::info!("lichess (you): could not revoke the token: {e}");
    }
    let games: Vec<String> = {
        let mut inner = me.inner.lock().unwrap();
        if let Some(run) = inner.run.take() {
            run.cancel();
        }
        if let Some((_, cancel)) = inner.seek.take() {
            cancel.cancel();
        }
        inner.token = None;
        inner.scopes.clear();
        inner.expires_at = None;
        inner.account = None;
        inner.challenges.clear();
        inner.seek_note = None;
        inner.games.keys().cloned().collect()
    };
    match std::fs::remove_file(token_path(&state.config.data)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(anyhow::Error::from(e).context("remove the Lichess token").into());
        }
        _ => {}
    }
    for id in games {
        me.game_over(state, &id);
    }
    me.set_state(state, BotState::Off, None);
    Ok(me.status())
}

/// Picks up the stored token after a restart; games still running come back on the event stream.
pub async fn resume(state: &Arc<AppState>) {
    let Some(token) = std::fs::read_to_string(token_path(&state.config.data)).ok() else { return };
    let token = token.trim().to_owned();
    if token.is_empty() {
        return;
    }
    state.me.inner.lock().unwrap().token = Some(token.clone());
    spawn_loop(state);
    // The scopes, without holding up the start.
    let state = state.clone();
    tokio::spawn(async move {
        if let Some((scopes, expires_at)) = token_info(&state.me, &token).await {
            let mut inner = state.me.inner.lock().unwrap();
            inner.scopes = scopes;
            inner.expires_at = expires_at;
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// The event stream
// ---------------------------------------------------------------------------------------------------------------------

fn spawn_loop(state: &Arc<AppState>) {
    let cancel = CancellationToken::new();
    state.me.inner.lock().unwrap().run = Some(cancel.clone());
    tokio::spawn(run(state.clone(), cancel));
}

async fn run(state: Arc<AppState>, cancel: CancellationToken) {
    let me = &state.me;
    let mut backoff = Duration::from_secs(1);
    while !cancel.is_cancelled() {
        me.set_state(&state, BotState::Connecting, None);
        let wait = match follow(&state, &cancel, &mut backoff).await {
            Ok(()) => backoff,
            Err(e) => {
                let limited = matches!(e.downcast_ref::<CallError>(), Some(CallError::RateLimited));
                tracing::warn!("lichess (you): {e:#}");
                if !cancel.is_cancelled() {
                    me.set_state(&state, BotState::Error, Some(format!("{e:#}")));
                }
                if limited { Duration::from_secs(61) } else { backoff }
            }
        };
        backoff = (backoff * 2).min(Duration::from_secs(60));
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

async fn follow(state: &Arc<AppState>, cancel: &CancellationToken, backoff: &mut Duration) -> Result<()> {
    let me = &state.me;
    let client = me.client().context("no Lichess token")?;
    let account = read_me(&client.get("/api/account").await?);
    me.inner.lock().unwrap().account = Some(account.clone());
    let client = me.client().context("no Lichess token")?;
    let mut events = client.stream("/api/stream/event").await?;
    // The stream starts with every challenge still open: what was known before is sent again.
    me.inner.lock().unwrap().challenges.clear();
    me.set_state(state, BotState::Online, None);
    *backoff = Duration::from_secs(1);
    loop {
        let event = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            e = events.next() => e?,
        };
        let Some(event) = event else { return Ok(()) };
        handle(state, &account, &event);
    }
}

/// Reads a challenge as the event stream sends it, from your side.
fn read_challenge(c: &Value, me: &MeAccount) -> ChallengeInfo {
    let challenger = &c["challenger"];
    let outgoing = challenger["id"].as_str() == Some(me.id.as_str());
    let other = if outgoing { &c["destUser"] } else { challenger };
    let tc = &c["timeControl"];
    let clock = (tc["type"] == "clock").then(|| Clock {
        initial: tc["limit"].as_u64().unwrap_or(0) as u32,
        increment: tc["increment"].as_u64().unwrap_or(0) as u32,
    });
    // `color` is the challenger's choice.
    let theirs = match c["color"].as_str() {
        Some("white") => ChallengeColor::White,
        Some("black") => ChallengeColor::Black,
        _ => ChallengeColor::Random,
    };
    let color = match (outgoing, theirs) {
        (true, c) | (false, c @ ChallengeColor::Random) => c,
        (false, ChallengeColor::White) => ChallengeColor::Black,
        (false, ChallengeColor::Black) => ChallengeColor::White,
    };
    let speed = c["speed"].as_str().unwrap_or_default().to_owned();
    let variant = c["variant"]["key"].as_str().unwrap_or("standard").to_owned();
    let id = c["id"].as_str().unwrap_or_default().to_owned();
    ChallengeInfo {
        url: c["url"].as_str().map(str::to_owned).unwrap_or_else(|| format!("https://lichess.org/{id}")),
        id,
        direction: if outgoing { Direction::Outgoing } else { Direction::Incoming },
        opponent: Opponent {
            name: other["name"].as_str().or(other["id"].as_str()).unwrap_or("someone").to_owned(),
            title: other["title"].as_str().map(str::to_owned),
            rating: other["rating"].as_u64().map(|r| r as u32),
        },
        rated: c["rated"].as_bool().unwrap_or(false),
        playable: board_speed(&speed) && matches!(variant.as_str(), "standard" | "fromPosition"),
        speed,
        variant,
        clock,
        days: c["timeControl"]["daysPerTurn"].as_u64().map(|d| d as u32),
        color,
    }
}

/// Whether the Board API plays a game of this speed (blitz and slower; a seek needs rapid or slower).
fn board_speed(speed: &str) -> bool {
    matches!(speed, "blitz" | "rapid" | "classical" | "correspondence")
}

fn emit_challenge(state: &AppState, c: &ChallengeInfo, event: &str, reason: Option<&str>) {
    state.events.emit("lichess.me.challenge", json!({ "challenge": c, "event": event, "reason": reason }));
}

fn handle(state: &Arc<AppState>, account: &MeAccount, event: &Value) {
    let me = &state.me;
    match event["type"].as_str().unwrap_or_default() {
        "gameStart" => {
            let g = &event["game"];
            let Some(id) = g["gameId"].as_str().or(g["id"].as_str()).map(str::to_owned) else { return };
            let speed = g["speed"].as_str().unwrap_or_default().to_owned();
            let variant = g["variant"]["key"].as_str().unwrap_or("standard");
            let board = g["compat"]["board"].as_bool().unwrap_or_else(|| board_speed(&speed))
                && matches!(variant, "standard" | "fromPosition");
            let mut new = false;
            me.with_games(state, |games| {
                if !games.contains_key(&id) {
                    games.insert(id.clone(), Running { match_id: None, board, speed });
                    new = true;
                }
            });
            {
                let mut inner = me.inner.lock().unwrap();
                inner.challenges.retain(|c| c.id != id);
                if inner.seek.is_some() {
                    inner.seek_note = Some("Someone took your seek: the game has started.".into());
                }
                if let Some((_, cancel)) = inner.seek.take() {
                    cancel.cancel();
                }
            }
            if new && board {
                tokio::spawn(game::play(state.clone(), id));
            }
            me.announce(state);
        }
        "gameFinish" => {
            let g = &event["game"];
            let Some(id) = g["gameId"].as_str().or(g["id"].as_str()) else { return };
            // A game played here ends with its own stream; one played elsewhere ends now.
            let elsewhere = me.inner.lock().unwrap().games.get(id).is_some_and(|r| !r.board);
            if elsewhere {
                me.game_over(state, id);
            }
        }
        "challenge" => {
            let c = read_challenge(&event["challenge"], account);
            {
                let mut inner = me.inner.lock().unwrap();
                inner.challenges.retain(|x| x.id != c.id);
                inner.challenges.push(c.clone());
            }
            emit_challenge(state, &c, "created", None);
            me.announce(state);
        }
        kind @ ("challengeCanceled" | "challengeDeclined") => {
            let c = read_challenge(&event["challenge"], account);
            me.inner.lock().unwrap().challenges.retain(|x| x.id != c.id);
            let reason = event["challenge"]["declineReasonKey"].as_str();
            let what = if kind == "challengeCanceled" { "canceled" } else { "declined" };
            emit_challenge(state, &c, what, reason);
            me.announce(state);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Finding games
// ---------------------------------------------------------------------------------------------------------------------

fn need_client(state: &AppState) -> Result<Lichess, ApiError> {
    state.me.client().ok_or(ApiError::conflict("your Lichess account is not connected: log in first"))
}

/// Lichess's estimate of a game's length, which sets its speed: the initial clock plus 40 increments.
fn estimate(clock_seconds: u32, increment: u32) -> u32 {
    clock_seconds + 40 * increment
}

/// A clock a challenge can have: 0, 15, 30, 45 or 90 seconds or whole minutes up to 180, an increment up to 180
/// seconds (60 against the AI), and blitz or slower, which is what the Board API plays.
fn check_clock(clock_seconds: u32, increment: u32, max_increment: u32) -> Result<(), ApiError> {
    if !(matches!(clock_seconds, 0 | 15 | 30 | 45 | 90)
        || (clock_seconds.is_multiple_of(60) && clock_seconds <= 10_800))
    {
        return Err(ApiError::bad_request(
            "Lichess clocks start at 0, 15, 30, 45 or 90 seconds, or a whole number of minutes up to 180",
        ));
    }
    if increment > max_increment {
        return Err(ApiError::bad_request(&format!("the increment is at most {max_increment} seconds here")));
    }
    if estimate(clock_seconds, increment) < 180 {
        return Err(ApiError::bad_request(
            "bullet cannot be played through Lichess's Board API: choose blitz or slower (initial + 40 × increment \
             at least 3 minutes), or play bullet on lichess.org",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MeSeekRequest {
    /// The initial clock in seconds. A seek must be rapid or slower: initial + 40 × increment at least 8 minutes.
    pub clock_seconds: u32,
    /// Seconds added after every move.
    #[serde(default)]
    pub increment: u32,
    #[serde(default)]
    pub rated: bool,
    /// The colour you want. Default random.
    #[serde(default)]
    pub color: ChallengeColor,
    /// Opponents' ratings, such as `1500-1800`. Lichess advises leaving it empty.
    #[serde(default)]
    pub rating_range: Option<String>,
}

/// Opens a seek in the lobby. It stays open until someone takes it (the game starts on the event stream) or it is
/// cancelled.
pub async fn seek(state: &Arc<AppState>, req: MeSeekRequest) -> Result<MeStatus, ApiError> {
    let client = need_client(state)?;
    if state.me.inner.lock().unwrap().state != BotState::Online {
        return Err(ApiError::conflict("not connected to Lichess yet: try again in a moment"));
    }
    if state.me.inner.lock().unwrap().seek.is_some() {
        return Err(ApiError::conflict("a seek is already open: cancel it first"));
    }
    if req.clock_seconds > 180 * 60 || req.increment > 180 {
        return Err(ApiError::bad_request("a seek's clock is at most 180 minutes plus 180 seconds"));
    }
    if estimate(req.clock_seconds, req.increment) < 480 {
        return Err(ApiError::bad_request(
            "Lichess's Board API seeks only rapid and classical games (initial + 40 × increment at least 8 minutes): \
             for blitz, challenge someone directly",
        ));
    }
    let range = req.rating_range.as_deref().map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned);
    if let Some(r) = &range {
        let ok = r.split_once('-').is_some_and(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok());
        if !ok {
            return Err(ApiError::bad_request("a rating range looks like 1500-1800"));
        }
    }
    // Minutes, which Lichess takes as a decimal.
    let minutes = format!("{}", f64::from(req.clock_seconds) / 60.0);
    let mut form = vec![
        ("rated", req.rated.to_string()),
        ("time", minutes),
        ("increment", req.increment.to_string()),
        ("variant", "standard".to_owned()),
    ];
    if req.color != ChallengeColor::Random {
        form.push(("color", req.color.key().to_owned()));
    }
    if let Some(r) = &range {
        form.push(("ratingRange", r.clone()));
    }
    let mut stream = client.post_stream("/api/board/seek", &form).await.map_err(call_error)?;
    let cancel = CancellationToken::new();
    let seek = MeSeek {
        clock_seconds: req.clock_seconds,
        increment: req.increment,
        rated: req.rated,
        color: req.color,
        rating_range: range,
    };
    let id = {
        let mut inner = state.me.inner.lock().unwrap();
        inner.seek = Some((seek, cancel.clone()));
        inner.seek_id += 1;
        inner.seek_note = None;
        inner.seek_id
    };
    state.me.announce(state);
    // The seek lives as long as this connection: dropping it cancels the seek at Lichess.
    let state2 = state.clone();
    tokio::spawn(async move {
        let note = loop {
            tokio::select! {
                _ = cancel.cancelled() => break None,
                line = stream.next() => match line {
                    Ok(Some(_)) => continue,
                    Ok(None) => break Some("The seek ended without a game.".to_owned()),
                    Err(e) => break Some(format!("The seek was lost: {e:#}.")),
                },
            }
        };
        let me = &state2.me;
        {
            let mut inner = me.inner.lock().unwrap();
            // Cancelled (or taken: gameStart) has already cleared it, and a newer seek is not this one.
            if inner.seek.is_some() && inner.seek_id == id {
                inner.seek = None;
                inner.seek_note = note;
            }
        }
        me.announce(&state2);
    });
    Ok(state.me.status())
}

/// Cancels the open seek, if any.
pub async fn stop_seek(state: &Arc<AppState>) -> MeStatus {
    {
        let mut inner = state.me.inner.lock().unwrap();
        if let Some((_, cancel)) = inner.seek.take() {
            cancel.cancel();
            inner.seek_note = Some("Seek cancelled.".into());
        }
    }
    state.me.announce(state);
    state.me.status()
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MeChallengeRequest {
    /// A Lichess username to challenge. Give exactly one of `username`, `ai_level` and `my_bot`.
    #[serde(default)]
    pub username: Option<String>,
    /// Lichess's own AI (Stockfish) at level 1 to 8. The game starts at once, casual.
    #[serde(default)]
    pub ai_level: Option<u8>,
    /// Your own bot (the BOT account the runner plays with). It answers by its own settings.
    #[serde(default)]
    pub my_bot: bool,
    /// The initial clock in seconds: 0, 15, 30, 45, 90 or whole minutes up to 180. Blitz or slower (initial + 40 ×
    /// increment at least 3 minutes), as the Board API plays.
    pub clock_seconds: u32,
    #[serde(default)]
    pub increment: u32,
    #[serde(default)]
    pub rated: bool,
    /// The colour you play. Default random.
    #[serde(default)]
    pub color: ChallengeColor,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 30 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Challenges a player, your bot or Lichess's AI.
pub async fn challenge(state: &Arc<AppState>, req: MeChallengeRequest) -> Result<ChallengeSent, ApiError> {
    let client = need_client(state)?;
    let picked = usize::from(req.username.is_some()) + usize::from(req.ai_level.is_some()) + usize::from(req.my_bot);
    if picked != 1 {
        return Err(ApiError::bad_request("give exactly one of username, ai_level and my_bot"));
    }
    if let Some(level) = req.ai_level {
        if !(1..=8).contains(&level) {
            return Err(ApiError::bad_request("the AI's level is 1 to 8"));
        }
        if req.rated {
            return Err(ApiError::bad_request("games against the AI are casual"));
        }
        check_clock(req.clock_seconds, req.increment, 60)?;
        let form = [
            ("level", level.to_string()),
            ("clock.limit", req.clock_seconds.to_string()),
            ("clock.increment", req.increment.to_string()),
            ("color", req.color.key().to_owned()),
            ("variant", "standard".to_owned()),
        ];
        let v = client.post("/api/challenge/ai", &form).await.map_err(call_error)?;
        let id = v["id"].as_str().unwrap_or_default().to_owned();
        return Ok(ChallengeSent {
            url: format!("https://lichess.org/{id}"),
            id,
            username: format!("Stockfish level {level}"),
            rated: false,
            clock_seconds: req.clock_seconds,
            increment: req.increment,
            color: req.color,
        });
    }
    let username = match (&req.username, req.my_bot) {
        (Some(name), _) => name.trim().to_owned(),
        (None, _) => match state.lichess.account() {
            Some(bot) => bot.username,
            None => return Err(ApiError::conflict("the bot's account is not known: connect the bot first")),
        },
    };
    if !valid_name(&username) {
        return Err(ApiError::bad_request("that is not a Lichess username"));
    }
    if state.me.account().is_some_and(|a| a.id == username.to_lowercase()) {
        return Err(ApiError::bad_request("you cannot challenge yourself"));
    }
    check_clock(req.clock_seconds, req.increment, 180)?;
    let form = [
        ("rated", req.rated.to_string()),
        ("clock.limit", req.clock_seconds.to_string()),
        ("clock.increment", req.increment.to_string()),
        ("color", req.color.key().to_owned()),
        ("variant", "standard".to_owned()),
    ];
    let v = client.post(&format!("/api/challenge/{username}"), &form).await.map_err(call_error)?;
    let c = if v["challenge"].is_object() { &v["challenge"] } else { &v };
    let id = c["id"].as_str().unwrap_or_default().to_owned();
    Ok(ChallengeSent {
        url: c["url"].as_str().map(str::to_owned).unwrap_or_else(|| format!("https://lichess.org/{id}")),
        id,
        username,
        rated: req.rated,
        clock_seconds: req.clock_seconds,
        increment: req.increment,
        color: req.color,
    })
}

fn check_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.len() > 12 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::bad_request("that is not a Lichess challenge id"));
    }
    Ok(())
}

/// Accepts a challenge to you. The game starts on the event stream. 400 for one the Board API cannot play (bullet,
/// variants): accept that on lichess.org.
pub async fn accept(state: &Arc<AppState>, id: &str) -> Result<MeStatus, ApiError> {
    check_id(id)?;
    let client = need_client(state)?;
    let known = state.me.inner.lock().unwrap().challenges.iter().find(|c| c.id == id).cloned();
    if let Some(c) = &known
        && !c.playable
    {
        return Err(ApiError::bad_request(&format!(
            "a {} {} game cannot be played here through the Board API: accept it on lichess.org",
            c.speed, c.variant
        )));
    }
    client.post(&format!("/api/challenge/{id}/accept"), &[]).await.map_err(call_error)?;
    state.me.inner.lock().unwrap().challenges.retain(|c| c.id != id);
    if let Some(c) = &known {
        emit_challenge(state, c, "accepted", None);
    }
    state.me.announce(state);
    Ok(state.me.status())
}

/// Lichess's decline reasons.
const REASONS: &[&str] = &[
    "generic",
    "later",
    "tooFast",
    "tooSlow",
    "timeControl",
    "rated",
    "casual",
    "standard",
    "variant",
    "noBot",
    "onlyBot",
];

/// Declines a challenge to you, with one of Lichess's reasons (default `generic`).
pub async fn decline(state: &Arc<AppState>, id: &str, reason: Option<String>) -> Result<MeStatus, ApiError> {
    check_id(id)?;
    let client = need_client(state)?;
    let reason = reason.unwrap_or_else(|| "generic".into());
    if !REASONS.contains(&reason.as_str()) {
        return Err(ApiError::bad_request(&format!("the reason is one of {}", REASONS.join(", "))));
    }
    client.post(&format!("/api/challenge/{id}/decline"), &[("reason", reason.clone())]).await.map_err(call_error)?;
    let known = {
        let mut inner = state.me.inner.lock().unwrap();
        let known = inner.challenges.iter().find(|c| c.id == id).cloned();
        inner.challenges.retain(|c| c.id != id);
        known
    };
    if let Some(c) = &known {
        emit_challenge(state, c, "declined", Some(&reason));
    }
    state.me.announce(state);
    Ok(state.me.status())
}

/// Withdraws a challenge of yours.
pub async fn cancel_challenge(state: &Arc<AppState>, id: &str) -> Result<MeStatus, ApiError> {
    check_id(id)?;
    let client = need_client(state)?;
    client.post(&format!("/api/challenge/{id}/cancel"), &[]).await.map_err(call_error)?;
    state.me.inner.lock().unwrap().challenges.retain(|c| c.id != id);
    state.me.announce(state);
    Ok(state.me.status())
}
