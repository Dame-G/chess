//! An engine at a seat: one UCI process for the game's life, its cores reserved in the ledger while it plays. It
//! searches with the game's clocks (`go wtime btime winc binc`) or the fixed time or nodes per move, and reports its
//! progress as `game.thinking`, at most five times a second, with scores turned to White's point of view.

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use dameless_core::{
    Error, GameId, Score, Side, TimeControl,
    events::{GameThinking, Payload},
    game::{SeatAction, SeatMove, Turn},
};
use dameless_engine::{
    proc,
    uci::{self, Engine, Info, Limits, SearchEvent},
};
use dameless_hub::{Bus, Ledger, Reservation};

use crate::{
    binaries::EngineBinary,
    error,
    seat::{SeatController, SeatKind},
};

/// How an engine seat is set up. The defaults are the tests' (Reckless's fastchess settings), as the old matches had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineOptions {
    /// MiB.
    pub hash: u32,
    pub threads: u32,
    /// Milliseconds kept in hand on every move: 0 here, where the clock is ours; more on Lichess, for the network.
    pub move_overhead: u32,
    /// Reckless's `Minimal`: off, so the engine reports while it thinks. It does not change the moves.
    pub minimal: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self { hash: 16, threads: 1, move_overhead: 0, minimal: false }
    }
}

/// How long a search for a move in a game without a clock or a limit (correspondence on Lichess) takes.
const UNCLOCKED_MS: u64 = 5_000;

/// The search under way: its position and what it has reported, kept so a dropped search can be picked up.
struct Flight {
    ply: u32,
    fen: String,
    last: Option<Info>,
    sent: Option<Instant>,
    unsent: bool,
}

pub struct EngineSeat {
    engine: Engine,
    flight: Option<Flight>,
    bus: Arc<Bus>,
    every: std::time::Duration,
    unclocked_ms: u64,
    _cores: Reservation,
}

impl EngineSeat {
    /// Starts the engine and sets it up, reserving its threads in the ledger under `owner` until the seat is dropped.
    pub async fn start(
        binary: &EngineBinary,
        options: EngineOptions,
        ledger: &Arc<Ledger>,
        bus: Arc<Bus>,
        owner: &str,
    ) -> Result<Self, Error> {
        let command = proc::Command::new(&binary.path).args(&binary.args);
        let mut engine = Engine::start_command(&command, uci::UCIOK_TIMEOUT).await.map_err(error::internal)?;
        let mut settings = vec![
            ("Hash".to_owned(), options.hash.to_string()),
            ("Threads".to_owned(), options.threads.to_string()),
            ("MoveOverhead".to_owned(), options.move_overhead.to_string()),
        ];
        if engine.options.iter().any(|o| o.name == "Minimal") {
            settings.push(("Minimal".to_owned(), options.minimal.to_string()));
        }
        engine.configure(&settings).await.map_err(error::internal)?;
        engine.new_game().await.map_err(error::internal)?;
        let cores = ledger.reserve(owner, options.threads.max(1));
        Ok(Self {
            engine,
            flight: None,
            bus,
            every: std::time::Duration::from_millis(200),
            unclocked_ms: UNCLOCKED_MS,
            _cores: cores,
        })
    }

    /// Reports at most once per `every` and searches unclocked games for `unclocked_ms` (tests shorten both).
    pub fn pace(mut self, every: std::time::Duration, unclocked_ms: u64) -> Self {
        self.every = every;
        self.unclocked_ms = unclocked_ms;
        self
    }

    fn limits(&self, turn: &Turn<'_>) -> Limits {
        match turn.time {
            TimeControl::Clock { increment_ms, initial_ms } => {
                let clocks = turn.clocks.unwrap_or(dameless_core::SidePair::both(initial_ms));
                Limits::clock(uci::Clock {
                    wtime_ms: clocks.white,
                    btime_ms: clocks.black,
                    winc_ms: increment_ms,
                    binc_ms: increment_ms,
                    movestogo: None,
                })
            }
            TimeControl::Movetime { ms } => Limits::movetime(ms),
            TimeControl::Nodes { nodes } => Limits::nodes(nodes),
            TimeControl::Correspondence { .. } => Limits::movetime(self.unclocked_ms),
        }
    }

