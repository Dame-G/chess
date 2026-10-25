//! Engine-vs-engine matches you can watch. A match plays two builds against each other over UCI, move by move, and
//! reports each move and the thinking engine's progress as events (`match.started`, `match.move`, `match.info`,
//! `match.ended`). Like live analysis it runs outside the job queue, so a long experiment never holds it up. One match
//! runs at a time, on two search threads. Every game is stored: finished ones replay, and `/pgn` exports them.
//!
//! The game itself is a plain record ([`Match`]) that the player task fills in; a later human-versus-build game can
//! reuse it by supplying the moves of one side from somewhere other than an engine. Games played elsewhere (the
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
use tokio::time::MissedTickBehavior;
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

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MatchRequest {
    /// The build that plays White, by id.
    pub white: i64,
    /// The build that plays Black, by id.
    pub black: i64,
    pub time: MatchTime,
    #[serde(default)]
    pub opening: Opening,
    /// Hash table size in MiB for each engine. Default 16, as in the tests.
    pub hash: Option<u32>,
    /// Search threads for each engine. Default 1, so a match uses two threads and can run beside a test.
    pub threads: Option<u32>,
    /// End decided games early, as the tests do: a side that has scored -4 or worse for three moves loses, and a
    /// level game drawn from move 30. Off by default, so the game is played out.
    /// Reckless's `Minimal` option, which the tests set: the engine then reports only its final line, so there is nothing
    /// to watch while it thinks. Off by default, so `match.info` events flow; it does not change how the engine plays.
    pub minimal: Option<bool>,
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
}

pub struct Matches {
    /// The one local match.
    live: Mutex<Option<Live>>,
    /// Games recorded from elsewhere (the Lichess bot's), any number at once, beside the local one.
    external: Mutex<HashMap<i64, Live>>,
}

impl Matches {
    pub fn new() -> Self {
        Self { live: Mutex::new(None), external: Mutex::new(HashMap::new()) }
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
    white: Build,
    black: Build,
    time: MatchTime,
    start: String,
    pos: Chess,
    book: bool,
    hash: u32,
    threads: u32,
    adjudicate: bool,
    minimal: bool,
}

pub async fn start(state: &Arc<AppState>, req: MatchRequest) -> Result<Match, ApiError> {
    if state.matches.live.lock().unwrap().is_some() {
        return Err(ApiError::conflict("a match is already running: stop it first"));
    }
    let (white, black) = {
        let (w, b) = (req.white, req.black);
        state.store.call(move |c| Ok((build::get(c, w)?, build::get(c, b)?))).await?
    };
    let white = white.ok_or(ApiError::bad_request(&format!("no build {}", req.white)))?;
    let black = black.ok_or(ApiError::bad_request(&format!("no build {}", req.black)))?;
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
    let plan = Plan {
        white,
        black,
        time: req.time,
        start,
        pos,
        book,
        hash: req.hash.unwrap_or(16).clamp(1, 4096),
        threads: req.threads.unwrap_or(1).clamp(1, 16),
        adjudicate: req.adjudicate.unwrap_or(false),
        minimal: req.minimal.unwrap_or(false),
    };

    // Both engines are up before the match is announced, so a missing binary is an error here and not a dead match.
    let mut engines = Vec::new();
    for b in [&plan.white, &plan.black] {
        let mut engine = Engine::start(&build::binary(&state.config.data, b)).await?;
        engine.set_option("Threads", plan.threads).await?;
        engine.set_option("Hash", plan.hash).await?;
        // As in the tests (Reckless's fastchess settings).
        engine.set_option("Minimal", plan.minimal).await?;
        engine.set_option("MoveOverhead", 0).await?;
        engine.send("ucinewgame").await?;
        engine.ready().await?;
        engines.push(engine);
    }
    let black_engine = engines.pop().unwrap();
    let white_engine = engines.pop().unwrap();

    let (clock_white_ms, clock_black_ms) = match plan.time {
        MatchTime::Clock { seconds, .. } => (Some((seconds * 1000.0) as u64), Some((seconds * 1000.0) as u64)),
        _ => (None, None),
    };
    let mut game = Match {
        id: 0,
        white: player(&plan.white),
        black: player(&plan.black),
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
    *state.matches.live.lock().unwrap() = Some(Live { id: game.id, cancel: cancel.clone(), shared: shared.clone() });
    state.events.emit("match.started", serde_json::json!({ "id": game.id, "match": game }));
    tokio::spawn(play(state.clone(), shared, cancel, white_engine, black_engine, plan));
    Ok(game)
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

/// Progress reports per second, at most, for the engine that is thinking.
const INFO_EVERY: Duration = Duration::from_millis(200);

/// How a search for one move came out.
enum Think {
    Move { best: String, took: Duration },
    Flagged,
    Crashed,
    Aborted,
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
    mut white: Engine,
    mut black: Engine,
    plan: Plan,
) {
    let id = shared.lock().unwrap().game.id;
    let ending = game_loop(&state, &shared, &cancel, [&mut white, &mut black], &plan).await;
    let game = conclude(&state, &shared, ending).await;
    // The match leaves the live slot before the end is announced, so a client that starts another one on the news can.
    {
        let mut live = state.matches.live.lock().unwrap();
        if live.as_ref().is_some_and(|l| l.id == id) {
            *live = None;
        }
    }
    emit_ended(&state, &game);
    let _ = white.send("quit").await;
    let _ = black.send("quit").await;
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
    engines: [&mut Engine; 2],
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
            return Ending { result: GameResult::Unfinished, reason: Reason::Aborted };
        }

        let side = Side::from(pos.turn());
        let ply = played.len() as u32 + 1;
        let engine = &mut *engines[side.index()];
        let command = go_command(plan.time, clock.as_ref());

        // The clock runs from the moment the search is asked for until its answer is read, measured on a monotonic
        // timer and charged to the side that was thinking.
        let began = Instant::now();
        shared.lock().unwrap().thinking = Some(began);
        let mut last: Option<Info> = None;
        let think = if let Err(e) = engine.go_command(&plan.start, &played, &command).await {
            tracing::warn!("match {id}: could not ask {side:?} for a move: {e:#}");
            Think::Crashed
        } else {
            let deadline = clock.as_ref().map(|c| tokio::time::Instant::now() + c.allowance(side));
            let mut unsent: Option<Info> = None;
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
                            break Think::Move { best, took };
                        }
                        Err(e) => {
                            tracing::warn!("match {id}: {side:?} stopped: {e:#}");
                            break Think::Crashed;
                        }
                    }
                }
            }
        };

        let (uci, san, took) = match think {
            Think::Aborted => return Ending { result: GameResult::Unfinished, reason: Reason::Aborted },
            Think::Flagged => return Ending::lost_by(side, Reason::Time),
            Think::Crashed => return Ending::lost_by(side, Reason::Crash),
            Think::Move { best, took } => {
                let Some(m) = best.parse::<UciMove>().ok().and_then(|u| u.to_move(&pos).ok()) else {
                    return Ending::lost_by(side, Reason::IllegalMove);
                };
                if let Some(c) = clock.as_mut()
                    && c.spend(side, took) == Spent::Flagged
                {
                    return Ending::lost_by(side, Reason::Time);
                }
                let san = SanPlus::from_move_and_play_unchecked(&mut pos, m);
                let uci = m.to_uci(CastlingMode::Standard).to_string();
                (uci, san.to_string(), took)
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
        state.matches.external.lock().unwrap().insert(id, Live { id, cancel, shared: shared.clone() });
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
