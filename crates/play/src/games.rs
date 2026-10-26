//! The games being played: starting a local game, your actions in any game you sit in, and the game as it is now.
//! Each game runs in its own task; [`Games`] keeps a handle on each while it lives and overlays its live part (clocks,
//! offers, what you may do) on the stored record, which holds none of it.

use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use dameless_core::{
    Error, GameId, PlayerKey, Side, SidePair, TimeControl,
    chess::Position,
    game::{Ending, Game, GameEnd, GameSummary, Seat, SeatAction, Source},
    referee::{Referee, Setup, Sitter},
};
use dameless_hub::Hub;
use dameless_store::{GameFilter, NewGame, SourceKind, now};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{RefOr, Schema, schema::ObjectBuilder, schema::Type},
};

use crate::{
    binaries::Binaries,
    engine_seat::{EngineOptions, EngineSeat},
    local::{self, LocalGame},
    record::{Live, Record},
    seat::{LocalSeat, MoveRequest, MoveSender, SeatController},
    timing::Timing,
};

// ---------------------------------------------------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------------------------------------------------

/// Who sits at a side of a local game: you, or Dameless at a version. Written as a string: `you`, `dameless` (the
/// champion), `dameless:<version>` (a name such as `v14` or a branch, or an id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatRequest {
    You,
    Dameless { version: Option<String> },
}

impl SeatRequest {
    /// The version a Dameless seat plays: the champion unless one is named.
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::You => None,
            Self::Dameless { version } => Some(version.as_deref().unwrap_or("champion")),
        }
    }
}

impl fmt::Display for SeatRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::You => f.write_str("you"),
            Self::Dameless { version: None } => f.write_str("dameless"),
            Self::Dameless { version: Some(v) } => write!(f, "dameless:{v}"),
        }
    }
}

impl FromStr for SeatRequest {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "you" => Ok(Self::You),
            "dameless" => Ok(Self::Dameless { version: None }),
            other => match other.strip_prefix("dameless:").map(str::trim) {
                Some(v) if !v.is_empty() => Ok(Self::Dameless { version: Some(v.to_owned()) }),
                _ => Err(format!("{other:?} is not a seat: use you, dameless or dameless:<version>")),
            },
        }
    }
}

impl Serialize for SeatRequest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for SeatRequest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

impl PartialSchema for SeatRequest {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .description(Some("`you`, `dameless` (the champion) or `dameless:<version>`"))
            .examples(["you", "dameless", "dameless:v14"])
            .into()
    }
}

impl ToSchema for SeatRequest {}

/// Where a game starts, when not from the standard position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StartFrom {
    /// A FEN (or `startpos`) and moves played from it (UCI or SAN).
    Position {
        fen: String,
        #[serde(default)]
        moves: Vec<String>,
    },
    /// A random line of the opening book the trials use.
    Book,
}

/// `POST /api/games`: a local game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NewGameRequest {
    pub white: SeatRequest,
    pub black: SeatRequest,
    pub time: TimeControl,
    #[serde(default)]
    pub from: Option<StartFrom>,
    /// Hash per engine, MiB. Default 16.
    #[serde(default)]
    pub hash: Option<u32>,
    /// Threads per engine. Default 1.
    #[serde(default)]
    pub threads: Option<u32>,
    /// End decided games early, as the trials do (engines' scores only).
    #[serde(default)]
    pub adjudicate: bool,
}

// ---------------------------------------------------------------------------------------------------------------------
// The games being played
// ---------------------------------------------------------------------------------------------------------------------

/// What a game's task is asked by the API, beside moves (which go straight to your seat).
pub(crate) enum Command {
    Act { action: SeatAction, ack: oneshot::Sender<Result<(), Error>> },
}

/// A game being played, as the API reaches it.
pub(crate) struct Handle {
    pub live: Arc<Live>,
    pub commands: mpsc::UnboundedSender<Command>,
    /// Where your moves go, in a game you play.
    pub moves: Option<MoveSender>,
    /// Stops the game: a local game is stopped (resigned or aborted), a Lichess game resigned (or aborted).
    pub cancel: CancellationToken,
}

struct Inner {
    hub: Arc<Hub>,
    binaries: Arc<dyn Binaries>,
    timing: Timing,
    live: Mutex<HashMap<GameId, Handle>>,
    /// Your Lichess username, shown on your seat when linked.
    your_name: Mutex<Option<String>>,
}

/// The games being played. Cheap to clone.
#[derive(Clone)]
pub struct Games {
    inner: Arc<Inner>,
}

