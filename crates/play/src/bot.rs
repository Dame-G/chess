//! Dameless on Lichess: a version of the engine plays as the user's BOT account through the Bot API. While online
//! (kept across restarts), the bot follows the account's event stream, takes the challenges its settings allow
//! (`core::lichess::bot`) and declines the rest, looks for games by challenging other bots when asked
//! (`core::lichess::seek`), keeps to Lichess's daily limit of 100 games against bots (`core::lichess::quota`), and
//! plays every game that starts with its own engine, resigning when the engine fails and claiming the game when the
//! opponent leaves.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use dameless_core::{
    ConnectionKey, Error, GameId, PlayerKey, Side, Timestamp,
    events::Payload,
    game::Seat,
    lichess::{
        Account, BotSettings, BotStatus, ChallengeEvent, ChallengeEventKind, ConnectionState, ConnectionStatus, Quota,
        SeekMode,
        bot::{self as rules, Decline, Offer},
        quota,
        seek::{self, OnlineBot, Outcome, Retry, SeekChallenge, SeekOpponent, Seeking, Situation, Step},
    },
};
use dameless_hub::Hub;
use dameless_lichess::{
    Error as LichessError, Lichess,
    challenge::{ChallengeRequest as LichessChallenge, DeclineReason},
    events::{Challenge as LichessChallengeInfo, Event},
    now_ms,
};
use dameless_store::ConnectionField;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{
    binaries::Binaries,
    clients::{Clients, account_of, challenge_of, clock_seconds, lichess_color},
    engine_seat::{EngineOptions, EngineSeat},
    error,
    games::{Games, random_below},
    lichess_game::{self, Api, LichessBot, LichessGame},
    timing::Timing,
    you::{ChallengeBody, ChallengeSent},
};

/// Where the bot's daily count of games against bots is kept between runs.
const QUOTA_KEY: &str = "lichess.bot.quota";

/// Why a bot's challenge is declined while the daily limit on bot games is reached.
const LIMIT_DETAIL: &str = "Played 100 games against bots today";

/// What is kept of the bot's connection between runs (the token is a file, the settings their own column).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Kept {
    #[serde(default)]
    online: bool,
    #[serde(default)]
    seek: SeekMode,
}

/// A game the bot is playing.
struct Playing {
    game: Option<GameId>,
    detach: CancellationToken,
}

#[derive(Default)]
struct State {
    account: Option<Account>,
    conn: ConnectionState,
    error: Option<String>,
    settings: BotSettings,
    online: bool,
    run: Option<CancellationToken>,
    games: HashMap<String, Playing>,
    /// Challenges accepted whose games have not started: whether from a bot, and when.
    accepted: HashMap<String, (bool, Instant)>,
    seeking: Seeking,
    seek_task: bool,
    quota: Quota,
}

impl State {
    /// Games running or about to.
    fn busy(&mut self, accepted_for: std::time::Duration) -> usize {
        self.accepted.retain(|_, (_, at)| at.elapsed() < accepted_for);
        self.games.len() + self.accepted.len()
    }
}

struct Inner {
    hub: Arc<Hub>,
    clients: Arc<Clients>,
    games: Games,
    binaries: Arc<dyn Binaries>,
    timing: Timing,
    state: Mutex<State>,
    /// Wakes the search for games when something it waits on may have changed.
    wake: Notify,
}

/// The bot's connection. Cheap to clone.
#[derive(Clone)]
pub struct Bot {
    inner: Arc<Inner>,
}

/// `14:32 UTC`: the time of day of `ms`, as the seek note tells it.
fn clock_text(ms: i64) -> String {
    let t = Timestamp::from_unix_ms(ms);
    format!("{} UTC", t.0.get(11..16).unwrap_or_default())
}

impl Bot {
    pub(crate) fn new(hub: Arc<Hub>, clients: Arc<Clients>, games: Games) -> Self {
        let timing = games.timing().clone();
        let binaries = games.binaries().clone();
        let mut state = State::default();
        state.seeking.timing = timing.seek;
        Self {
            inner: Arc::new(Inner {
                hub,
                clients,
                games,
                binaries,
                timing,
                state: Mutex::new(state),
                wake: Notify::new(),
            }),
        }
    }

