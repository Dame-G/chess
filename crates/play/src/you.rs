//! You on Lichess: while your token is stored, the app follows your account's event stream and becomes your Lichess
//! client. Each game you play that the Board API can play (blitz and slower, standard chess) becomes a game here,
//! played on the board; games it cannot play (bullet, variants, started on lichess.org) are listed as elsewhere. Any
//! game of yours running on Lichess turns the fair-play policy on. Also: seeks (rapid and slower), challenges (a
//! player or Stockfish 1–8; blitz and slower), and answering challenges.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chess_core::{
    Color, ConnectionKey, Error, GameId, PlayerKey, Side, Speed, TimeControl, Timestamp,
    events::Payload,
    game::Seat,
    lichess::{
        Account, Challenge, ChallengeEvent, ChallengeEventKind, ConnectionState, ConnectionStatus, Direction,
        ElsewhereGame, YouSeek, YouStatus, board,
    },
};
use chess_hub::Hub;
use chess_lichess::{
    Error as LichessError, Lichess,
    board::{SeekRequest, SeekTime},
    challenge::{AiChallenge, ChallengeRequest as LichessChallenge, DeclineReason},
    events::{Event, GameInfo},
};
use chess_store::ConnectionField;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{
    clients::{Clients, account_of, challenge_of, clock_seconds, lichess_color},
    error,
    games::Games,
    lichess_game::{self, LichessGame, LichessYou},
    timing::Timing,
};

/// `POST /api/connections/you/seek`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SeekRequestBody {
    pub time: TimeControl,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub color: Color,
    /// `1500-1800`; Lichess advises leaving it empty.
    #[serde(default)]
    pub rating_range: Option<String>,
}

/// `POST /api/connections/{key}/challenge`: exactly one of `username` and `ai_level`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChallengeBody {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub ai_level: Option<u8>,
    pub time: Option<TimeControl>,
    #[serde(default)]
    pub rated: bool,
    /// The colour the connection plays.
    #[serde(default)]
    pub color: Color,
}

/// A challenge sent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChallengeSent {
    /// The challenge id, which becomes the game's when accepted (an AI game's id at once).
    pub id: String,
    pub url: String,
    pub opponent: String,
    pub rated: bool,
    pub time: TimeControl,
    pub color: Color,
}

/// A game of yours running on Lichess.
struct Running {
    /// The game here, for one played here.
    game: Option<GameId>,
    /// Whether it is played here, through the Board API.
    board: bool,
    speed: Speed,
    /// Stops following it here (the token was forgotten); Lichess plays on.
    detach: CancellationToken,
}

#[derive(Default)]
struct State {
    account: Option<Account>,
    scopes: Vec<String>,
    expires_at: Option<Timestamp>,
    conn: ConnectionState,
    error: Option<String>,
    run: Option<CancellationToken>,
    running: HashMap<String, Running>,
    challenges: Vec<Challenge>,
    seek: Option<(YouSeek, CancellationToken, u64)>,
    seeks: u64,
    seek_note: Option<String>,
}

/// What is kept of your connection between runs (the token is a file).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Kept {
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_at: Option<Timestamp>,
}

struct Inner {
    hub: Arc<Hub>,
    clients: Arc<Clients>,
    games: Games,
    timing: Timing,
    state: Mutex<State>,
}

/// Your Lichess connection. Cheap to clone.
#[derive(Clone)]
pub struct You {
    inner: Arc<Inner>,
}

const SCOPES: &[&str] = chess_lichess::oauth::SCOPES;

impl You {
    pub(crate) fn new(hub: Arc<Hub>, clients: Arc<Clients>, games: Games) -> Self {
        let timing = games.timing().clone();
        Self { inner: Arc::new(Inner { hub, clients, games, timing, state: Mutex::new(State::default()) }) }
    }

    fn client(&self) -> Option<Lichess> {
        self.inner.clients.get(ConnectionKey::You)
    }

