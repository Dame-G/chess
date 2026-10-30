//! An engine at a seat: one UCI process for the game's life, its cores reserved in the ledger while it plays. It
//! searches with the game's clocks (`go wtime btime winc binc`) or the fixed time or nodes per move, and reports its
//! progress as `game.thinking`, at most five times a second, with scores turned to White's point of view.
//!
//! At a level ([`EngineSeat::at_level`]) it searches the level's node budget with its MultiPV, whatever the game's
//! clock, and plays the move `core::levels::choose_move` picks among the lines, drawing from the seat's seeded RNG. An
//! engine with a fixed node budget searches that budget whatever the clock.

use std::{collections::BTreeMap, sync::Arc, time::Instant};

use async_trait::async_trait;
use chess_core::{
    Error, GameId, Score, Side, TimeControl,
    events::{GameThinking, Payload},
    game::{SeatAction, SeatMove, Turn},
    levels::{self, Level},
};
use chess_engine::{
    proc,
    uci::{self, Engine, Info, Limits, SearchEvent},
};
use chess_hub::{Bus, Ledger, Reservation};

use chess_work::EngineBinary;

use crate::{
    error,
    seat::{SeatController, SeatKind},
};

/// How an engine seat is set up. Options an engine does not declare are not sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineOptions {
    /// MiB.
    pub hash: u32,
    pub threads: u32,
    /// Milliseconds kept in hand on every move: 0 here, where the clock is ours; more on Lichess, for the network.
    pub move_overhead: u32,
    /// `Minimal` (Reckless's): off, so the engine reports while it thinks. It does not change the moves.
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
    /// Each MultiPV line's latest report: (depth, first move, score from the side to move).
    lines: BTreeMap<u32, (u32, String, uci::Score)>,
}

impl Flight {
    fn new(ply: u32, fen: &str) -> Self {
        Self { ply, fen: fen.to_owned(), last: None, sent: None, unsent: false, lines: BTreeMap::new() }
    }
}

/// How a seat at a level plays: its level, the nodes per move, and the RNG its choices draw from.
struct AtLevel {
    level: Level,
    nodes: Option<u64>,
    rng: SplitMix64,
}

