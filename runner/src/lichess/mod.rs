//! The Lichess bot: one of our builds plays as the user's Lichess BOT account through the Bot API
//! (<https://lichess.org/api#tag/Bot>). The bot follows the account's event stream, takes the challenges its settings
//! allow and declines the rest, and plays every game that starts, each in its own task with its own engine. Every game
//! is recorded as a [`Match`](crate::matches::Match), so the board shows it live and replays it later.
//!
//! The user makes the account and its token (scope `bot:play`) on lichess.org. The token is kept in the data
//! directory in the file `lichess-token`, readable by the user alone where the system allows it, and no route ever
//! returns it. The settings, and whether the bot was online, are rows of the `meta` table, so a restarted runner
//! comes back online by itself.

pub mod client;
mod game;
pub mod rules;
#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

pub use self::game::{Brains, engines};
use self::{
    client::{CallError, Lichess},
    rules::{Challenge, Settings},
};
use crate::{
    api::{ApiError, AppState},
    engine::build::{self, Build},
};

/// The Lichess account the token belongs to.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct Account {
    /// The lower-case id, as games name players.
    pub id: String,
    pub username: String,
    pub title: Option<String>,
    /// Whether it is a BOT account, the only kind the Bot API plays with.
    pub bot: bool,
}

/// Where the bot's connection to Lichess stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum BotState {
    /// Not switched on.
    Off,
    /// Switched on, opening the event stream.
    Connecting,
    /// Following the event stream: challenges are answered and games played.
    Online,
    /// Switched on but failing; it keeps trying, and `error` says what went wrong.
    Error,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct LichessStatus {
    /// Whether a token is stored.
    pub configured: bool,
    /// The account the token belongs to, once Lichess has said.
    pub account: Option<Account>,
    /// Whether the bot is switched on. It stays on across restarts until stopped.
    pub online: bool,
    pub state: BotState,
    /// What went wrong last, while `state` is `error`.
    pub error: Option<String>,
    /// The matches of the games being played now.
    pub playing: Vec<i64>,
    pub settings: Settings,
}

struct Inner {
    token: Option<String>,
    account: Option<Account>,
    settings: Settings,
    online: bool,
    state: BotState,
    error: Option<String>,
    /// Stops the event-stream loop.
    run: Option<CancellationToken>,
    /// The games being played, by Lichess id, with their records once they have one.
    games: HashMap<String, Option<i64>>,
    /// Challenges accepted whose games have not started yet: they count against `max_games`.
    accepted: HashSet<String>,
}

pub struct Bot {
    base: String,
    http: reqwest::Client,
    brains: Brains,
    inner: Mutex<Inner>,
}

impl Bot {
    /// The bot for the Lichess at `base` (`LICHESS_URL`, https://lichess.org by default), with engines started by
    /// `brains`.
    pub fn new(base: &str, brains: Brains) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            brains,
            inner: Mutex::new(Inner {
                token: None,
                account: None,
                settings: Settings::default(),
                online: false,
                state: BotState::Off,
                error: None,
                run: None,
                games: HashMap::new(),
                accepted: HashSet::new(),
            }),
        }
    }

    /// A client with the stored token, or `None` without one.
    fn client(&self) -> Option<Lichess> {
        let inner = self.inner.lock().unwrap();
        let token = inner.token.as_ref()?;
        Some(Lichess::new(self.http.clone(), &self.base, token, inner.account.as_ref().map(|a| a.username.as_str())))
    }

    fn account(&self) -> Option<Account> {
        self.inner.lock().unwrap().account.clone()
    }

    fn settings(&self) -> Settings {
        self.inner.lock().unwrap().settings.clone()
    }

    pub fn status(&self) -> LichessStatus {
        let inner = self.inner.lock().unwrap();
        let mut playing: Vec<i64> = inner.games.values().filter_map(|g| *g).collect();
        playing.sort_unstable();
        LichessStatus {
            configured: inner.token.is_some(),
            account: inner.account.clone(),
            online: inner.online,
            state: inner.state,
            error: inner.error.clone(),
            playing,
            settings: inner.settings.clone(),
        }
    }

    fn announce(&self, state: &AppState) {
        state.events.emit("lichess.status", serde_json::to_value(self.status()).unwrap_or_default());
    }

    /// Moves the connection to `to`, announcing it when that is a change.
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

    fn game_recorded(&self, state: &AppState, id: &str, match_id: i64) {
        if let Some(slot) = self.inner.lock().unwrap().games.get_mut(id) {
            *slot = Some(match_id);
        }
        self.announce(state);
    }

    fn game_over(&self, state: &AppState, id: &str) {
        self.inner.lock().unwrap().games.remove(id);
        self.announce(state);
    }

    /// How many games are running or about to.
    fn busy(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.games.len() + inner.accepted.len()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------------------------------------------------

const SETTINGS_KEY: &str = "lichess.settings";
const ONLINE_KEY: &str = "lichess.online";

pub fn token_path(data: &Path) -> PathBuf {
    data.join("lichess-token")
}

/// Writes the token where only this user can read it: mode 0600 on Unix. On Windows the per-user data folder is
/// already private to the user.
fn write_token(path: &Path, token: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt, os::unix::fs::PermissionsExt};
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        // A file that was already there keeps its mode on open, so it is set again.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(token.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, token)?;
    Ok(())
}