    fn need_client(&self) -> Result<Lichess, Error> {
        self.client().ok_or_else(|| Error::conflict("your Lichess account is not connected: log in first"))
    }

    pub fn account(&self) -> Option<Account> {
        self.inner.state.lock().unwrap().account.clone()
    }

    /// The connection as the app shows it.
    pub fn status(&self) -> ConnectionStatus {
        let s = self.inner.state.lock().unwrap();
        let configured = self.client().is_some();
        let mut playing: Vec<GameId> = s.running.values().filter_map(|r| r.game).collect();
        playing.sort();
        let mut elsewhere: Vec<ElsewhereGame> = s
            .running
            .iter()
            .filter(|(_, r)| !r.board)
            .map(|(id, r)| ElsewhereGame {
                id: id.clone(),
                url: format!("{}/{id}", self.inner.clients.url()),
                speed: r.speed,
            })
            .collect();
        elsewhere.sort_by(|a, b| a.id.cmp(&b.id));
        let missing_scopes = if configured && !s.scopes.is_empty() {
            SCOPES.iter().filter(|x| !s.scopes.iter().any(|t| t == *x)).map(|x| (*x).to_owned()).collect()
        } else {
            Vec::new()
        };
        ConnectionStatus {
            key: ConnectionKey::You,
            configured,
            account: s.account.clone(),
            state: s.conn,
            error: s.error.clone(),
            playing,
            challenges: s.challenges.clone(),
            you: Some(YouStatus {
                scopes: s.scopes.clone(),
                missing_scopes,
                expires_at: s.expires_at.clone(),
                elsewhere,
                seek: s.seek.as_ref().map(|(seek, _, _)| seek.clone()),
                seek_note: s.seek_note.clone(),
            }),
        }
    }

    /// Keeps the connection's row current and tells whoever follows (`connection.upserted`).
    async fn announce(&self) {
        let status = self.status();
        let kept = {
            let s = self.inner.state.lock().unwrap();
            Kept { scopes: s.scopes.clone(), expires_at: s.expires_at.clone() }
        };
        let store = &self.inner.hub.store;
        if let Err(e) =
            store.put_connection_field(ConnectionKey::You, ConnectionField::Account, status.account.as_ref()).await
        {
            tracing::warn!("lichess (you): could not store the connection: {e}");
        }
        let _ = store.put_connection_field(ConnectionKey::You, ConnectionField::State, Some(&kept)).await;
        self.inner.games.set_your_name(status.account.as_ref().map(|a| a.username.clone()));
        self.inner.hub.bus.emit(Payload::ConnectionUpserted(Box::new(status)));
    }

    /// Keeps the fair-play policy current from your games on Lichess, played here or elsewhere.
    fn update_policy(&self) {
        let correspondence: Vec<bool> = {
            let s = self.inner.state.lock().unwrap();
            s.running.values().map(|r| r.speed == Speed::Correspondence).collect()
        };
        self.inner.hub.policy.set_live_games(&correspondence);
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

    /// Picks up what the last run kept (scopes, expiry), and starts following the account when a token is stored.
    pub(crate) async fn resume(&self) {
        let kept: Option<Kept> =
            self.inner.hub.store.connection_field(ConnectionKey::You, ConnectionField::State).await.ok().flatten();
        let account: Option<Account> =
            self.inner.hub.store.connection_field(ConnectionKey::You, ConnectionField::Account).await.ok().flatten();
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(k) = kept {
                s.scopes = k.scopes;
                s.expires_at = k.expires_at;
            }
            s.account = account;
        }
        if self.client().is_some() {
            self.connect();
        }
    }

    /// A token was stored for this account: follow it.
    pub(crate) async fn connected(&self, account: Account, scopes: Vec<String>, expires_at: Option<Timestamp>) {
        {
            let mut s = self.inner.state.lock().unwrap();
            s.account = Some(account);
            s.scopes = scopes;
            s.expires_at = expires_at;
            s.challenges.clear();
        }
        self.connect();
        self.announce().await;
    }