pub struct EngineSeat {
    engine: Engine,
    flight: Option<Flight>,
    bus: Arc<Bus>,
    every: std::time::Duration,
    unclocked_ms: u64,
    level: Option<AtLevel>,
    /// The engine's fixed nodes per move, whatever the clock.
    fixed_nodes: Option<u64>,
    _cores: Option<Reservation>,
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
        let mut seat = Self::launch(binary, options, bus).await?;
        seat._cores = Some(ledger.reserve(owner, options.threads.max(1)));
        Ok(seat)
    }

    /// Starts the engine without reserving cores: for work (calibration) that holds a grant for its engines.
    pub(crate) async fn launch(binary: &EngineBinary, options: EngineOptions, bus: Arc<Bus>) -> Result<Self, Error> {
        let command = proc::Command::new(&binary.path).args(&binary.args);
        let mut engine = Engine::start_command(&command, uci::UCIOK_TIMEOUT).await.map_err(error::internal)?;
        let declared = |name: &str| engine.options.iter().any(|o| o.name.eq_ignore_ascii_case(name));
        let mut settings: Vec<(String, String)> = binary.options.clone();
        settings.extend(
            [
                ("Hash", options.hash.to_string()),
                ("Threads", options.threads.to_string()),
                ("MoveOverhead", options.move_overhead.to_string()),
                ("Minimal", options.minimal.to_string()),
            ]
            .into_iter()
            .filter(|(name, _)| declared(name))
            .map(|(name, value)| (name.to_owned(), value)),
        );
        engine.configure(&settings).await.map_err(error::internal)?;
        engine.new_game().await.map_err(error::internal)?;
        Ok(Self {
            engine,
            flight: None,
            bus,
            every: std::time::Duration::from_millis(200),
            unclocked_ms: UNCLOCKED_MS,
            level: None,
            fixed_nodes: binary.engine.nodes,
            _cores: None,
        })
    }

    /// Plays at `level`: `nodes` per move (the level's own when `None`; full strength then plays on the game's
    /// clock), with the level's MultiPV, choosing among the lines with an RNG seeded by `seed`.
    pub async fn at_level(mut self, level: Level, nodes: Option<u64>, seed: u64) -> Result<Self, Error> {
        if level.multipv > 1 && self.engine.options.iter().any(|o| o.name.eq_ignore_ascii_case("MultiPV")) {
            let settings = [("MultiPV".to_owned(), level.multipv.to_string())];
            self.engine.configure(&settings).await.map_err(error::internal)?;
        }
        let nodes = nodes.or(level.nodes);
        self.level = Some(AtLevel { level, nodes, rng: SplitMix64::new(seed) });
        Ok(self)
    }

    /// Reports at most once per `every` and searches unclocked games for `unclocked_ms` (tests shorten both).
    pub fn pace(mut self, every: std::time::Duration, unclocked_ms: u64) -> Self {
        self.every = every;
        self.unclocked_ms = unclocked_ms;
        self
    }

    fn limits(&self, turn: &Turn<'_>) -> Limits {
        if let Some(nodes) = self.level.as_ref().and_then(|l| l.nodes).or(self.fixed_nodes) {
            return Limits::nodes(nodes);
        }
        match turn.time {
            TimeControl::Clock { increment_ms, initial_ms } => {
                let clocks = turn.clocks.unwrap_or(chess_core::SidePair::both(initial_ms));
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
        // any other is ended first, its bestmove read and dropped.
        let same = self.flight.as_ref().is_some_and(|f| f.ply == turn.ply && f.fen == turn.fen);
        if !(same && self.engine.searching()) {
            if self.engine.searching() {
                self.engine.finish(std::time::Duration::from_secs(5)).await.map_err(error::internal)?;
            }
            let position = uci::Position { fen: turn.start_fen.map(str::to_owned), moves: turn.moves.to_vec() };
            let limits = self.limits(turn);
            self.engine.go(&position, &limits).await.map_err(error::internal)?;
            self.flight = Some(Flight::new(turn.ply, turn.fen));
        }
        loop {
            let event = self.engine.next().await.map_err(error::internal)?;
            let Some(flight) = self.flight.as_mut() else { return Err(Error::internal("no search in flight")) };
            if let SearchEvent::Info(info) = &event
                && let Some(first) = info.pv.first()
            {
                flight.lines.insert(info.multipv.max(1), (info.depth, first.clone(), info.score));
            }
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
                    let flight = self.flight.take().unwrap_or_else(|| Flight::new(turn.ply, ""));
                    let last = flight.last;
                    if flight.unsent
                        && let Some(info) = &last
                    {
                        self.report(turn.game, turn.ply, turn.side, info);
                    }
                    let Some(uci) = best.best else {
                        return Err(Error::internal("the engine found no move"));
                    };
                    if let Some(chosen) = self.choose(&flight.lines, &uci, turn.side) {
                        return Ok(chosen);
                    }
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

    /// At a level with more than one line: the move `choose_move` picks among the lines, with its own score and depth.
    /// `None` plays the engine's best move as usual.
    fn choose(&mut self, lines: &BTreeMap<u32, (u32, String, uci::Score)>, best: &str, side: Side) -> Option<SeatMove> {
        let at = self.level.as_mut()?;
        if at.level.multipv <= 1 || lines.len() < 2 {
            return None;
        }
        let candidates: Vec<(String, Score)> =
            lines.values().map(|(_, mv, score)| (mv.clone(), own_score(*score))).collect();
        let u = at.rng.next_f64();
        let uci = levels::choose_move(&candidates, at.level.temperature, u).unwrap_or_else(|| best.to_owned());
        let (depth, _, score) = lines.values().find(|(_, mv, _)| *mv == uci)?;
        Some(SeatMove { uci, eval: Some(white_score(*score, side)), depth: Some(*depth), nodes: None })
    }

    /// Ends the engine.
    pub async fn quit(self) {
        self.engine.quit().await;
    }
}

/// A small, fast, seedable RNG (SplitMix64): a level's choices are reproducible from the seed.
#[derive(Clone, Debug)]
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform below `n` (at least 1).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

/// An engine's score as `core` writes it, still from the side to move.
fn own_score(score: uci::Score) -> Score {
    match score {
        uci::Score::Cp(cp) => Score::Cp(cp),
        uci::Score::Mate(m) => Score::Mate(m),
    }
}

/// An engine's score (from the side to move) as White's.
fn white_score(score: uci::Score, side: Side) -> Score {
    own_score(score).from_side(side)
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
