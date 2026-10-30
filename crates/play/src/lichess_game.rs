//! A game played on Lichess, from its `gameFull` to its end. Lichess is the referee: the game stream says what happens
//! and is mirrored into the record move by move (clocks, offers, the opponent leaving, the end). The seats translate:
//! the opponent's moves are the stream's ([`LichessOpponent`]); ours go to Lichess and count once the stream shows
//! them, from the board through the Board API ([`LichessYou`]).
//!
//! The stream is followed by its own task, which reconnects when it drops and writes what it reads into a [`Mirror`];
//! the game's task and the seats watch the mirror.

use std::time::Instant;

use async_trait::async_trait;
use dameless_core::{
    ConnectionKey, Error, PlayerKey, Side, SidePair, Speed, TimeControl,
    chess::Position,
    game::{GameEnd, GameSummary, Move, Offers, Seat, SeatAction, SeatMove, Source, Turn},
    lichess::board,
};
use dameless_lichess::{
    Error as LichessError, GameStatus as LichessStatus, Lichess, Ndjson,
    game::{GameEvent, GameFull, GamePlayer, GameState},
};
use dameless_store::NewGame;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    error,
    games::{Command, Games, Handle},
    record::Record,
    seat::{MoveRequest, MoveSender, SeatController, SeatKind, legal_uci},
    timing::Timing,
};

// ---------------------------------------------------------------------------------------------------------------------
// The Board API
// ---------------------------------------------------------------------------------------------------------------------

pub(crate) fn side(s: dameless_lichess::Side) -> Side {
    match s {
        dameless_lichess::Side::White => Side::White,
        dameless_lichess::Side::Black => Side::Black,
    }
}

async fn open_stream(client: &Lichess, id: &str) -> Result<Ndjson<GameEvent>, LichessError> {
    client.board().stream_game(id).await
}

/// Sends a seat action to Lichess.
async fn post(client: &Lichess, id: &str, action: SeatAction) -> Result<(), Error> {
    use SeatAction as A;
    let board = client.board();
    let r = match action {
        A::Resign => board.resign(id).await,
        A::Abort => board.abort(id).await,
        A::ClaimVictory => board.claim_victory(id).await,
        A::ClaimDraw => board.claim_draw(id).await,
        A::OfferDraw | A::AcceptDraw => board.draw(id, true).await,
        A::DeclineDraw => board.draw(id, false).await,
        A::Takeback | A::AcceptTakeback => board.takeback(id, true).await,
        A::DeclineTakeback => board.takeback(id, false).await,
        A::Move => return Err(Error::bad_request("a move is played with its UCI")),
    };
    r.map_err(error::lichess)
}

