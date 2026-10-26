//! One game you play on Lichess, from its `gameFull` to its end, through the Board API. The game stream
//! (`/api/board/game/stream/{id}`) says what happens and is recorded move by move in the game's [`Match`]; what you ask
//! through the match routes (a move, a draw offer, a takeback, resigning) goes to Lichess, and the answer comes once
//! Lichess has acted on it.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::super::{
    call_error,
    client::{CallError, Lichess, Ndjson},
    rules as bot_rules,
};
use crate::{
    api::{ApiError, AppState},
    matches::{
        self, Ack, Action, LichessAccount, LichessGame, Match, MatchMove, MatchStatus, MatchTime, Player, Recorder,
        Reply, Request,
        rules::{self, Ending, GameResult, Side},
    },
};

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
    initial: u64,
    increment: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct GameState {
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
    #[serde(default)]
    wdraw: bool,
    #[serde(default)]
    bdraw: bool,
    #[serde(default)]
    wtakeback: bool,
    #[serde(default)]
    btakeback: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameFull {
    #[serde(default)]
    rated: bool,
    #[serde(default)]
    speed: String,
    clock: Option<FullClock>,
    days_per_turn: Option<u64>,
    white: Seat,
    black: Seat,
    #[serde(default)]
    initial_fen: String,
    state: GameState,
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

/// Plays game `id` until it ends, then takes it off the running list.
pub async fn play(state: Arc<AppState>, id: String) {
    if let Err(e) = run(&state, &id).await {
        tracing::warn!("lichess game {id} (you): {e:#}");
    }
    state.me.game_over(&state, &id);
}

fn stream_path(id: &str) -> String {
    format!("/api/board/game/stream/{id}")
}

async fn open(client: &Lichess, id: &str) -> Result<Ndjson> {
    let mut wait = Duration::from_secs(1);
    for tries in 0.. {
        match client.stream(&stream_path(id)).await {
            Ok(s) => return Ok(s),
            Err(e @ (CallError::Unauthorized | CallError::Status { status: 400..=499, .. })) if tries >= 2 => {
                return Err(e.into());
            }
            Err(e) if tries >= 8 => return Err(e.into()),
            Err(CallError::RateLimited) => wait = Duration::from_secs(61),
            Err(_) => {}
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(Duration::from_secs(30));
    }
    unreachable!()
}

async fn run(state: &Arc<AppState>, id: &str) -> Result<()> {
    let me = &state.me;
    let client = me.client().context("no Lichess token")?;
    let account = me.account().context("your Lichess account is not known yet")?;
    let mut stream = open(&client, id).await?;
    let full = loop {
        match stream.next().await? {
            Some(v) if v["type"] == "gameFull" => break v,
            Some(_) => continue,
            None => bail!("the game stream ended before the game was sent"),
        }
    };
    let full: GameFull = serde_json::from_value(full).context("read the game")?;
    let our = if full.white.id == account.id {
        Side::White
    } else if full.black.id == account.id {
        Side::Black
    } else {
        bail!("{} plays neither side", account.username);
    };
    let (start, pos0) = parse_start(&full.initial_fen)?;
    let seat = if our == Side::White { &full.white } else { &full.black };
    let you = Player {
        name: account.username.clone(),
        rating: seat.rating,
        title: seat.title.clone(),
        lichess: Some(account.username.clone()),
        human: true,
        ..Player::default()
    };
    let theirs = if our == Side::White { full.black.player() } else { full.white.player() };
    let (white, black) = if our == Side::White { (you, theirs) } else { (theirs, you) };
    let time = match (full.clock, full.days_per_turn) {
        (Some(c), _) => MatchTime::Clock { seconds: c.initial as f64 / 1000.0, increment: c.increment as f64 / 1000.0 },
        // Correspondence: the time per move is the days per turn.
        (None, Some(days)) => MatchTime::Movetime { ms: days * 86_400_000 },
        (None, None) => MatchTime::Movetime { ms: 0 },
    };
    let clock_ms = full.clock.map(|c| c.initial);
    let mut game = Match {
        id: 0,
        white,
        black,
        time,
        start_fen: start.clone(),
        book: false,
        hash: 0,
        threads: 0,
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
            account: LichessAccount::Me,
            opening: None,
            draw_offer: None,
            takeback_offer: None,
        }),
    };
    // A game you come back to (after a restart) goes on in the record it has, as far as it agrees.
    if let Some(prev) = matches::find_lichess(state, id, LichessAccount::Me).await? {
        let played: Vec<&str> = full.state.moves.split_whitespace().collect();
        let keep = prev.moves.iter().zip(&played).take_while(|(m, uci)| m.uci == **uci).count();
        game.id = prev.id;
        game.created_at = prev.created_at;
        game.moves = prev.moves.into_iter().take(keep).collect();
    }
    let cancel = CancellationToken::new();
    let (rec, requests) = Recorder::open_human(state, game, cancel.clone()).await?;
    let rec = Arc::new(rec);
    me.game_recorded(state, id, rec.id());
    let mut board = Board::new(state.clone(), rec.clone(), pos0, our, full.clock.is_some())?;
    let ending = board.follow(&client, id, stream, full.state, requests, &cancel).await;
    board.answer_pending(Err(ApiError::conflict("the game is over")));
    rec.finish(ending).await;
    Ok(())
}

/// The game as it stands on our side of the stream.
struct Board {
    state: Arc<AppState>,
    rec: Arc<Recorder>,
    pos0: Chess,
    pos: Chess,
    history: Vec<u64>,
    played: Vec<String>,
    our: Side,
    clocked: bool,
    clocks: Option<(u64, u64)>,
    /// Offers standing now: (draw, takeback), by side.
    draw: Option<Side>,
    takeback: Option<Side>,
    /// Your move sent to Lichess, answered once the game has it: the number of moves it makes.
    pending: Option<(usize, Ack)>,
}

impl Board {
    fn new(state: Arc<AppState>, rec: Arc<Recorder>, pos0: Chess, our: Side, clocked: bool) -> Result<Self> {
        let mut b = Self {
            state,
            rec,
            pos: pos0.clone(),
            history: vec![rules::hash(&pos0)],
            pos0,
            played: Vec::new(),
            our,
            clocked,
            clocks: None,
            draw: None,
            takeback: None,
            pending: None,
        };
        for mv in b.rec.game().moves {
            b.play_uci(&mv.uci)?;
        }
        Ok(b)
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

    fn answer_pending(&mut self, answer: Result<Reply, ApiError>) {
        if let Some((_, ack)) = self.pending.take() {
            let _ = ack.send(answer);
        }
    }

    /// Takes in a game state: records new moves and offers, and says whether the game is over.
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
            let turn = self.pos.turn().into();
            self.rec.update(|m| m.turn = turn);
            self.state.events.emit("match.takeback", json!({ "id": self.rec.id(), "plies": common }));
        }
        let new = moves.len().saturating_sub(self.played.len());
        for (i, uci) in moves.iter().enumerate().skip(self.played.len()) {
            let (side, san) = self.play_uci(uci)?;
            let last = i + 1 == moves.len();
            let now = (self.clocked && last).then_some((st.wtime, st.btime));
            let took = match (self.clocks, now) {
                (Some(before), Some(after)) if new == 1 => {
                    let (b, a, inc) = match side {
                        Side::White => (before.0, after.0, st.winc),
                        Side::Black => (before.1, after.1, st.binc),
                    };
                    (b + inc).saturating_sub(a)
                }
                _ => 0,
            };
            let mv = MatchMove {
                ply: self.played.len() as u32,
                side,
                uci: (*uci).to_owned(),
                san,
                fen_after: Fen::from_position(&self.pos, EnPassantMode::Legal).to_string(),
                clock_white_ms: now.map(|c| c.0),
                clock_black_ms: now.map(|c| c.1),
                eval: None,
                depth: None,
                nodes: None,
                time_ms: took,
            };
            self.rec.push(mv, self.pos.turn().into()).await;
        }
        if self.pending.as_ref().is_some_and(|(n, _)| self.played.len() >= *n) {
            self.answer_pending(Ok(Reply::Moved));
        }
        if self.clocked {
            self.clocks = Some((st.wtime, st.btime));
            self.rec.clocks(Some(st.wtime), Some(st.btime));
        }
        let side = |w: bool, b: bool| match (w, b) {
            (true, _) => Some(Side::White),
            (_, true) => Some(Side::Black),
            _ => None,
        };
        let (draw, takeback) = (side(st.wdraw, st.bdraw), side(st.wtakeback, st.btakeback));
        if (draw, takeback) != (self.draw, self.takeback) {
            self.draw = draw;
            self.takeback = takeback;
            self.rec.update(|m| {
                if let Some(l) = m.lichess.as_mut() {
                    l.draw_offer = draw;
                    l.takeback_offer = takeback;
                }
            });
            self.state
                .events
                .emit("match.offer", json!({ "id": self.rec.id(), "draw_offer": draw, "takeback_offer": takeback }));
        }
        let running = self.clocked && self.played.len() >= 2 && st.status == "started";
        self.rec.thinking(running.then(Instant::now));
        Ok(bot_rules::ending(&st.status, st.winner, &self.pos, &self.history))
    }

    fn their(&self) -> Side {
        match self.our {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }

    /// Acts on what you asked through the match routes.
    async fn request(&mut self, client: &Lichess, id: &str, Request { action, ack }: Request) {
        let post = |path: String| async move { client.post(&path, &[]).await.map(drop).map_err(call_error) };
        let base = format!("/api/board/game/{id}");
        let answer = match action {
            Action::Move(uci) => {
                if Side::from(self.pos.turn()) != self.our {
                    Err(ApiError::conflict("it is not your turn"))
                } else if self.pending.is_some() {
                    Err(ApiError::conflict("your last move is still on its way"))
                } else {
                    match uci.parse::<UciMove>().ok().and_then(|u| u.to_move(&self.pos).ok()) {
                        None => Err(ApiError::bad_request(&format!("{uci} is not a legal move there"))),
                        Some(m) => {
                            let uci = m.to_uci(CastlingMode::Standard).to_string();
                            match post(format!("{base}/move/{uci}")).await {
                                Ok(()) => {
                                    // Answered when the game stream has it.
                                    self.pending = Some((self.played.len() + 1, ack));
                                    return;
                                }
                                Err(e) => Err(e),
                            }
                        }
                    }
                }
            }
            Action::Draw => {
                let theirs = self.draw == Some(self.their());
                post(format!("{base}/draw/yes"))
                    .await
                    .map(|()| if theirs { Reply::Draw { accepted: true } } else { Reply::Offered })
            }
            Action::Takeback => {
                let theirs = self.takeback == Some(self.their());
                post(format!("{base}/takeback/yes"))
                    .await
                    .map(|()| if theirs { Reply::TakenBack } else { Reply::Offered })
            }
            Action::DeclineDraw if self.draw != Some(self.their()) => {
                Err(ApiError::conflict("there is no draw offer to decline"))
            }
            Action::DeclineDraw => post(format!("{base}/draw/no")).await.map(|()| Reply::Done),
            Action::DeclineTakeback if self.takeback != Some(self.their()) => {
                Err(ApiError::conflict("there is no takeback to decline"))
            }
            Action::DeclineTakeback => post(format!("{base}/takeback/no")).await.map(|()| Reply::Done),
            Action::Abort => post(format!("{base}/abort")).await.map(|()| Reply::Done),
            Action::ClaimVictory => post(format!("{base}/claim-victory")).await.map(|()| Reply::Done),
            Action::ClaimDraw => post(format!("{base}/claim-draw")).await.map(|()| Reply::Done),
        };
        let _ = ack.send(answer);
    }

    /// Follows the game to its end, reconnecting the stream if it drops, and carries out what you ask.
    async fn follow(
        &mut self,
        client: &Lichess,
        id: &str,
        stream: Ndjson,
        first: GameState,
        mut requests: mpsc::UnboundedReceiver<Request>,
        cancel: &CancellationToken,
    ) -> Ending {
        let mut stream = Some(stream);
        let mut reconnect = tokio::time::Instant::now();
        let mut backoff = Duration::from_secs(1);
        let mut next_state = Some(first);
        let mut leaving = false;
        loop {
            if let Some(st) = next_state.take() {
                match self.apply(&st).await {
                    Ok(Some(end)) => return end,
                    Ok(None) => {}
                    Err(e) => {
                        if let Some(end) = bot_rules::ending(&st.status, st.winner, &self.pos, &self.history) {
                            return end;
                        }
                        tracing::warn!("lichess game {id} (you): {e:#}");
                    }
                }
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled(), if !leaving => {
                    // `DELETE /api/matches/{id}`: abort while that is allowed, else resign.
                    leaving = true;
                    let base = format!("/api/board/game/{id}");
                    if self.played.len() < 2 && client.post(&format!("{base}/abort"), &[]).await.is_ok() {
                        continue;
                    }
                    if let Err(e) = client.post(&format!("{base}/resign"), &[]).await {
                        tracing::warn!("lichess game {id} (you): could not resign: {e}");
                        leaving = false;
                    }
                }
                Some(r) = requests.recv() => self.request(client, id, r).await,
                line = async { stream.as_mut().unwrap().next().await }, if stream.is_some() => match line {
                    Ok(Some(v)) => {
                        backoff = Duration::from_secs(1);
                        match v["type"].as_str() {
                            Some("gameFull") => next_state = serde_json::from_value(v["state"].clone()).ok(),
                            Some("gameState") => next_state = serde_json::from_value(v).ok(),
                            Some("opponentGone") => opponent_gone(&self.state, self.rec.id(), &v),
                            _ => {}
                        }
                    }
                    Ok(None) | Err(_) => {
                        stream = None;
                        reconnect = tokio::time::Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                    }
                },
                _ = tokio::time::sleep_until(reconnect), if stream.is_none() => {
                    match client.stream(&stream_path(id)).await {
                        Ok(s) => stream = Some(s),
                        Err(CallError::Status { status: 404, .. }) => {
                            return Ending { result: GameResult::Unfinished, reason: rules::Reason::Other };
                        }
                        Err(e) => {
                            tracing::warn!("lichess game {id} (you): the game stream: {e}");
                            let wait = if matches!(e, CallError::RateLimited) { Duration::from_secs(61) } else { backoff };
                            reconnect = tokio::time::Instant::now() + wait;
                            backoff = (backoff * 2).min(Duration::from_secs(30));
                        }
                    }
                }
            }
        }
    }
}

/// `match.opponent_gone`: the opponent left (or came back), and in how many seconds you may claim the game.
fn opponent_gone(state: &AppState, match_id: i64, v: &Value) {
    state.events.emit(
        "match.opponent_gone",
        json!({ "id": match_id, "gone": v["gone"].as_bool().unwrap_or(false), "claim_win_in_seconds": v["claimWinInSeconds"] }),
    );
}