async fn meta_get(state: &AppState, key: &'static str) -> Result<Option<String>> {
    state
        .store
        .call(move |c| Ok(c.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?))
        .await
}

async fn meta_set(state: &AppState, key: &'static str, value: String) -> Result<()> {
    state
        .store
        .call(move |c| {
            c.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [key, &value],
            )?;
            Ok(())
        })
        .await
}

/// The build a setting names: an id, or the newest build of a ref (or of a commit prefix).
pub async fn resolve_build(state: &AppState, spec: &str) -> Result<Option<Build>> {
    let spec = spec.to_owned();
    state
        .store
        .call(move |c| {
            if let Ok(id) = spec.parse::<i64>() {
                return build::get(c, id);
            }
            Ok(build::list(c)?.into_iter().find(|b| b.refname == spec || b.commit.starts_with(&spec)))
        })
        .await
}

// ---------------------------------------------------------------------------------------------------------------------
// Account and token
// ---------------------------------------------------------------------------------------------------------------------

fn call_error(e: CallError) -> ApiError {
    match e {
        CallError::RateLimited => ApiError::too_many_requests(&e.to_string()),
        CallError::Status { status: 400..=499, .. } => ApiError::bad_request(&e.to_string()),
        e => ApiError::bad_gateway(&e.to_string()),
    }
}

fn read_account(v: &Value) -> Account {
    let title = v["title"].as_str().map(str::to_owned);
    Account {
        id: v["id"].as_str().unwrap_or_default().to_owned(),
        username: v["username"].as_str().unwrap_or_default().to_owned(),
        bot: title.as_deref() == Some("BOT"),
        title,
    }
}

/// Asks Lichess whose the token is.
async fn fetch_account(client: &Lichess) -> Result<Account, CallError> {
    Ok(read_account(&client.get("/api/account").await?))
}

/// Picks up where the last run left off: the token, the settings, and, if the bot was online, the bot.
pub async fn resume(state: &Arc<AppState>) -> Result<()> {
    let token = std::fs::read_to_string(token_path(&state.config.data)).ok().map(|t| t.trim().to_owned());
    let settings = match meta_get(state, SETTINGS_KEY).await? {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => Settings::default(),
    };
    let online = meta_get(state, ONLINE_KEY).await?.as_deref() == Some("true");
    {
        let mut inner = state.lichess.inner.lock().unwrap();
        inner.token = token.filter(|t| !t.is_empty());
        inner.settings = settings;
        inner.online = online && inner.token.is_some();
    }
    if state.lichess.inner.lock().unwrap().online {
        spawn_loop(state);
    } else if let Some(client) = state.lichess.client() {
        // Not online, but the status should still name the account.
        let state = state.clone();
        tokio::spawn(async move {
            if let Ok(account) = fetch_account(&client).await {
                state.lichess.inner.lock().unwrap().account = Some(account);
            }
        });
    }
    Ok(())
}

/// Stores a token after Lichess has accepted it, and reports the account it belongs to.
pub async fn set_token(state: &Arc<AppState>, token: &str) -> Result<LichessStatus, ApiError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ApiError::bad_request("the token is empty"));
    }
    let client = Lichess::new(state.lichess.http.clone(), &state.lichess.base, token, None);
    let account = fetch_account(&client).await.map_err(|e| match e {
        CallError::Unauthorized => {
            ApiError::bad_request("Lichess rejected that token: make one with the bot:play scope on lichess.org")
        }
        e => call_error(e),
    })?;
    write_token(&token_path(&state.config.data), token).context("store the Lichess token")?;
    let restart = {
        let mut inner = state.lichess.inner.lock().unwrap();
        inner.token = Some(token.to_owned());
        inner.account = Some(account);
        inner.run.take().inspect(CancellationToken::cancel).is_some() || inner.online
    };
    if restart {
        spawn_loop(state);
    }
    state.lichess.announce(state);
    Ok(state.lichess.status())
}