    fn client(&self) -> Option<Lichess> {
        self.inner.clients.get(ConnectionKey::Bot)
    }

    fn need_client(&self) -> Result<Lichess, Error> {
        self.client().ok_or_else(|| Error::conflict("the bot has no Lichess token: set one first"))
    }

    pub fn account(&self) -> Option<Account> {
        self.inner.state.lock().unwrap().account.clone()
    }

    pub fn settings(&self) -> BotSettings {
        self.inner.state.lock().unwrap().settings.clone()
    }

    fn accepted_for(&self) -> std::time::Duration {
        self.inner.timing.accepted_for
    }

    /// The connection as the app shows it.
    pub fn status(&self) -> ConnectionStatus {
        let configured = self.client().is_some();
        let s = self.inner.state.lock().unwrap();
        let mut playing: Vec<GameId> = s.games.values().filter_map(|g| g.game).collect();
        playing.sort();
        let now = now_ms();
        ConnectionStatus {
            key: ConnectionKey::Bot,
            configured,
            account: s.account.clone(),
            state: s.conn,
            error: s.error.clone(),
            playing,
            challenges: Vec::new(),
            you: None,
            bot: Some(BotStatus {
                online: s.online,
                settings: s.settings.clone(),
                seek: s.seeking.status(),
                bot_games_today: s.quota.today(now),
                bot_limit_until: s.quota.until(now).map(Timestamp::from_unix_ms),
            }),
        }
    }

    /// Keeps the connection's row current, tells whoever follows, and wakes the search, which may wait on it.
    async fn announce(&self) {
        let status = self.status();
        let (kept, quota) = {
            let s = self.inner.state.lock().unwrap();
            let seek = if s.seeking.mode == SeekMode::Always { SeekMode::Always } else { SeekMode::Off };
            (Kept { online: s.online, seek }, s.quota.clone())
        };
        let store = &self.inner.hub.store;
        if let Err(e) =
            store.put_connection_field(ConnectionKey::Bot, ConnectionField::Account, status.account.as_ref()).await
        {
            tracing::warn!("lichess (bot): could not store the connection: {e}");
        }
        let _ = store.put_connection_field(ConnectionKey::Bot, ConnectionField::State, Some(&kept)).await;
        let _ = store.put_meta(QUOTA_KEY, &quota).await;
        self.inner.hub.bus.emit(Payload::ConnectionUpserted(Box::new(status)));
        self.inner.wake.notify_one();
    }

    async fn set_conn(&self, to: ConnectionState, error: Option<String>) {
        let changed = {
            let mut s = self.inner.state.lock().unwrap();
            let changed = s.conn != to || s.error != error;
            s.conn = to;
            s.error = error;
            changed
        };
        if changed {
            self.announce().await;
        }
    }

