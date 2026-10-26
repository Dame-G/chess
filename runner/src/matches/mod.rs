//! Engine-vs-engine matches you can watch. A match plays two builds against each other over UCI, move by move, and
//! reports each move and the thinking engine's progress as events (`match.started`, `match.move`, `match.info`,
//! `match.ended`). Like live analysis it runs outside the job queue, so a long experiment never holds it up. One match
//! runs at a time, on two search threads. Every game is stored: finished ones replay, and `/pgn` exports them.
//!
//! The game itself is a plain record ([`Match`]) that the player task fills in, from an engine or, for a seat a person
//! takes, from the moves the board sends (`POST /api/matches/{id}/moves`). Games played elsewhere (the
//! Lichess bot's) fill the same record through a [`Recorder`], so they show on the same board and replay the same way.

mod pgn;
pub mod rules;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove};
use tokio::{
    sync::{mpsc, oneshot},
    time::MissedTickBehavior,
};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use self::rules::{Clock, Ending, GRACE, GameResult, Judged, Reason, Side, Spent};
use crate::{
    api::{ApiError, AppState},
    engine::{
        build::{self, Build},
        uci::{Engine, Info, Score, Search},
    },
    tools,
};

pub use pgn::to_pgn;

/// How a match is timed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MatchTime {
    /// A clock for each side: `seconds` for the game plus `increment` seconds after every move.
    Clock { seconds: f64, increment: f64 },
    /// A fixed number of nodes per move. There is no clock.
    Nodes { nodes: u64 },
    /// A fixed time per move, in milliseconds. There is no clock.
    Movetime { ms: u64 },
}

/// Where the game starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Opening {
    /// The standard start position.
    #[default]
    Start,
    /// A FEN (or `startpos`) with moves played from it, in UCI notation.
    Position {
        fen: String,
        #[serde(default)]
        moves: Vec<String>,
    },
    /// A random line of the opening book the tests use.
    Book,
}

/// Who plays a side of a match you start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum Seat {
    /// A build, by id.
    Build(i64),
    /// You, moving on the board through `POST /api/matches/{id}/moves`.
    Human(HumanSeat),
}

/// The string `"human"`: the seat a person takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum HumanSeat {
    #[serde(rename = "human")]
    Human,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MatchRequest {
    /// Who plays White: a build id, or `"human"` for you.
    pub white: Seat,
    /// Who plays Black: a build id, or `"human"` for you. At least one side must be a build.
    pub black: Seat,
    pub time: MatchTime,
    #[serde(default)]
    pub opening: Opening,
    /// Hash table size in MiB for each engine. Default 16, as in the tests.
    pub hash: Option<u32>,
    /// Search threads for each engine. Default 1, so a match uses two threads and can run beside a test.
    pub threads: Option<u32>,
    /// Reckless's `Minimal` option, which the tests set: the engine then reports only its final line, so there is nothing
    /// to watch while it thinks. Off by default, so `match.info` events flow; it does not change how the engine plays.
    pub minimal: Option<bool>,
    /// End decided games early, as the tests do: a side that has scored -4 or worse for three moves loses, and a
    /// level game drawn from move 30. Off by default, so the game is played out.
    pub adjudicate: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct Player {
    /// The build that played, by id. Absent for a Lichess opponent, which is not one of ours.
    #[serde(default)]
    pub build: Option<i64>,
    /// The ref the build was made from, such as `main`; for a Lichess opponent, its username.
    pub name: String,
    /// The first seven characters of its commit; empty for a Lichess opponent.
    pub commit: String,
    /// The Lichess rating the player had for this game's speed.
    #[serde(default)]
    pub rating: Option<u32>,
    /// The Lichess title, such as `BOT` or `GM`.
    #[serde(default)]
    pub title: Option<String>,
    /// The Lichess username the player played under, ours included.
    #[serde(default)]
    pub lichess: Option<String>,
    /// A person moving on the board rather than an engine; `name` is then "You".
    #[serde(default)]
    pub human: bool,
}

/// What a game played on Lichess adds to the record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LichessGame {
    /// The Lichess game id, such as `q7ZvsdUF`.
    pub id: String,
    /// `https://lichess.org/<id>`.
    pub url: String,
    pub rated: bool,
    /// `bullet`, `blitz`, `rapid` or `classical`, as Lichess names the game's speed.
    pub speed: String,
    /// The side our build played.
    pub our_side: Side,
}

/// Which matches a list holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Matches between two builds on this machine.
    Local,
    /// Games the bot played on Lichess.
    Lichess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Running,
    Finished,
    /// Stopped before the game ended.
    Aborted,
}

/// One move of a game, with what the engine that played it reported.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct MatchMove {
    /// 1 for the first move of the game.
    pub ply: u32,
    pub side: Side,
    pub uci: String,
    pub san: String,
    pub fen_after: String,
    /// What each clock holds after the move, increment included. Absent when the match has no clock.
    pub clock_white_ms: Option<u64>,
    pub clock_black_ms: Option<u64>,
    /// The mover's last score, from the mover's side.
    pub eval: Option<Score>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    /// Measured think time.
    pub time_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Match {
    pub id: i64,
    pub white: Player,
    pub black: Player,
    pub time: MatchTime,
    /// The position the game started from: `startpos` or a FEN. Any opening moves are already played in it.
    pub start_fen: String,
    /// Whether the opening came from the book.
    pub book: bool,
    pub hash: u32,
    pub threads: u32,
    pub adjudicate: bool,
    /// Whether the engines ran with `Minimal` on, and so reported nothing while thinking.
    #[serde(default)]
    pub minimal: bool,
    pub status: MatchStatus,
    pub result: GameResult,
    /// Why the game ended, once it has.
    pub reason: Option<Reason>,
    /// The side to move.
    pub turn: Side,
    /// What each clock holds now, the thinking side's counting down. Absent when the match has no clock.
    pub clock_white_ms: Option<u64>,
    pub clock_black_ms: Option<u64>,
    pub moves: Vec<MatchMove>,
    pub created_at: String,
    pub finished_at: Option<String>,
    /// Set when the bot played the game on Lichess. Its clocks are Lichess's, and the opponent's moves carry no
    /// evaluation.
    #[serde(default)]
    pub lichess: Option<LichessGame>,
}

