//! A local game's task: the Referee (`core::referee`) owns the rules and the clocks, this task owns time. Each turn it
//! asks the seat to move, measuring the think time on a monotonic clock, flags the side whose clock runs out (with
//! the tests' 250 ms grace), and carries out your actions (draw offers, takebacks, resigning) as they come.

use std::time::Instant;

use dameless_core::{
    Side,
    game::{SeatAction, Turn},
    referee::{Acted, Referee},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    games::{Command, Games},
    record::Record,
    seat::{SeatController, SeatKind},
};

/// What a turn came to.
enum Turned {
    Moved(Result<dameless_core::game::SeatMove, dameless_core::Error>, u64),
    Flagged,
    Stopped,
    Command(Command),
}

/// Plays the game to its end, records the end, and lets the game go.
pub(crate) struct LocalGame {
    pub games: Games,
    pub record: Record,
    pub referee: Referee,
    pub start_fen: Option<String>,
    pub time: dameless_core::TimeControl,
    pub seats: [Box<dyn SeatController>; 2],
    pub commands: mpsc::UnboundedReceiver<Command>,
    pub cancel: CancellationToken,
    /// The side you play, if you play.
    pub you: Option<Side>,
    /// A game at a level: the levels are refitted when it ends.
    pub refit: bool,
}

pub(crate) async fn run(game: LocalGame) {
    let LocalGame { games, record, mut referee, start_fen, time, mut seats, mut commands, cancel, you, refit } = game;
    let clocked = referee.clock().is_some();
    let actions = |r: &Referee| you.map(|s| r.actions_for(s)).unwrap_or_default();
    record.set(referee.offers(), actions(&referee), referee.clocks(), clocked);

    let end = loop {
        if let Some(end) = referee.ending() {
            break end;
        }
        let side = referee.turn();
        let fen = referee.position().fen();
        let moves = referee.uci_moves();
        let turn = Turn {
            game: record.id,
            side,
            ply: referee.next_ply(),
            start_fen: start_fen.as_deref(),
            moves: &moves,
            fen: &fen,
            time,
            clocks: referee.clocks(),
        };
        let deadline = referee.allowance().map(|ms| tokio::time::Instant::now() + std::time::Duration::from_millis(ms));
        let began = Instant::now();
        let turned = {
            let seat = &mut seats[side.index()];
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Turned::Stopped,
                _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => Turned::Flagged,
                Some(cmd) = commands.recv() => Turned::Command(cmd),
                r = seat.next_move(turn) => Turned::Moved(r, began.elapsed().as_millis() as u64),
            }
        };
        match turned {
            Turned::Stopped => break referee.stop(you),
            Turned::Flagged => break referee.flag(),
            Turned::Command(Command::Act { action, ack }) => {
                let Some(side) = you else {
                    let _ = ack.send(Err(dameless_core::Error::bad_request("you do not play in this game")));
                    continue;
                };
                match referee.act(side, action) {
                    Ok(Acted::TakenBack(plies)) => {
                        record.undone(plies, referee.turn()).await;
                        let _ = ack.send(Ok(()));
                    }
                    Ok(Acted::Ended(_)) => {
                        let _ = ack.send(Ok(()));
                    }
                    Ok(Acted::Offered | Acted::Declined | Acted::DrawDeclined) => {
                        let _ = ack.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = ack.send(Err(e));
                    }
                }
                // The other seat hears of what it is offered (an engine's answer is the Referee's).
                if matches!(action, SeatAction::OfferDraw | SeatAction::Takeback) {
                    let _ = seats[side.other().index()].action(action).await;
                }
            }
            Turned::Moved(Ok(mv), think) => match referee.apply(&mv, think) {
                Ok(played) => {
                    let fen = referee.position().fen();
                    record.moved(played, fen, referee.turn(), referee.clocks(), clocked).await;
                }
                Err(e) => {
                    if referee.ending().is_none() {
                        tracing::warn!("game {}: {e}", record.id);
                    }
                }
            },
            Turned::Moved(Err(e), _) => {
                if seats[side.index()].kind() == SeatKind::Engine {
                    tracing::warn!("game {}: the engine playing {} stopped: {e}", record.id, side.name());
                    break referee.engine_failed(side);
                }
                break referee.stop(you);
            }
        }
        record.set(referee.offers(), actions(&referee), referee.clocks(), clocked && referee.ending().is_none());
    };
    record.finish(end).await;
    games.forget(record.id);
    // The engines stop with their seats.
    drop(seats);
    if refit {
        games.levels().refit_quietly().await;
    }
}