    /// Picks up where the last run left off: the settings, the daily count, and, if the bot was online (and
    /// seeking always), the bot.
    pub(crate) async fn resume(&self) {
        let store = &self.inner.hub.store;
        let settings: Option<BotSettings> =
            store.connection_field(ConnectionKey::Bot, ConnectionField::Settings).await.ok().flatten();
        let kept: Kept =
            store.connection_field(ConnectionKey::Bot, ConnectionField::State).await.ok().flatten().unwrap_or_default();
        let account: Option<Account> =
            store.connection_field(ConnectionKey::Bot, ConnectionField::Account).await.ok().flatten();
        let quota: Quota = store.meta(QUOTA_KEY).await.ok().flatten().unwrap_or_default();
        let online = kept.online && self.client().is_some();
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(settings) = settings {
                s.settings = settings.checked().unwrap_or_default();
            }
            s.account = account;
            s.quota = quota;
            s.online = online;
            if online && kept.seek == SeekMode::Always {
                s.seeking.mode = SeekMode::Always;
                s.seeking.note = Some("Looking for a bot to play.".into());
            }
        }
        if online {
            self.connect();
            self.ensure_seek_task();
        }
    }

    /// A token was stored: the account is known, and the bot follows it again if it was online.
    pub(crate) async fn connected(&self, account: Account) {
        let online = {
            let mut s = self.inner.state.lock().unwrap();
            s.account = Some(account);
            s.online
        };
        if online {
            self.connect();
        }
        self.announce().await;
    }

    /// The token was forgotten: offline, not seeking. Games in progress stop being followed here.
    pub(crate) async fn disconnected(&self) {
        self.halt_seek().await;
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(run) = s.run.take() {
                run.cancel();
            }
            for g in s.games.values() {
                g.detach.cancel();
            }
            s.games.clear();
            s.accepted.clear();
            s.online = false;
            s.account = None;
        }
        self.set_conn(ConnectionState::Off, None).await;
        self.announce().await;
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Settings, online
    // -----------------------------------------------------------------------------------------------------------------

    /// Replaces the settings. The version, when given, must be one there is (it is resolved afresh at each game, so
    /// `champion` follows the champion).
    pub async fn set_settings(&self, settings: BotSettings) -> Result<BotSettings, Error> {
        let settings = settings.checked().map_err(Error::bad_request)?;
        if let Some(version) = &settings.version {
            self.inner.binaries.resolve(version).await?;
        }
        self.inner
            .hub
            .store
            .put_connection_field(ConnectionKey::Bot, ConnectionField::Settings, Some(&settings))
            .await?;
        self.inner.state.lock().unwrap().settings = settings.clone();
        self.announce().await;
        Ok(settings)
    }

    /// Whether a version is chosen and there is such a version.
    async fn playable(&self) -> bool {
        match self.settings().version {
            Some(v) => self.inner.binaries.resolve(&v).await.is_ok(),
            None => false,
        }
    }

    /// Takes the bot online (it follows the event stream until stopped, and comes back by itself after a restart) or
    /// offline (no more challenges answered or sent; games in progress are played out).
    pub async fn set_online(&self, online: bool) -> Result<ConnectionStatus, Error> {
        if !online {
            self.halt_seek().await;
            {
                let mut s = self.inner.state.lock().unwrap();
                s.online = false;
                if let Some(run) = s.run.take() {
                    run.cancel();
                }
            }
            self.set_conn(ConnectionState::Off, None).await;
            self.announce().await;
            return Ok(self.status());
        }
        let client = self.need_client()?;
        let account = match self.account() {
            Some(a) => a,
            None => account_of(&client.account().await.map_err(error::lichess)?),
        };
        if !account.is_bot() {
            return Err(Error::conflict(format!(
                "{} is not a BOT account: upgrade it on Lichess first",
                account.username
            )));
        }
        if !self.playable().await {
            return Err(Error::conflict("choose a version for the bot to play first"));
        }
        let running = {
            let mut s = self.inner.state.lock().unwrap();
            s.account = Some(account);
            s.online = true;
            s.run.is_some()
        };
        if !running {
            self.connect();
        }
        self.announce().await;
        Ok(self.status())
    }

    fn connect(&self) {
        let cancel = CancellationToken::new();
        if let Some(old) = self.inner.state.lock().unwrap().run.replace(cancel.clone()) {
            old.cancel();
        }
        let me = self.clone();
        tokio::spawn(async move { me.run(cancel).await });
    }

    async fn run(&self, cancel: CancellationToken) {
        let mut backoff = self.inner.timing.backoff();
        while !cancel.is_cancelled() {
            self.set_conn(ConnectionState::Connecting, None).await;
            let wait = match self.follow(&cancel, &mut backoff).await {
                Ok(()) => backoff.after(&LichessError::Unreachable("the stream ended".into())),
                Err(e) => {
                    tracing::warn!("lichess (bot): {e}");
                    if !cancel.is_cancelled() {
                        self.set_conn(ConnectionState::Error, Some(e.to_string())).await;
                    }
                    backoff.after(&e)
                }
            };
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }

    async fn follow(
        &self,
        cancel: &CancellationToken,
        backoff: &mut dameless_lichess::Backoff,
    ) -> Result<(), LichessError> {
        let Some(client) = self.client() else { return Err(LichessError::NoToken) };
        let account = account_of(&client.account().await?);
        if !account.is_bot() {
            return Err(LichessError::Unexpected(format!("{} is not a BOT account", account.username)));
        }
        client.set_username(&account.username);
        self.inner.state.lock().unwrap().account = Some(account.clone());
        let mut events = client.events().await?;
        self.reconcile(&client).await;
        self.set_conn(ConnectionState::Online, None).await;
        backoff.reset();
        loop {
            let event = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                e = events.next() => e,
            };
            match event {
                Ok(Some(event)) => self.handle(&client, &account, event).await,
                Ok(None) => return Ok(()),
                Err(LichessError::Unexpected(e)) => tracing::info!("lichess (bot): {e}"),
                Err(e) => return Err(e),
            }
        }
    }

    /// Closes stored games the bot was playing when the app stopped and that Lichess no longer has.
    async fn reconcile(&self, client: &Lichess) {
        let Ok(ongoing) = client.ongoing_games(50).await else { return };
        let ids: Vec<String> = ongoing.iter().map(|g| g.id().to_owned()).collect();
        let Ok(stored) = self.inner.games.stored_live_lichess(ConnectionKey::Bot).await else { return };
        for g in stored {
            if g.source.lichess_id().is_some_and(|id| !ids.iter().any(|i| i == id)) {
                let _ = self.inner.games.close_stale(g.id).await;
            }
        }
    }

    fn emit(
        &self,
        c: &LichessChallengeInfo,
        me: &str,
        event: ChallengeEventKind,
        reason: Option<&str>,
        detail: Option<&str>,
    ) {
        let challenge = challenge_of(c, ConnectionKey::Bot, me);
        let e =
            ChallengeEvent { challenge, event, reason: reason.map(str::to_owned), detail: detail.map(str::to_owned) };
        self.inner.hub.bus.emit(Payload::Challenge(Box::new(e)));
    }

    async fn handle(&self, client: &Lichess, me: &Account, event: Event) {
        match event {
            Event::Challenge { challenge: c, .. } => {
                // Our own challenges to others come through the stream as well.
                if c.is_from(&me.id) {
                    return;
                }
                self.answer(client, me, &c).await;
            }
            Event::ChallengeCanceled { challenge: c } => {
                self.inner.state.lock().unwrap().accepted.remove(&c.id);
                self.emit(&c, &me.id, ChallengeEventKind::Canceled, None, None);
                if c.is_from(&me.id) {
                    self.settle(&c.id, Outcome::Canceled).await;
                }
            }
            Event::ChallengeDeclined { challenge: c } => {
                let reason = c.decline_reason_key.clone();
                self.emit(&c, &me.id, ChallengeEventKind::Declined, reason.as_deref(), None);
                self.settle(&c.id, Outcome::Declined(reason)).await;
            }
            Event::GameStart { game } => {
                let id = game.id().to_owned();
                if id.is_empty() {
                    return;
                }
                let detach = CancellationToken::new();
                {
                    let mut s = self.inner.state.lock().unwrap();
                    let against_bot = s.accepted.remove(&id).is_some_and(|(bot, _)| bot)
                        || s.seeking.pending_id() == Some(id.as_str());
                    if s.games.contains_key(&id) {
                        return;
                    }
                    s.games.insert(id.clone(), Playing { game: None, detach: detach.clone() });
                    if against_bot {
                        s.quota.add(&id, now_ms());
                    }
                }
                self.settle(&id, Outcome::Started).await;
                let me = self.clone();
                let client = client.clone();
                let Some(account) = me.account() else { return };
                tokio::spawn(async move { me.play(client, account, id, detach).await });
                self.announce().await;
            }
            Event::GameFinish { game } => {
                self.inner.state.lock().unwrap().accepted.remove(game.id());
            }
            Event::Other => {}
        }
    }

    /// Takes or declines a challenge to the bot, by its settings.
    async fn answer(&self, client: &Lichess, me: &Account, c: &LichessChallengeInfo) {
        let from_bot = c.challenger.as_ref().and_then(|u| u.title.as_deref()) == Some("BOT");
        let offer = Offer {
            variant: if c.variant.key.is_empty() { "standard".into() } else { c.variant.key.clone() },
            kind: c.time_control.kind.clone(),
            limit: c.time_control.limit.unwrap_or(0),
            increment: c.time_control.increment.unwrap_or(0),
            speed: c.speed.clone(),
            rated: c.rated,
            from_bot,
        };
        let playable = self.playable().await;
        let (settings, busy, limited) = {
            let mut s = self.inner.state.lock().unwrap();
            let busy = s.busy(self.accepted_for());
            (s.settings.clone(), busy, s.quota.until(now_ms()).is_some())
        };
        // Over the daily limit on bot games, Lichess would refuse to start a bot's game: it is declined as busy. People's
        // are still taken.
        let decision = match rules::decide(&offer, &settings, busy, playable) {
            Ok(()) if from_bot && limited => Err((Decline::Later, Some(LIMIT_DETAIL))),
            Ok(()) => Ok(()),
            Err(reason) => Err((reason, None)),
        };
        match decision {
            Ok(()) => match client.accept_challenge(&c.id).await {
                Ok(()) => {
                    let room = {
                        let mut s = self.inner.state.lock().unwrap();
                        s.accepted.insert(c.id.clone(), (from_bot, Instant::now()));
                        let busy = s.busy(self.accepted_for());
                        let max = s.settings.max_games as usize;
                        let name = c.challenger.as_ref().map_or("someone", |u| u.name.as_str()).to_owned();
                        s.seeking.make_room(&name, busy, max)
                    };
                    self.emit(c, &me.id, ChallengeEventKind::Accepted, None, None);
                    if let Some(pending) = room {
                        self.withdraw(client, &pending.id).await;
                        self.announce().await;
                    }
                }
                Err(e) => {
                    if let Some(limit) = e.bot_daily_limit(now_ms()).filter(|l| l.ours) {
                        self.limited(limit.retry_after.as_millis() as i64).await;
                    }
                    tracing::info!("lichess (bot): could not accept challenge {}: {e}", c.id);
                }
            },
            Err((reason, detail)) => {
                let key = lichess_reason(reason);
                if let Err(e) = client.decline_challenge(&c.id, key).await {
                    tracing::info!("lichess (bot): could not decline challenge {}: {e}", c.id);
                }
                self.emit(c, &me.id, ChallengeEventKind::Declined, Some(reason.key()), detail);
            }
        }
    }

    /// Lichess says we have played our bot games for the day: none until `after_ms` has passed.
    async fn limited(&self, after_ms: i64) {
        {
            let now = now_ms();
            self.inner.state.lock().unwrap().quota.limited(now + after_ms, now);
        }
        self.announce().await;
    }

    async fn withdraw(&self, client: &Lichess, id: &str) {
        if let Err(e) = client.cancel_challenge(id).await {
            tracing::info!("lichess (bot): could not withdraw challenge {id}: {e}");
        }
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Games
    // -----------------------------------------------------------------------------------------------------------------

    async fn play(&self, client: Lichess, account: Account, id: String, detach: CancellationToken) {
        if let Err(e) = self.play_inner(&client, &account, &id, detach).await {
            tracing::warn!("lichess game {id} (bot): {e}");
        }
        {
            let mut s = self.inner.state.lock().unwrap();
            s.games.remove(&id);
            s.seeking.game_ended(now_ms());
        }
        self.announce().await;
    }

    async fn play_inner(
        &self,
        client: &Lichess,
        account: &Account,
        id: &str,
        detach: CancellationToken,
    ) -> Result<(), Error> {
        let (stream, full) = lichess_game::prepare(client, Api::Bot, id, &self.inner.timing).await?;
        let plies = full.state.plies();
        let Some(our) = full.side_of(&account.id).map(lichess_game::side) else {
            return Err(Error::internal(format!("{} plays neither side", account.username)));
        };
        let lichess_side = |s: Side| match s {
            Side::White => dameless_lichess::Side::White,
            Side::Black => dameless_lichess::Side::Black,
        };
        // A bot's game counts against the daily limit once, whoever challenged whom.
        if full.player(lichess_side(our.other())).title.as_deref() == Some("BOT") {
            let added = self.inner.state.lock().unwrap().quota.add(id, now_ms());
            if added {
                self.announce().await;
            }
        }
        if !matches!(full.variant.key.as_str(), "standard" | "fromPosition" | "") {
            leave(client, id, plies).await;
            return Err(Error::bad_request(format!("the {} variant is not played: left the game", full.variant.key)));
        }
        let settings = self.settings();
        let Some(version) = settings.version.clone() else {
            leave(client, id, plies).await;
            return Err(Error::conflict("no version is chosen to play: left the game"));
        };
        let binary = match self.inner.binaries.binary(&version).await {
            Ok(b) => b,
            Err(e) => {
                leave(client, id, plies).await;
                return Err(e);
            }
        };
        let options = EngineOptions {
            hash: settings.hash,
            threads: settings.threads,
            move_overhead: settings.move_overhead,
            minimal: false,
        };
        let owner = format!("Lichess game {id}");
        let engine =
            match EngineSeat::start(&binary, options, &self.inner.hub.ledger, self.inner.hub.bus.clone(), &owner).await
            {
                Ok(e) => e.pace(self.inner.timing.info_every, self.inner.timing.unclocked_movetime_ms),
                Err(e) => {
                    leave(client, id, plies).await;
                    return Err(e);
                }
            };
        let player = full.player(lichess_side(our));
        let seat = Seat {
            version: Some(binary.version.id),
            build: Some(binary.build),
            rating: player.rating,
            title: player.title.clone(),
            ..Seat::new(PlayerKey::Dameless, binary.version.name.clone())
        };
        let mirror = lichess_game::mirror();
        let ours = LichessBot::new(engine, client.clone(), id, mirror.subscribe());
        let game = LichessGame {
            games: self.inner.games.clone(),
            connection: ConnectionKey::Bot,
            client: client.clone(),
            api: Api::Bot,
            full,
            stream,
            mirror,
            our,
            our_seat: seat,
            ours: Box::new(ours),
            moves: None,
            lichess_url: self.inner.clients.url().to_owned(),
            cancel: CancellationToken::new(),
            detach,
        };
        let me = self.clone();
        let lichess_id = id.to_owned();
        lichess_game::run(game, move |game_id| {
            if let Some(g) = me.inner.state.lock().unwrap().games.get_mut(&lichess_id) {
                g.game = Some(game_id);
            }
            let me = me.clone();
            tokio::spawn(async move { me.announce().await });
        })
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Challenging others, looking for games
    // -----------------------------------------------------------------------------------------------------------------

    /// Challenges a player; the game, if accepted, starts on the event stream, so the bot must be online.
    pub async fn challenge(&self, req: ChallengeBody) -> Result<ChallengeSent, Error> {
        let client = self.need_client()?;
        if !self.inner.state.lock().unwrap().online {
            return Err(Error::conflict("the bot is offline: take it online first, or nobody will play the game"));
        }
        if req.ai_level.is_some() || req.my_bot {
            return Err(Error::bad_request("the bot challenges players by username"));
        }
        let username = req.username.map(|u| u.trim().to_owned()).unwrap_or_default();
        if !dameless_core::ids::valid_lichess_name(&username) {
            return Err(Error::bad_request("that is not a Lichess username"));
        }
        let time = req.time.ok_or_else(|| Error::bad_request("a challenge needs a clock, such as 3+2"))?;
        let (initial, increment) = clock_seconds(&time)?;
        if initial == 0 && increment == 0 {
            return Err(Error::bad_request("a game needs some time on the clock"));
        }
        let request = LichessChallenge::clock(req.rated, initial, increment).color(lichess_color(req.color));
        let c = match client.challenge_user(&username, &request).await {
            Ok(c) => c,
            Err(e) => {
                if let Some(limit) = e.bot_daily_limit(now_ms()).filter(|l| l.ours) {
                    self.limited(limit.retry_after.as_millis() as i64).await;
                }
                return Err(error::lichess(e));
            }
        };
        Ok(ChallengeSent {
            url: c.url.clone().unwrap_or_else(|| format!("{}/{}", self.inner.clients.url(), c.id)),
            id: c.id,
            opponent: username,
            rated: req.rated,
            time,
            color: req.color,
        })
    }

    /// The bots online now, with their ratings: who there is to challenge.
    pub async fn online_bots(&self) -> Result<Vec<OnlineBot>, Error> {
        let client = self.need_client()?;
        let bots = client.online_bots(100).await.map_err(error::lichess)?;
        Ok(bots.iter().map(online_bot).collect())
    }

    /// Looks for games: `once` until one starts, `always` after every game too (and after a restart), `off` stops and
    /// withdraws the challenge waiting for an answer.
    pub async fn set_seek(&self, mode: SeekMode) -> Result<ConnectionStatus, Error> {
        if mode == SeekMode::Off {
            self.halt_seek().await;
            self.announce().await;
            return Ok(self.status());
        }
        self.need_client()?;
        {
            let mut s = self.inner.state.lock().unwrap();
            if !s.online {
                return Err(Error::conflict("the bot is offline: take it online first"));
            }
            if s.seeking.mode == SeekMode::Off {
                s.seeking.note = Some("Looking for a bot to play.".into());
            }
            s.seeking.set_mode(mode);
        }
        self.ensure_seek_task();
        self.announce().await;
        Ok(self.status())
    }

    /// Turns the search off and withdraws the pending challenge.
    async fn halt_seek(&self) {
        let pending = {
            let mut s = self.inner.state.lock().unwrap();
            let pending = s.seeking.set_mode(SeekMode::Off);
            s.seeking.note = None;
            pending
        };
        if let (Some(p), Some(client)) = (pending, self.client()) {
            self.withdraw(&client, &p.id).await;
        }
        self.inner.wake.notify_one();
    }

    fn ensure_seek_task(&self) {
        let spawn = !std::mem::replace(&mut self.inner.state.lock().unwrap().seek_task, true);
        if spawn {
            let me = self.clone();
            tokio::spawn(async move { me.seek_loop().await });
        }
        self.inner.wake.notify_one();
    }

    /// What became of one of our challenges, for the search.
    async fn settle(&self, id: &str, outcome: Outcome) {
        let settled = self.inner.state.lock().unwrap().seeking.settle(id, outcome, now_ms());
        if settled {
            self.announce().await;
        }
    }

    async fn seek_loop(&self) {
        loop {
            let step = {
                let mut s = self.inner.state.lock().unwrap();
                let now = now_ms();
                let busy = s.busy(self.accepted_for());
                let situation = Situation {
                    online: s.online,
                    connected: s.conn == ConnectionState::Online,
                    busy,
                    max_games: s.settings.max_games as usize,
                    limit_until: s.quota.until(now),
                };
                let step = s.seeking.step(now, situation);
                if step == Step::Stop {
                    s.seek_task = false;
                }
                step
            };
            match step {
                Step::Stop => break,
                Step::Wait(None) => self.inner.wake.notified().await,
                Step::Wait(Some(at)) => {
                    let wait = std::time::Duration::from_millis((at - now_ms()).max(1) as u64);
                    tokio::select! {
                        _ = self.inner.wake.notified() => {}
                        _ = tokio::time::sleep(wait) => {}
                    }
                }
                Step::Expired(p) => {
                    let current = self.inner.state.lock().unwrap().seeking.expire(&p.challenge.id, now_ms());
                    if current {
                        if let Some(client) = self.client() {
                            self.withdraw(&client, &p.challenge.id).await;
                        }
                        self.announce().await;
                    }
                }
                Step::Limited(until) => {
                    self.inner.state.lock().unwrap().seeking.note_limit(until, &clock_text(until));
                    self.announce().await;
                }
                Step::Lifted => self.announce().await,
                Step::Go => self.attempt().await,
            }
        }
    }

    async fn attempt(&self) {
        let result = self.challenge_once().await;
        let stale = {
            let mut s = self.inner.state.lock().unwrap();
            let now = now_ms();
            match result {
                Ok((challenge, opponent)) => s.seeking.sent(challenge, &opponent, now),
                Err((retry, limited)) => {
                    if let Some(after) = limited {
                        s.quota.limited(now + after, now);
                    }
                    s.seeking.failed(retry, now);
                    None
                }
            }
        };
        if let Some(id) = stale
            && let Some(client) = self.client()
        {
            // Stopped while the challenge was on its way.
            self.withdraw(&client, &id).await;
        }
        self.announce().await;
    }

    /// Picks a clock and a bot and challenges it. On failure: why, and (when it is our daily limit) for how long.
    async fn challenge_once(&self) -> Result<(SeekChallenge, String), (Retry, Option<i64>)> {
        let timing = self.inner.timing.seek;
        let wait = |note: &str| (Retry { note: note.to_owned(), after: timing.retry, ..Retry::default() }, None);
        let (Some(client), Some(me)) = (self.client(), self.account()) else {
            return Err(wait("Not signed in to Lichess; trying again in a minute."));
        };
        let (settings, ratings) = {
            let s = self.inner.state.lock().unwrap();
            (s.settings.clone(), s.seeking.ratings)
        };
        let Some(clock) = seek::pick_clock(&settings.clocks(), &mut random_below) else {
            return Err(wait("No clocks to look for games at: set some in the settings."));
        };
        let unreachable = |e: LichessError| match e {
            LichessError::RateLimited { .. } => (
                Retry {
                    note: "Lichess asked us to slow down; trying again in a minute.".into(),
                    after: timing.rate_limited,
                    ..Retry::default()
                },
                None,
            ),
            e => (
                Retry { note: format!("{e}; trying again in a minute."), after: timing.retry, ..Retry::default() },
                None,
            ),
        };
        let ratings = match ratings {
            Some(r) => r,
            None => {
                let r = account_of(&client.account().await.map_err(unreachable)?).ratings;
                self.inner.state.lock().unwrap().seeking.ratings = Some(r);
                r
            }
        };
        let speed = clock.speed();
        let ours = ratings.settled(speed);
        let bots: Vec<OnlineBot> = client.online_bots(100).await.map_err(unreachable)?.iter().map(online_bot).collect();
        let avoided = self.inner.state.lock().unwrap().seeking.avoided(now_ms());
        let Some((opponent, rating)) =
            seek::pick(&bots, &me.id, speed, ours, settings.seek_range, &avoided, &mut random_below)
        else {
            return Err(wait(&format!("No bots to play at {clock} are online; trying again in a minute.")));
        };
        let request = LichessChallenge::clock(settings.seek_rated, clock.initial, clock.increment);
        let name = opponent.name.clone();
        let sent = client.challenge_user(&name, &request).await.map_err(|e| {
            if let Some(limit) = e.bot_daily_limit(now_ms()) {
                let after = limit.retry_after.as_millis() as i64;
                if limit.ours {
                    return (Retry { note: e.to_string(), after: timing.polite, ..Retry::default() }, Some(after));
                }
                return (
                    Retry {
                        note: format!(
                            "{name} has played its {} games against bots today. Trying another bot.",
                            quota::BOT_GAMES_PER_DAY
                        ),
                        after: timing.polite,
                        rest: Some((name.to_lowercase(), after)),
                        ..Retry::default()
                    },
                    None,
                );
            }
            match &e {
                LichessError::Refused { error, .. } => (
                    Retry {
                        note: format!("Could not challenge {name}: {}. Trying another bot.", error.message),
                        after: timing.polite,
                        shun: Some(name.to_lowercase()),
                        ..Retry::default()
                    },
                    None,
                ),
                _ => unreachable(e),
            }
        })?;
        let url = sent.url.clone().unwrap_or_else(|| format!("{}/{}", self.inner.clients.url(), sent.id));
        Ok((
            SeekChallenge {
                id: sent.id,
                url,
                opponent: SeekOpponent { name: opponent.name.clone(), title: opponent.title.clone(), rating },
                clock,
                rated: settings.seek_rated,
            },
            opponent.name.to_lowercase(),
        ))
    }
}

fn online_bot(u: &dameless_lichess::User) -> OnlineBot {
    let a = account_of(u);
    OnlineBot { name: a.username, title: a.title, ratings: a.ratings }
}

/// Lichess's decline reason for ours.
fn lichess_reason(d: Decline) -> DeclineReason {
    match d {
        Decline::Generic => DeclineReason::Generic,
        Decline::Later => DeclineReason::Later,
        Decline::TooFast => DeclineReason::TooFast,
        Decline::TooSlow => DeclineReason::TooSlow,
        Decline::TimeControl => DeclineReason::TimeControl,
        Decline::Rated => DeclineReason::Rated,
        Decline::Casual => DeclineReason::Casual,
        Decline::Variant => DeclineReason::Variant,
        Decline::NoBot => DeclineReason::NoBot,
        Decline::OnlyBot => DeclineReason::OnlyBot,
    }
}

/// Leaves a game the bot cannot play: aborts it while that is allowed, else resigns.
async fn leave(client: &Lichess, id: &str, plies: usize) {
    if plies < 2 && client.bot().abort(id).await.is_ok() {
        return;
    }
    if let Err(e) = client.bot().resign(id).await {
        tracing::warn!("lichess game {id} (bot): could not resign: {e}");
    }
}
