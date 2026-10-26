//! A Lichess of its own, on 127.0.0.1, for tests: accounts by token, event streams, Bot and Board API games with a
//! scripted opponent, challenges, seeks, the games export (including one that breaks), the explorer, the tablebase,
//! profiles, leaderboards, OAuth, and failures on demand (a 429, a 503). Every request it receives is recorded.
//! Nothing leaves the machine.
//!
//! Behind the `mock` feature, so the crates above can play whole games against it:
//!
//! ```ignore
//! let mock = MockLichess::start().await;
//! mock.add_account("tok", MockAccount::user("Dame"));
//! let id = mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700)).reply(Reply::FirstLegal));
//! let client = mock.client(Some("tok"));
//! client.board().make_move(&id, "e2e4").await?;   // Alice answers with her first legal move
//! assert_eq!(mock.game(&id).unwrap().moves.len(), 2);
//! ```
//!
//! Moves are checked for legality (with shakmaty), and checkmate, stalemate and insufficient material end the game as
//! Lichess would. Clocks do not run: they stay where a test puts them ([`MockLichess::set_clocks`]).

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::StreamExt;
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, Position, fen::Fen, uci::UciMove};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    Color, Config, GameStatus, Lichess, Side, Timing,
    challenge::{ChallengeRequest, ChallengeTime, DeclineReason},
    oauth::{CLIENT_ID, challenge_of},
};

// ---------------------------------------------------------------------------------------------------------------------
// What tests describe
// ---------------------------------------------------------------------------------------------------------------------

/// An account the mock knows by its token.
#[derive(Clone, Debug)]
pub struct MockAccount {
    pub id: String,
    pub username: String,
    pub title: Option<String>,
    /// Ratings by perf.
    pub perfs: BTreeMap<String, u32>,
    /// What `POST /api/token/test` reports. Default the three scopes the app asks for.
    pub scopes: Vec<String>,
    /// When the token expires, ms since 1970.
    pub expires: Option<i64>,
    /// Has played games (so it cannot become a BOT).
    pub played: bool,
}

impl MockAccount {
    /// A person's account.
    pub fn user(username: &str) -> Self {
        Self {
            id: username.to_lowercase(),
            username: username.to_owned(),
            title: None,
            perfs: BTreeMap::new(),
            scopes: crate::oauth::SCOPES.iter().map(|s| (*s).to_owned()).collect(),
            expires: Some(1_900_000_000_000),
            played: false,
        }
    }

    /// A BOT account.
    pub fn bot(username: &str) -> Self {
        let mut a = Self::user(username);
        a.title = Some("BOT".into());
        a.scopes = vec!["bot:play".into()];
        a
    }

    pub fn rating(mut self, perf: &str, rating: u32) -> Self {
        self.perfs.insert(perf.to_owned(), rating);
        self
    }

    pub fn scopes(mut self, scopes: &[&str]) -> Self {
        self.scopes = scopes.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_owned());
        self
    }

    pub fn played(mut self) -> Self {
        self.played = true;
        self
    }

    fn json(&self) -> Value {
        let perfs: serde_json::Map<String, Value> = self
            .perfs
            .iter()
            .map(|(k, r)| (k.clone(), json!({ "rating": r, "games": 120, "rd": 60, "prog": 0 })))
            .collect();
        json!({
            "id": self.id, "username": self.username, "title": self.title, "perfs": perfs,
            "createdAt": 1_600_000_000_000i64, "seenAt": 1_700_000_000_000i64,
            "url": format!("https://lichess.org/@/{}", self.username),
        })
    }
}

/// A request the mock received.
#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    /// The form body, when there was one.
    pub form: HashMap<String, String>,
    pub body: String,
    /// The bearer token, if any.
    pub token: Option<String>,
    pub user_agent: String,
}

/// Who sits at a mock game or sends a challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seat {
    /// An account of the mock, by id: it plays through the API with its token.
    Account(String),
    /// Someone else on Lichess: the mock plays for them (see [`Reply`] and the `opponent_*` methods).
    User { id: String, name: String, rating: Option<u32>, title: Option<String> },
    /// Lichess's AI at a level. It answers with its first legal move.
    Ai(u8),
}

impl Seat {
    pub fn account(id: &str) -> Self {
        Self::Account(id.to_lowercase())
    }

    pub fn user(name: &str, rating: u32) -> Self {
        Self::User { id: name.to_lowercase(), name: name.to_owned(), rating: Some(rating), title: None }
    }

    pub fn titled(name: &str, rating: u32, title: &str) -> Self {
        Self::User { id: name.to_lowercase(), name: name.to_owned(), rating: Some(rating), title: Some(title.into()) }
    }

    pub fn ai(level: u8) -> Self {
        Self::Ai(level)
    }
}

/// How the mock plays for a seat that is not an account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reply {
    /// Not at all: the test plays its moves with [`MockLichess::opponent_move`].
    #[default]
    None,
    /// The first legal move, at once.
    FirstLegal,
}

/// A game to start.
#[derive(Clone, Debug)]
pub struct GameSetup {
    pub white: Seat,
    pub black: Seat,
    /// Seconds and seconds; `None` with `days` for correspondence.
    pub clock: Option<(u32, u32)>,
    pub days: Option<u32>,
    pub rated: bool,
    pub variant: String,
    pub initial_fen: Option<String>,
    /// How the mock plays for seats that are not accounts. Lichess's AI always answers.
    pub reply: Reply,
    /// Whether a seat the mock plays for takes draw offers and takeback proposals at once.
    pub accepts_offers: bool,
    /// The game id; a fresh one when `None`.
    pub id: Option<String>,
}

impl GameSetup {
    /// A casual 3+2 game.
    pub fn new(white: Seat, black: Seat) -> Self {
        Self {
            white,
            black,
            clock: Some((180, 2)),
            days: None,
            rated: false,
            variant: "standard".into(),
            initial_fen: None,
            reply: Reply::None,
            accepts_offers: false,
            id: None,
        }
    }

    pub fn clock(mut self, limit_seconds: u32, increment_seconds: u32) -> Self {
        self.clock = Some((limit_seconds, increment_seconds));
        self.days = None;
        self
    }

    pub fn correspondence(mut self, days: u32) -> Self {
        self.clock = None;
        self.days = Some(days);
        self
    }

    pub fn rated(mut self, rated: bool) -> Self {
        self.rated = rated;
        self
    }

    pub fn reply(mut self, reply: Reply) -> Self {
        self.reply = reply;
        self
    }

    pub fn accepts_offers(mut self) -> Self {
        self.accepts_offers = true;
        self
    }

    pub fn fen(mut self, fen: &str) -> Self {
        self.initial_fen = Some(fen.to_owned());
        self.variant = "fromPosition".into();
        self
    }

    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_owned());
        self
    }

    fn speed(&self) -> &'static str {
        match (self.clock, self.days) {
            (Some((limit, inc)), _) => speed_of(limit, inc),
            _ => "correspondence",
        }
    }
}

/// Lichess's speed of a clock: initial + 40 × increment.
fn speed_of(limit: u32, increment: u32) -> &'static str {
    match limit + 40 * increment {
        0..30 => "ultraBullet",
        30..180 => "bullet",
        180..480 => "blitz",
        480..1500 => "rapid",
        _ => "classical",
    }
}

/// A mock game as it stands.
#[derive(Clone, Debug)]
pub struct MockGame {
    pub id: String,
    pub white: Seat,
    pub black: Seat,
    pub moves: Vec<String>,
    pub status: GameStatus,
    pub winner: Option<Side>,
    pub draw_offer: Option<Side>,
    pub takeback_offer: Option<Side>,
    pub wtime: u64,
    pub btime: u64,
}

/// How someone the mock plays for answers a challenge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeAnswer {
    Accept,
    Decline(DeclineReason),
    /// Leaves it unanswered.
    Ignore,
}

/// A seek the mock received.
#[derive(Clone, Debug)]
pub struct MockSeek {
    pub id: String,
    /// The account seeking.
    pub account: String,
    pub form: HashMap<String, String>,
    /// Still waiting for an opponent.
    pub open: bool,
    /// The connection was dropped while open.
    pub canceled: bool,
}

