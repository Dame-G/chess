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
    engines::SeatSpec,
    game::{Ending, Game, GameEnd, GameSummary, Seat, SeatAction, Source},
    levels::{Entrant, Level},
    referee::{Referee, Setup, Sitter},
};
use dameless_hub::Hub;
use dameless_store::{GameFilter, NewGame, SourceKind, now};
use dameless_work::EngineSource;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{RefOr, Schema, schema::ObjectBuilder, schema::Type},
};

use crate::{
    engine_seat::{EngineOptions, EngineSeat},
    levels::Levels,
    local::{self, LocalGame},
    record::{Live, Record},
    seat::{LocalSeat, MoveRequest, MoveSender, SeatController},
    timing::Timing,
};

// ---------------------------------------------------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------------------------------------------------

/// Who sits at a side of a local game: you, or an engine of the engines list, at full strength (or its fixed budget)
/// or at a level of the ladder. Written as a string: `you`, `<engine id>` (`stockfish`, `maia-1500`), and
/// `<engine id>@<level>` (`stockfish@5`) for a level. The object form `{ "player": "stockfish", "level": 5 }` reads
/// too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatRequest {
    You,
    Engine { id: String, level: Option<u32> },
}

impl SeatRequest {
    /// An engine at full strength.
    pub fn engine(id: &str) -> Self {
        Self::Engine { id: id.to_owned(), level: None }
    }

    /// An engine at a level.
    pub fn at_level(id: &str, level: u32) -> Self {
        Self::Engine { id: id.to_owned(), level: Some(level) }
    }

    /// The engine an engine seat plays.
    pub fn engine_id(&self) -> Option<&str> {
        match self {
            Self::You => None,
            Self::Engine { id, .. } => Some(id),
        }
    }

    /// The level an engine seat plays at, when it plays at one.
    pub fn level(&self) -> Option<u32> {
        match self {
            Self::You => None,
            Self::Engine { level, .. } => *level,
        }
    }
}

impl fmt::Display for SeatRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::You => f.write_str("you"),
            Self::Engine { id, level: None } => f.write_str(id),
            Self::Engine { id, level: Some(l) } => write!(f, "{id}@{l}"),
        }
    }
}

impl FromStr for SeatRequest {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match SeatSpec::parse(s)? {
            SeatSpec::You => Self::You,
            SeatSpec::Engine { id, level } => Self::Engine { id, level },
        })
    }
}

impl Serialize for SeatRequest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// A seat as JSON: the string form, or an object.
#[derive(Deserialize)]
#[serde(untagged)]
enum SeatWire {
    Text(String),
    Object {
        player: String,
        #[serde(default)]
        level: Option<u32>,
    },
}

impl<'de> Deserialize<'de> for SeatRequest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match SeatWire::deserialize(d)? {
            SeatWire::Text(text) => text.parse().map_err(serde::de::Error::custom),
            SeatWire::Object { player, level } => match player.trim().parse::<Self>() {
                Ok(Self::You) if level.is_none() => Ok(Self::You),
                Ok(Self::Engine { id, level: None }) => Ok(Self::Engine { id, level }),
                _ => Err(serde::de::Error::custom(format!("{player:?} is not a seat: use you or an engine's id"))),
            },
        }
    }
}

impl PartialSchema for SeatRequest {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .description(Some(
                "`you`, an engine's id (`stockfish`, `maia-1500`), or `<engine id>@<level>` for a level of the \
                 analysis engine's ladder (`stockfish@5`). The object `{ player, level? }` reads too.",
            ))
            .examples(["you", "stockfish", "stockfish@5", "maia-1500"])
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
    /// Seeds the choices of seats at a level, so a game can be replayed; random when none.
    #[serde(default)]
    pub seed: Option<u64>,
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
    engines: Arc<dyn EngineSource>,
    timing: Timing,
    live: Mutex<HashMap<GameId, Handle>>,
    /// Your Lichess username, shown on your seat when linked.
    your_name: Mutex<Option<String>>,
    levels: Levels,
}

/// The games being played. Cheap to clone.
#[derive(Clone)]
pub struct Games {
    inner: Arc<Inner>,
}

impl Games {
    pub fn new(hub: Arc<Hub>, engines: Arc<dyn EngineSource>, timing: Timing) -> Self {
        let levels = Levels::new(hub.clone());
        Self::with_levels(hub, engines, timing, levels)
    }

    /// With these levels (another ladder, for tests).
    pub fn with_levels(hub: Arc<Hub>, engines: Arc<dyn EngineSource>, timing: Timing, levels: Levels) -> Self {
        Self {
            inner: Arc::new(Inner {
                hub,
                engines,
                timing,
                live: Mutex::new(HashMap::new()),
                your_name: Mutex::new(None),
                levels,
            }),
        }
    }

    /// The levels: their ratings, the refit, calibration.
    pub fn levels(&self) -> &Levels {
        &self.inner.levels
    }