/// A match without its moves, for lists.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MatchSummary {
    pub id: i64,
    pub white: Player,
    pub black: Player,
    pub time: MatchTime,
    pub start_fen: String,
    pub status: MatchStatus,
    pub result: GameResult,
    pub reason: Option<Reason>,
    pub plies: u32,
    pub created_at: String,
    pub lichess: Option<LichessGame>,
}

/// A build as a player of a game.
pub fn player(b: &Build) -> Player {
    Player {
        build: Some(b.id),
        name: b.refname.clone(),
        commit: b.commit.chars().take(7).collect(),
        ..Player::default()
    }
}

impl Match {
    pub fn summary(&self) -> MatchSummary {
        MatchSummary {
            id: self.id,
            white: self.white.clone(),
            black: self.black.clone(),
            time: self.time,
            start_fen: self.start_fen.clone(),
            status: self.status,
            result: self.result,
            reason: self.reason,
            plies: self.moves.len() as u32,
            created_at: self.created_at.clone(),
            lichess: self.lichess.clone(),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------------------------------------------------

fn save(c: &Connection, m: &Match) -> Result<()> {
    let status = serde_json::to_value(m.status)?;
    c.execute(
        "UPDATE matches SET status = ?2, data = ?3 WHERE id = ?1",
        params![m.id, status.as_str().unwrap_or_default(), serde_json::to_string(m)?],
    )?;
    Ok(())
}

fn load(c: &Connection, id: i64) -> Result<Option<Match>> {
    let data: Option<String> = c.query_row("SELECT data FROM matches WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    data.map(|d| serde_json::from_str(&d).context("read a stored match")).transpose()
}

fn recent(c: &Connection, limit: u32, source: Option<Source>) -> Result<Vec<MatchSummary>> {
    let filter = match source {
        None => "",
        Some(Source::Local) => "WHERE json_extract(data, '$.lichess') IS NULL",
        Some(Source::Lichess) => "WHERE json_extract(data, '$.lichess') IS NOT NULL",
    };
    let mut stmt = c.prepare(&format!("SELECT data FROM matches {filter} ORDER BY id DESC LIMIT ?1"))?;
    let rows = stmt.query_map([limit], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for data in rows {
        out.push(serde_json::from_str::<Match>(&data?).context("read a stored match")?.summary());
    }
    Ok(out)
}

/// A match that was running when the runner stopped cannot go on: it is kept as it stood, aborted.
pub async fn recover(state: &AppState) -> Result<()> {
    state
        .store
        .call(|c| {
            let ids: Vec<i64> = c
                .prepare("SELECT id FROM matches WHERE status = 'running'")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for id in ids {
                if let Some(mut m) = load(c, id)? {
                    m.status = MatchStatus::Aborted;
                    m.reason = Some(Reason::Aborted);
                    m.finished_at = Some(now(c)?);
                    save(c, &m)?;
                }
            }
            Ok(())
        })
        .await
}

fn now(c: &Connection) -> Result<String> {
    Ok(c.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |r| r.get(0))?)
}

// ---------------------------------------------------------------------------------------------------------------------
// The running match
// ---------------------------------------------------------------------------------------------------------------------

/// The match in progress, shared between the player task and the API.
struct Shared {
    game: Match,
    /// When the side to move started thinking, so a reader sees its clock run down between moves.
    thinking: Option<Instant>,
}

struct Live {
    id: i64,
    cancel: CancellationToken,
    shared: Arc<Mutex<Shared>>,
    /// Where a human player's moves go, when the match has one.
    human: Option<mpsc::UnboundedSender<Request>>,
}

/// What a human player asks of their game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A move in UCI notation.
    Move(String),
    /// Take back the last move of each side. Only in a game without a clock, on your own turn.
    Takeback,
    /// Offer a draw, which the engine accepts or declines by [`rules::accepts_draw`].
    Draw,
}

/// How the game answered an [`Action`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Moved,
    TakenBack,
    Draw { accepted: bool },
}

type Ack = oneshot::Sender<Result<Reply, ApiError>>;

struct Request {
    action: Action,
    ack: Ack,
}

pub struct Matches {
    /// The one local match.
    live: Mutex<Option<Live>>,
    /// Held while a local match starts (its engines load before it takes `live`), so two starts cannot both pass the
    /// check that none is running.
    starting: tokio::sync::Mutex<()>,
    /// Games recorded from elsewhere (the Lichess bot's), any number at once, beside the local one.
    external: Mutex<HashMap<i64, Live>>,
}

impl Matches {
    pub fn new() -> Self {
        Self { live: Mutex::new(None), starting: tokio::sync::Mutex::new(()), external: Mutex::new(HashMap::new()) }
    }

    fn shared(&self, id: i64) -> Option<Arc<Mutex<Shared>>> {
        if let Some(shared) = self.live.lock().unwrap().as_ref().filter(|l| l.id == id).map(|l| l.shared.clone()) {
            return Some(shared);
        }
        self.external.lock().unwrap().get(&id).map(|l| l.shared.clone())
    }
}

/// The running match as a reader should see it: the thinking side's clock already run down.
fn snapshot(shared: &Mutex<Shared>) -> Match {
    let s = shared.lock().unwrap();
    let mut m = s.game.clone();
    if let (Some(since), MatchStatus::Running) = (s.thinking, m.status) {
        let spent = since.elapsed().as_millis() as u64;
        let clock = match m.turn {
            Side::White => &mut m.clock_white_ms,
            Side::Black => &mut m.clock_black_ms,
        };
        if let Some(left) = clock {
            *left = left.saturating_sub(spent);
        }
    }
    m
}

pub async fn get(state: &AppState, id: i64) -> Result<Option<Match>> {
    if let Some(shared) = state.matches.shared(id) {
        return Ok(Some(snapshot(&shared)));
    }
    state.store.call(move |c| load(c, id)).await
}

pub async fn list(state: &AppState, limit: u32, source: Option<Source>) -> Result<Vec<MatchSummary>> {
    state.store.call(move |c| recent(c, limit, source)).await
}

/// Stops the running match, which is recorded as aborted, and returns it. 409 when that match is not running.
pub async fn stop(state: &AppState, id: i64) -> Result<Match, ApiError> {
    let live = state
        .matches
        .live
        .lock()
        .unwrap()
        .as_ref()
        .filter(|l| l.id == id)
        .map(|l| (l.cancel.clone(), l.shared.clone()));
    // A game recorded from elsewhere ends where it is played: cancelling asks its player to resign (or abort) it, and
    // the record ends when that game does.
    let external = state.matches.external.lock().unwrap().get(&id).map(|l| l.cancel.clone());
    if let Some(cancel) = external {
        cancel.cancel();
        return get(state, id).await?.ok_or(ApiError::not_found("no such match"));
    }
    let Some((cancel, shared)) = live else {
        return match get(state, id).await? {
            Some(_) => Err(ApiError::conflict("that match is not running")),
            None => Err(ApiError::not_found("no such match")),
        };
    };
    cancel.cancel();
    // The player task records the end; wait for it so the answer is the final record.
    for _ in 0..100 {
        if shared.lock().unwrap().game.status != MatchStatus::Running {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    get(state, id).await?.ok_or(ApiError::not_found("no such match"))
}

/// Everything about the match that is known before the first move.
struct Plan {
    /// Which sides a person plays.
    humans: [bool; 2],
    time: MatchTime,
    start: String,
    pos: Chess,
    book: bool,
    hash: u32,
    threads: u32,
    adjudicate: bool,
    minimal: bool,
}

/// How long to wait for a game to answer a human's request before saying it moved on.
const ANSWER: Duration = Duration::from_secs(5);

pub async fn start(state: &Arc<AppState>, req: MatchRequest) -> Result<Match, ApiError> {
    let Ok(_starting) = state.matches.starting.try_lock() else {
        return Err(ApiError::conflict("a match is already starting"));
    };
    if state.matches.live.lock().unwrap().is_some() {
        return Err(ApiError::conflict("a match is already running: stop it first"));
    }
    let seats = [req.white, req.black];
    if seats.iter().all(|s| matches!(s, Seat::Human(_))) {
        return Err(ApiError::bad_request("at least one side must be a build"));
    }
    // A seat is a person or a build that exists.
    let mut builds: [Option<Build>; 2] = [None, None];
    for (i, seat) in seats.iter().enumerate() {
        if let Seat::Build(id) = *seat {
            let found = state.store.call(move |c| build::get(c, id)).await?;
            let found = found.ok_or(ApiError::bad_request(&format!("no build {id}")))?;
            if found.deleted_at.is_some() {
                return Err(ApiError::bad_request(&format!("build {id} was deleted: build its commit again")));
            }
            builds[i] = Some(found);
        }
    }
    match req.time {
        MatchTime::Clock { seconds, increment } if !(seconds > 0.0 && increment >= 0.0) => {
            return Err(ApiError::bad_request("a clock needs a positive time and a non-negative increment"));
        }
        MatchTime::Nodes { nodes: 0 } | MatchTime::Movetime { ms: 0 } => {
            return Err(ApiError::bad_request("a move needs some nodes or some time"));
        }
        _ => {}
    }

    let (pos, book) = opening(state, &req.opening).await?;
    let start = if pos == Chess::default() {
        "startpos".to_owned()
    } else {
        Fen::from_position(&pos, EnPassantMode::Legal).to_string()
    };
    let humans = [builds[0].is_none(), builds[1].is_none()];
    let plan = Plan {
        humans,
        time: req.time,
        start,
        pos,
        book,
        hash: req.hash.unwrap_or(16).clamp(1, 4096),
        threads: req.threads.unwrap_or(1).clamp(1, 16),
        // A person's moves carry no score, so there is nothing to adjudicate.
        adjudicate: req.adjudicate.unwrap_or(false) && !humans.iter().any(|h| *h),
        minimal: req.minimal.unwrap_or(false),
    };

    // The engines are up before the match is announced, so a missing binary is an error here and not a dead match.
    let mut engines: [Option<Engine>; 2] = [None, None];
    for (i, b) in builds.iter().enumerate() {
        let Some(b) = b else { continue };
        let mut engine = Engine::start(&build::binary(&state.config.data, b)).await?;
        engine.set_option("Threads", plan.threads).await?;
        engine.set_option("Hash", plan.hash).await?;
        // As in the tests (Reckless's fastchess settings).
        engine.set_option("Minimal", plan.minimal).await?;
        engine.set_option("MoveOverhead", 0).await?;
        engine.send("ucinewgame").await?;
        engine.ready().await?;
        engines[i] = Some(engine);
    }
    let players = [0, 1].map(|i| builds[i].as_ref().map_or_else(you, player));
    begin(state, plan, players, engines).await
}

/// The person at the board.
fn you() -> Player {
    Player { name: "You".to_owned(), human: true, ..Player::default() }
}

/// Records the match, announces it and starts its player task.
async fn begin(
    state: &Arc<AppState>,
    plan: Plan,
    [white, black]: [Player; 2],
    engines: [Option<Engine>; 2],
) -> Result<Match, ApiError> {
    let (clock_white_ms, clock_black_ms) = match plan.time {
        MatchTime::Clock { seconds, .. } => (Some((seconds * 1000.0) as u64), Some((seconds * 1000.0) as u64)),
        _ => (None, None),
    };
    let mut game = Match {
        id: 0,
        white,
        black,
        time: plan.time,
        start_fen: plan.start.clone(),
        book: plan.book,
        hash: plan.hash,
        threads: plan.threads,
        adjudicate: plan.adjudicate,
        minimal: plan.minimal,
        status: MatchStatus::Running,
        result: GameResult::Unfinished,
        reason: None,
        turn: plan.pos.turn().into(),
        clock_white_ms,
        clock_black_ms,
        moves: Vec::new(),
        created_at: String::new(),
        finished_at: None,
        lichess: None,
    };
    game = state
        .store
        .call(move |c| {
            game.created_at = now(c)?;
            c.execute("INSERT INTO matches (status, data) VALUES ('running', '{}')", [])?;
            game.id = c.last_insert_rowid();
            save(c, &game)?;
            Ok(game)
        })
        .await?;

    let cancel = CancellationToken::new();
    let shared = Arc::new(Mutex::new(Shared { game: game.clone(), thinking: None }));
    let (human_tx, human_rx) = mpsc::unbounded_channel();
    let human = plan.humans.iter().any(|h| *h).then_some(human_tx);
    *state.matches.live.lock().unwrap() =
        Some(Live { id: game.id, cancel: cancel.clone(), shared: shared.clone(), human });
    state.events.emit("match.started", serde_json::json!({ "id": game.id, "match": game }));
    tokio::spawn(play(state.clone(), shared, cancel, engines, human_rx, plan));
    Ok(game)
}

/// A human player's move, takeback or draw offer. The answer comes once the game has acted on it: for a move, after
/// it is on the board and stored. 400 when the match has no human player or the move is not legal, 409 when the match
/// is over or it is not your turn (or, for a takeback, there is nothing to take back or the game has a clock).
pub async fn act(state: &AppState, id: i64, action: Action) -> Result<Reply, ApiError> {
    let found = {
        let live = state.matches.live.lock().unwrap();
        live.as_ref().filter(|l| l.id == id).map(|l| (l.human.clone(), l.shared.clone()))
    };
    let Some((human, shared)) = found else {
        return match get(state, id).await? {
            Some(_) => Err(ApiError::conflict("that match is not running")),
            None => Err(ApiError::not_found("no such match")),
        };
    };
    let Some(human) = human else {
        return Err(ApiError::bad_request("that match has no human player"));
    };
    let yours = {
        let s = shared.lock().unwrap();
        match s.game.turn {
            Side::White => s.game.white.human,
            Side::Black => s.game.black.human,
        }
    };
    if !yours {
        return Err(ApiError::conflict("it is not your turn"));
    }
    let (ack, answer) = oneshot::channel();
    human.send(Request { action, ack }).map_err(|_| ApiError::conflict("that match is not running"))?;
    match tokio::time::timeout(ANSWER, answer).await {
        Ok(Ok(reply)) => reply,
        _ => Err(ApiError::conflict("the game moved on")),
    }
}

/// The position a game starts from, and whether the book chose it.
async fn opening(state: &AppState, opening: &Opening) -> Result<(Chess, bool), ApiError> {
    let parse = |fen: &str| -> Result<Chess, ApiError> {
        if fen == "startpos" {
            return Ok(Chess::default());
        }
        fen.parse::<Fen>()
            .map_err(|e| ApiError::bad_request(&format!("that FEN does not parse: {e}")))?
            .into_position(CastlingMode::Standard)
            .map_err(|e| ApiError::bad_request(&format!("that is not a legal position: {e}")))
    };
    match opening {
        Opening::Start => Ok((Chess::default(), false)),
        Opening::Position { fen, moves } => {
            let mut pos = parse(fen)?;
            for uci in moves {
                let m = uci
                    .parse::<UciMove>()
                    .ok()
                    .and_then(|u| u.to_move(&pos).ok())
                    .ok_or(ApiError::bad_request(&format!("{uci} is not a legal move there")))?;
                pos.play_unchecked(m);
            }
            Ok((pos, false))
        }
        Opening::Book => {
            let book = tools::book(&state.config.data, &|_| {}).await?;
            let text = tokio::fs::read_to_string(&book).await.context("read the opening book")?;
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            let mut bytes = [0u8; 4];
            getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("random number: {e}"))?;
            let line = lines
                .get(u32::from_le_bytes(bytes) as usize % lines.len().max(1))
                .ok_or(ApiError::from(anyhow::anyhow!("the opening book is empty")))?;
            // Book lines are four fields and may carry operations after a semicolon; the counters are optional.
            let fields: Vec<&str> = line.split(';').next().unwrap_or_default().split_whitespace().collect();
            let fen = if fields.len() >= 6 {
                fields[..6].join(" ")
            } else {
                format!("{} 0 1", fields[..4.min(fields.len())].join(" "))
            };
            Ok((parse(&fen)?, true))
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Playing
// ---------------------------------------------------------------------------------------------------------------------

/// How often, at most, a thinking engine's progress is reported (`match.info`), here and for the Lichess bot's games.
pub const INFO_EVERY: Duration = Duration::from_millis(200);

/// How a search for one move came out.
enum Think {
    /// `ack` answers the human who made the move, once it is on the board.
    Move {
        best: String,
        took: Duration,
        ack: Option<Ack>,
    },
    Flagged,
    Crashed,
    Aborted,
    /// The human took back their last move and the reply to it.
    Takeback(Ack),
    /// The engine accepted the human's offer of a draw.
    DrawAgreed,
}

fn go_command(time: MatchTime, clock: Option<&Clock>) -> String {
    match (time, clock) {
        (MatchTime::Nodes { nodes }, _) => format!("go nodes {nodes}"),
        (MatchTime::Movetime { ms }, _) => format!("go movetime {ms}"),
        (MatchTime::Clock { .. }, Some(c)) => format!(
            "go wtime {} btime {} winc {i} binc {i}",
            c.left(Side::White),
            c.left(Side::Black),
            i = c.increment()
        ),
        (MatchTime::Clock { .. }, None) => "go infinite".to_owned(),
    }
}

/// An engine's score as the adjudication reads it: centipawns from its own side, mates as huge.
fn centipawns(score: Score) -> i32 {
    match score {
        Score::Cp(cp) => cp,
        Score::Mate(m) if m > 0 => 30_000 - m,
        Score::Mate(m) => -30_000 - m,
    }
}

fn emit_info(state: &AppState, id: i64, ply: u32, side: Side, info: &Info) {
    state.events.emit(
        "match.info",
        serde_json::json!({
            "id": id,
            "ply": ply,
            "side": side,
            "depth": info.depth,
            "seldepth": info.seldepth,
            "score": info.score,
            "nodes": info.nodes,
            "nps": info.nps,
            "pv": info.pv,
        }),
    );
}

/// Plays the game to its end, then records and announces it.
async fn play(
    state: Arc<AppState>,
    shared: Arc<Mutex<Shared>>,
    cancel: CancellationToken,
    mut engines: [Option<Engine>; 2],
    mut requests: mpsc::UnboundedReceiver<Request>,
    plan: Plan,
) {
    let id = shared.lock().unwrap().game.id;
    let ending = game_loop(&state, &shared, &cancel, &mut engines, &mut requests, &plan).await;
    let game = conclude(&state, &shared, ending).await;
    // The match leaves the live slot before the end is announced, so a client that starts another one on the news can.
    {
        let mut live = state.matches.live.lock().unwrap();
        if live.as_ref().is_some_and(|l| l.id == id) {
            *live = None;
        }
    }
    emit_ended(&state, &game);
    for engine in engines.iter_mut().flatten() {
        let _ = engine.send("quit").await;
    }
}

/// Records the end of a game: its result, its reason and when it finished.
async fn conclude(state: &AppState, shared: &Mutex<Shared>, ending: Ending) -> Match {
    let game = {
        let mut s = shared.lock().unwrap();
        s.thinking = None;
        s.game.status = if ending.reason == Reason::Aborted { MatchStatus::Aborted } else { MatchStatus::Finished };
        s.game.result = ending.result;
        s.game.reason = Some(ending.reason);
        s.game.clone()
    };
    let id = game.id;
    let mut stored = game.clone();
    let saved = state
        .store
        .call(move |c| {
            stored.finished_at = Some(now(c)?);
            save(c, &stored)?;
            Ok(stored)
        })
        .await;
    match saved {
        Ok(stored) => {
            shared.lock().unwrap().game.finished_at = stored.finished_at.clone();
            stored
        }
        Err(e) => {
            tracing::error!("could not store match {id}: {e:#}");
            game
        }
    }
}

fn emit_ended(state: &AppState, game: &Match) {
    state.events.emit(
        "match.ended",
        serde_json::json!({ "id": game.id, "result": game.result, "reason": game.reason, "status": game.status }),
    );
}

fn emit_move(state: &AppState, id: i64, mv: &MatchMove) {
    state.events.emit(
        "match.move",
        serde_json::json!({
            "id": id,
            "ply": mv.ply,
            "side": mv.side,
            "uci": mv.uci,
            "san": mv.san,
            "fen_after": mv.fen_after,
            "clock_white_ms": mv.clock_white_ms,
            "clock_black_ms": mv.clock_black_ms,
            "eval": mv.eval,
            "depth": mv.depth,
            "nodes": mv.nodes,
            "time_ms": mv.time_ms,
        }),
    );
}

async fn game_loop(
    state: &AppState,
    shared: &Mutex<Shared>,
    cancel: &CancellationToken,
    engines: &mut [Option<Engine>; 2],
    requests: &mut mpsc::UnboundedReceiver<Request>,
    plan: &Plan,
) -> Ending {
    let id = shared.lock().unwrap().game.id;
    let mut pos = plan.pos.clone();
    let mut history = vec![rules::hash(&pos)];
    let mut played: Vec<String> = Vec::new();
    let mut judged: Vec<Judged> = Vec::new();
    let mut clock = match plan.time {
        MatchTime::Clock { seconds, increment } => {
            Some(Clock::new((seconds * 1000.0) as u64, (increment * 1000.0) as u64, GRACE))
        }
        _ => None,
    };

    loop {
        if let Some(end) = rules::detect(&pos, &history) {
            return end;
        }
        if plan.adjudicate
            && let Some(end) = rules::adjudicate(&judged, pos.fullmoves().get())
        {
            return end;
        }
        if cancel.is_cancelled() {
            return stopped(plan, played.len());
        }

        let side = Side::from(pos.turn());
        let ply = played.len() as u32 + 1;
        let command = go_command(plan.time, clock.as_ref());

        // The clock runs from the moment the search is asked for until its answer is read, measured on a monotonic
        // timer and charged to the side that was thinking.
        let began = Instant::now();
        shared.lock().unwrap().thinking = Some(began);
        let mut last: Option<Info> = None;
        let think = if plan.humans[side.index()] {
            let deadline = clock.as_ref().map(|c| tokio::time::Instant::now() + c.allowance(side));
            let human = Turn { cancel, requests: &mut *requests, deadline, began };
            wait_for_human(human, &pos, side, &played, &judged, clock.is_some()).await
        } else if let Err(e) = engines[side.index()]
            .as_mut()
            .expect("an engine plays this side")
            .go_command(&plan.start, &played, &command)
            .await
        {
            tracing::warn!("match {id}: could not ask {side:?} for a move: {e:#}");
            Think::Crashed
        } else {
            let deadline = clock.as_ref().map(|c| tokio::time::Instant::now() + c.allowance(side));
            let mut unsent: Option<Info> = None;
            let engine = engines[side.index()].as_mut().expect("an engine plays this side");
            let mut tick = tokio::time::interval(INFO_EVERY);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break Think::Aborted,
                    _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                        break Think::Flagged;
                    }
                    _ = tick.tick() => {
                        if let Some(info) = unsent.take() {
                            emit_info(state, id, ply, side, &info);
                        }
                        // Not your turn: whatever a human sent now was meant for a position that is gone.
                        while let Ok(late) = requests.try_recv() {
                            let _ = late.ack.send(Err(ApiError::conflict("it is not your turn")));
                        }
                    }
                    next = engine.next() => match next {
                        Ok(Search::Info(info)) => {
                            if info.multipv == 1 {
                                last = Some(info.clone());
                                unsent = Some(info);
                            }
                        }
                        Ok(Search::Done(best)) => {
                            let took = began.elapsed();
                            if let Some(info) = unsent.take() {
                                emit_info(state, id, ply, side, &info);
                            }
                            break Think::Move { best, took, ack: None };
                        }
                        Err(e) => {
                            tracing::warn!("match {id}: {side:?} stopped: {e:#}");
                            break Think::Crashed;
                        }
                    }
                }
            }
        };

        let (uci, san, took, ack) = match think {
            Think::Aborted => return stopped(plan, played.len()),
            Think::Flagged => return rules::flagged(&pos, side),
            Think::Crashed => return Ending::lost_by(side, Reason::Crash),
            Think::DrawAgreed => return Ending { result: GameResult::Draw, reason: Reason::Agreement },
            Think::Takeback(ack) => {
                let keep = played.len() - 2;
                played.truncate(keep);
                history.truncate(keep + 1);
                judged.truncate(keep);
                pos = replay(&plan.pos, &played);
                let stored = {
                    let mut s = shared.lock().unwrap();
                    s.thinking = None;
                    s.game.turn = pos.turn().into();
                    s.game.moves.truncate(keep);
                    s.game.clone()
                };
                if let Err(e) = state.store.call(move |c| save(c, &stored)).await {
                    tracing::error!("could not store match {id}: {e:#}");
                }
                state.events.emit("match.takeback", serde_json::json!({ "id": id, "plies": keep }));
                let _ = ack.send(Ok(Reply::TakenBack));
                continue;
            }
            Think::Move { best, took, ack } => {
                let Some(m) = best.parse::<UciMove>().ok().and_then(|u| u.to_move(&pos).ok()) else {
                    return Ending::lost_by(side, Reason::IllegalMove);
                };
                if let Some(c) = clock.as_mut()
                    && c.spend(side, took) == Spent::Flagged
                {
                    return rules::flagged(&pos, side);
                }
                let san = SanPlus::from_move_and_play_unchecked(&mut pos, m);
                let uci = m.to_uci(CastlingMode::Standard).to_string();
                (uci, san.to_string(), took, ack)
            }
        };

        history.push(rules::hash(&pos));
        played.push(uci.clone());
        judged.push(Judged { side, cp: last.as_ref().map(|i| centipawns(i.score)) });
        let clocks = clock.map(|c| (c.left(Side::White), c.left(Side::Black)));
        let mv = MatchMove {
            ply,
            side,
            uci,
            san,
            fen_after: Fen::from_position(&pos, EnPassantMode::Legal).to_string(),
            clock_white_ms: clocks.map(|c| c.0),
            clock_black_ms: clocks.map(|c| c.1),
            eval: last.as_ref().map(|i| i.score),
            depth: last.as_ref().map(|i| i.depth),
            nodes: last.as_ref().map(|i| i.nodes),
            time_ms: took.as_millis() as u64,
        };
        let stored = {
            let mut s = shared.lock().unwrap();
            s.thinking = None;
            s.game.turn = pos.turn().into();
            s.game.clock_white_ms = mv.clock_white_ms;
            s.game.clock_black_ms = mv.clock_black_ms;
            s.game.moves.push(mv.clone());
            s.game.clone()
        };
        if let Err(e) = state.store.call(move |c| save(c, &stored)).await {
            tracing::error!("could not store match {id}: {e:#}");
        }
        emit_move(state, id, &mv);
        if let Some(ack) = ack {
            let _ = ack.send(Ok(Reply::Moved));
        }
    }
}

/// How a game ends when it is stopped from outside: a person who stops a game that has got going resigns it, and
/// anything else (an engine match, a game before each side has moved) is aborted.
fn stopped(plan: &Plan, plies: usize) -> Ending {
    match plan.humans.iter().position(|h| *h) {
        Some(i) if plies >= 2 => Ending::lost_by(if i == 0 { Side::White } else { Side::Black }, Reason::Resigned),
        _ => Ending { result: GameResult::Unfinished, reason: Reason::Aborted },
    }
}

/// The position after `played` from `start`; the moves were legal when they were played.
fn replay(start: &Chess, played: &[String]) -> Chess {
    let mut pos = start.clone();
    for uci in played {
        if let Some(m) = uci.parse::<UciMove>().ok().and_then(|u| u.to_move(&pos).ok()) {
            pos.play_unchecked(m);
        }
    }
    pos
}

/// What a human's turn waits on.
struct Turn<'a> {
    cancel: &'a CancellationToken,
    requests: &'a mut mpsc::UnboundedReceiver<Request>,
    /// When the clock runs out, if there is one.
    deadline: Option<tokio::time::Instant>,
    /// When the turn began, which is when the clock started running.
    began: Instant,
}

/// Waits for the human to move, flag, or leave, answering takebacks and draw offers on the way.
async fn wait_for_human(
    turn: Turn<'_>,
    pos: &Chess,
    side: Side,
    played: &[String],
    judged: &[Judged],
    clocked: bool,
) -> Think {
    let Turn { cancel, requests, deadline, began } = turn;
    loop {
        let request = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Think::Aborted,
            _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                return Think::Flagged;
            }
            request = requests.recv() => match request {
                Some(r) => r,
                None => return Think::Aborted,
            },
        };
        let Request { action, ack } = request;
        match action {
            Action::Move(uci) => {
                if uci.parse::<UciMove>().ok().and_then(|u| u.to_move(pos).ok()).is_none() {
                    let _ = ack.send(Err(ApiError::bad_request(&format!("{uci} is not a legal move there"))));
                    continue;
                }
                return Think::Move { best: uci, took: began.elapsed(), ack: Some(ack) };
            }
            Action::Takeback if clocked => {
                let _ = ack.send(Err(ApiError::conflict("a game with a clock has no takebacks")));
            }
            Action::Takeback if played.len() < 2 => {
                let _ = ack.send(Err(ApiError::conflict("there is nothing to take back")));
            }
            Action::Takeback => return Think::Takeback(ack),
            Action::Draw => {
                let engine = judged.iter().rev().find(|j| j.side != side).and_then(|j| j.cp);
                let accepted = rules::accepts_draw(engine, played.len());
                let _ = ack.send(Ok(Reply::Draw { accepted }));
                if accepted {
                    return Think::DrawAgreed;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Games played elsewhere
// ---------------------------------------------------------------------------------------------------------------------

/// The record of a game whose moves come from somewhere other than this module's player, such as a Lichess game the
/// bot plays. It is stored, listed, followed and replayed like any match, and announced with the same events, but it
/// runs beside the local match rather than taking its slot, and any number can run at once.
pub struct Recorder {
    state: Arc<AppState>,
    shared: Arc<Mutex<Shared>>,
    id: i64,
}

/// The stored match of a Lichess game, so a game the bot comes back to (after a restart, or a dropped stream) goes on
/// in the same record.
pub async fn find_lichess(state: &AppState, game: &str) -> Result<Option<Match>> {
    let game = game.to_owned();
    state
        .store
        .call(move |c| {
            let data: Option<String> = c
                .query_row(
                    "SELECT data FROM matches WHERE json_extract(data, '$.lichess.id') = ?1 ORDER BY id DESC LIMIT 1",
                    [game],
                    |r| r.get(0),
                )
                .optional()?;
            data.map(|d| serde_json::from_str(&d).context("read a stored match")).transpose()
        })
        .await
}

impl Recorder {
    /// Starts recording `game`, running: a new record when its id is 0, else the stored one it continues. `cancel` is
    /// what `DELETE /api/matches/{id}` triggers; the game's player decides what stopping means.
    pub async fn open(state: &Arc<AppState>, mut game: Match, cancel: CancellationToken) -> Result<Self> {
        game.status = MatchStatus::Running;
        game.result = GameResult::Unfinished;
        game.reason = None;
        game.finished_at = None;
        let game = state
            .store
            .call(move |c| {
                if game.id == 0 {
                    game.created_at = now(c)?;
                    c.execute("INSERT INTO matches (status, data) VALUES ('running', '{}')", [])?;
                    game.id = c.last_insert_rowid();
                }
                save(c, &game)?;
                Ok(game)
            })
            .await?;
        let id = game.id;
        let shared = Arc::new(Mutex::new(Shared { game: game.clone(), thinking: None }));
        state.matches.external.lock().unwrap().insert(id, Live { id, cancel, shared: shared.clone(), human: None });
        state.events.emit("match.started", serde_json::json!({ "id": id, "match": game }));
        Ok(Self { state: state.clone(), shared, id })
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    /// The record as it stands.
    pub fn game(&self) -> Match {
        self.shared.lock().unwrap().game.clone()
    }

    /// Whether the side to move's clock is running, and since when, so readers see it run down between moves.
    pub fn thinking(&self, since: Option<Instant>) {
        self.shared.lock().unwrap().thinking = since;
    }

    /// The clocks as the game's host reports them now.
    pub fn clocks(&self, white_ms: Option<u64>, black_ms: Option<u64>) {
        let mut s = self.shared.lock().unwrap();
        s.game.clock_white_ms = white_ms;
        s.game.clock_black_ms = black_ms;
    }

    /// Adds a move, stores the game and announces the move. `turn` is the side to move after it.
    pub async fn push(&self, mv: MatchMove, turn: Side) {
        let stored = {
            let mut s = self.shared.lock().unwrap();
            s.game.turn = turn;
            if mv.clock_white_ms.is_some() || mv.clock_black_ms.is_some() {
                s.game.clock_white_ms = mv.clock_white_ms;
                s.game.clock_black_ms = mv.clock_black_ms;
            }
            s.game.moves.push(mv.clone());
            s.game.clone()
        };
        let id = self.id;
        if let Err(e) = self.state.store.call(move |c| save(c, &stored)).await {
            tracing::error!("could not store match {id}: {e:#}");
        }
        emit_move(&self.state, id, &mv);
    }

    /// Drops the recorded moves from `len` on, when the host's game and the record part ways (a takeback).
    pub fn truncate(&self, len: usize) {
        self.shared.lock().unwrap().game.moves.truncate(len);
    }

    /// Announces a progress report of the engine choosing the move at `ply`.
    pub fn info(&self, ply: u32, side: Side, info: &Info) {
        emit_info(&self.state, self.id, ply, side, info);
    }

    /// Records the end, takes the game off the running list and announces it.
    pub async fn finish(&self, ending: Ending) -> Match {
        let game = conclude(&self.state, &self.shared, ending).await;
        self.state.matches.external.lock().unwrap().remove(&self.id);
        emit_ended(&self.state, &game);
        game
    }
}

impl Drop for Recorder {
    /// A player that gives up without finishing (it failed) leaves the running list all the same; the stored record
    /// stays as it stood until the next start marks it aborted.
    fn drop(&mut self) {
        self.state.matches.external.lock().unwrap().remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    use crate::{config::Config, store::Store};

    /// A state with nothing in it but a store, for games between people: no engine needed.
    fn state() -> Arc<AppState> {
        let config = Config {
            data: std::env::temp_dir(),
            reckless: ".".into(),
            listen: "127.0.0.1:0".parse().unwrap(),
            token: "t".into(),
            lichess_url: String::new(),
        };
        Arc::new(AppState::new(config, Store::memory().unwrap(), crate::lichess::engines()))
    }

    async fn game(state: &Arc<AppState>, time: MatchTime) -> i64 {
        let plan = Plan {
            humans: [true, true],
            time,
            start: "startpos".into(),
            pos: Chess::default(),
            book: false,
            hash: 16,
            threads: 1,
            adjudicate: false,
            minimal: false,
        };
        begin(state, plan, [you(), you()], [None, None]).await.unwrap().id
    }

    const CASUAL: MatchTime = MatchTime::Movetime { ms: 1000 };

    async fn play_moves(state: &AppState, id: i64, moves: &[&str]) {
        for uci in moves {
            act(state, id, Action::Move((*uci).to_owned())).await.unwrap();
        }
    }

    async fn ended(state: &AppState, id: i64) -> Match {
        for _ in 0..200 {
            let m = get(state, id).await.unwrap().unwrap();
            if m.status != MatchStatus::Running {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the game never ended");
    }

    #[tokio::test]
    async fn a_human_move_must_be_legal_and_lands_on_the_board() {
        let state = state();
        let id = game(&state, CASUAL).await;
        let bad = act(&state, id, Action::Move("e2e5".into())).await.unwrap_err();
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        let junk = act(&state, id, Action::Move("hello".into())).await.unwrap_err();
        assert_eq!(junk.status, StatusCode::BAD_REQUEST);
        assert_eq!(act(&state, id, Action::Move("e2e4".into())).await.unwrap(), Reply::Moved);
        let m = get(&state, id).await.unwrap().unwrap();
        assert_eq!(m.moves.len(), 1);
        assert_eq!(m.moves[0].san, "e4");
        assert_eq!(m.turn, Side::Black);
        // It is Black's move now: a White move is not legal for them.
        let wrong = act(&state, id, Action::Move("d2d4".into())).await.unwrap_err();
        assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_game_between_people_ends_in_mate_and_frees_the_slot() {
        let state = state();
        let id = game(&state, CASUAL).await;
        play_moves(&state, id, &["f2f3", "e7e5", "g2g4", "d8h4"]).await;
        let m = ended(&state, id).await;
        assert_eq!(m.status, MatchStatus::Finished);
        assert_eq!(m.result, GameResult::Black);
        assert_eq!(m.reason, Some(Reason::Checkmate));
        let over = act(&state, id, Action::Move("a2a3".into())).await.unwrap_err();
        assert_eq!(over.status, StatusCode::CONFLICT);
        assert!(state.matches.live.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn takeback_removes_a_move_of_each_side_but_only_without_a_clock() {
        let state = state();
        let id = game(&state, CASUAL).await;
        let none = act(&state, id, Action::Takeback).await.unwrap_err();
        assert_eq!(none.status, StatusCode::CONFLICT);
        play_moves(&state, id, &["e2e4", "e7e5"]).await;
        assert_eq!(act(&state, id, Action::Takeback).await.unwrap(), Reply::TakenBack);
        let m = get(&state, id).await.unwrap().unwrap();
        assert!(m.moves.is_empty());
        assert_eq!(m.turn, Side::White);
        // The game goes on from the start.
        play_moves(&state, id, &["d2d4"]).await;
        stop(&state, id).await.unwrap();

        let timed = game(&state, MatchTime::Clock { seconds: 60.0, increment: 1.0 }).await;
        play_moves(&state, timed, &["e2e4", "e7e5"]).await;
        let refused = act(&state, timed, Action::Takeback).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::CONFLICT);
        stop(&state, timed).await.unwrap();
    }

    #[tokio::test]
    async fn the_clock_is_charged_like_an_engines_and_flags() {
        let state = state();
        let id = game(&state, MatchTime::Clock { seconds: 60.0, increment: 2.0 }).await;
        play_moves(&state, id, &["e2e4"]).await;
        let m = get(&state, id).await.unwrap().unwrap();
        // White spent next to nothing and got the increment; Black's clock is running down.
        assert!(m.clock_white_ms.unwrap() > 61_000);
        assert!(m.clock_black_ms.unwrap() <= 60_000);
        stop(&state, id).await.unwrap();

        let id = game(&state, MatchTime::Clock { seconds: 0.1, increment: 0.0 }).await;
        let m = ended(&state, id).await;
        assert_eq!(m.reason, Some(Reason::Time));
        assert_eq!(m.result, GameResult::Black);
    }

    #[tokio::test]
    async fn stopping_resigns_a_game_that_has_got_going_and_aborts_one_that_has_not() {
        let state = state();
        let id = game(&state, CASUAL).await;
        let m = stop(&state, id).await.unwrap();
        assert_eq!((m.status, m.reason), (MatchStatus::Aborted, Some(Reason::Aborted)));

        let id = game(&state, CASUAL).await;
        play_moves(&state, id, &["e2e4", "e7e5"]).await;
        let m = stop(&state, id).await.unwrap();
        assert_eq!((m.status, m.reason, m.result), (MatchStatus::Finished, Some(Reason::Resigned), GameResult::Black));
    }

    #[tokio::test]
    async fn a_match_needs_a_build_and_moves_need_a_match() {
        let state = state();
        let err = act(&state, 99, Action::Move("e2e4".into())).await.unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        let req = MatchRequest {
            white: Seat::Human(HumanSeat::Human),
            black: Seat::Human(HumanSeat::Human),
            time: CASUAL,
            opening: Opening::Start,
            hash: None,
            threads: None,
            minimal: None,
            adjudicate: None,
        };
        assert_eq!(start(&state, req.clone()).await.unwrap_err().status, StatusCode::BAD_REQUEST);
        // While another match is starting (its engines loading), a second start is refused rather than racing it.
        let starting = state.matches.starting.lock().await;
        assert_eq!(start(&state, req).await.unwrap_err().status, StatusCode::CONFLICT);
        drop(starting);
    }

    #[test]
    fn a_seat_reads_as_a_build_id_or_human() {
        let req: MatchRequest =
            serde_json::from_str(r#"{"white":"human","black":3,"time":{"type":"movetime","ms":500}}"#).unwrap();
        assert_eq!((req.white, req.black), (Seat::Human(HumanSeat::Human), Seat::Build(3)));
        let robot = r#"{"white":"robot","black":3,"time":{"type":"nodes","nodes":1}}"#;
        assert!(serde_json::from_str::<MatchRequest>(robot).is_err());
    }
}
