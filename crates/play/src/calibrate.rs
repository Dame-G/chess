//! Calibration: the levels play each other in the background (`WorkKind::Calibrate`, priority Background), and the
//! ladder is refitted after each match.
//!
//! The work is queued by [`Levels::prepare_calibration`] with its matches resolved (planned by `core::levels::plan`
//! when the request names none). Each game is played here, outside the live games: two engine seats at their levels
//! with fixed nodes (`Level::calibration_nodes`, so full strength plays 1,000,000), levels 1 to 6 choosing among their
//! MultiPV lines with `core::levels::choose_move`, from four random plies, colours alternating. As many games run at once as the ledger grants cores
//! for, two per game. Games are stored as finished local games, with their entrants in `level_games` marked
//! `calibration`; they are not announced one by one, only the refit is (`levels.updated`).
//!
//! It checkpoints after each batch of games and stops at a safe point between batches.
//!
//! [`Levels::prepare_calibration`]: crate::Levels::prepare_calibration

use std::time::Instant;

use async_trait::async_trait;
use dameless_core::{
    Error, GameId, SidePair, TimeControl,
    chess::Position,
    game::{GameEnd, GameStatus, Outcome, Seat, Source, Turn},
    levels::{Entrant, Level, PlannedMatch},
    referee::{Referee, Setup, Sitter},
    work::{Progress, WorkKind},
};
use dameless_hub::{Executor, WorkContext};
use dameless_store::{NewGame, now};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use dameless_work::EngineBinary;

use crate::{
    engine_seat::{EngineOptions, EngineSeat, SplitMix64},
    games::{Games, random_u64, seat_seed},
    levels::CalibrateSpec,
    seat::SeatController,
};

/// Plies played at random from the start, so games between deterministic levels differ.
const RANDOM_PLIES: usize = 4;

/// Where a calibration stands: games played per match, and failures.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateCheckpoint {
    pub done: Vec<u32>,
    pub failed: u32,
}

/// What a finished calibration did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalibrateResult {
    pub games: u32,
    pub failed: u32,
    pub matches: Vec<PlannedMatch>,
}

/// The `calibrate` work's executor. Register it with the scheduler.
#[derive(Clone)]
pub struct Calibrate {
    games: Games,
}

impl Calibrate {
    pub fn new(games: Games) -> Self {
        Self { games }
    }
}

/// One calibration game to play.
struct Pairing {
    white: Level,
    black: Level,
    seed: u64,
    start_fen: Option<String>,
}

/// The start of a game: a few random plies from the start.
fn opening(rng: &mut SplitMix64) -> Result<Option<String>, Error> {
    let mut pos = Position::start();
    for _ in 0..RANDOM_PLIES {
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        pos.play(&moves[rng.below(moves.len())])?;
    }
    Ok(Some(Position::from_fen(&pos.fen())?.fen()))
}

/// Plays one game to its end and stores it. On `cancel` the game is stored as aborted and the error is returned.
async fn play(games: &Games, binary: &EngineBinary, p: Pairing, cancel: &CancellationToken) -> Result<GameId, Error> {
    let hub = games.hub();
    let options = EngineOptions::default();
    let mut seats: Vec<Box<dyn SeatController>> = Vec::with_capacity(2);
    let mut names: Vec<Seat> = Vec::with_capacity(2);
    for (index, level) in [&p.white, &p.black].into_iter().enumerate() {
        let engine = EngineSeat::launch(binary, options, hub.bus.clone())
            .await?
            .pace(std::time::Duration::from_secs(3600), games.timing().unclocked_movetime_ms)
            .at_level(level.clone(), Some(level.calibration_nodes()), seat_seed(p.seed, index))
            .await?;
        seats.push(Box::new(engine));
        let (name, rating) = games.levels().seat_name(level, &binary.engine.name).await;
        names.push(Seat {
            engine: Some(binary.engine.label().to_owned()),
            rating: Some(rating),
            level: Some(level.id),
            ..Seat::new(binary.engine.player(), name)
        });
    }
    let time = TimeControl::Nodes { nodes: p.white.calibration_nodes().max(p.black.calibration_nodes()) };
    let mut referee = Referee::new(Setup {
        start_fen: p.start_fen.clone(),
        time,
        seats: SidePair::new(Sitter::Engine, Sitter::Engine),
        adjudicate: true,
    })?;
    let black = names.pop().expect("two seats");
    let white = names.pop().expect("two seats");
    let mut new = NewGame::starting(white, black, Source::Local, time, now());
    new.status = GameStatus::Live;
    new.start_fen = p.start_fen.clone();
    let id = hub.store.insert_game(new).await?;
    hub.store.record_level_game(id, Entrant::Level(p.white.id), Entrant::Level(p.black.id), true).await?;

    let end = loop {
        if let Some(end) = referee.ending() {
            break end;
        }
        let side = referee.turn();
        let fen = referee.position().fen();
        let moves = referee.uci_moves();
        let turn = Turn {
            game: id,
            side,
            ply: referee.next_ply(),
            start_fen: p.start_fen.as_deref(),
            moves: &moves,
            fen: &fen,
            time,
            clocks: None,
        };
        let began = Instant::now();
        let answer = tokio::select! {
            _ = cancel.cancelled() => None,
            r = seats[side.index()].next_move(turn) => Some(r),
        };
        match answer {
            None => break GameEnd::aborted(),
            Some(Ok(mv)) => {
                if let Err(e) = referee.apply(&mv, began.elapsed().as_millis() as u64)
                    && referee.ending().is_none()
                {
                    tracing::warn!("calibration game {id}: {e}");
                    break referee.engine_failed(side);
                }
            }
            Some(Err(e)) => {
                tracing::warn!("calibration game {id}: the engine playing {} stopped: {e}", side.name());
                break referee.engine_failed(side);
            }
        }
    };
    hub.store.replace_moves(id, referee.moves().to_vec()).await?;
    hub.store.finish_game(id, end, now()).await?;
    if cancel.is_cancelled() {
        return Err(Error::conflict("stopped"));
    }
    Ok(id)
}