    pub(crate) fn hub(&self) -> &Arc<Hub> {
        &self.inner.hub
    }

    pub(crate) fn engines(&self) -> &Arc<dyn EngineSource> {
        &self.inner.engines
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

    /// Starts a local game: its engines are started before it is stored, so a broken engine is an error here and not
    /// a dead game. Answers the game as it starts.
    pub async fn start(&self, req: NewGameRequest) -> Result<Game, Error> {
        let seats = [&req.white, &req.black];
        if seats.iter().all(|s| **s == SeatRequest::You) {
            return Err(Error::bad_request("one side at least must be an engine"));
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
        // Levels are checked before any engine starts.
        let levels: Vec<Option<Level>> = seats
            .iter()
            .map(|s| s.level().map(|id| self.inner.levels.level(id).cloned()).transpose())
            .collect::<Result<_, _>>()?;
        let seed = req.seed.unwrap_or_else(random_u64);

        let mut players: Vec<(Seat, Box<dyn SeatController>, Sitter)> = Vec::new();
        let mut moves: Option<MoveSender> = None;
        for (index, request) in seats.into_iter().enumerate() {
            match request.engine_id() {
                None => {
                    let (seat, tx) = LocalSeat::new();
                    moves = Some(tx);
                    players.push((self.your_seat(), Box::new(seat), Sitter::Person));
                }
                Some(id) => {
                    let binary = self.inner.engines.ready(Some(id)).await?;
                    if levels[index].is_some() && !binary.engine.levels {
                        return Err(Error::bad_request(format!(
                            "{} has no levels: the ladder is the analysis engine's (as in stockfish@5)",
                            binary.engine.name
                        )));
                    }
                    let engine = EngineSeat::start(
                        &binary,
                        options,
                        &self.inner.hub.ledger,
                        self.inner.hub.bus.clone(),
                        &format!("Local game: {}", binary.engine.name),
                    )
                    .await?
                    .pace(self.inner.timing.info_every, self.inner.timing.unclocked_movetime_ms);
                    let mut seat = Seat {
                        engine: Some(binary.engine.label().to_owned()),
                        rating: binary.engine.rating,
                        ..Seat::new(binary.engine.player(), binary.engine.name.clone())
                    };
                    let engine = match &levels[index] {
                        Some(level) => {
                            let (name, rating) = self.inner.levels.seat_name(level, &binary.engine.name).await;
                            seat.name = name;
                            seat.level = Some(level.id);
                            seat.rating = Some(rating);
                            engine.at_level(level.clone(), None, seat_seed(seed, index)).await?
                        }
                        None => engine,
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
        // A game at a level counts for the levels' fit when every seat is a level or you.
        let entrants: Vec<Option<Entrant>> = [&req.white, &req.black]
            .iter()
            .map(|r| match r {
                SeatRequest::You => Some(Entrant::You),
                SeatRequest::Engine { level, .. } => level.map(Entrant::Level),
            })
            .collect();
        let at_level = levels.iter().any(Option::is_some);
        let refit = at_level
            && match entrants[..] {
                [Some(w), Some(b)] => {
                    self.inner.hub.store.record_level_game(record.id, w, b, false).await?;
                    true
                }
                _ => false,
            };
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
            refit,
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

/// A random 64-bit number.
pub(crate) fn random_u64() -> u64 {
    let mut bytes = [0u8; 8];
    let _ = getrandom::fill(&mut bytes);
    u64::from_le_bytes(bytes)
}

/// Each seat's own seed from the game's, so the two sides of a game between levels do not choose alike.
pub(crate) fn seat_seed(seed: u64, index: usize) -> u64 {
    seed ^ (index as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seats_read_as_strings() {
        assert_eq!("you".parse::<SeatRequest>().unwrap(), SeatRequest::You);
        assert_eq!("stockfish".parse::<SeatRequest>().unwrap(), SeatRequest::engine("stockfish"));
        assert_eq!("stockfish@5".parse::<SeatRequest>().unwrap(), SeatRequest::at_level("stockfish", 5));
        assert_eq!(SeatRequest::at_level("stockfish", 5).to_string(), "stockfish@5");
        assert!("Bob Smith".parse::<SeatRequest>().is_err());
        let object: SeatRequest = serde_json::from_value(serde_json::json!({ "player": "stockfish", "level": 3 })).unwrap();
        assert_eq!(object, SeatRequest::at_level("stockfish", 3));
        let req: NewGameRequest = serde_json::from_value(serde_json::json!({
            "white": "you", "black": "maia-1500", "time": { "type": "clock", "initial_ms": 300000, "increment_ms": 3000 },
            "from": { "type": "position", "fen": "startpos", "moves": ["e4"] }
        }))
        .unwrap();
        assert_eq!(req.black.to_string(), "maia-1500");
        assert!(!req.adjudicate);
    }
}
