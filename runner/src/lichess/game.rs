//! One Lichess game, from its `gameFull` to its end. The game stream (`/api/bot/game/stream/{id}`) says what happens;
//! on our turn the engine thinks with Lichess's clocks and the move is posted back. Every move, ours and the
//! opponent's, goes into the game's [`Match`] record, so the board follows it live and replays it later.
//!
//! The engine is behind [`Brain`], so the loop can be tested with a stand-in that needs no binary.

use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::Value;
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove};
use tokio_util::sync::CancellationToken;

use super::{
    client::{CallError, Lichess, Ndjson},
    rules::{self as bot_rules, Variant},
};
use crate::{
    api::AppState,
    engine::{
        build,
        uci::{Engine, Info, Search},
    },
    matches::{
        self, LichessGame, Match, MatchMove, MatchStatus, MatchTime, Player, Recorder,
        rules::{self, Ending, GameResult, Side},
    },
};

// ---------------------------------------------------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------------------------------------------------

/// What chooses our moves: a UCI engine in the runner, a stand-in in tests.
pub trait Brain: Send {
    /// Searches the position (`startpos` or a FEN, then `moves`) with the given `go` command and answers the best move
    /// in UCI notation, passing every progress report to `info` on the way.
    fn think<'a>(
        &'a mut self,
        start: &'a str,
        moves: &'a [String],
        go: &'a str,
        info: &'a mut (dyn FnMut(&Info) + Send),
    ) -> BoxFuture<'a, Result<String>>;

    /// Ends the engine.
    fn quit(self: Box<Self>) -> BoxFuture<'static, ()>;
}

/// What a game's engine is started with.
pub struct BrainSpec {
    pub binary: PathBuf,
    pub hash: u32,
    pub threads: u32,
    pub move_overhead: u32,
}

/// Starts the engine for a game.
pub type Brains = Arc<dyn Fn(BrainSpec) -> BoxFuture<'static, Result<Box<dyn Brain>>> + Send + Sync>;

/// The real thing: a build's binary over UCI, one process per game kept up from the first move to the last.
pub fn engines() -> Brains {
    Arc::new(|spec: BrainSpec| {
        Box::pin(async move {
            let mut engine = Engine::start(&spec.binary).await?;
            engine.set_option("Threads", spec.threads).await?;
            engine.set_option("Hash", spec.hash).await?;
            engine.set_option("MoveOverhead", spec.move_overhead).await?;
            // Reports while thinking, so the board has something to show; it does not change the moves.
            engine.set_option("Minimal", false).await?;
            engine.send("ucinewgame").await?;
            engine.ready().await?;
            Ok(Box::new(Uci(engine)) as Box<dyn Brain>)
        })
    })
}

struct Uci(Engine);

impl Brain for Uci {
    fn think<'a>(
        &'a mut self,
        start: &'a str,
        moves: &'a [String],
        go: &'a str,
        info: &'a mut (dyn FnMut(&Info) + Send),
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            self.0.go_command(start, moves, go).await?;
            loop {
                match self.0.next().await? {
                    Search::Info(i) => info(&i),
                    Search::Done(best) => return Ok(best),
                }
            }
        })
    }

    fn quit(mut self: Box<Self>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let _ = self.0.send("quit").await;
        })
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// What the game stream sends
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seat {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    title: Option<String>,
    rating: Option<u32>,
    ai_level: Option<u32>,
}