/// Opens a game's stream and reads its `gameFull`, trying again for a while: a game that has just started may not
/// stream yet.
pub(crate) async fn prepare(
    client: &Lichess,
    id: &str,
    timing: &Timing,
) -> Result<(Ndjson<GameEvent>, GameFull), Error> {
    let mut backoff = timing.backoff();
    let mut tries = 0;
    let mut stream = loop {
        match open_stream(client, id).await {
            Ok(s) => break s,
            Err(e) => {
                tries += 1;
                let refused = matches!(e, LichessError::Refused { .. } | LichessError::NoToken);
                if tries >= timing.stream_tries || (refused && tries >= 2) {
                    return Err(error::lichess(e));
                }
                tokio::time::sleep(backoff.after(&e)).await;
            }
        }
    };
    loop {
        match stream.next().await {
            Ok(Some(GameEvent::GameFull(full))) => return Ok((stream, *full)),
            Ok(Some(_)) | Err(LichessError::Unexpected(_)) => continue,
            Ok(None) => return Err(Error::upstream("the game stream ended before the game was sent")),
            Err(e) => return Err(error::lichess(e)),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The mirror
// ---------------------------------------------------------------------------------------------------------------------

/// The game as Lichess's stream last told it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Mirror {
    /// Every move so far, UCI.
    pub moves: Vec<String>,
    /// Both clocks after each ply, when it was the last move of a state Lichess sent (white, black).
    pub clock_after: Vec<Option<(u64, u64)>>,
    /// The clocks of the last state.
    pub clocks: Option<(u64, u64)>,
    pub status: LichessStatus,
    pub winner: Option<Side>,
    pub draw: Option<Side>,
    pub takeback: Option<Side>,
    /// The opponent left: the seconds Lichess gave until a claim, and since when.
    pub gone: Option<(u32, Instant)>,
    /// The stream cannot be opened again: the game is gone from Lichess (or our access to it).
    pub lost: bool,
}

impl Mirror {
    fn apply(&mut self, st: &GameState, clocked: bool) {
        let moves: Vec<String> = st.moves().map(str::to_owned).collect();
        let common = self.moves.iter().zip(&moves).take_while(|(a, b)| a == b).count();
        self.clock_after.truncate(common);
        let new = moves.len().saturating_sub(common);
        for i in 0..new {
            let last = i + 1 == new;
            self.clock_after.push((clocked && last).then_some((st.wtime, st.btime)));
        }
        self.moves = moves;
        if clocked {
            self.clocks = Some((st.wtime, st.btime));
        }
        self.status = st.status;
        self.winner = st.winner.map(side);
        self.draw = st.draw_offer().map(side);
        self.takeback = st.takeback_offer().map(side);
    }
}

/// Which game's stream to follow, and how.
struct Follow {
    client: Lichess,
    id: String,
    clocked: bool,
    timing: Timing,
}

/// Follows the stream until the game ends or `cancel` fires, reopening it when it drops.
async fn follow(f: Follow, mut stream: Ndjson<GameEvent>, tx: watch::Sender<Mirror>, cancel: CancellationToken) {
    let Follow { client, id, clocked, timing } = f;
    let mut backoff = timing.backoff();
    loop {
        let line = tokio::select! {
            _ = cancel.cancelled() => return,
            line = stream.next() => line,
        };
        match line {
            Ok(Some(event)) => {
                backoff.reset();
                match event {
                    GameEvent::GameFull(full) => tx.send_modify(|m| m.apply(&full.state, clocked)),
                    GameEvent::GameState(st) => tx.send_modify(|m| m.apply(&st, clocked)),
                    GameEvent::OpponentGone(g) => tx.send_modify(|m| {
                        m.gone = g.gone.then(|| (g.claim_win_in_seconds.unwrap_or(0) as u32, Instant::now()));
                    }),
                    GameEvent::ChatLine(_) | GameEvent::Other => {}
                }
            }
            Err(LichessError::Unexpected(_)) => {}
            Ok(None) | Err(_) => {
                if tx.borrow().status.is_over() {
                    return;
                }
                // The game is still on (no final status came): the stream is opened again.
                loop {
                    let wait = backoff.after(&LichessError::Unreachable("the stream ended".into()));
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(wait) => {}
                    }
                    match open_stream(&client, &id).await {
                        Ok(s) => {
                            stream = s;
                            break;
                        }
                        Err(LichessError::Refused { status: 401 | 403 | 404, .. } | LichessError::NoToken) => {
                            tx.send_modify(|m| m.lost = true);
                            return;
                        }
                        Err(e) => {
                            tracing::warn!("lichess game {id}: the game stream: {e}");
                            if matches!(e, LichessError::RateLimited { .. }) {
                                tokio::time::sleep(timing.rate_limited).await;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Waits until the mirror shows ply `ply`, and answers its move. 409 when the game ends first.
async fn wait_ply(rx: &mut watch::Receiver<Mirror>, ply: u32) -> Result<String, Error> {
    let n = ply as usize;
    let m = rx
        .wait_for(|m| m.moves.len() >= n || m.status.is_over() || m.lost)
        .await
        .map_err(|_| Error::conflict("the game stream ended"))?;
    m.moves.get(n - 1).cloned().ok_or_else(|| Error::conflict("the game is over"))
}

// ---------------------------------------------------------------------------------------------------------------------
// The seats
// ---------------------------------------------------------------------------------------------------------------------

/// The other player of a Lichess game: their moves are what the stream shows.
pub(crate) struct LichessOpponent {
    mirror: watch::Receiver<Mirror>,
}

#[async_trait]
impl SeatController for LichessOpponent {
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove, Error> {
        wait_ply(&mut self.mirror, turn.ply).await.map(SeatMove::bare)
    }

    async fn action(&mut self, _action: SeatAction) -> Result<(), Error> {
        Ok(())
    }

    fn kind(&self) -> SeatKind {
        SeatKind::LichessOpponent
    }
}

/// You, in your game on Lichess: your moves from the board go to the Board API (as do your offers, takebacks,
/// resignations and claims), and count once the stream shows them. A move you make on lichess.org counts too.
pub(crate) struct LichessYou {
    client: Lichess,
    id: String,
    mirror: watch::Receiver<Mirror>,
    moves: mpsc::UnboundedReceiver<MoveRequest>,
}

impl LichessYou {
    pub(crate) fn new(client: Lichess, id: &str, mirror: watch::Receiver<Mirror>) -> (Self, MoveSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { client, id: id.to_owned(), mirror, moves: rx }, tx)
    }
}

#[async_trait]
impl SeatController for LichessYou {
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove, Error> {
        loop {
            let req = {
                let mut mirror = self.mirror.clone();
                tokio::select! {
                    m = wait_ply(&mut mirror, turn.ply) => return m.map(SeatMove::bare),
                    req = self.moves.recv() => req,
                }
            };
            let Some(req) = req else { return Err(Error::conflict("nobody is at the board")) };
            let answer = match legal_uci(turn.fen, &req.uci) {
                Ok(uci) => self.client.board().make_move(&self.id, &uci).await.map_err(error::lichess),
                Err(e) => Err(e),
            };
            let _ = req.ack.send(answer);
        }
    }

    async fn action(&mut self, action: SeatAction) -> Result<(), Error> {
        post(&self.client, &self.id, action).await
    }

    fn kind(&self) -> SeatKind {
        SeatKind::LichessYou
    }
}


// ---------------------------------------------------------------------------------------------------------------------
// The game's task
// ---------------------------------------------------------------------------------------------------------------------

/// A seat of a Lichess game as the record shows it.
pub(crate) fn seat_of(p: &GamePlayer) -> Seat {
    match p.ai_level {
        Some(level) => Seat {
            level: Some(u32::from(level.clamp(1, 8))),
            ..Seat::new(PlayerKey::Engine("stockfish".into()), format!("Stockfish level {level}"))
        },
        None => {
            let id = p.id.clone().unwrap_or_else(|| "anonymous".into());
            let name = p.name.clone().unwrap_or_else(|| id.clone());
            let key = PlayerKey::lichess(&id).unwrap_or(PlayerKey::Lichess("anonymous".into()));
            Seat { rating: p.rating, title: p.title.clone(), ..Seat::new(key, name) }
        }
    }
}

/// A Lichess game's time control: its clock, or its days per move.
pub(crate) fn time_of(full: &GameFull) -> TimeControl {
    match (full.clock, full.days_per_turn) {
        (Some(c), _) => TimeControl::clock(c.initial, c.increment),
        (None, Some(days)) => TimeControl::Correspondence { days },
        (None, None) => TimeControl::Correspondence { days: 0 },
    }
}

pub(crate) fn speed_of(full: &GameFull) -> Speed {
    full.speed.as_deref().and_then(Speed::parse).or_else(|| time_of(full).speed()).unwrap_or(Speed::Correspondence)
}

/// Everything a Lichess game's task starts from.
pub(crate) struct LichessGame {
    pub games: Games,
    pub connection: ConnectionKey,
    pub client: Lichess,
    pub full: GameFull,
    pub stream: Ndjson<GameEvent>,
    pub mirror: watch::Sender<Mirror>,
    pub our: Side,
    pub our_seat: Seat,
    pub ours: Box<dyn SeatController>,
    /// Where your moves go, in your game.
    pub moves: Option<MoveSender>,
    /// The Lichess the game is on, for its link.
    pub lichess_url: String,
    /// Stops the game: resigns it, or aborts it before both sides have moved.
    pub cancel: CancellationToken,
    /// Stops following the game here without ending it (your token was forgotten): Lichess plays on.
    pub detach: CancellationToken,
}

/// A mirror for a game, to give its seats before the game's task starts.
pub(crate) fn mirror() -> watch::Sender<Mirror> {
    watch::Sender::new(Mirror::default())
}

/// What woke the game's task.
enum Woke {
    Stop,
    Detach,
    Claim,
    Refresh,
    Command(Command),
    Changed,
    Moved(Result<SeatMove, Error>, u64),
}

/// Plays the game to its end, mirroring Lichess, and answers how it ended. `on_record` hears the record's id once it
/// exists.
pub(crate) async fn run(
    game: LichessGame,
    on_record: impl FnOnce(dameless_core::GameId),
) -> Result<GameSummary, Error> {
    let LichessGame {
        games,
        connection,
        client,
        full,
        stream,
        mirror: tx,
        our,
        our_seat,
        mut ours,
        moves,
        lichess_url,
        cancel,
        detach,
    } = game;
    let hub = games.hub().clone();
    let timing = games.timing().clone();
    let id = full.id.clone();
    let theirs_seat = seat_of(full.player(match our.other() {
        Side::White => dameless_lichess::Side::White,
        Side::Black => dameless_lichess::Side::Black,
    }));
    let time = time_of(&full);
    let clocked = full.clock.is_some();
    let increment = full.clock.map_or(0, |c| c.increment);
    let source = Source::Lichess { connection, id: id.clone(), rated: full.rated, speed: speed_of(&full) };
    let start = Position::from_start(full.start_fen())?;
    let start_fen = (!start.is_standard_start()).then(|| start.fen());
    let (white, black) = match our {
        Side::White => (our_seat, theirs_seat),
        Side::Black => (theirs_seat, our_seat),
    };
    let mut new = NewGame::starting(white, black, source.clone(), time, dameless_store::now());
    new.start_fen = start_fen.clone();
    new.url = source.url(&lichess_url);
    if let Some(created) = full.created_at {
        new.started_at = dameless_core::Timestamp::from_unix_ms(created);
    }

    // A game come back to goes on in its record, as far as the record agrees with Lichess.
    let lichess_moves: Vec<String> = full.state.moves().map(str::to_owned).collect();
    let mut pos = start.clone();
    let mut played: Vec<String> = Vec::new();
    let record = match hub.store.find_source(source.clone()).await? {
        Some(stored) => {
            let stored = hub.store.game(stored.id).await?.map(|g| g.moves).unwrap_or_default();
            let mut kept = Vec::new();
            for (m, uci) in stored.into_iter().zip(&lichess_moves) {
                if m.uci != *uci || pos.play(uci).is_err() {
                    break;
                }
                played.push(uci.clone());
                kept.push(m);
            }
            let found = hub.store.find_source(source.clone()).await?.map(|s| s.id);
            let found = found.ok_or_else(|| Error::internal("the stored game is gone"))?;
            Record::resume(&hub, found, &new, kept, pos.turn()).await?
        }
        None => Record::create(&hub, new, pos.turn()).await?,
    };
    let (commands_tx, mut commands) = mpsc::unbounded_channel();
    games.register(
        record.id,
        Handle { live: record.live.clone(), commands: commands_tx, moves, cancel: cancel.clone() },
    );
    on_record(record.id);
    record.started().await;

    tx.send_modify(|m| m.apply(&full.state, clocked));
    let mut rx = tx.subscribe();
    let mut theirs = LichessOpponent { mirror: tx.subscribe() };
    let follower = CancellationToken::new();
    let f = Follow { client: client.clone(), id: id.clone(), clocked, timing: timing.clone() };
    tokio::spawn(follow(f, stream, tx, follower.clone()));

    let you = (connection == ConnectionKey::You).then_some(our);
    // Our own last move, with the engine's report, until Lichess shows it.
    let mut pending: Option<(u32, SeatMove, u64)> = None;
    let mut leaving = false;
    let mut last_clocks: Option<(u64, u64)> = None;
    let mut claim_at: Option<tokio::time::Instant> = None;
    let mut claim_for: Option<Instant> = None;
    let end = loop {
        let m = rx.borrow_and_update().clone();
        // A takeback: the record goes back to where the game is.
        let common = played.iter().zip(&m.moves).take_while(|(a, b)| a == b).count();
        if common < played.len() {
            played.truncate(common);
            pos = Position::after(start_fen.as_deref(), &played)?;
            record.undone(common as u32, pos.turn()).await;
            pending = None;
        }
        let new = m.moves.len().saturating_sub(played.len());
        let mut broken = false;
        for i in played.len()..m.moves.len() {
            let mover = pos.turn();
            let Ok(done) = pos.play(&m.moves[i]) else {
                tracing::warn!("lichess game {id}: Lichess sent {}, which is not legal here", m.moves[i]);
                broken = true;
                break;
            };
            played.push(done.uci.clone());
            let ply = played.len() as u32;
            let after = m.clock_after.get(i).copied().flatten();
            let ours_now = mover == our && pending.as_ref().is_some_and(|(p, _, _)| *p == ply);
            let mine = if ours_now { pending.take() } else { None };
            let think = match (&mine, last_clocks, after) {
                (Some((_, _, took)), _, _) => Some(*took),
                // The opponent's time: what its clock lost, less the increment it got back.
                (None, Some(before), Some(now)) if new == 1 => {
                    let (b, a) = if mover == Side::White { (before.0, now.0) } else { (before.1, now.1) };
                    Some((b + increment).saturating_sub(a))
                }
                _ => None,
            };
            if after.is_some() {
                last_clocks = after;
            }
            let mv = Move {
                uci: done.uci,
                san: done.san,
                clock_ms: after.map(|(w, b)| if mover == Side::White { w } else { b }),
                think_ms: think,
                eval: mine.as_ref().and_then(|(_, m, _)| m.eval),
                depth: mine.as_ref().and_then(|(_, m, _)| m.depth),
                nodes: mine.as_ref().and_then(|(_, m, _)| m.nodes),
            };
            let running = clocked && played.len() >= 2 && m.status == LichessStatus::Started;
            record.moved(mv, pos.fen(), pos.turn(), after.map(|(w, b)| SidePair::new(w, b)), running).await;
        }
        if m.clocks.is_some() {
            last_clocks = m.clocks;
        }
        if broken && !leaving {
            leaving = true;
            leave(ours.as_mut(), played.len()).await;
        }

        // Offers, the opponent's absence and what you may do now.
        let can_claim = m.gone.is_some_and(|(s, at)| at.elapsed().as_secs() >= u64::from(s));
        let offers = Offers { draw: m.draw, takeback: m.takeback };
        let over = m.status.is_over() || m.lost;
        let actions = match you {
            Some(y) if !over => board::actions(y, pos.turn(), played.len() as u32, offers, can_claim),
            _ => Vec::new(),
        };
        let running = clocked && played.len() >= 2 && m.status == LichessStatus::Started;
        let gone = m.gone;
        if record.live.state.lock().unwrap().gone != gone {
            record.change(|s| s.gone = gone);
        }
        record.set(offers, actions, m.clocks.map(|(w, b)| SidePair::new(w, b)), running);

        if m.status.is_over() {
            let end = dameless_core::lichess::ending(m.status.key(), m.winner, Some(&pos));
            break end.or(Some(GameEnd::aborted()));
        }
        if m.lost {
            break Some(GameEnd { outcome: None, ending: dameless_core::game::Ending::Other });
        }

        // You are offered the claim once the opponent has been gone long enough.
        match m.gone {
            Some((secs, at)) if claim_for != Some(at) => {
                claim_for = Some(at);
                claim_at = Some(
                    tokio::time::Instant::from_std(at)
                        + std::time::Duration::from_secs(secs.into())
                        + timing.claim_margin,
                );
            }
            None => {
                claim_for = None;
                claim_at = None;
            }
            _ => {}
        }

        let side_to_move = pos.turn();
        let fen = pos.fen();
        let turn = Turn {
            game: record.id,
            side: side_to_move,
            ply: played.len() as u32 + 1,
            start_fen: start_fen.as_deref(),
            moves: &played,
            fen: &fen,
            time,
            clocks: m.clocks.map(|(w, b)| SidePair::new(w, b)),
        };
        let began = Instant::now();
        let woke = {
            let seat: &mut dyn SeatController = if side_to_move == our { ours.as_mut() } else { &mut theirs };
            tokio::select! {
                biased;
                _ = detach.cancelled() => Woke::Detach,
                _ = cancel.cancelled(), if !leaving => Woke::Stop,
                _ = async { tokio::time::sleep_until(claim_at.unwrap()).await }, if claim_at.is_some() => {
                    if you.is_some() { Woke::Refresh } else { Woke::Claim }
                }
                Some(cmd) = commands.recv() => Woke::Command(cmd),
                r = seat.next_move(turn), if !leaving => Woke::Moved(r, began.elapsed().as_millis() as u64),
                changed = rx.changed() => if changed.is_ok() { Woke::Changed } else {
                    // The follower stopped without an end: the game is gone from Lichess.
                    Woke::Moved(Err(Error::conflict("the game stream ended")), 0)
                },
            }
        };
        match woke {
            Woke::Detach => break None,
            Woke::Stop => {
                leaving = true;
                leave(ours.as_mut(), played.len()).await;
            }
            Woke::Claim => {
                claim_at = None;
                if let Err(e) = ours.action(SeatAction::ClaimVictory).await {
                    // Too early by Lichess's clock, or a blip: again shortly, unless the opponent comes back first.
                    tracing::info!("lichess game {id}: could not claim the win: {e}");
                    claim_at = Some(tokio::time::Instant::now() + timing.reconnect_max);
                }
            }
            Woke::Refresh => claim_at = None,
            Woke::Command(Command::Act { action, ack }) => {
                let _ = ack.send(ours.action(action).await);
            }
            Woke::Changed => {}
            Woke::Moved(Ok(mv), took) => {
                if side_to_move == our {
                    pending = Some((played.len() as u32 + 1, mv, took));
                }
            }
            Woke::Moved(Err(e), _) => {
                if rx.has_changed().is_err() && !rx.borrow().status.is_over() {
                    break Some(GameEnd { outcome: None, ending: dameless_core::game::Ending::Other });
                }
                if side_to_move == our && !leaving {
                    tracing::warn!("lichess game {id}: {e}: leaving the game");
                    leaving = true;
                    leave(ours.as_mut(), played.len()).await;
                }
            }
        }
    };
    follower.cancel();
    let Some(end) = end else {
        games.forget(record.id);
        return Ok(record.live.view().summary);
    };
    let summary = record.finish(end).await;
    games.forget(record.id);
    drop(ours);
    Ok(summary)
}

/// Leaves a game we cannot or will not play: aborts it while that is allowed (before both sides have moved), else
/// resigns.
async fn leave(ours: &mut dyn SeatController, plies: usize) {
    if plies < 2 && ours.action(SeatAction::Abort).await.is_ok() {
        return;
    }
    if let Err(e) = ours.action(SeatAction::Resign).await {
        tracing::warn!("lichess: could not resign: {e}");
    }
}