    fn report(&self, game: GameId, ply: u32, side: Side, info: &Info) {
        self.bus.emit(Payload::GameThinking(GameThinking {
            id: game,
            ply,
            side,
            depth: info.depth,
            score: Some(white_score(info.score, side)),
            nodes: info.nodes,
            nps: info.nps,
            pv: info.pv.clone(),
        }));
    }

    /// Searches `turn` and answers the move with the engine's own last report.
    pub(crate) async fn search(&mut self, turn: &Turn<'_>) -> Result<SeatMove, Error> {
        // A search the runtime dropped (something else woke it) is picked up again when it is for the same position;
        // any other is ended first.
        let same = self.flight.as_ref().is_some_and(|f| f.ply == turn.ply && f.fen == turn.fen);
        if !(same && self.engine.searching()) {
            if self.engine.searching() {
                self.engine.finish(std::time::Duration::from_secs(5)).await.map_err(error::internal)?;
            }
            let position = uci::Position { fen: turn.start_fen.map(str::to_owned), moves: turn.moves.to_vec() };
            let limits = self.limits(turn);
            self.engine.go(&position, &limits).await.map_err(error::internal)?;
            self.flight =
                Some(Flight { ply: turn.ply, fen: turn.fen.to_owned(), last: None, sent: None, unsent: false });
        }
        loop {
            let event = self.engine.next().await.map_err(error::internal)?;
            let Some(flight) = self.flight.as_mut() else { return Err(Error::internal("no search in flight")) };
            match event {
                SearchEvent::Info(info) if info.multipv == 1 => {
                    let report = flight.sent.is_none_or(|t| t.elapsed() >= self.every);
                    if report {
                        flight.sent = Some(Instant::now());
                    }
                    flight.unsent = !report;
                    flight.last = Some(info.clone());
                    if report {
                        self.report(turn.game, turn.ply, turn.side, &info);
                    }
                }
                SearchEvent::Info(_) => {}
                SearchEvent::BestMove(best) => {
                    let flight = self.flight.take().unwrap_or_else(|| Flight {
                        ply: turn.ply,
                        fen: String::new(),
                        last: None,
                        sent: None,
                        unsent: false,
                    });
                    let last = flight.last;
                    if flight.unsent
                        && let Some(info) = &last
                    {
                        self.report(turn.game, turn.ply, turn.side, info);
                    }
                    let Some(uci) = best.best else {
                        return Err(Error::internal("the engine found no move"));
                    };
                    return Ok(SeatMove {
                        uci,
                        eval: last.as_ref().map(|i| white_score(i.score, turn.side)),
                        depth: last.as_ref().map(|i| i.depth),
                        nodes: last.as_ref().map(|i| i.nodes),
                    });
                }
            }
        }
    }

    /// Ends the engine.
    pub async fn quit(self) {
        self.engine.quit().await;
    }
}

/// An engine's score (from the side to move) as White's.
fn white_score(score: uci::Score, side: Side) -> Score {
    let own = match score {
        uci::Score::Cp(cp) => Score::Cp(cp),
        uci::Score::Mate(m) => Score::Mate(m),
    };
    own.from_side(side)
}

#[async_trait]
impl SeatController for EngineSeat {
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove, Error> {
        self.search(&turn).await
    }

    async fn action(&mut self, _action: SeatAction) -> Result<(), Error> {
        // The Referee answers offers for an engine (`core::referee::accepts_draw`).
        Ok(())
    }

    fn kind(&self) -> SeatKind {
        SeatKind::Engine
    }
}