/// What Lichess sends the browser back with after a login.
#[derive(Clone, Debug)]
pub struct Authorized {
    pub code: String,
    pub state: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

// ---------------------------------------------------------------------------------------------------------------------
// The mock's state
// ---------------------------------------------------------------------------------------------------------------------

struct Game {
    setup: GameSetup,
    id: String,
    moves: Vec<String>,
    status: GameStatus,
    winner: Option<Side>,
    draw: Option<Side>,
    takeback: Option<Side>,
    wtime: u64,
    btime: u64,
    /// The side that left.
    gone: Option<Side>,
    streams: Vec<UnboundedSender<Value>>,
    created_at: i64,
}

impl Game {
    fn seat(&self, side: Side) -> &Seat {
        match side {
            Side::White => &self.setup.white,
            Side::Black => &self.setup.black,
        }
    }

    fn side_of(&self, account: &str) -> Option<Side> {
        if self.setup.white == Seat::Account(account.to_owned()) {
            Some(Side::White)
        } else if self.setup.black == Seat::Account(account.to_owned()) {
            Some(Side::Black)
        } else {
            None
        }
    }

    fn position(&self) -> Chess {
        position(self.setup.initial_fen.as_deref(), &self.moves).expect("the mock's moves are legal")
    }

    fn turn(&self) -> Side {
        if self.position().turn() == shakmaty::Color::White { Side::White } else { Side::Black }
    }

    fn view(&self) -> MockGame {
        MockGame {
            id: self.id.clone(),
            white: self.setup.white.clone(),
            black: self.setup.black.clone(),
            moves: self.moves.clone(),
            status: self.status,
            winner: self.winner,
            draw_offer: self.draw,
            takeback_offer: self.takeback,
            wtime: self.wtime,
            btime: self.btime,
        }
    }
}

fn position(fen: Option<&str>, moves: &[String]) -> Result<Chess, String> {
    let mut pos: Chess = match fen.filter(|f| *f != "startpos") {
        None => Chess::default(),
        Some(f) => f
            .parse::<Fen>()
            .map_err(|e| e.to_string())?
            .into_position(CastlingMode::Standard)
            .map_err(|e| e.to_string())?,
    };
    for m in moves {
        let mv = m.parse::<UciMove>().map_err(|e| e.to_string())?.to_move(&pos).map_err(|e| e.to_string())?;
        pos.play_unchecked(mv);
    }
    Ok(pos)
}

struct Challenge {
    id: String,
    challenger: Seat,
    dest: Seat,
    rated: bool,
    time: ChallengeTime,
    color: Color,
    variant: String,
}

#[derive(Default)]
struct Feed {
    subscribers: Vec<UnboundedSender<Value>>,
    /// What tests pushed while nobody listened.
    queue: Vec<Value>,
}

struct Failure {
    prefix: String,
    status: u16,
    body: Value,
}

struct Seek {
    id: String,
    account: String,
    form: HashMap<String, String>,
    open: bool,
    canceled: bool,
    sender: Option<UnboundedSender<Value>>,
}

struct OAuthCode {
    challenge: String,
    redirect_uri: String,
    token: String,
}

#[derive(Default)]
struct Inner {
    accounts: HashMap<String, MockAccount>,
    requests: Vec<RecordedRequest>,
    failures: VecDeque<Failure>,
    delays: Vec<(String, Duration)>,
    inflight: HashMap<String, usize>,
    max_inflight: HashMap<String, usize>,
    feeds: HashMap<String, Feed>,
    games: BTreeMap<String, Game>,
    challenges: BTreeMap<String, Challenge>,
    sticky_answers: HashMap<String, ChallengeAnswer>,
    queued_answers: HashMap<String, VecDeque<ChallengeAnswer>>,
    seeks: Vec<Seek>,
    exports: HashMap<String, Vec<Value>>,
    export_break: Option<usize>,
    online_bots: Vec<Value>,
    users: HashMap<String, Value>,
    leaderboards: HashMap<String, Vec<Value>>,
    explorer: HashMap<String, Vec<Value>>,
    tablebase: Option<Value>,
    codes: HashMap<String, OAuthCode>,
    grant_as: Option<String>,
    next_id: u64,
}

type Shared = Arc<Mutex<Inner>>;

impl Inner {
    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{:06}", self.next_id)
    }

    fn account_by_token(&self, headers: &HeaderMap) -> Option<MockAccount> {
        self.accounts.get(&bearer(headers)?).cloned()
    }

    fn account(&self, id: &str) -> Option<&MockAccount> {
        self.accounts.values().find(|a| a.id == id)
    }

    /// Sends an event to whoever follows the account's stream now. Nobody listening, nobody hears it: a new
    /// connection learns the state instead, as on Lichess.
    fn emit(&mut self, account: &str, event: Value) {
        if let Some(feed) = self.feeds.get_mut(account) {
            feed.subscribers.retain(|tx| tx.send(event.clone()).is_ok());
        }
    }

    fn broadcast(&mut self, id: &str, event: Value) {
        if let Some(game) = self.games.get_mut(id) {
            game.streams.retain(|tx| tx.send(event.clone()).is_ok());
        }
    }

    fn seat_json(&self, seat: &Seat, speed: &str) -> Value {
        match seat {
            Seat::Account(id) => match self.account(id) {
                Some(a) => json!({ "id": a.id, "name": a.username, "title": a.title, "rating": a.perfs.get(speed) }),
                None => json!({ "id": id, "name": id }),
            },
            Seat::User { id, name, rating, title } => {
                json!({ "id": id, "name": name, "title": title, "rating": rating })
            }
            Seat::Ai(level) => json!({ "aiLevel": level }),
        }
    }

    fn state_json(game: &Game) -> Value {
        let inc = u64::from(game.setup.clock.map_or(0, |c| c.1)) * 1000;
        json!({
            "type": "gameState", "moves": game.moves.join(" "), "wtime": game.wtime, "btime": game.btime,
            "winc": inc, "binc": inc, "status": game.status.key(), "winner": game.winner,
            "wdraw": game.draw == Some(Side::White), "bdraw": game.draw == Some(Side::Black),
            "wtakeback": game.takeback == Some(Side::White), "btakeback": game.takeback == Some(Side::Black),
        })
    }

    fn full_json(&self, game: &Game) -> Value {
        let speed = game.setup.speed();
        json!({
            "type": "gameFull", "id": game.id, "rated": game.setup.rated, "speed": speed, "perf": { "name": speed },
            "variant": { "key": game.setup.variant, "name": game.setup.variant },
            "clock": game.setup.clock.map(|(l, i)| json!({ "initial": u64::from(l) * 1000, "increment": u64::from(i) * 1000 })),
            "daysPerTurn": game.setup.days,
            "white": self.seat_json(&game.setup.white, speed), "black": self.seat_json(&game.setup.black, speed),
            "initialFen": game.setup.initial_fen.clone().unwrap_or_else(|| "startpos".into()),
            "state": Self::state_json(game), "createdAt": game.created_at,
        })
    }

    /// The game as the event stream describes it to the player of `side`.
    fn info_json(&self, game: &Game, side: Side) -> Value {
        let speed = game.setup.speed();
        let pos = game.position();
        let opponent = match game.seat(side.other()) {
            Seat::Ai(level) => json!({ "id": null, "username": format!("A.I. level {level}"), "ai": level }),
            seat => {
                let s = self.seat_json(seat, speed);
                json!({ "id": s["id"], "username": s["name"], "rating": s["rating"] })
            }
        };
        let source = if matches!(game.seat(side.other()), Seat::Ai(_)) { "ai" } else { "friend" };
        let status_id = match game.status {
            GameStatus::Created => 10,
            GameStatus::Started => 20,
            GameStatus::Aborted => 25,
            GameStatus::Mate => 30,
            GameStatus::Resign => 31,
            GameStatus::Stalemate => 32,
            GameStatus::Timeout => 33,
            GameStatus::Draw => 34,
            GameStatus::Outoftime => 35,
            _ => 60,
        };
        let moved = game.moves.iter().enumerate().any(|(i, _)| (i % 2 == 0) == (side == Side::White));
        json!({
            "gameId": game.id, "id": game.id, "fullId": format!("{}abcd", game.id), "color": side,
            "fen": Fen::from_position(&pos, shakmaty::EnPassantMode::Legal).to_string(),
            "hasMoved": moved, "isMyTurn": game.turn() == side && !game.status.is_over(),
            "lastMove": game.moves.last().cloned().unwrap_or_default(), "opponent": opponent,
            "perf": speed, "rated": game.setup.rated, "secondsLeft": (if side == Side::White { game.wtime } else { game.btime }) / 1000,
            "source": source, "status": { "id": status_id, "name": game.status.key() }, "speed": speed,
            "variant": { "key": game.setup.variant, "name": game.setup.variant },
            "compat": { "bot": true, "board": !matches!(speed, "bullet" | "ultraBullet") },
            "winner": game.winner,
        })
    }