impl Games {
    pub fn new(hub: Arc<Hub>, binaries: Arc<dyn Binaries>, timing: Timing) -> Self {
        Self {
            inner: Arc::new(Inner {
                hub,
                binaries,
                timing,
                live: Mutex::new(HashMap::new()),
                your_name: Mutex::new(None),
            }),
        }
    }

    pub(crate) fn hub(&self) -> &Arc<Hub> {
        &self.inner.hub
    }

    pub(crate) fn binaries(&self) -> &Arc<dyn Binaries> {
        &self.inner.binaries
    }

    pub(crate) fn timing(&self) -> &Timing {
        &self.inner.timing
    }

    /// Your Lichess username, for your seat in local games (none: "You").
    pub fn set_your_name(&self, name: Option<String>) {
        *self.inner.your_name.lock().unwrap() = name;
    }

    fn your_seat(&self) -> Seat {
        let name = self.inner.your_name.lock().unwrap().clone().unwrap_or_else(|| "You".to_owned());
        Seat::new(PlayerKey::You, name)
    }

    pub(crate) fn register(&self, id: GameId, handle: Handle) {
        self.inner.live.lock().unwrap().insert(id, handle);
    }

    pub(crate) fn forget(&self, id: GameId) {
        self.inner.live.lock().unwrap().remove(&id);
    }

    fn live_handle(&self, id: GameId) -> Option<(Arc<Live>, mpsc::UnboundedSender<Command>, Option<MoveSender>)> {
        let live = self.inner.live.lock().unwrap();
        live.get(&id).map(|h| (h.live.clone(), h.commands.clone(), h.moves.clone()))
    }

    /// Games stored as live from a run that ended without finishing them. Local games cannot go on and are recorded
    /// as aborted; Lichess games are left to their connection, which takes them up again (or closes them).
    pub async fn recover(&self) -> Result<u32, Error> {
        let mut n = 0;
        for g in self.inner.hub.store.live_games().await? {
            if g.source == Source::Local {
                let s = self.inner.hub.store.finish_game(g.id, GameEnd::aborted(), now()).await?;
                self.inner.hub.bus.emit(dameless_core::events::Payload::GameUpserted(Box::new(s)));
                n += 1;
            }
        }
        Ok(n)
    }

    /// The game as it is now: a live game's clocks, offers and your actions laid over the stored record.
    pub async fn game(&self, id: GameId) -> Result<Game, Error> {
        if let Some((live, _, _)) = self.live_handle(id) {
            return Ok(live.view());
        }
        self.inner.hub.store.game(id).await?.ok_or_else(|| Error::not_found(format!("No game {id}")))
    }

    /// The games being played now, as they stand.
    pub fn live(&self) -> Vec<GameSummary> {
        let live = self.inner.live.lock().unwrap();
        let mut games: Vec<GameSummary> = live.values().map(|h| h.live.view().summary).collect();
        games.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
        games
    }

    /// Stored Lichess games of a connection still marked live, for it to take up or close.
    pub(crate) async fn stored_live_lichess(
        &self,
        connection: dameless_core::ConnectionKey,
    ) -> Result<Vec<GameSummary>, Error> {
        let games = self
            .inner
            .hub
            .store
            .games(GameFilter {
                sources: vec![SourceKind::Lichess],
                statuses: vec![dameless_core::game::GameStatus::Starting, dameless_core::game::GameStatus::Live],
                limit: dameless_store::games::MAX_LIMIT,
                ..GameFilter::default()
            })
            .await?;
        let playing: Vec<GameId> = self.inner.live.lock().unwrap().keys().copied().collect();
        Ok(games
            .into_iter()
            .filter(|g| matches!(&g.source, Source::Lichess { connection: c, .. } if *c == connection))
            .filter(|g| !playing.contains(&g.id))
            .collect())
    }

    /// Closes a stored Lichess game that is not being played any more and whose end is not known.
    pub(crate) async fn close_stale(&self, id: GameId) -> Result<(), Error> {
        let end = GameEnd { outcome: None, ending: Ending::Other };
        let s = self.inner.hub.store.finish_game(id, end, now()).await?;
        self.inner.hub.bus.emit(dameless_core::events::Payload::GameUpserted(Box::new(s)));
        Ok(())
    }