    /// The token was forgotten: stop following, forget the seek and the challenges. Games in progress stop being
    /// followed here; on Lichess they go on.
    pub(crate) async fn disconnected(&self) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(run) = s.run.take() {
                run.cancel();
            }
            if let Some((_, cancel, _)) = s.seek.take() {
                cancel.cancel();
            }
            for r in s.running.values() {
                r.detach.cancel();
            }
            *s = State::default();
        }
        self.update_policy();
        self.set_conn(ConnectionState::Off, None).await;
        self.announce().await;
    }

    fn connect(&self) {
        let cancel = CancellationToken::new();
        if let Some(old) = self.inner.state.lock().unwrap().run.replace(cancel.clone()) {
            old.cancel();
        }
        let me = self.clone();
        tokio::spawn(async move { me.run(cancel).await });
    }

    /// Follows the event stream until cancelled, reconnecting with a growing pause (a minute after a 429).
    async fn run(&self, cancel: CancellationToken) {
        let mut backoff = self.inner.timing.backoff();
        while !cancel.is_cancelled() {
            self.set_conn(ConnectionState::Connecting, None).await;
            let wait = match self.follow(&cancel, &mut backoff).await {
                Ok(()) => backoff.after(&LichessError::Unreachable("the stream ended".into())),
                Err(e) => {
                    tracing::warn!("lichess (you): {e}");
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
        backoff: &mut chess_lichess::Backoff,
    ) -> Result<(), LichessError> {
        let Some(client) = self.client() else { return Err(LichessError::NoToken) };
        let account = account_of(&client.account().await?);
        client.set_username(&account.username);
        self.inner.state.lock().unwrap().account = Some(account.clone());
        let mut events = client.events().await?;
        // The stream starts with every open challenge and game in progress: what was known is learnt again.
        {
            let mut s = self.inner.state.lock().unwrap();
            s.challenges.clear();
            s.running.retain(|_, r| r.board);
        }
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
                Err(LichessError::Unexpected(e)) => tracing::info!("lichess (you): {e}"),
                Err(e) => return Err(e),
            }
        }
    }

    /// Closes stored games of yours that the app was playing when it stopped and that Lichess no longer has.
    async fn reconcile(&self, client: &Lichess) {
        let Ok(ongoing) = client.ongoing_games(50).await else { return };
        let ids: Vec<String> = ongoing.iter().map(|g| g.id().to_owned()).collect();
        let Ok(stored) = self.inner.games.stored_live_lichess(ConnectionKey::You).await else { return };
        for g in stored {
            if g.source.lichess_id().is_some_and(|id| !ids.iter().any(|i| i == id)) {
                let _ = self.inner.games.close_stale(g.id).await;
            }
        }
    }

    async fn emit_challenge(&self, challenge: Challenge, event: ChallengeEventKind, reason: Option<String>) {
        let e = ChallengeEvent { challenge, event, reason, detail: None };
        self.inner.hub.bus.emit(Payload::Challenge(Box::new(e)));
    }

    async fn handle(&self, client: &Lichess, account: &Account, event: Event) {
        match event {
            Event::GameStart { game } => self.game_started(client, account, &game).await,
            Event::GameFinish { game } => {
                // A game played here ends with its own stream; one played elsewhere ends now.
                let elsewhere = {
                    let mut s = self.inner.state.lock().unwrap();
                    let elsewhere = s.running.get(game.id()).is_some_and(|r| !r.board);
                    if elsewhere {
                        s.running.remove(game.id());
                    }
                    elsewhere
                };
                if elsewhere {
                    self.update_policy();
                    self.announce().await;
                }
            }
            Event::Challenge { challenge, .. } => {
                let c = challenge_of(&challenge, ConnectionKey::You, &account.id);
                {
                    let mut s = self.inner.state.lock().unwrap();
                    s.challenges.retain(|x| x.id != c.id);
                    s.challenges.push(c.clone());
                }
                self.emit_challenge(c, ChallengeEventKind::Created, None).await;
                self.announce().await;
            }
            Event::ChallengeCanceled { challenge } | Event::ChallengeDeclined { challenge } => {
                let declined =
                    challenge.decline_reason_key.is_some() || challenge.status.as_deref() == Some("declined");
                let reason = challenge.decline_reason_key.clone();
                let c = challenge_of(&challenge, ConnectionKey::You, &account.id);
                self.inner.state.lock().unwrap().challenges.retain(|x| x.id != c.id);
                let kind = if declined { ChallengeEventKind::Declined } else { ChallengeEventKind::Canceled };
                self.emit_challenge(c, kind, reason).await;
                self.announce().await;
            }
            Event::Other => {}
        }
    }

    async fn game_started(&self, client: &Lichess, account: &Account, game: &GameInfo) {
        let id = game.id().to_owned();
        if id.is_empty() {
            return;
        }
        let speed = game.speed.as_deref().and_then(Speed::parse).unwrap_or(Speed::Correspondence);
        let variant = game.variant.as_ref().map_or("standard", |v| v.key.as_str());
        let board = game.compat.map_or_else(|| board::playable_speed(speed), |c| c.board)
            && matches!(variant, "standard" | "fromPosition" | "");
        let new = {
            let mut s = self.inner.state.lock().unwrap();
            s.challenges.retain(|c| c.id != id);
            if let Some((_, cancel, _)) = s.seek.take() {
                cancel.cancel();
                s.seek_note = Some("Someone took your seek: the game has started.".into());
            }
            let new = !s.running.contains_key(&id);
            if new {
                s.running.insert(id.clone(), Running { game: None, board, speed, detach: CancellationToken::new() });
            }
            new
        };
        self.update_policy();
        if new && board {
            let me = self.clone();
            let client = client.clone();
            let account = account.clone();
            tokio::spawn(async move { me.play(client, account, id).await });
        }
        self.announce().await;
    }

    /// Plays your game `id` here until it ends.
    async fn play(&self, client: Lichess, account: Account, id: String) {
        if let Err(e) = self.play_inner(client, &account, &id).await {
            tracing::warn!("lichess game {id} (you): {e}");
        }
        self.inner.state.lock().unwrap().running.remove(&id);
        self.update_policy();
        self.announce().await;
    }

    async fn play_inner(&self, client: Lichess, account: &Account, id: &str) -> Result<(), Error> {
        let (stream, full) = lichess_game::prepare(&client, id, &self.inner.timing).await?;
        let Some(our) = full.side_of(&account.id).map(lichess_game::side) else {
            return Err(Error::internal(format!("{} plays neither side", account.username)));
        };
        let player = full.player(match our {
            Side::White => chess_lichess::Side::White,
            Side::Black => chess_lichess::Side::Black,
        });
        let seat = Seat {
            rating: player.rating,
            title: player.title.clone(),
            ..Seat::new(PlayerKey::You, account.username.clone())
        };
        let detach = self.inner.state.lock().unwrap().running.get(id).map(|r| r.detach.clone()).unwrap_or_default();
        let mirror = lichess_game::mirror();
        let (ours, moves) = LichessYou::new(client.clone(), id, mirror.subscribe());
        let game = LichessGame {
            games: self.inner.games.clone(),
            connection: ConnectionKey::You,
            client: client.clone(),
            full,
            stream,
            mirror,
            our,
            our_seat: seat,
            ours: Box::new(ours),
            moves: Some(moves),
            lichess_url: self.inner.clients.url().to_owned(),
            cancel: CancellationToken::new(),
            detach,
        };
        let me = self.clone();
        let lichess_id = id.to_owned();
        lichess_game::run(game, move |game_id| {
            if let Some(r) = me.inner.state.lock().unwrap().running.get_mut(&lichess_id) {
                r.game = Some(game_id);
            }
            let me = me.clone();
            tokio::spawn(async move { me.announce().await });
        })
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Finding games
    // -----------------------------------------------------------------------------------------------------------------

    /// Opens a seek in the lobby, rapid or slower (the Board API's rule). It stays open until someone takes it (the
    /// game then starts) or it is cancelled.
    pub async fn seek(&self, req: SeekRequestBody) -> Result<ConnectionStatus, Error> {
        let client = self.need_client()?;
        if self.inner.state.lock().unwrap().conn != ConnectionState::Online {
            return Err(Error::conflict("not connected to Lichess yet: try again in a moment"));
        }
        if self.inner.state.lock().unwrap().seek.is_some() {
            return Err(Error::conflict("a seek is already open: cancel it first"));
        }
        let (initial, increment) = clock_seconds(&req.time)?;
        board::check_seek(initial, increment)?;
        let range = board::rating_range(req.rating_range.as_deref())?;
        let rating_range = range.as_deref().and_then(|r| {
            let (a, b) = r.split_once('-')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        });
        let request = SeekRequest {
            rated: req.rated,
            time: SeekTime::Clock { limit_seconds: initial, increment_seconds: increment },
            color: lichess_color(req.color),
            variant: "standard".into(),
            rating_range,
        };
        let mut seek = client.board().seek(&request).await.map_err(error::lichess)?;
        let cancel = CancellationToken::new();
        let shown = YouSeek { time: req.time, rated: req.rated, color: req.color, rating_range: range };
        let number = {
            let mut s = self.inner.state.lock().unwrap();
            s.seeks += 1;
            s.seek = Some((shown, cancel.clone(), s.seeks));
            s.seek_note = None;
            s.seeks
        };
        self.announce().await;
        // The seek lives as long as its connection: dropping it cancels the seek at Lichess.
        let me = self.clone();
        tokio::spawn(async move {
            let note = tokio::select! {
                _ = cancel.cancelled() => None,
                r = seek.closed() => Some(match r {
                    Ok(()) => "The seek ended.".to_owned(),
                    Err(e) => format!("The seek was lost: {e}."),
                }),
            };
            drop(seek);
            {
                let mut s = me.inner.state.lock().unwrap();
                // Cancelled or taken has cleared it already, and a newer seek is not this one.
                if s.seek.as_ref().is_some_and(|(_, _, n)| *n == number) {
                    s.seek = None;
                    s.seek_note = note;
                }
            }
            me.announce().await;
        });
        Ok(self.status())
    }

    /// Cancels the open seek, if any.
    pub async fn cancel_seek(&self) -> ConnectionStatus {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some((_, cancel, _)) = s.seek.take() {
                cancel.cancel();
                s.seek_note = Some("Seek cancelled.".into());
            }
        }
        self.announce().await;
        self.status()
    }

    /// Challenges a player (blitz or slower) or Lichess's AI (`ai_level`, casual).
    pub async fn challenge(&self, req: ChallengeBody) -> Result<ChallengeSent, Error> {
        let client = self.need_client()?;
        if req.username.is_some() == req.ai_level.is_some() {
            return Err(Error::bad_request("give exactly one of username and ai_level"));
        }
        let time = req.time.ok_or_else(|| Error::bad_request("a challenge needs a clock, such as 5+3"))?;
        let (initial, increment) = clock_seconds(&time)?;
        let base = self.inner.clients.url().to_owned();
        if let Some(level) = req.ai_level {
            board::check_ai(level, req.rated)?;
            board::check_challenge(initial, increment, true)?;
            let ai = AiChallenge::clock(level, initial, increment).color(lichess_color(req.color));
            let game = client.challenge_ai(&ai).await.map_err(error::lichess)?;
            return Ok(ChallengeSent {
                url: format!("{base}/{}", game.id),
                id: game.id,
                opponent: format!("Stockfish level {level}"),
                rated: false,
                time,
                color: req.color,
            });
        }
        let username = req.username.unwrap_or_default().trim().to_owned();
        if !chess_core::ids::valid_lichess_name(&username) {
            return Err(Error::bad_request("that is not a Lichess username"));
        }
        if self.account().is_some_and(|a| a.id == username.to_lowercase()) {
            return Err(Error::bad_request("you cannot challenge yourself"));
        }
        board::check_challenge(initial, increment, false)?;
        let request = LichessChallenge::clock(req.rated, initial, increment).color(lichess_color(req.color));
        let c = client.challenge_user(&username, &request).await.map_err(error::lichess)?;
        Ok(ChallengeSent {
            url: c.url.clone().unwrap_or_else(|| format!("{base}/{}", c.id)),
            id: c.id,
            opponent: username,
            rated: req.rated,
            time,
            color: req.color,
        })
    }

    /// Accepts a challenge to you; the game starts on the event stream. 400 for one the Board API cannot play (bullet,
    /// variants): accept that on lichess.org.
    pub async fn accept(&self, id: &str) -> Result<ConnectionStatus, Error> {
        let client = self.need_client()?;
        let known = self.inner.state.lock().unwrap().challenges.iter().find(|c| c.id == id).cloned();
        if let Some(c) = &known
            && !c.playable
        {
            return Err(Error::bad_request(format!(
                "a {} {} game cannot be played here through the Board API: accept it on lichess.org",
                c.speed, c.variant
            )));
        }
        client.accept_challenge(id).await.map_err(error::lichess)?;
        self.inner.state.lock().unwrap().challenges.retain(|c| c.id != id);
        if let Some(c) = known {
            self.emit_challenge(c, ChallengeEventKind::Accepted, None).await;
        }
        self.announce().await;
        Ok(self.status())
    }

    /// Declines a challenge to you with one of Lichess's reasons (default `generic`).
    pub async fn decline(&self, id: &str, reason: Option<&str>) -> Result<ConnectionStatus, Error> {
        let client = self.need_client()?;
        let reason = decline_reason(reason.unwrap_or("generic"))?;
        client.decline_challenge(id, reason).await.map_err(error::lichess)?;
        let known = {
            let mut s = self.inner.state.lock().unwrap();
            let known = s.challenges.iter().find(|c| c.id == id).cloned();
            s.challenges.retain(|c| c.id != id);
            known
        };
        if let Some(c) = known {
            self.emit_challenge(c, ChallengeEventKind::Declined, Some(reason.key().to_owned())).await;
        }
        self.announce().await;
        Ok(self.status())
    }

    /// Withdraws a challenge of yours.
    pub async fn cancel(&self, id: &str) -> Result<ConnectionStatus, Error> {
        let client = self.need_client()?;
        client.cancel_challenge(id).await.map_err(error::lichess)?;
        let known = {
            let mut s = self.inner.state.lock().unwrap();
            let known = s.challenges.iter().find(|c| c.id == id && c.direction == Direction::Outgoing).cloned();
            s.challenges.retain(|c| c.id != id);
            known
        };
        if let Some(c) = known {
            self.emit_challenge(c, ChallengeEventKind::Canceled, None).await;
        }
        self.announce().await;
        Ok(self.status())
    }

    /// The games of yours on Lichess that are running now, as `(lichess id, correspondence)`.
    pub fn running(&self) -> Vec<(String, bool)> {
        let s = self.inner.state.lock().unwrap();
        s.running.iter().map(|(id, r)| (id.clone(), r.speed == Speed::Correspondence)).collect()
    }
}

/// One of Lichess's decline reasons, by its key.
pub(crate) fn decline_reason(key: &str) -> Result<DeclineReason, Error> {
    use DeclineReason as D;
    let all = [
        D::Generic,
        D::Later,
        D::TooFast,
        D::TooSlow,
        D::TimeControl,
        D::Rated,
        D::Casual,
        D::Standard,
        D::Variant,
        D::NoBot,
        D::OnlyBot,
    ];
    all.into_iter().find(|d| d.key() == key.trim()).ok_or_else(|| {
        Error::bad_request(format!(
            "the reason is one of {}",
            all.iter().map(|d| d.key()).collect::<Vec<_>>().join(", ")
        ))
    })
}