    fn start_game(&mut self, setup: GameSetup) -> String {
        let id = setup.id.clone().unwrap_or_else(|| self.fresh_id("g"));
        let initial = setup.clock.map_or(u64::from(setup.days.unwrap_or(1)) * 86_400_000, |(l, _)| u64::from(l) * 1000);
        let game = Game {
            setup,
            id: id.clone(),
            moves: Vec::new(),
            status: GameStatus::Started,
            winner: None,
            draw: None,
            takeback: None,
            wtime: initial,
            btime: initial,
            gone: None,
            streams: Vec::new(),
            created_at: crate::time::now_ms(),
        };
        let accounts: Vec<(String, Side)> = [Side::White, Side::Black]
            .into_iter()
            .filter_map(|s| match game.seat(s) {
                Seat::Account(a) => Some((a.clone(), s)),
                _ => None,
            })
            .collect();
        let infos: Vec<(String, Value)> =
            accounts.iter().map(|(a, s)| (a.clone(), self.info_json(&game, *s))).collect();
        self.games.insert(id.clone(), game);
        for (account, info) in infos {
            if let Some(a) = self.accounts.values_mut().find(|x| x.id == account) {
                a.played = true;
            }
            self.emit(&account, json!({ "type": "gameStart", "game": info }));
        }
        self.auto_reply(&id);
        id
    }

    /// Plays a move; ends the game on checkmate, stalemate or insufficient material.
    fn apply_move(&mut self, id: &str, uci: &str) -> Result<(), String> {
        let game = self.games.get_mut(id).ok_or("No such game")?;
        if game.status.is_over() {
            return Err("The game is over".into());
        }
        let pos = game.position();
        let mv = uci.parse::<UciMove>().ok().and_then(|u| u.to_move(&pos).ok()).ok_or_else(|| {
            format!("Piece on {} cannot move to {}", uci.get(..2).unwrap_or(uci), uci.get(2..).unwrap_or(""))
        })?;
        let mover = game.turn();
        let mut after = pos;
        after.play_unchecked(mv);
        game.moves.push(uci.to_owned());
        game.draw = None;
        game.takeback = None;
        if after.is_checkmate() {
            game.status = GameStatus::Mate;
            game.winner = Some(mover);
        } else if after.is_stalemate() {
            game.status = GameStatus::Stalemate;
        } else if after.is_insufficient_material() {
            game.status = GameStatus::Draw;
        }
        let state = Self::state_json(game);
        let over = game.status.is_over();
        self.broadcast(id, state);
        if over {
            self.close(id);
        }
        Ok(())
    }

    /// Ends a game: the last state on its streams, the streams closed, and `gameFinish` to its accounts.
    fn end(&mut self, id: &str, status: GameStatus, winner: Option<Side>) {
        let Some(game) = self.games.get_mut(id) else { return };
        if game.status.is_over() {
            return;
        }
        game.status = status;
        game.winner = winner;
        game.draw = None;
        game.takeback = None;
        let state = Self::state_json(game);
        self.broadcast(id, state);
        self.close(id);
    }

    fn close(&mut self, id: &str) {
        let Some(game) = self.games.get_mut(id) else { return };
        game.streams.clear();
        let game = &self.games[id];
        let finishes: Vec<(String, Value)> = [Side::White, Side::Black]
            .into_iter()
            .filter_map(|s| match game.seat(s) {
                Seat::Account(a) => Some((a.clone(), self.info_json(game, s))),
                _ => None,
            })
            .collect();
        for (account, info) in finishes {
            self.emit(&account, json!({ "type": "gameFinish", "game": info }));
        }
    }

    /// Plays for the side to move when the mock plays for it: Lichess's AI always, a user with `Reply::FirstLegal`.
    fn auto_reply(&mut self, id: &str) {
        let Some(game) = self.games.get(id) else { return };
        if game.status.is_over() {
            return;
        }
        let answers = match game.seat(game.turn()) {
            Seat::Account(_) => false,
            Seat::Ai(_) => true,
            Seat::User { .. } => game.setup.reply == Reply::FirstLegal,
        };
        if answers {
            let pos = game.position();
            let reply = pos.legal_moves()[0].to_uci(CastlingMode::Standard).to_string();
            let _ = self.apply_move(id, &reply);
        }
    }

    /// Takes back the proposal of `proposer`: their last move, and the reply to it if there was one.
    fn take_back(&mut self, id: &str, proposer: Side) {
        let Some(game) = self.games.get_mut(id) else { return };
        let plies = if game.turn() == proposer { 2 } else { 1 };
        let keep = game.moves.len().saturating_sub(plies);
        game.moves.truncate(keep);
        game.takeback = None;
        game.draw = None;
        let state = Self::state_json(game);
        self.broadcast(id, state);
    }

    fn offer(&mut self, id: &str, side: Side, draw: bool) {
        let Some(game) = self.games.get_mut(id) else { return };
        if draw {
            game.draw = Some(side);
        } else {
            game.takeback = Some(side);
        }
        let state = Self::state_json(game);
        self.broadcast(id, state);
    }

    fn challenge_user_json(&self, seat: &Seat) -> Value {
        match seat {
            Seat::Account(id) => match self.account(id) {
                Some(a) => {
                    json!({ "id": a.id, "name": a.username, "title": a.title, "rating": a.perfs.get("blitz"), "online": true })
                }
                None => json!({ "id": id, "name": id }),
            },
            Seat::User { id, name, rating, title } => {
                json!({ "id": id, "name": name, "title": title, "rating": rating, "online": true })
            }
            Seat::Ai(level) => json!({ "id": format!("ai{level}"), "name": format!("A.I. level {level}") }),
        }
    }

    fn challenge_json(&self, c: &Challenge, status: &str) -> Value {
        let (kind, limit, increment, days, show, speed) = match c.time {
            ChallengeTime::Clock { limit_seconds, increment_seconds } => (
                "clock",
                Some(limit_seconds),
                Some(increment_seconds),
                None,
                Some(format!("{}+{increment_seconds}", f64::from(limit_seconds) / 60.0)),
                speed_of(limit_seconds, increment_seconds),
            ),
            ChallengeTime::Correspondence { days } => {
                ("correspondence", None, None, Some(days), None, "correspondence")
            }
            ChallengeTime::Unlimited => ("unlimited", None, None, None, None, "correspondence"),
        };
        json!({
            "id": c.id, "url": format!("https://lichess.org/{}", c.id), "status": status,
            "challenger": self.challenge_user_json(&c.challenger), "destUser": self.challenge_user_json(&c.dest),
            "variant": { "key": c.variant, "name": c.variant }, "rated": c.rated, "speed": speed,
            "timeControl": { "type": kind, "limit": limit, "increment": increment, "daysPerTurn": days, "show": show },
            "color": c.color.key(), "finalColor": if c.color == Color::Black { "black" } else { "white" },
            "perf": { "name": speed },
        })
    }

    fn challenge_parties(c: &Challenge) -> Vec<String> {
        [&c.challenger, &c.dest]
            .into_iter()
            .filter_map(|s| match s {
                Seat::Account(a) => Some(a.clone()),
                _ => None,
            })
            .collect()
    }

    fn create_challenge(&mut self, c: Challenge) -> Value {
        let created = self.challenge_json(&c, "created");
        let speed = created["speed"].as_str().unwrap_or_default().to_owned();
        let event = json!({ "type": "challenge", "challenge": created.clone(),
            "compat": { "bot": true, "board": !matches!(speed.as_str(), "bullet" | "ultraBullet") } });
        for party in Self::challenge_parties(&c) {
            self.emit(&party, event.clone());
        }
        self.challenges.insert(c.id.clone(), c);
        created
    }

    fn accept_challenge(&mut self, id: &str) -> Option<String> {
        let c = self.challenges.remove(id)?;
        let (white, black) = if c.color == Color::Black { (c.dest, c.challenger) } else { (c.challenger, c.dest) };
        let mut setup = GameSetup::new(white, black).rated(c.rated).id(&c.id);
        setup.variant = c.variant;
        match c.time {
            ChallengeTime::Clock { limit_seconds, increment_seconds } => {
                setup = setup.clock(limit_seconds, increment_seconds)
            }
            ChallengeTime::Correspondence { days } => setup = setup.correspondence(days),
            ChallengeTime::Unlimited => setup = setup.correspondence(14),
        }
        Some(self.start_game(setup))
    }