/// A result as people write it.
fn result(outcome: Option<Outcome>) -> &'static str {
    match outcome {
        Some(Outcome::WhiteWins) => "1-0",
        Some(Outcome::BlackWins) => "0-1",
        Some(Outcome::Draw) => "½-½",
        None => "no result",
    }
}

#[async_trait]
impl Executor for Calibrate {
    fn kind(&self) -> WorkKind {
        WorkKind::Calibrate
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: CalibrateSpec = cx.spec()?;
        let levels = self.games.levels();
        let mut state: CalibrateCheckpoint = cx.checkpoint().unwrap_or_default();
        state.done.resize(spec.matches.len(), 0);
        let total: u32 = spec.matches.iter().map(|m| m.games).sum();
        let binary = self.games.engines().ready(Some(&spec.engine)).await?;
        let mut rng = SplitMix64::new(random_u64());
        if state.done.iter().all(|d| *d == 0) {
            cx.log(format!(
                "Calibrating {}: {} with {} games, from random openings.",
                binary.engine.label(),
                spec.matches.iter().map(|m| format!("levels {} and {}", m.a, m.b)).collect::<Vec<_>>().join(", "),
                total,
            ));
        }
        let started = Instant::now();
        let first: u32 = state.done.iter().sum();
        for (i, m) in spec.matches.iter().enumerate() {
            let (a, b) = (levels.level(m.a)?.clone(), levels.level(m.b)?.clone());
            let mut played_any = false;
            while state.done[i] < m.games {
                if !cx.safe_point(None).await {
                    cx.save_checkpoint(&state).await?;
                    return Err(cx.stopped());
                }
                let total_cores = cx.hub.ledger.total().max(2);
                let Some(grant) = cx.cores(2, total_cores).await else {
                    cx.save_checkpoint(&state).await?;
                    return Err(cx.stopped());
                };
                let at_once = (grant.cores() / 2).max(1).min(m.games - state.done[i]);
                let mut pairings = Vec::with_capacity(at_once as usize);
                for k in 0..at_once {
                    let n = state.done[i] + k;
                    let (white, black) =
                        if n.is_multiple_of(2) { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
                    pairings.push(Pairing { white, black, seed: rng.next_u64(), start_fen: opening(&mut rng)? });
                }
                let results =
                    futures::future::join_all(pairings.into_iter().map(|p| play(&self.games, &binary, p, &cx.cancel)))
                        .await;
                drop(grant);
                if cx.cancel.is_cancelled() {
                    cx.save_checkpoint(&state).await?;
                    return Err(cx.stopped());
                }
                let mut ok = 0;
                for r in results {
                    match r {
                        Ok(id) => {
                            ok += 1;
                            if let Ok(Some(g)) = cx.hub.store.game_summary(id).await {
                                cx.log(format!(
                                    "Game {id}: {} – {}, {} in {} plies.",
                                    g.white.name,
                                    g.black.name,
                                    result(g.outcome),
                                    g.plies
                                ));
                            }
                        }
                        Err(e) => {
                            state.failed += 1;
                            cx.log(format!("A game failed: {}", e.message()));
                        }
                    }
                }
                // Every game of a batch failing means the engine cannot play: stop rather than loop.
                if ok == 0 {
                    cx.save_checkpoint(&state).await?;
                    return Err(Error::internal("every calibration game of a batch failed"));
                }
                state.done[i] += at_once;
                played_any = true;
                cx.save_checkpoint(&state).await?;
                let done: u32 = state.done.iter().sum();
                cx.progress_now(Progress {
                    done: u64::from(done),
                    total: Some(u64::from(total)),
                    unit: "games".into(),
                    eta_seconds: Progress::eta(
                        u64::from(done - first),
                        u64::from(total - first),
                        started.elapsed().as_secs(),
                    ),
                    text: Some(format!("{done} of {total} games, levels {} and {}", m.a, m.b)),
                })
                .await?;
            }
            if played_any {
                let fit = levels.refit().await?;
                let show = |id: u32| {
                    fit.ratings.iter().find(|r| r.entrant == Entrant::Level(id)).map_or_else(
                        || "-".to_owned(),
                        |r| format!("{:.0} ± {:.0}", r.estimate.elo, r.estimate.error()),
                    )
                };
                cx.log(format!("Refitted: level {} at {}, level {} at {}.", m.a, show(m.a), m.b, show(m.b)));
            }
        }
        let games: u32 = state.done.iter().sum();
        cx.log(format!("Done: {games} games, {} failed.", state.failed));
        let result = CalibrateResult { games, failed: state.failed, matches: spec.matches };
        serde_json::to_value(result).map_err(|e| Error::internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openings_are_random_plies() {
        let mut rng = SplitMix64::new(7);
        let fen = opening(&mut rng).unwrap().unwrap();
        let pos = Position::from_fen(&fen).unwrap();
        assert!(!pos.is_standard_start());
        assert_eq!(opening(&mut SplitMix64::new(7)).unwrap().unwrap(), fen, "the same seed, the same opening");
    }
}