/// Forgets the token and takes the bot offline. Games in progress are played out with the token they started with.
pub async fn clear_token(state: &Arc<AppState>) -> Result<LichessStatus, ApiError> {
    {
        let mut inner = state.lichess.inner.lock().unwrap();
        if let Some(run) = inner.run.take() {
            run.cancel();
        }
        inner.token = None;
        inner.account = None;
        inner.online = false;
    }
    meta_set(state, ONLINE_KEY, "false".into()).await?;
    match std::fs::remove_file(token_path(&state.config.data)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(anyhow::Error::from(e).context("remove the Lichess token").into());
        }
        _ => {}
    }
    state.lichess.set_state(state, BotState::Off, None);
    Ok(state.lichess.status())
}

fn need_client(state: &AppState) -> Result<Lichess, ApiError> {
    state.lichess.client().ok_or(ApiError::conflict("no Lichess token: set one with `chess lichess token`"))
}

/// The account, asked of Lichess when it is not known yet.
async fn need_account(state: &AppState) -> Result<Account, ApiError> {
    if let Some(account) = state.lichess.account() {
        return Ok(account);
    }
    let account = fetch_account(&need_client(state)?).await.map_err(call_error)?;
    state.lichess.inner.lock().unwrap().account = Some(account.clone());
    Ok(account)
}

/// Turns the account into a BOT account. Lichess does this once and for good, and only for an account that has never
/// played a game.
pub async fn upgrade(state: &Arc<AppState>) -> Result<LichessStatus, ApiError> {
    let client = need_client(state)?;
    if need_account(state).await?.bot {
        return Err(ApiError::conflict("this is already a BOT account"));
    }
    client.post("/api/bot/account/upgrade", &[]).await.map_err(call_error)?;
    let account = fetch_account(&client).await.map_err(call_error)?;
    state.lichess.inner.lock().unwrap().account = Some(account);
    state.lichess.announce(state);
    Ok(state.lichess.status())
}

// ---------------------------------------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------------------------------------

/// Replaces the settings. The build, when given, must exist now; a ref is still resolved afresh at each game.
pub async fn put_settings(state: &Arc<AppState>, settings: Settings) -> Result<Settings, ApiError> {
    let settings = settings.checked().map_err(|e| ApiError::bad_request(&e))?;
    if let Some(spec) = &settings.build
        && resolve_build(state, spec).await?.is_none()
    {
        return Err(ApiError::bad_request(&format!("no build of {spec}: build it first with `chess build {spec}`")));
    }
    meta_set(state, SETTINGS_KEY, serde_json::to_string(&settings).map_err(anyhow::Error::from)?).await?;
    state.lichess.inner.lock().unwrap().settings = settings.clone();
    state.lichess.announce(state);
    Ok(settings)
}

// ---------------------------------------------------------------------------------------------------------------------
// Online and offline
// ---------------------------------------------------------------------------------------------------------------------

/// Takes the bot online: it follows the event stream until stopped, and comes back by itself after a restart.
pub async fn start(state: &Arc<AppState>) -> Result<LichessStatus, ApiError> {
    need_client(state)?;
    let account = need_account(state).await?;
    if !account.bot {
        return Err(ApiError::conflict(&format!(
            "{} is not a BOT account: upgrade it first (POST /api/lichess/upgrade), which cannot be undone",
            account.username
        )));
    }
    let settings = state.lichess.settings();
    let playable = match &settings.build {
        Some(spec) => resolve_build(state, spec).await?.is_some(),
        None => false,
    };
    if !playable {
        return Err(ApiError::conflict("choose a build that exists first (`chess lichess settings --build <ref>`)"));
    }
    meta_set(state, ONLINE_KEY, "true".into()).await?;
    let running = {
        let mut inner = state.lichess.inner.lock().unwrap();
        inner.online = true;
        inner.run.is_some()
    };
    if !running {
        spawn_loop(state);
    }
    Ok(state.lichess.status())
}

/// Takes the bot offline: no more challenges are answered. Games in progress are played out.
pub async fn stop(state: &Arc<AppState>) -> Result<LichessStatus, ApiError> {
    {
        let mut inner = state.lichess.inner.lock().unwrap();
        inner.online = false;
        if let Some(run) = inner.run.take() {
            run.cancel();
        }
    }
    meta_set(state, ONLINE_KEY, "false".into()).await?;
    state.lichess.set_state(state, BotState::Off, None);
    Ok(state.lichess.status())
}

fn spawn_loop(state: &Arc<AppState>) {
    let cancel = CancellationToken::new();
    state.lichess.inner.lock().unwrap().run = Some(cancel.clone());
    tokio::spawn(run(state.clone(), cancel));
}