    /// Does `action` in game `id` as you: 404 for no such game, 409 when the game is over or the action is not one of
    /// [`Game::actions`] now, 400 for a move that is not legal (or for a game you do not play). A move answers once it
    /// is on the board; anything else once the game (or Lichess) has acted on it.
    pub async fn act(&self, id: GameId, action: SeatAction, uci: Option<String>) -> Result<Game, Error> {
        let Some((live, commands, moves)) = self.live_handle(id) else {
            return match self.inner.hub.store.game_summary(id).await? {
                Some(_) => Err(Error::conflict("the game is over")),
                None => Err(Error::not_found(format!("No game {id}"))),
            };
        };
        let view = live.view();
        if view.summary.you.is_none() {
            return Err(Error::bad_request("you do not play in this game"));
        }
        if !view.actions.contains(&action) {
            return Err(Error::conflict(format!("you cannot {} now", action.as_str().replace('_', " "))));
        }
        let answer = self.inner.timing.answer;
        if action == SeatAction::Move {
            let uci = uci.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty());
            let uci = uci.ok_or_else(|| Error::bad_request("a move needs its UCI (or SAN)"))?;
            let moves = moves.ok_or_else(|| Error::bad_request("you do not play in this game"))?;
            let mut version = live.version.subscribe();
            let before = live.plies();
            let (ack, answered) = oneshot::channel();
            moves.send(MoveRequest { uci, ack }).map_err(|_| Error::conflict("the game is over"))?;
            match tokio::time::timeout(answer, answered).await {
                Ok(Ok(result)) => result?,
                _ => return Err(Error::conflict("the game moved on")),
            }
            // On the board once the game has it (Lichess's stream, for a Lichess game).
            let _ = tokio::time::timeout(answer, async {
                while live.plies() <= before && live.is_live() {
                    if version.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await;
            return Ok(live.view());
        }
        let (ack, answered) = oneshot::channel();
        commands.send(Command::Act { action, ack }).map_err(|_| Error::conflict("the game is over"))?;
        match tokio::time::timeout(answer, answered).await {
            Ok(Ok(result)) => result?,
            _ => return Err(Error::conflict("the game moved on")),
        }
        Ok(live.view())
    }

    /// Stops a game being played: a local game you sit in is resigned once each side has moved (else aborted), an
    /// engine game aborted, a Lichess game resigned (or aborted before both sides have moved). Answers the game once
    /// it has ended, or as it stands after a few seconds.
    pub async fn stop(&self, id: GameId) -> Result<Game, Error> {
        let handle = {
            let live = self.inner.live.lock().unwrap();
            live.get(&id).map(|h| (h.live.clone(), h.cancel.clone()))
        };
        let Some((live, cancel)) = handle else {
            return match self.inner.hub.store.game(id).await? {
                Some(_) => Err(Error::conflict("the game is over")),
                None => Err(Error::not_found(format!("No game {id}"))),
            };
        };
        let mut version = live.version.subscribe();
        cancel.cancel();
        let _ = tokio::time::timeout(self.inner.timing.answer, async {
            while live.is_live() {
                if version.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
        Ok(live.view())
    }

    /// Starts a local game: its engines are started (and their versions built when needed) before it is stored, so a
    /// broken engine is an error here and not a dead game. Answers the game as it starts.
    pub async fn start(&self, req: NewGameRequest) -> Result<Game, Error> {
        let seats = [&req.white, &req.black];
        if seats.iter().all(|s| **s == SeatRequest::You) {
            return Err(Error::bad_request("one side at least must be Dameless"));
        }
        match req.time {
            TimeControl::Clock { initial_ms: 0, .. } => {
                return Err(Error::bad_request("a clock needs some time on it"));
            }
            TimeControl::Movetime { ms: 0 } | TimeControl::Nodes { nodes: 0 } => {
                return Err(Error::bad_request("a move needs some time or some nodes"));
            }
            TimeControl::Correspondence { .. } => {
                return Err(Error::bad_request("correspondence is for Lichess: use a clock, a time or nodes per move"));
            }
            _ => {}
        }
        let start = self.opening(req.from.as_ref()).await?;
        let start_fen = (!start.is_standard_start()).then(|| start.fen());
        let options = EngineOptions {
            hash: req.hash.unwrap_or(16).clamp(1, 4096),
            threads: req.threads.unwrap_or(1).clamp(1, 64),
            ..EngineOptions::default()
        };
        let adjudicate = req.adjudicate && seats.iter().all(|s| **s != SeatRequest::You);

        let mut players: Vec<(Seat, Box<dyn SeatController>, Sitter)> = Vec::new();
        let mut moves: Option<MoveSender> = None;
        for request in seats {
            match request.version() {
                None => {
                    let (seat, tx) = LocalSeat::new();
                    moves = Some(tx);
                    players.push((self.your_seat(), Box::new(seat), Sitter::Person));
                }
                Some(version) => {
                    let binary = self.inner.binaries.binary(version).await?;
                    let engine = EngineSeat::start(
                        &binary,
                        options,
                        &self.inner.hub.ledger,
                        self.inner.hub.bus.clone(),
                        &format!("Local game: {}", binary.version.name),
                    )
                    .await?
                    .pace(self.inner.timing.info_every, self.inner.timing.unclocked_movetime_ms);
                    let seat = Seat {
                        version: Some(binary.version.id),
                        build: Some(binary.build),
                        ..Seat::new(PlayerKey::Dameless, binary.version.name.clone())
                    };
                    players.push((seat, Box::new(engine), Sitter::Engine));
                }
            }
        }
        let [(white, white_seat, white_sitter), (black, black_seat, black_sitter)]: [_; 2] =
            players.try_into().map_err(|_| Error::internal("a game has two seats"))?;
        let referee = Referee::new(Setup {
            start_fen: start_fen.clone(),
            time: req.time,
            seats: SidePair::new(white_sitter, black_sitter),
            adjudicate,
        })?;
        let you = [(&req.white, Side::White), (&req.black, Side::Black)]
            .into_iter()
            .find(|(r, _)| **r == SeatRequest::You)
            .map(|(_, s)| s);

        let mut new = NewGame::starting(white, black, Source::Local, req.time, now());
        new.start_fen = start_fen.clone();
        let record = Record::create(&self.inner.hub, new, referee.turn()).await?;
        let (commands_tx, commands) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        self.register(
            record.id,
            Handle { live: record.live.clone(), commands: commands_tx, moves, cancel: cancel.clone() },
        );
        record.started().await;
        let actions = you.map(|s| referee.actions_for(s)).unwrap_or_default();
        record.set(referee.offers(), actions, referee.clocks(), referee.clock().is_some());
        let view = record.live.view();
        tokio::spawn(local::run(LocalGame {
            games: self.clone(),
            record,
            referee,
            start_fen,
            time: req.time,
            seats: [white_seat, black_seat],
            commands,
            cancel,
            you,
        }));
        Ok(view)
    }

    /// The position a game starts from.
    async fn opening(&self, from: Option<&StartFrom>) -> Result<Position, Error> {
        match from {
            None => Ok(Position::start()),
            Some(StartFrom::Position { fen, moves }) => {
                let mut pos = Position::from_fen(fen)?;
                for m in moves {
                    pos.play(m)?;
                }
                // A game from here starts afresh: its history is the position itself.
                Ok(Position::from_fen(&pos.fen())?)
            }
            Some(StartFrom::Book) => {
                let tools = self.inner.hub.config.tools_dir();
                let book = dameless_engine::tools::installed(&tools)
                    .book
                    .ok_or_else(|| Error::conflict("the opening book is not installed: run `dameless setup`"))?;
                let text = tokio::fs::read_to_string(&book)
                    .await
                    .map_err(|e| Error::internal(format!("read the opening book: {e}")))?;
                let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                if lines.is_empty() {
                    return Err(Error::internal("the opening book is empty"));
                }
                let line = lines[random_below(lines.len())];
                Ok(Position::from_fen(line)?)
            }
        }
    }

    /// Waits until game `id` is no longer live, or `wait` has passed. For tests and the CLI's `--wait`.
    pub async fn wait_finished(&self, id: GameId, wait: Duration) -> Result<Game, Error> {
        if let Some((live, _, _)) = self.live_handle(id) {
            let mut version = live.version.subscribe();
            let _ = tokio::time::timeout(wait, async {
                while live.is_live() {
                    if version.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await;
        }
        self.game(id).await
    }
}

/// A random number below `n` (at least 1).
pub(crate) fn random_below(n: usize) -> usize {
    let mut bytes = [0u8; 8];
    let _ = getrandom::fill(&mut bytes);
    (u64::from_le_bytes(bytes) % n.max(1) as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seats_read_as_strings() {
        assert_eq!("you".parse::<SeatRequest>().unwrap(), SeatRequest::You);
        assert_eq!("dameless".parse::<SeatRequest>().unwrap().version(), Some("champion"));
        assert_eq!("dameless:v14".parse::<SeatRequest>().unwrap().version(), Some("v14"));
        assert!("bob".parse::<SeatRequest>().is_err());
        let req: NewGameRequest = serde_json::from_value(serde_json::json!({
            "white": "you", "black": "dameless:v3", "time": { "type": "clock", "initial_ms": 300000, "increment_ms": 3000 },
            "from": { "type": "position", "fen": "startpos", "moves": ["e4"] }
        }))
        .unwrap();
        assert_eq!(req.black.to_string(), "dameless:v3");
        assert!(!req.adjudicate);
    }
}