impl Seat {
    fn player(&self) -> Player {
        let name = match self.ai_level {
            Some(level) => format!("Stockfish level {level}"),
            None if self.name.is_empty() => self.id.clone(),
            None => self.name.clone(),
        };
        Player {
            name: name.clone(),
            rating: self.rating,
            title: self.title.clone(),
            lichess: self.ai_level.is_none().then_some(name),
            ..Player::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct FullClock {
    /// Milliseconds.
    initial: u64,
    /// Milliseconds.
    increment: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct GameState {
    /// Every move so far, in UCI notation, separated by spaces.
    #[serde(default)]
    moves: String,
    #[serde(default)]
    wtime: u64,
    #[serde(default)]
    btime: u64,
    #[serde(default)]
    winc: u64,
    #[serde(default)]
    binc: u64,
    #[serde(default)]
    status: String,
    winner: Option<Side>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameFull {
    #[serde(default)]
    rated: bool,
    #[serde(default)]
    speed: String,
    #[serde(default)]
    variant: Variant,
    clock: Option<FullClock>,
    white: Seat,
    black: Seat,
    #[serde(default)]
    initial_fen: String,
    state: GameState,
}

// ---------------------------------------------------------------------------------------------------------------------
// Playing
// ---------------------------------------------------------------------------------------------------------------------

/// Progress reports per second, at most, while our engine thinks.
const INFO_EVERY: Duration = Duration::from_millis(200);

/// A search without a clock (a correspondence or unlimited game that was already running) takes this long.
const UNCLOCKED_MOVETIME: u64 = 5_000;

/// Plays game `id` to its end. `cancel` asks to leave it: resign, or abort if it has not got going.
pub async fn play(state: Arc<AppState>, id: String, cancel: CancellationToken) {
    if let Err(e) = run(&state, &id, &cancel).await {
        tracing::warn!("lichess game {id}: {e:#}");
    }
    state.lichess.game_over(&state, &id);
}

/// Opens the game stream, trying again for a while: a game that has just started may not stream yet.
async fn open(client: &Lichess, id: &str) -> Result<Ndjson> {
    let mut wait = Duration::from_secs(1);
    let mut tries = 0;
    loop {
        match client.stream(&format!("/api/bot/game/stream/{id}")).await {
            Ok(s) => return Ok(s),
            Err(e @ (CallError::Unauthorized | CallError::Status { status: 400..=499, .. })) if tries >= 2 => {
                return Err(e.into());
            }
            Err(e) if tries >= 8 => return Err(e.into()),
            Err(CallError::RateLimited) => wait = Duration::from_secs(61),
            Err(_) => {}
        }
        tries += 1;
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(Duration::from_secs(30));
    }
}

/// Leaves a game we cannot or will not play: aborts it while that is allowed (before both sides have moved),
/// otherwise resigns.
async fn leave(client: &Lichess, id: &str, plies: usize) {
    if plies < 2 && client.post(&format!("/api/bot/game/{id}/abort"), &[]).await.is_ok() {
        return;
    }
    if let Err(e) = client.post(&format!("/api/bot/game/{id}/resign"), &[]).await {
        tracing::warn!("lichess game {id}: could not resign: {e}");
    }
}

fn parse_start(fen: &str) -> Result<(String, Chess)> {
    if fen.is_empty() || fen == "startpos" {
        return Ok(("startpos".into(), Chess::default()));
    }
    let pos: Chess = fen
        .parse::<Fen>()
        .context("Lichess sent a FEN that does not parse")?
        .into_position(CastlingMode::Standard)
        .context("Lichess sent an illegal start position")?;
    if pos == Chess::default() {
        return Ok(("startpos".into(), pos));
    }
    Ok((Fen::from_position(&pos, EnPassantMode::Legal).to_string(), pos))
}

async fn run(state: &Arc<AppState>, id: &str, cancel: &CancellationToken) -> Result<()> {
    let bot = &state.lichess;
    let client = bot.client().context("no Lichess token")?;
    let me = bot.account().context("the Lichess account is not known yet")?;
    let mut stream = open(&client, id).await?;
    let full = loop {
        match stream.next().await? {
            Some(v) if v["type"] == "gameFull" => break v,
            Some(_) => continue,
            None => bail!("the game stream ended before the game was sent"),
        }
    };
    let full: GameFull = serde_json::from_value(full).context("read the game")?;
    let plies = full.state.moves.split_whitespace().count();
    let our = if full.white.id == me.id {
        Side::White
    } else if full.black.id == me.id {
        Side::Black
    } else {
        bail!("{} plays neither side", me.username);
    };
    let their_seat = if our == Side::White { &full.black } else { &full.white };
    if their_seat.title.as_deref() == Some("BOT") {
        super::quota::bot_game_started(state, id).await;
    }
    if !matches!(full.variant.key.as_str(), "standard" | "fromPosition") {
        leave(&client, id, plies).await;
        bail!("the {} variant is not played: left the game", full.variant.key);
    }
    let (start, pos0) = parse_start(&full.initial_fen)?;

    // The build is resolved now, so a ref means its newest build when the game starts.
    let settings = bot.settings();
    let found = match &settings.build {
        Some(spec) => super::resolve_build(state, spec).await?,
        None => None,
    };
    let Some(b) = found else {
        leave(&client, id, plies).await;
        bail!("no build is chosen to play: left the game");
    };
    let spec = BrainSpec {
        binary: build::binary(&state.config.data, &b),
        hash: settings.hash,
        threads: settings.threads,
        move_overhead: settings.move_overhead,
    };
    let brain = match (bot.brains)(spec).await {
        Ok(brain) => brain,
        Err(e) => {
            leave(&client, id, plies).await;
            return Err(e.context(format!("could not start build {}: left the game", b.id)));
        }
    };

    // A game the bot comes back to goes on in the record it already has.
    let mut ours = matches::player(&b);
    let seat = if our == Side::White { &full.white } else { &full.black };
    ours.rating = seat.rating;
    ours.title = seat.title.clone();
    ours.lichess = Some(me.username.clone());
    let theirs = if our == Side::White { full.black.player() } else { full.white.player() };
    let (white, black) = if our == Side::White { (ours, theirs) } else { (theirs, ours) };
    let previous = matches::find_lichess(state, id).await?;
    let time = match full.clock {
        Some(c) => MatchTime::Clock { seconds: c.initial as f64 / 1000.0, increment: c.increment as f64 / 1000.0 },
        None => MatchTime::Movetime { ms: UNCLOCKED_MOVETIME },
    };
    let clock_ms = full.clock.map(|c| c.initial);
    let mut game = Match {
        id: 0,
        white,
        black,
        time,
        start_fen: start.clone(),
        book: false,
        hash: settings.hash,
        threads: settings.threads,
        adjudicate: false,
        minimal: false,
        status: MatchStatus::Running,
        result: GameResult::Unfinished,
        reason: None,
        turn: pos0.turn().into(),
        clock_white_ms: clock_ms,
        clock_black_ms: clock_ms,
        moves: Vec::new(),
        created_at: String::new(),
        finished_at: None,
        lichess: Some(LichessGame {
            id: id.to_owned(),
            url: format!("https://lichess.org/{id}"),
            rated: full.rated,
            speed: full.speed.clone(),
            our_side: our,
        }),
    };
    if let Some(prev) = previous {
        // Keep what was recorded (our evaluations) as far as it agrees with the game.
        let played: Vec<&str> = full.state.moves.split_whitespace().collect();
        let keep = prev.moves.iter().zip(&played).take_while(|(m, uci)| m.uci == **uci).count();
        game.id = prev.id;
        game.created_at = prev.created_at;
        game.moves = prev.moves.into_iter().take(keep).collect();
    }
    let rec = Arc::new(Recorder::open(state, game, cancel.clone()).await?);
    bot.game_recorded(state, id, rec.id());

    let mut table = Table::new(rec.clone(), start, pos0, our, full.clock.is_some())?;
    let ending = table.play(&client, id, stream, full.state, brain, cancel).await;
    rec.finish(ending).await;
    Ok(())
}

/// When to claim the game after `opponentGone`: Lichess says how many seconds until a claim is allowed. `None` when
/// the opponent is back.
fn claim_time(v: &Value) -> Option<tokio::time::Instant> {
    if v["gone"].as_bool() != Some(true) {
        return None;
    }
    let wait = v["claimWinInSeconds"].as_u64().unwrap_or(0);
    // A moment past the deadline, so the claim does not arrive early.
    Some(tokio::time::Instant::now() + Duration::from_secs(wait) + Duration::from_millis(500))
}

/// Our search for a move: it owns the engine while it runs and hands it back with the answer.
type Thinking = Pin<Box<dyn Future<Output = Thought> + Send>>;

struct Thought {
    brain: Box<dyn Brain>,
    best: Result<String>,
    last: Option<Info>,
    took: Duration,
    /// The number of moves played when the search began.
    at: usize,
}

/// What we know of our own last move before Lichess confirms it.
struct Pending {
    ply: u32,
    last: Option<Info>,
    took: Duration,
}

/// The game as it stands on our side of the stream.
struct Table {
    rec: Arc<Recorder>,
    start: String,
    pos0: Chess,
    pos: Chess,
    history: Vec<u64>,
    played: Vec<String>,
    our: Side,
    clocked: bool,
    /// The clocks of the last state, to tell how long the opponent took, and its increments.
    clocks: Option<(u64, u64)>,
    increments: (u64, u64),
    pending: Option<Pending>,
}

impl Table {
    fn new(rec: Arc<Recorder>, start: String, pos0: Chess, our: Side, clocked: bool) -> Result<Self> {
        let mut t = Self {
            rec,
            start,
            pos: pos0.clone(),
            history: vec![rules::hash(&pos0)],
            pos0,
            played: Vec::new(),
            our,
            clocked,
            clocks: None,
            increments: (0, 0),
            pending: None,
        };
        // Moves already in the record (a game the bot came back to) are already played.
        for mv in t.rec.game().moves {
            t.play_uci(&mv.uci)?;
        }
        Ok(t)
    }

    fn play_uci(&mut self, uci: &str) -> Result<(Side, String)> {
        let side = Side::from(self.pos.turn());
        let m = uci
            .parse::<UciMove>()
            .ok()
            .and_then(|u| u.to_move(&self.pos).ok())
            .with_context(|| format!("Lichess sent {uci}, which is not legal here"))?;
        let san = SanPlus::from_move_and_play_unchecked(&mut self.pos, m).to_string();
        self.history.push(rules::hash(&self.pos));
        self.played.push(uci.to_owned());
        Ok((side, san))
    }

    /// Takes in a game state: records the new moves and says whether the game is over.
    async fn apply(&mut self, st: &GameState) -> Result<Option<Ending>> {
        let moves: Vec<&str> = st.moves.split_whitespace().collect();
        let common = self.played.iter().zip(&moves).take_while(|(a, b)| a == *b).count();
        if common < self.played.len() {
            // A takeback: the record goes back to where the game is.
            let keep: Vec<String> = self.played[..common].to_vec();
            self.pos = self.pos0.clone();
            self.history.truncate(1);
            self.played.clear();
            for uci in &keep {
                self.play_uci(uci)?;
            }
            self.rec.truncate(common);
        }
        let new = moves.len().saturating_sub(self.played.len());
        for (i, uci) in moves.iter().enumerate().skip(self.played.len()) {
            let (side, san) = self.play_uci(uci)?;
            let ply = self.played.len() as u32;
            let last = i + 1 == moves.len();
            let now = (self.clocked && last).then_some((st.wtime, st.btime));
            let ours = side == self.our;
            let pending = self.pending.take_if(|p| p.ply == ply && ours);
            let took = match (&pending, self.clocks, now) {
                (Some(p), _, _) => p.took.as_millis() as u64,
                // The opponent's time: what its clock lost, less the increment it got back.
                (None, Some(before), Some(after)) if new == 1 => {
                    let (b, a, inc) = match side {
                        Side::White => (before.0, after.0, st.winc),
                        Side::Black => (before.1, after.1, st.binc),
                    };
                    (b + inc).saturating_sub(a)
                }
                _ => 0,
            };
            let info = pending.and_then(|p| p.last);
            let mv = MatchMove {
                ply,
                side,
                uci: (*uci).to_owned(),
                san,
                fen_after: Fen::from_position(&self.pos, EnPassantMode::Legal).to_string(),
                clock_white_ms: now.map(|c| c.0),
                clock_black_ms: now.map(|c| c.1),
                eval: info.as_ref().map(|i| i.score),
                depth: info.as_ref().map(|i| i.depth),
                nodes: info.as_ref().map(|i| i.nodes),
                time_ms: took,
            };
            self.rec.push(mv, self.pos.turn().into()).await;
        }
        if self.clocked {
            self.clocks = Some((st.wtime, st.btime));
            self.increments = (st.winc, st.binc);
            self.rec.clocks(Some(st.wtime), Some(st.btime));
        }
        // Lichess starts the clocks once both sides have moved.
        let running = self.clocked && self.played.len() >= 2 && st.status == "started";
        self.rec.thinking(running.then(Instant::now));
        Ok(bot_rules::ending(&st.status, st.winner, &self.pos, &self.history))
    }

    fn our_turn(&self) -> bool {
        Side::from(self.pos.turn()) == self.our
    }

    fn go(&self) -> String {
        match self.clocks {
            Some((w, b)) if self.clocked => {
                let (winc, binc) = self.increments;
                format!("go wtime {w} btime {b} winc {winc} binc {binc}")
            }
            _ => format!("go movetime {UNCLOCKED_MOVETIME}"),
        }
    }

    /// Follows the game to its end: records each state, thinks and moves on our turn, claims the game when the
    /// opponent has gone for long enough, and leaves it when asked. Reconnects the stream if it drops.
    async fn play(
        &mut self,
        client: &Lichess,
        id: &str,
        stream: Ndjson,
        first: GameState,
        brain: Box<dyn Brain>,
        cancel: &CancellationToken,
    ) -> Ending {
        let mut stream = Some(stream);
        let mut brain = Some(brain);
        let mut thinking: Option<Thinking> = None;
        let mut reconnect = tokio::time::Instant::now();
        let mut backoff = Duration::from_secs(1);
        let mut claim_at: Option<tokio::time::Instant> = None;
        let mut leaving = false;
        // The search we last started, by moves played, so one state is not answered twice.
        let mut asked: Option<usize> = None;
        let mut next_state = Some(first);

        let ending = loop {
            if let Some(st) = next_state.take() {
                match self.apply(&st).await {
                    Ok(Some(end)) => break end,
                    Ok(None) => {}
                    Err(e) => {
                        // The moves do not read, but the status still says when the game is over: without this a
                        // finished game would be streamed again and again.
                        if let Some(end) = bot_rules::ending(&st.status, st.winner, &self.pos, &self.history) {
                            break end;
                        }
                        if !leaving {
                            tracing::warn!("lichess game {id}: {e:#}");
                            leave(client, id, self.played.len()).await;
                            leaving = true;
                        }
                    }
                }
            }
            if !leaving
                && thinking.is_none()
                && self.our_turn()
                && asked != Some(self.played.len())
                && let Some(b) = brain.take()
            {
                asked = Some(self.played.len());
                thinking = Some(self.think(b));
            }

            tokio::select! {
                biased;
                _ = cancel.cancelled(), if !leaving => {
                    leaving = true;
                    leave(client, id, self.played.len()).await;
                }
                _ = async { tokio::time::sleep_until(claim_at.unwrap()).await }, if claim_at.is_some() => {
                    claim_at = None;
                    if let Err(e) = client.post(&format!("/api/bot/game/{id}/claim-victory"), &[]).await {
                        // Too early by Lichess's clock, or a network blip: claim again shortly, unless the opponent
                        // comes back (`opponentGone` with `gone: false`) or the game ends first.
                        tracing::warn!("lichess game {id}: could not claim the win: {e}");
                        claim_at = Some(tokio::time::Instant::now() + Duration::from_secs(5));
                    }
                }
                thought = async { thinking.as_mut().unwrap().await }, if thinking.is_some() => {
                    thinking = None;
                    let Thought { brain: b, best, last, took, at } = thought;
                    match best {
                        Err(e) => {
                            // The engine is gone: the game is lost, so it is resigned rather than left to time out.
                            tracing::warn!("lichess game {id}: the engine stopped: {e:#}");
                            b.quit().await;
                            leaving = true;
                            leave(client, id, self.played.len()).await;
                        }
                        // The game moved on while we thought (a takeback): think again.
                        Ok(_) if at != self.played.len() => {
                            brain = Some(b);
                            asked = None;
                        }
                        Ok(best) => {
                            brain = Some(b);
                            let legal = best.parse::<UciMove>().ok().and_then(|u| u.to_move(&self.pos).ok());
                            if legal.is_none() {
                                tracing::warn!("lichess game {id}: the engine answered {best}, not a legal move");
                                leaving = true;
                                leave(client, id, self.played.len()).await;
                            } else {
                                self.pending = Some(Pending { ply: at as u32 + 1, last, took });
                                if let Err(e) = client.post(&format!("/api/bot/game/{id}/move/{best}"), &[]).await {
                                    tracing::warn!("lichess game {id}: could not play {best}: {e}");
                                }
                            }
                        }
                    }
                }
                line = async { stream.as_mut().unwrap().next().await }, if stream.is_some() => match line {
                    Ok(Some(v)) => {
                        backoff = Duration::from_secs(1);
                        match v["type"].as_str() {
                            Some("gameFull") => next_state = serde_json::from_value(v["state"].clone()).ok(),
                            Some("gameState") => next_state = serde_json::from_value(v).ok(),
                            Some("opponentGone") => claim_at = claim_time(&v),
                            _ => {}
                        }
                    }
                    Ok(None) | Err(_) => {
                        // The game is still on (no final status came), so the stream is opened again.
                        stream = None;
                        reconnect = tokio::time::Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                    }
                },
                _ = tokio::time::sleep_until(reconnect), if stream.is_none() => {
                    match client.stream(&format!("/api/bot/game/stream/{id}")).await {
                        Ok(s) => stream = Some(s),
                        Err(CallError::Status { status: 404, .. }) => {
                            break Ending { result: GameResult::Unfinished, reason: rules::Reason::Other };
                        }
                        Err(e) => {
                            tracing::warn!("lichess game {id}: the game stream: {e}");
                            let wait = if matches!(e, CallError::RateLimited) { Duration::from_secs(61) } else { backoff };
                            reconnect = tokio::time::Instant::now() + wait;
                            backoff = (backoff * 2).min(Duration::from_secs(30));
                        }
                    }
                }
            }
        };
        // A search still running is dropped with its engine; an idle engine is told to quit.
        drop(thinking);
        if let Some(b) = brain {
            b.quit().await;
        }
        ending
    }

    fn think(&self, mut brain: Box<dyn Brain>) -> Thinking {
        let rec = self.rec.clone();
        let (start, moves, go) = (self.start.clone(), self.played.clone(), self.go());
        let (ply, side, at) = (self.played.len() as u32 + 1, self.our, self.played.len());
        Box::pin(async move {
            let began = Instant::now();
            let mut last: Option<Info> = None;
            let mut sent: Option<Instant> = None;
            let mut unsent = false;
            let best = {
                let mut report = |i: &Info| {
                    if i.multipv != 1 {
                        return;
                    }
                    last = Some(i.clone());
                    if sent.is_none_or(|t| t.elapsed() >= INFO_EVERY) {
                        rec.info(ply, side, i);
                        sent = Some(Instant::now());
                        unsent = false;
                    } else {
                        unsent = true;
                    }
                };
                brain.think(&start, &moves, &go, &mut report).await
            };
            if unsent && let Some(i) = &last {
                rec.info(ply, side, i);
            }
            Thought { brain, best, last, took: began.elapsed(), at }
        })
    }
}