/// Follows the event stream until cancelled, reconnecting with a growing pause, and a minute's when Lichess asks.
async fn run(state: Arc<AppState>, cancel: CancellationToken) {
    let bot = &state.lichess;
    let mut backoff = Duration::from_secs(1);
    while !cancel.is_cancelled() {
        bot.set_state(&state, BotState::Connecting, None);
        let wait = match follow(&state, &cancel, &mut backoff).await {
            Ok(()) => backoff,
            Err(e) => {
                let limited = matches!(e.downcast_ref::<CallError>(), Some(CallError::RateLimited));
                tracing::warn!("lichess: {e:#}");
                if !cancel.is_cancelled() {
                    bot.set_state(&state, BotState::Error, Some(format!("{e:#}")));
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
    // A loop replaced by a newer one (a new token) leaves the state to it.
    let current = bot.inner.lock().unwrap().run.as_ref().is_some_and(|r| !r.is_cancelled());
    if !current {
        bot.set_state(&state, BotState::Off, None);
    }
}

async fn follow(state: &Arc<AppState>, cancel: &CancellationToken, backoff: &mut Duration) -> Result<()> {
    let bot = &state.lichess;
    let client = bot.client().context("no Lichess token")?;
    let me = match bot.account() {
        Some(a) => a,
        None => {
            let a = fetch_account(&client).await?;
            bot.inner.lock().unwrap().account = Some(a.clone());
            a
        }
    };
    if !me.bot {
        anyhow::bail!("{} is not a BOT account", me.username);
    }
    // With the username known, the client says whose bot this is.
    let client = bot.client().context("no Lichess token")?;
    let mut events = client.stream("/api/stream/event").await?;
    bot.set_state(state, BotState::Online, None);
    *backoff = Duration::from_secs(1);
    loop {
        let event = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            e = events.next() => e?,
        };
        let Some(event) = event else { return Ok(()) };
        if let Err(e) = handle(state, &client, &me, &event).await {
            tracing::warn!("lichess: {e:#}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------------------------------------------------

fn user_json(u: &Option<rules::LichessUser>) -> Value {
    match u {
        Some(u) => json!({ "name": u.name, "title": u.title, "rating": u.rating }),
        None => Value::Null,
    }
}

fn challenge_json(c: &Challenge, direction: &str, decision: &str, reason: Option<&str>) -> Value {
    let clock = match (&c.time_control.limit, &c.time_control.increment) {
        (Some(limit), Some(increment)) if c.time_control.kind == "clock" => {
            json!({ "initial": limit, "increment": increment })
        }
        _ => Value::Null,
    };
    json!({
        "id": c.id,
        "direction": direction,
        "from": user_json(&c.challenger),
        "to": user_json(&c.dest_user),
        "rated": c.rated,
        "speed": c.speed,
        "variant": c.variant.key,
        "clock": clock,
        "decision": decision,
        "reason": reason,
    })
}

async fn handle(state: &Arc<AppState>, client: &Lichess, me: &Account, event: &Value) -> Result<()> {
    let bot = &state.lichess;
    match event["type"].as_str().unwrap_or_default() {
        "challenge" => {
            let c: Challenge = serde_json::from_value(event["challenge"].clone()).context("read a challenge")?;
            // Our own challenges to others come through the stream as well.
            if c.challenger.as_ref().is_some_and(|u| u.id == me.id) {
                return Ok(());
            }
            let settings = bot.settings();
            let playable = match &settings.build {
                Some(spec) => resolve_build(state, spec).await?.is_some(),
                None => false,
            };
            match rules::decide(&c, &settings, bot.busy(), playable) {
                Ok(()) => {
                    match client.post(&format!("/api/challenge/{}/accept", c.id), &[]).await {
                        Ok(_) => {
                            bot.inner.lock().unwrap().accepted.insert(c.id.clone());
                            state.events.emit("lichess.challenge", challenge_json(&c, "incoming", "accepted", None));
                        }
                        // Withdrawn meanwhile, most likely; `challengeCanceled` says so.
                        Err(e) => tracing::info!("lichess: could not accept challenge {}: {e}", c.id),
                    }
                }
                Err(reason) => {
                    let form = [("reason", reason.key().to_owned())];
                    if let Err(e) = client.post(&format!("/api/challenge/{}/decline", c.id), &form).await {
                        tracing::info!("lichess: could not decline challenge {}: {e}", c.id);
                    }
                    state
                        .events
                        .emit("lichess.challenge", challenge_json(&c, "incoming", "declined", Some(reason.key())));
                }
            }
        }
        "challengeCanceled" => {
            let c: Challenge = serde_json::from_value(event["challenge"].clone()).unwrap_or_default();
            bot.inner.lock().unwrap().accepted.remove(&c.id);
            let direction = if c.challenger.as_ref().is_some_and(|u| u.id == me.id) { "outgoing" } else { "incoming" };
            state.events.emit("lichess.challenge", challenge_json(&c, direction, "canceled", None));
        }
        "challengeDeclined" => {
            // Someone declined one of ours.
            let c: Challenge = serde_json::from_value(event["challenge"].clone()).unwrap_or_default();
            let reason = event["challenge"]["declineReasonKey"].as_str().map(str::to_owned);
            state.events.emit("lichess.challenge", challenge_json(&c, "outgoing", "declined", reason.as_deref()));
        }
        "gameStart" => {
            let game = &event["game"];
            let Some(id) = game["gameId"].as_str().or(game["id"].as_str()).map(str::to_owned) else {
                return Ok(());
            };
            let cancel = {
                let mut inner = bot.inner.lock().unwrap();
                inner.accepted.remove(&id);
                if inner.games.contains_key(&id) {
                    return Ok(());
                }
                inner.games.insert(id.clone(), None);
                // What `DELETE /api/matches/{id}` triggers: the game's task resigns.
                CancellationToken::new()
            };
            tokio::spawn(game::play(state.clone(), id, cancel));
        }
        "gameFinish" => {
            // The game's own stream ends its task; only a challenge that never became a game is left to forget.
            if let Some(id) = event["game"]["gameId"].as_str().or(event["game"]["id"].as_str()) {
                bot.inner.lock().unwrap().accepted.remove(id);
            }
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Challenging others
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChallengeColor {
    #[default]
    Random,
    White,
    Black,
}

impl ChallengeColor {
    fn key(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct ChallengeRequest {
    /// Who to challenge: a Lichess username, usually another bot's.
    pub username: String,
    /// The initial clock in seconds. Lichess takes 0, 15, 30, 45, 60, 90, or a whole number of minutes.
    pub clock_seconds: u32,
    /// Seconds added after every move.
    #[serde(default)]
    pub increment: u32,
    #[serde(default)]
    pub rated: bool,
    /// The colour our build plays. Default random.
    #[serde(default)]
    pub color: ChallengeColor,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ChallengeSent {
    /// The challenge id, which becomes the game id if it is accepted.
    pub id: String,
    pub url: String,
    pub username: String,
    pub rated: bool,
    pub clock_seconds: u32,
    pub increment: u32,
    pub color: ChallengeColor,
}

/// Challenges a player. The game, if accepted, starts on the event stream, so the bot must be online.
pub async fn challenge(state: &Arc<AppState>, req: ChallengeRequest) -> Result<ChallengeSent, ApiError> {
    let client = need_client(state)?;
    if !state.lichess.inner.lock().unwrap().online {
        return Err(ApiError::conflict("the bot is offline: start it first, or nobody will play the game"));
    }
    let username = req.username.trim().to_owned();
    if username.is_empty() || !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(ApiError::bad_request("that is not a Lichess username"));
    }
    if req.clock_seconds == 0 && req.increment == 0 {
        return Err(ApiError::bad_request("a game needs some time on the clock"));
    }
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

/// A bot's rating at each speed it has played.
#[derive(Clone, Debug, Default, PartialEq, Serialize, ToSchema)]
pub struct Ratings {
    pub bullet: Option<u32>,
    pub blitz: Option<u32>,
    pub rapid: Option<u32>,
    pub classical: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct OnlineBot {
    pub name: String,
    pub title: Option<String>,
    pub ratings: Ratings,
}

fn read_bot(v: &Value) -> OnlineBot {
    let rating = |speed: &str| v["perfs"][speed]["rating"].as_u64().map(|r| r as u32);
    OnlineBot {
        name: v["username"].as_str().or(v["id"].as_str()).unwrap_or_default().to_owned(),
        title: v["title"].as_str().map(str::to_owned),
        ratings: Ratings {
            bullet: rating("bullet"),
            blitz: rating("blitz"),
            rapid: rating("rapid"),
            classical: rating("classical"),
        },
    }
}

/// The bots online on Lichess now, up to fifty: who there is to challenge.
pub async fn online_bots(state: &Arc<AppState>) -> Result<Vec<OnlineBot>, ApiError> {
    let client = need_client(state)?;
    let lines = client.get_lines("/api/bot/online?nb=50").await.map_err(call_error)?;
    Ok(lines.iter().map(read_bot).collect())
}