    fn decline_challenge(&mut self, id: &str, reason: DeclineReason) -> bool {
        let Some(c) = self.challenges.remove(id) else { return false };
        let mut v = self.challenge_json(&c, "declined");
        v["declineReason"] = json!(format!("I'm not accepting challenges ({}).", reason.key()));
        v["declineReasonKey"] = json!(reason.key());
        if let Seat::Account(a) = &c.challenger {
            self.emit(a, json!({ "type": "challengeDeclined", "challenge": v }));
        }
        true
    }

    fn cancel_challenge(&mut self, id: &str) -> bool {
        let Some(c) = self.challenges.remove(id) else { return false };
        let v = self.challenge_json(&c, "canceled");
        for party in Self::challenge_parties(&c) {
            self.emit(&party, json!({ "type": "challengeCanceled", "challenge": v.clone() }));
        }
        true
    }

    fn answer_for(&mut self, user: &str) -> ChallengeAnswer {
        if let Some(a) = self.queued_answers.get_mut(user).and_then(VecDeque::pop_front) {
            return a;
        }
        self.sticky_answers.get(user).copied().unwrap_or(ChallengeAnswer::Ignore)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The handle tests hold
// ---------------------------------------------------------------------------------------------------------------------

/// A running mock Lichess. Dropping it stops the server.
pub struct MockLichess {
    url: String,
    shared: Shared,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for MockLichess {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl MockLichess {
    /// Starts a mock on a free port of 127.0.0.1.
    pub async fn start() -> Self {
        let shared: Shared = Arc::new(Mutex::new(Inner::default()));
        let app = router(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self { url, shared, server }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.shared.lock().unwrap()
    }

    /// The mock's address, to use as `LICHESS_URL`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// A config pointing at the mock, with every wait shortened (a 429 pauses 300 ms, not 61 s).
    pub fn config(&self) -> Config {
        let mut config = Config::new(&self.url);
        config.timing = Timing {
            rate_limit_pause: Duration::from_millis(300),
            request_timeout: Duration::from_secs(10),
            stream_idle: Duration::from_secs(10),
            import_break: Duration::from_millis(20),
            explorer_wait: Duration::from_millis(400),
        };
        config
    }

    /// A client of the mock for `token` (or anonymous).
    pub fn client(&self, token: Option<&str>) -> Lichess {
        Lichess::new(self.config(), token)
    }

    /// Waits (up to ten seconds) until `ok` holds, polling; panics with `what` otherwise.
    pub async fn until(&self, what: &str, ok: impl Fn(&Self) -> bool) {
        let waited = tokio::time::timeout(Duration::from_secs(10), async {
            while !ok(self) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(waited.is_ok(), "timed out waiting for {what}");
    }

    // Accounts and requests

    /// Makes `token` an account's.
    pub fn add_account(&self, token: &str, account: MockAccount) {
        self.inner().accounts.insert(token.to_owned(), account);
    }

    /// Whether the mock still knows `token` (it is forgotten once revoked).
    pub fn has_token(&self, token: &str) -> bool {
        self.inner().accounts.contains_key(token)
    }

    /// Every request received, oldest first.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.inner().requests.clone()
    }

    /// The requests whose path starts with `prefix`.
    pub fn requests_to(&self, prefix: &str) -> Vec<RecordedRequest> {
        self.inner().requests.iter().filter(|r| r.path.starts_with(prefix)).cloned().collect()
    }

    pub fn clear_requests(&self) {
        self.inner().requests.clear();
    }

    /// The most requests ever in flight at once with `token`, from arrival to the answer's head.
    pub fn max_concurrent(&self, token: &str) -> usize {
        self.inner().max_inflight.get(token).copied().unwrap_or(0)
    }

    /// Answers the next request whose path starts with `prefix` with `status` and `body`, once.
    pub fn fail_next(&self, prefix: &str, status: u16, body: Value) {
        self.inner().failures.push_back(Failure { prefix: prefix.to_owned(), status, body });
    }

    /// Answers the next request to `prefix` with a 429.
    pub fn rate_limit_next(&self, prefix: &str) {
        self.fail_next(prefix, 429, json!({ "error": "Too many requests. Try again later." }));
    }

    /// Answers the next challenge to `prefix` as Lichess does when the day's games between bots are used up: ours
    /// (`ours`) or the challenged bot's.
    pub fn bot_daily_limit_next(&self, prefix: &str, ours: bool, seconds: u64) {
        let body = if ours {
            json!({ "error": "You played 100 games against other bots today, please wait before challenging another bot.",
                "ratelimit": { "key": "bot.vsBot.day", "seconds": seconds } })
        } else {
            json!({ "error": "They played 100 games against other bots today, please wait to challenge them.",
                "ratelimit": { "key": "bot.vsBot.day", "seconds": seconds } })
        };
        self.fail_next(prefix, if ours { 429 } else { 400 }, body);
    }

    /// Slows every request to `prefix` by `delay`.
    pub fn delay(&self, prefix: &str, delay: Duration) {
        self.inner().delays.push((prefix.to_owned(), delay));
    }

    // Event streams

    /// Sends `event` on the account's event stream; queued until someone connects if nobody follows it now.
    pub fn push_event(&self, account: &str, event: Value) {
        let mut inner = self.inner();
        let feed = inner.feeds.entry(account.to_lowercase()).or_default();
        feed.subscribers.retain(|tx| !tx.is_closed());
        if feed.subscribers.is_empty() {
            feed.queue.push(event);
        } else {
            feed.subscribers.retain(|tx| tx.send(event.clone()).is_ok());
        }
    }

    /// How many connections follow the account's event stream now.
    pub fn event_streams(&self, account: &str) -> usize {
        let mut inner = self.inner();
        let feed = inner.feeds.entry(account.to_lowercase()).or_default();
        feed.subscribers.retain(|tx| !tx.is_closed());
        feed.subscribers.len()
    }

    // Games

    /// Starts a game: `gameStart` on its accounts' event streams, and the AI's first move if it is White.
    pub fn start_game(&self, setup: GameSetup) -> String {
        self.inner().start_game(setup)
    }

    pub fn game(&self, id: &str) -> Option<MockGame> {
        self.inner().games.get(id).map(Game::view)
    }

    /// The games in progress that `account` plays.
    pub fn ongoing_games(&self, account: &str) -> Vec<String> {
        self.inner()
            .games
            .values()
            .filter(|g| !g.status.is_over() && g.side_of(account).is_some())
            .map(|g| g.id.clone())
            .collect()
    }

    /// How many connections follow the game's stream now.
    pub fn game_streams(&self, id: &str) -> usize {
        let mut inner = self.inner();
        inner.games.get_mut(id).map_or(0, |g| {
            g.streams.retain(|tx| !tx.is_closed());
            g.streams.len()
        })
    }

    /// Plays `uci` for the side to move, whoever it is. Panics if the move is illegal or the game is over.
    pub fn opponent_move(&self, id: &str, uci: &str) {
        let mut inner = self.inner();
        if let Err(e) = inner.apply_move(id, uci) {
            panic!("the mock cannot play {uci} in {id}: {e}");
        }
    }

    /// Sends a line on the game's streams, as it is.
    pub fn push_game_event(&self, id: &str, event: Value) {
        self.inner().broadcast(id, event);
    }

    /// Sends the game's current state on its streams again.
    pub fn push_game_state(&self, id: &str) {
        let mut inner = self.inner();
        let Some(state) = inner.games.get(id).map(Inner::state_json) else { return };
        inner.broadcast(id, state);
    }

    /// Sets both clocks (ms); the next state sent carries them.
    pub fn set_clocks(&self, id: &str, wtime: u64, btime: u64) {
        if let Some(g) = self.inner().games.get_mut(id) {
            g.wtime = wtime;
            g.btime = btime;
        }
    }

    /// `side` offers a draw.
    pub fn offer_draw(&self, id: &str, side: Side) {
        self.inner().offer(id, side, true);
    }

    /// `side` proposes a takeback.
    pub fn offer_takeback(&self, id: &str, side: Side) {
        self.inner().offer(id, side, false);
    }

    /// The standing draw offer is accepted: the game is drawn.
    pub fn accept_draw(&self, id: &str) {
        let mut inner = self.inner();
        if inner.games.get(id).is_some_and(|g| g.draw.is_some()) {
            inner.end(id, GameStatus::Draw, None);
        }
    }

    /// The standing takeback proposal is accepted.
    pub fn accept_takeback(&self, id: &str) {
        let mut inner = self.inner();
        if let Some(proposer) = inner.games.get(id).and_then(|g| g.takeback) {
            inner.take_back(id, proposer);
        }
    }

    /// `side` leaves the game: `opponentGone` on the game's streams, and the other side may claim after `seconds`
    /// (the mock allows it at once).
    pub fn opponent_gone(&self, id: &str, side: Side, seconds: u64) {
        let mut inner = self.inner();
        if let Some(g) = inner.games.get_mut(id) {
            g.gone = Some(side);
        }
        inner.broadcast(id, json!({ "type": "opponentGone", "gone": true, "claimWinInSeconds": seconds }));
    }

    /// The side that left comes back.
    pub fn opponent_back(&self, id: &str) {
        let mut inner = self.inner();
        if let Some(g) = inner.games.get_mut(id) {
            g.gone = None;
        }
        inner.broadcast(id, json!({ "type": "opponentGone", "gone": false }));
    }

    /// Ends the game with this status (a flag, a resignation on lichess.org, …).
    pub fn end_game(&self, id: &str, status: GameStatus, winner: Option<Side>) {
        self.inner().end(id, status, winner);
    }

    // Challenges

    /// `from` challenges the account `to`: a `challenge` event on its stream. Answers the challenge id.
    pub fn send_challenge(&self, from: Seat, to: &str, req: &ChallengeRequest) -> String {
        let mut inner = self.inner();
        let id = inner.fresh_id("c");
        let c = Challenge {
            id: id.clone(),
            challenger: from,
            dest: Seat::account(to),
            rated: req.rated,
            time: req.time,
            color: req.color,
            variant: req.variant.clone(),
        };
        inner.create_challenge(c);
        id
    }

    /// How `user` (someone the mock plays for) answers every challenge from now on. Unset, challenges go unanswered.
    pub fn answer_challenges(&self, user: &str, answer: ChallengeAnswer) {
        self.inner().sticky_answers.insert(user.to_lowercase(), answer);
    }

    /// How `user` answers the next challenges, in turn, before falling back to [`MockLichess::answer_challenges`].
    pub fn queue_challenge_answers(&self, user: &str, answers: &[ChallengeAnswer]) {
        self.inner().queued_answers.entry(user.to_lowercase()).or_default().extend(answers.iter().copied());
    }

    /// The challenges still open, as Lichess would describe them.
    pub fn challenges(&self) -> Vec<Value> {
        let inner = self.inner();
        inner.challenges.values().map(|c| inner.challenge_json(c, "created")).collect()
    }

    /// The challenged side accepts: the game starts (its id is the challenge's).
    pub fn accept_challenge(&self, id: &str) -> Option<String> {
        self.inner().accept_challenge(id)
    }

    /// The challenged side declines: `challengeDeclined` to the challenger.
    pub fn decline_challenge(&self, id: &str, reason: DeclineReason) -> bool {
        self.inner().decline_challenge(id, reason)
    }

    /// The challenger withdraws: `challengeCanceled` to both.
    pub fn cancel_challenge(&self, id: &str) -> bool {
        self.inner().cancel_challenge(id)
    }

    // Seeks

    pub fn seeks(&self) -> Vec<MockSeek> {
        self.inner()
            .seeks
            .iter()
            .map(|s| MockSeek {
                id: s.id.clone(),
                account: s.account.clone(),
                form: s.form.clone(),
                open: s.open,
                canceled: s.canceled,
            })
            .collect()
    }

    /// Someone takes the seek: its connection closes and the game starts. Answers the game id.
    pub fn take_seek(&self, seek: &str, opponent: Seat) -> String {
        let mut inner = self.inner();
        let s = inner.seeks.iter_mut().find(|s| s.id == seek && s.open).expect("an open seek");
        s.open = false;
        s.sender = None;
        let (account, form) = (s.account.clone(), s.form.clone());
        let me = Seat::Account(account);
        let (white, black) =
            if form.get("color").map(String::as_str) == Some("black") { (opponent, me) } else { (me, opponent) };
        let mut setup = GameSetup::new(white, black).rated(form.get("rated").map(String::as_str) == Some("true"));
        match form.get("days") {
            Some(days) => setup = setup.correspondence(days.parse().unwrap_or(3)),
            None => {
                let minutes: f64 = form.get("time").and_then(|t| t.parse().ok()).unwrap_or(10.0);
                let increment = form.get("increment").and_then(|t| t.parse().ok()).unwrap_or(0);
                setup = setup.clock((minutes * 60.0).round() as u32, increment);
            }
        }
        inner.start_game(setup)
    }

    // Everything else

    /// What `GET /api/bot/online` lists.
    pub fn set_online_bots(&self, bots: Vec<Value>) {
        self.inner().online_bots = bots;
    }

    /// The games `GET /api/games/user/{username}` exports (see [`exported_game`]). Unknown users are a 404.
    pub fn set_games_export(&self, username: &str, games: Vec<Value>) {
        self.inner().exports.insert(username.to_lowercase(), games);
    }

    /// The next export sends this many games and then breaks the connection.
    pub fn break_export_after(&self, games: usize) {
        self.inner().export_break = Some(games);
    }

    /// A public profile (`GET /api/user/{id}`), by its `id`.
    pub fn set_user(&self, user: Value) {
        let id = user["id"].as_str().unwrap_or_default().to_lowercase();
        self.inner().users.insert(id, user);
    }

    pub fn set_leaderboard(&self, perf: &str, users: Vec<Value>) {
        self.inner().leaderboards.insert(perf.to_owned(), users);
    }

    /// What the explorer answers for `db` (`lichess`, `masters`, `player`). The player explorer sends each document in
    /// turn and keeps the connection open, as it does while indexing.
    pub fn set_explorer(&self, db: &str, documents: Vec<Value>) {
        self.inner().explorer.insert(db.to_owned(), documents);
    }

    pub fn set_tablebase(&self, answer: Value) {
        self.inner().tablebase = Some(answer);
    }

    /// The person opens the login link and allows access: the code Lichess would send back, which exchanges for
    /// `token` (an account of the mock).
    pub fn authorize(&self, url: &str, token: &str) -> Result<Authorized, String> {
        authorize(&mut self.inner(), url, token)
    }

    /// Makes the mock's `/oauth` page grant every login at once as `token`, redirecting the browser (303) back to the
    /// redirect URI with a code.
    pub fn grant_logins_as(&self, token: &str) {
        self.inner().grant_as = Some(token.to_owned());
    }
}

/// A finished game as the export sends it: a 3+2 rated blitz Scholar's mate won by White, with clocks, analysis and
/// the opening.
pub fn exported_game(id: &str, created_at: i64, white: &str, black: &str) -> Value {
    json!({
        "id": id, "rated": true, "variant": "standard", "speed": "blitz", "perf": "blitz", "createdAt": created_at,
        "lastMoveAt": created_at + 60_000, "status": "mate", "winner": "white", "source": "pool",
        "players": {
            "white": { "user": { "name": white, "id": white.to_lowercase() }, "rating": 1800, "ratingDiff": 6 },
            "black": { "user": { "name": black, "id": black.to_lowercase() }, "rating": 1790, "ratingDiff": -6 }
        },
        "opening": { "eco": "C20", "name": "King's Pawn Game", "ply": 2 },
        "moves": "e4 e5 Bc4 Nc6 Qh5 Nf6 Qxf7#",
        "clocks": [18003, 18003, 17800, 17900, 17500, 17000, 17300],
        "analysis": [{ "eval": 30 }, { "eval": 35 }, { "eval": 20 }, { "eval": 25 }, { "eval": 10 },
            { "mate": 1, "best": "g7g6", "judgment": { "name": "Blunder", "comment": "Checkmate is now unavoidable." } }],
        "clock": { "initial": 180, "increment": 2, "totalTime": 260 }
    })
}

// ---------------------------------------------------------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------------------------------------------------------

fn router(shared: Shared) -> Router {
    Router::new()
        .route("/oauth", get(oauth_page))
        .route("/api/token", post(token_exchange).delete(token_revoke))
        .route("/api/token/test", post(token_test))
        .route("/api/account", get(account))
        .route("/api/account/playing", get(playing))
        .route("/api/bot/account/upgrade", post(upgrade))
        .route("/api/bot/online", get(online_bots))
        .route("/api/stream/event", get(event_stream))
        .route("/api/bot/game/stream/{id}", get(game_stream))
        .route("/api/board/game/stream/{id}", get(game_stream))
        .route("/api/bot/game/{id}/{action}", post(game_action))
        .route("/api/board/game/{id}/{action}", post(game_action))
        .route("/api/bot/game/{id}/{action}/{arg}", post(game_action_arg))
        .route("/api/board/game/{id}/{action}/{arg}", post(game_action_arg))
        .route("/api/board/seek", post(seek))
        .route("/api/challenge", get(list_challenges))
        .route("/api/challenge/ai", post(challenge_ai))
        .route("/api/challenge/{username}", post(challenge_user))
        .route("/api/challenge/{id}/{action}", post(challenge_action))
        .route("/api/games/user/{username}", get(export))
        .route("/api/user/{username}", get(user))
        .route("/api/player/top/{nb}/{perf}", get(leaderboard))
        .route("/explorer/{db}", get(explorer))
        .route("/tablebase/standard", get(tablebase))
        .layer(middleware::from_fn_with_state(shared.clone(), record))
        .with_state(shared)
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
}

fn pairs(query: &str) -> HashMap<String, String> {
    let mut url = reqwest::Url::parse("http://x/").expect("a valid URL");
    url.set_query(Some(query));
    url.query_pairs().into_owned().collect()
}

fn refuse(status: u16, message: &str) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST);
    (status, Json(json!({ "error": message }))).into_response()
}

fn ok() -> Response {
    Json(json!({ "ok": true })).into_response()
}

/// Records the request, then fails it, delays it, or lets it through, counting what is in flight per token.
async fn record(State(shared): State<Shared>, req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 20).await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let is_form = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-www-form-urlencoded"));
    let token = bearer(&parts.headers);
    let path = parts.uri.path().to_owned();
    let recorded = RecordedRequest {
        method: parts.method.to_string(),
        path: path.clone(),
        query: pairs(parts.uri.query().unwrap_or_default()),
        form: if is_form { pairs(&text) } else { HashMap::new() },
        body: text,
        token: token.clone(),
        user_agent: parts.headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned(),
    };
    let key = token.unwrap_or_default();
    let (failure, delay) = {
        let mut inner = shared.lock().unwrap();
        inner.requests.push(recorded);
        let failure =
            inner.failures.iter().position(|f| path.starts_with(&f.prefix)).and_then(|i| inner.failures.remove(i));
        let delay: Duration = inner.delays.iter().filter(|(p, _)| path.starts_with(p)).map(|(_, d)| *d).sum();
        let now = {
            let n = inner.inflight.entry(key.clone()).or_default();
            *n += 1;
            *n
        };
        let max = inner.max_inflight.entry(key.clone()).or_default();
        *max = (*max).max(now);
        (failure, delay)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    let res = match failure {
        Some(f) => (StatusCode::from_u16(f.status).unwrap_or(StatusCode::BAD_REQUEST), Json(f.body)).into_response(),
        None => next.run(Request::from_parts(parts, Body::from(bytes))).await,
    };
    if let Some(n) = shared.lock().unwrap().inflight.get_mut(&key) {
        *n = n.saturating_sub(1);
    }
    res
}

/// A stream of JSON lines with a keep-alive every half second, ending when `rx` closes. `guard` lives as long as the
/// connection, so its `Drop` tells when the client hung up.
fn ndjson_body<G: Send + 'static>(rx: UnboundedReceiver<Value>, guard: G) -> Response {
    let stream = futures::stream::unfold((rx, guard), |(mut rx, guard)| async move {
        tokio::select! {
            v = rx.recv() => v.map(|v| (Ok::<_, Infallible>(Bytes::from(format!("{v}\n"))), (rx, guard))),
            _ = tokio::time::sleep(Duration::from_millis(500)) => Some((Ok(Bytes::from_static(b"\n")), (rx, guard))),
        }
    });
    ([(header::CONTENT_TYPE, "application/x-ndjson")], Body::from_stream(stream)).into_response()
}

fn authorize(inner: &mut Inner, url: &str, token: &str) -> Result<Authorized, String> {
    let url = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let get = |k: &str| q.get(k).cloned().ok_or_else(|| format!("the login link has no {k}"));
    if get("response_type")? != "code" || get("code_challenge_method")? != "S256" {
        return Err("Lichess wants response_type=code and code_challenge_method=S256".into());
    }
    if get("client_id")?.is_empty() {
        return Err("the login link has no client_id".into());
    }
    let code = inner.fresh_id("liu_");
    let redirect_uri = get("redirect_uri")?;
    inner.codes.insert(
        code.clone(),
        OAuthCode { challenge: get("code_challenge")?, redirect_uri: redirect_uri.clone(), token: token.to_owned() },
    );
    Ok(Authorized {
        code,
        state: get("state")?,
        redirect_uri,
        scopes: get("scope").unwrap_or_default().split_whitespace().map(str::to_owned).collect(),
    })
}

async fn oauth_page(State(shared): State<Shared>, req: Request) -> Response {
    let mut inner = shared.lock().unwrap();
    let Some(token) = inner.grant_as.clone() else {
        return (StatusCode::BAD_REQUEST, "The mock grants no logins (grant_logins_as).").into_response();
    };
    let url = format!("http://mock{}", req.uri());
    match authorize(&mut inner, &url, &token) {
        Ok(a) => {
            let mut back = match reqwest::Url::parse(&a.redirect_uri) {
                Ok(u) => u,
                Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            };
            back.query_pairs_mut().append_pair("code", &a.code).append_pair("state", &a.state);
            (StatusCode::SEE_OTHER, [(header::LOCATION, back.to_string())]).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn token_exchange(State(shared): State<Shared>, req: Request) -> Response {
    let body = axum::body::to_bytes(req.into_body(), 1 << 16).await.unwrap_or_default();
    let f = pairs(&String::from_utf8_lossy(&body));
    let mut inner = shared.lock().unwrap();
    let invalid = |why: &str| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant", "error_description": why }))).into_response()
    };
    if f.get("grant_type").map(String::as_str) != Some("authorization_code") {
        return invalid("grant_type must be authorization_code");
    }
    if f.get("client_id").map(String::as_str) != Some(CLIENT_ID) {
        return invalid("unknown client_id");
    }
    let key = f.get("code").cloned().unwrap_or_default();
    let Some(code) = inner.codes.get(&key) else {
        return invalid("unknown or used code");
    };
    if f.get("redirect_uri") != Some(&code.redirect_uri) {
        return invalid("redirect_uri mismatch");
    }
    if f.get("code_verifier").map(|v| challenge_of(v)) != Some(code.challenge.clone()) {
        return invalid("hash of code_verifier does not match code_challenge");
    }
    // Used once it has worked.
    let token = inner.codes.remove(&key).map(|c| c.token).unwrap_or_default();
    Json(json!({ "token_type": "Bearer", "access_token": token, "expires_in": 31_536_000 })).into_response()
}

async fn token_revoke(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(token) = bearer(&headers) {
        shared.lock().unwrap().accounts.remove(&token);
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn token_test(State(shared): State<Shared>, body: String) -> Response {
    let inner = shared.lock().unwrap();
    let mut out = serde_json::Map::new();
    for token in body.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let info = inner
            .accounts
            .get(token)
            .map_or(Value::Null, |a| json!({ "scopes": a.scopes.join(","), "userId": a.id, "expires": a.expires }));
        out.insert(token.to_owned(), info);
    }
    Json(Value::Object(out)).into_response()
}

async fn account(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    match shared.lock().unwrap().account_by_token(&headers) {
        Some(a) => Json(a.json()).into_response(),
        None => refuse(401, "No such token"),
    }
}

async fn playing(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    let inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let now: Vec<Value> = inner
        .games
        .values()
        .filter(|g| !g.status.is_over())
        .filter_map(|g| g.side_of(&me.id).map(|s| inner.info_json(g, s)))
        .collect();
    Json(json!({ "nowPlaying": now })).into_response()
}

async fn upgrade(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    let mut inner = shared.lock().unwrap();
    let Some(token) = bearer(&headers).filter(|t| inner.accounts.contains_key(t)) else {
        return refuse(401, "No such token");
    };
    let account = inner.accounts.get_mut(&token).expect("checked");
    if account.played {
        return refuse(400, "This account has already played games and cannot become a bot account.");
    }
    account.title = Some("BOT".into());
    ok()
}

async fn online_bots(State(shared): State<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    let inner = shared.lock().unwrap();
    let nb = q.get("nb").and_then(|n| n.parse().ok()).unwrap_or(usize::MAX);
    let body: String = inner.online_bots.iter().take(nb).map(|b| format!("{b}\n")).collect();
    ([(header::CONTENT_TYPE, "application/x-ndjson")], body).into_response()
}

async fn event_stream(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let (tx, rx) = unbounded_channel();
    // A new connection learns the games in progress and the open challenges first.
    for game in inner.games.values().filter(|g| !g.status.is_over()) {
        if let Some(side) = game.side_of(&me.id) {
            let _ = tx.send(json!({ "type": "gameStart", "game": inner.info_json(game, side) }));
        }
    }
    for c in inner.challenges.values() {
        if Inner::challenge_parties(c).contains(&me.id) {
            let _ = tx.send(json!({ "type": "challenge", "challenge": inner.challenge_json(c, "created") }));
        }
    }
    let feed = inner.feeds.entry(me.id.clone()).or_default();
    for event in feed.queue.drain(..) {
        let _ = tx.send(event);
    }
    feed.subscribers.push(tx);
    ndjson_body(rx, ())
}

async fn game_stream(State(shared): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let Some(game) = inner.games.get(&id).filter(|g| g.side_of(&me.id).is_some()) else {
        return refuse(404, "No such game");
    };
    let (tx, rx) = unbounded_channel();
    let _ = tx.send(inner.full_json(game));
    let over = game.status.is_over();
    if !over {
        inner.games.get_mut(&id).expect("checked").streams.push(tx);
    }
    ndjson_body(rx, ())
}

async fn game_action(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Path((id, action)): Path<(String, String)>,
) -> Response {
    act(&shared, &headers, &id, &action, None, &HashMap::new())
}

async fn game_action_arg(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Path((id, action, arg)): Path<(String, String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    act(&shared, &headers, &id, &action, Some(&arg), &q)
}

/// One action on a game by the account whose token came with it.
fn act(
    shared: &Shared,
    headers: &HeaderMap,
    id: &str,
    action: &str,
    arg: Option<&str>,
    q: &HashMap<String, String>,
) -> Response {
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(headers) else { return refuse(401, "No such token") };
    let Some(game) = inner.games.get(id) else { return refuse(404, "No such game") };
    let Some(side) = game.side_of(&me.id) else { return refuse(404, "No such game") };
    if action == "chat" {
        return ok();
    }
    if game.status.is_over() {
        return refuse(400, "The game is over");
    }
    let (turn, plies, gone, accepts) = (game.turn(), game.moves.len(), game.gone, game.setup.accepts_offers);
    let their = game.draw == Some(side.other());
    let their_takeback = game.takeback == Some(side.other());
    let mock_plays_them = !matches!(game.seat(side.other()), Seat::Account(_));
    match (action, arg) {
        ("move", Some(uci)) => {
            if turn != side {
                return refuse(400, "Not your turn, or game already over");
            }
            if let Err(e) = inner.apply_move(id, uci) {
                return refuse(400, &e);
            }
            if q.get("offeringDraw").map(String::as_str) == Some("true") {
                inner.offer(id, side, true);
            }
            inner.auto_reply(id);
        }
        ("resign", None) => inner.end(id, GameStatus::Resign, Some(side.other())),
        ("abort", None) => {
            if plies >= 2 {
                return refuse(400, "This game can no longer be aborted");
            }
            inner.end(id, GameStatus::Aborted, None);
        }
        ("claim-victory", None) => {
            if gone != Some(side.other()) {
                return refuse(400, "The opponent has not left");
            }
            inner.end(id, GameStatus::Timeout, Some(side));
        }
        ("claim-draw", None) => {
            if gone != Some(side.other()) {
                return refuse(400, "The opponent has not left");
            }
            inner.end(id, GameStatus::Draw, None);
        }
        ("draw", Some("yes" | "accept" | "true")) => {
            if their || (mock_plays_them && accepts) {
                inner.end(id, GameStatus::Draw, None);
            } else {
                inner.offer(id, side, true);
            }
        }
        ("draw", Some(_)) => {
            if their && let Some(g) = inner.games.get_mut(id) {
                g.draw = None;
                let state = Inner::state_json(g);
                inner.broadcast(id, state);
            }
        }
        ("takeback", Some("yes" | "accept" | "true")) => {
            if their_takeback {
                inner.take_back(id, side.other());
            } else if mock_plays_them && accepts {
                inner.take_back(id, side);
            } else {
                inner.offer(id, side, false);
            }
        }
        ("takeback", Some(_)) => {
            if their_takeback && let Some(g) = inner.games.get_mut(id) {
                g.takeback = None;
                let state = Inner::state_json(g);
                inner.broadcast(id, state);
            }
        }
        _ => return refuse(404, "No such action"),
    }
    ok()
}

/// Marks a seek cancelled when its connection goes while it is still open.
struct SeekGuard {
    shared: Shared,
    id: String,
}

impl Drop for SeekGuard {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.shared.lock()
            && let Some(s) = inner.seeks.iter_mut().find(|s| s.id == self.id && s.open)
        {
            s.open = false;
            s.canceled = true;
        }
    }
}

async fn seek(State(shared): State<Shared>, headers: HeaderMap, req: Request) -> Response {
    let body = axum::body::to_bytes(req.into_body(), 1 << 16).await.unwrap_or_default();
    let form = pairs(&String::from_utf8_lossy(&body));
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let id = inner.fresh_id("s");
    if form.contains_key("days") {
        // A correspondence seek stays in the lobby: Lichess answers its id at once.
        inner.seeks.push(Seek { id: id.clone(), account: me.id, form, open: true, canceled: false, sender: None });
        return Json(json!({ "id": id })).into_response();
    }
    let (tx, rx) = unbounded_channel();
    inner.seeks.push(Seek { id: id.clone(), account: me.id, form, open: true, canceled: false, sender: Some(tx) });
    drop(inner);
    ndjson_body(rx, SeekGuard { shared: shared.clone(), id })
}

async fn list_challenges(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    let inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();
    for c in inner.challenges.values() {
        let mut v = inner.challenge_json(c, "created");
        if c.dest == Seat::Account(me.id.clone()) {
            v["direction"] = json!("in");
            incoming.push(v);
        } else if c.challenger == Seat::Account(me.id.clone()) {
            v["direction"] = json!("out");
            outgoing.push(v);
        }
    }
    Json(json!({ "in": incoming, "out": outgoing })).into_response()
}

fn challenge_time(form: &HashMap<String, String>) -> ChallengeTime {
    let n = |k: &str| form.get(k).and_then(|v| v.parse::<u32>().ok());
    match (n("clock.limit"), n("clock.increment"), n("days")) {
        (Some(limit_seconds), increment, _) => {
            ChallengeTime::Clock { limit_seconds, increment_seconds: increment.unwrap_or(0) }
        }
        (None, _, Some(days)) => ChallengeTime::Correspondence { days },
        _ => ChallengeTime::Unlimited,
    }
}

fn challenge_color(form: &HashMap<String, String>) -> Color {
    match form.get("color").map(String::as_str) {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        _ => Color::Random,
    }
}

async fn challenge_user(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Path(username): Path<String>,
    req: Request,
) -> Response {
    let body = axum::body::to_bytes(req.into_body(), 1 << 16).await.unwrap_or_default();
    let form = pairs(&String::from_utf8_lossy(&body));
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let dest_id = username.to_lowercase();
    if dest_id == me.id {
        return refuse(400, "You cannot challenge yourself");
    }
    let dest = if inner.account(&dest_id).is_some() {
        Seat::Account(dest_id.clone())
    } else {
        Seat::User { id: dest_id.clone(), name: username.clone(), rating: None, title: None }
    };
    let id = inner.fresh_id("c");
    let c = Challenge {
        id: id.clone(),
        challenger: Seat::Account(me.id),
        dest,
        rated: form.get("rated").map(String::as_str) == Some("true"),
        time: challenge_time(&form),
        color: challenge_color(&form),
        variant: form.get("variant").cloned().unwrap_or_else(|| "standard".into()),
    };
    let created = inner.create_challenge(c);
    let answer = inner.answer_for(&dest_id);
    drop(inner);
    // Answered a moment after Lichess has answered us, as a person or a bot would.
    if answer != ChallengeAnswer::Ignore {
        let shared = shared.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let mut inner = shared.lock().unwrap();
            match answer {
                ChallengeAnswer::Accept => {
                    inner.accept_challenge(&id);
                }
                ChallengeAnswer::Decline(reason) => {
                    inner.decline_challenge(&id, reason);
                }
                ChallengeAnswer::Ignore => {}
            }
        });
    }
    Json(created).into_response()
}

async fn challenge_ai(State(shared): State<Shared>, headers: HeaderMap, req: Request) -> Response {
    let body = axum::body::to_bytes(req.into_body(), 1 << 16).await.unwrap_or_default();
    let form = pairs(&String::from_utf8_lossy(&body));
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let Some(level) = form.get("level").and_then(|l| l.parse::<u8>().ok()).filter(|l| (1..=8).contains(l)) else {
        return refuse(400, "level must be 1 to 8");
    };
    let mine = challenge_color(&form);
    let (white, black, side) = if mine == Color::Black {
        (Seat::Ai(level), Seat::Account(me.id.clone()), Side::Black)
    } else {
        (Seat::Account(me.id.clone()), Seat::Ai(level), Side::White)
    };
    let mut setup = GameSetup::new(white, black);
    match challenge_time(&form) {
        ChallengeTime::Clock { limit_seconds, increment_seconds } => {
            setup = setup.clock(limit_seconds, increment_seconds)
        }
        ChallengeTime::Correspondence { days } => setup = setup.correspondence(days),
        ChallengeTime::Unlimited => setup = setup.correspondence(14),
    }
    let speed = setup.speed();
    let id = inner.start_game(setup);
    let created_at = inner.games[&id].created_at;
    Json(json!({
        "id": id, "variant": { "key": "standard", "name": "Standard" }, "speed": speed, "perf": speed,
        "rated": false, "fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "turns": 0,
        "source": "ai", "status": { "id": 20, "name": "started" }, "createdAt": created_at, "player": side,
    }))
    .into_response()
}

async fn challenge_action(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Path((id, action)): Path<(String, String)>,
    req: Request,
) -> Response {
    let body = axum::body::to_bytes(req.into_body(), 1 << 16).await.unwrap_or_default();
    let form = pairs(&String::from_utf8_lossy(&body));
    let mut inner = shared.lock().unwrap();
    let Some(me) = inner.account_by_token(&headers) else { return refuse(401, "No such token") };
    let me = Seat::Account(me.id);
    let Some(c) = inner.challenges.get(&id) else { return refuse(404, "No such challenge") };
    let (to_me, from_me) = (c.dest == me, c.challenger == me);
    match action.as_str() {
        "accept" if to_me => {
            inner.accept_challenge(&id);
        }
        "decline" if to_me => {
            let reason = form
                .get("reason")
                .and_then(|r| serde_json::from_value::<DeclineReason>(json!(r)).ok())
                .unwrap_or(DeclineReason::Generic);
            inner.decline_challenge(&id, reason);
        }
        "cancel" if from_me || to_me => {
            inner.cancel_challenge(&id);
        }
        _ => return refuse(404, "No such challenge"),
    }
    ok()
}

async fn export(
    State(shared): State<Shared>,
    Path(username): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let mut inner = shared.lock().unwrap();
    let Some(games) = inner.exports.get(&username.to_lowercase()).cloned() else {
        return refuse(404, "Not found");
    };
    let n = |k: &str| q.get(k).and_then(|v| v.parse::<i64>().ok());
    let (since, until, max) = (n("since"), n("until"), n("max"));
    let perfs: Option<Vec<&str>> = q.get("perfType").map(|p| p.split(',').collect());
    let rated = q.get("rated").map(|r| r == "true");
    let mut games: Vec<Value> = games
        .into_iter()
        .filter(|g| {
            let created = g["createdAt"].as_i64().unwrap_or(0);
            since.is_none_or(|s| created >= s)
                && until.is_none_or(|u| created <= u)
                && perfs.as_ref().is_none_or(|p| p.contains(&g["speed"].as_str().unwrap_or_default()))
                && rated.is_none_or(|r| g["rated"].as_bool() == Some(r))
        })
        .collect();
    games.sort_by_key(|g| std::cmp::Reverse(g["createdAt"].as_i64().unwrap_or(0)));
    if q.get("sort").map(String::as_str) == Some("dateAsc") {
        games.reverse();
    }
    if let Some(max) = max {
        games.truncate(max.max(0) as usize);
    }
    let breaks_after = inner.export_break.take();
    let lines: Vec<Result<Bytes, std::io::Error>> =
        games.iter().take(breaks_after.unwrap_or(usize::MAX)).map(|g| Ok(Bytes::from(format!("{g}\n")))).collect();
    let body = futures::stream::iter(lines);
    let body = match breaks_after {
        // The games go out first; then the connection breaks.
        Some(_) => Body::from_stream(body.chain(futures::stream::once(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Err(std::io::Error::other("the mock broke the export"))
        }))),
        None => Body::from_stream(body),
    };
    ([(header::CONTENT_TYPE, "application/x-ndjson")], body).into_response()
}

async fn user(State(shared): State<Shared>, Path(username): Path<String>) -> Response {
    match shared.lock().unwrap().users.get(&username.to_lowercase()) {
        Some(u) => Json(u.clone()).into_response(),
        None => refuse(404, "Not found"),
    }
}

async fn leaderboard(State(shared): State<Shared>, Path((nb, perf)): Path<(usize, String)>) -> Response {
    let inner = shared.lock().unwrap();
    let users: Vec<Value> = inner.leaderboards.get(&perf).into_iter().flatten().take(nb).cloned().collect();
    Json(json!({ "users": users })).into_response()
}

async fn explorer(State(shared): State<Shared>, headers: HeaderMap, Path(db): Path<String>) -> Response {
    let inner = shared.lock().unwrap();
    if inner.account_by_token(&headers).is_none() {
        return refuse(401, "Unauthorized");
    }
    let docs = inner
        .explorer
        .get(&db)
        .cloned()
        .unwrap_or_else(|| vec![json!({ "white": 0, "draws": 0, "black": 0, "moves": [] })]);
    drop(inner);
    if db != "player" {
        return Json(docs.into_iter().next().unwrap_or(Value::Null)).into_response();
    }
    let (tx, rx) = unbounded_channel();
    tokio::spawn(async move {
        for doc in docs {
            if tx.send(doc).is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        // Still indexing: the connection stays open until the client goes.
        tx.closed().await;
    });
    ndjson_body(rx, ())
}

async fn tablebase(State(shared): State<Shared>) -> Response {
    let answer = shared.lock().unwrap().tablebase.clone().unwrap_or_else(|| json!({ "category": "unknown" }));
    Json(answer).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::Event, game::GameEvent};

    #[tokio::test]
    async fn a_game_ends_on_checkmate_and_the_ai_answers() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        let id = mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Bob", 1500)));
        let client = mock.client(Some("tok"));
        let mut events = client.events().await.unwrap();
        events.next().await.unwrap();
        for (ours, theirs) in [("e2e4", "e7e5"), ("f1c4", "b8c6"), ("d1h5", "g8f6")] {
            client.board().make_move(&id, ours).await.unwrap();
            mock.opponent_move(&id, theirs);
        }
        client.board().make_move(&id, "h5f7").await.unwrap();
        let g = mock.game(&id).unwrap();
        assert_eq!((g.status, g.winner), (GameStatus::Mate, Some(Side::White)));
        let Some(Event::GameFinish { game }) = events.next().await.unwrap() else { panic!() };
        assert_eq!(game.status.unwrap().name, GameStatus::Mate);

        let ai = mock.start_game(GameSetup::new(Seat::ai(2), Seat::account("dame")));
        assert_eq!(mock.game(&ai).unwrap().moves.len(), 1);
        let mut stream = client.board().stream_game(&ai).await.unwrap();
        let Some(GameEvent::GameFull(full)) = stream.next().await.unwrap() else { panic!() };
        assert_eq!(full.white.ai_level, Some(2));
    }

    #[tokio::test]
    async fn a_test_s_failures_come_once_and_streams_are_counted() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::bot("RecklessBot"));
        mock.bot_daily_limit_next("/api/challenge/", true, 600);
        let client = mock.client(Some("tok"));
        let req = ChallengeRequest::clock(true, 180, 0);
        let e = client.challenge_user("alpha", &req).await.unwrap_err();
        let limit = e.bot_daily_limit(0).unwrap();
        assert!(limit.ours);
        assert_eq!(limit.retry_after, Duration::from_secs(600));
        mock.until("the pause", |_| client.rate_limited_for().is_none()).await;
        client.challenge_user("alpha", &req).await.unwrap();
        let events = client.events().await.unwrap();
        mock.until("the event stream", |m| m.event_streams("recklessbot") == 1).await;
        drop(events);
        mock.until("the stream to go", |m| m.event_streams("recklessbot") == 0).await;
    }
}
