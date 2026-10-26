//! The record of a game being played: the stored row, kept up to date move by move, and the live part only memory
//! holds (the clocks as they run, offers, what you may do, the opponent's absence). Every change is announced on the
//! bus: `game.upserted` for the summary, `game.moved`, `game.undone` and `game.changed` for the detail.

use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use dameless_core::{
    Error, GameId, Side, SidePair,
    events::{GameChanged, GameMoved, GameUndone, OpponentGone, Payload},
    game::{Game, GameEnd, GameStatus, GameSummary, Move, Offers, SeatAction},
};
use dameless_hub::Hub;
use dameless_store::{GameUpdate, NewGame, now};
use tokio::sync::watch;

/// What memory holds of a game being played.
pub(crate) struct LiveState {
    /// The record as stored, with the clocks as of the last move or state, the offers and your actions.
    pub game: Game,
    /// The side to move.
    pub turn: Side,
    /// Since when the side to move's clock has been running, when it runs.
    pub running: Option<Instant>,
    /// The opponent left: the seconds Lichess gave until a claim, from when it said so.
    pub gone: Option<(u32, Instant)>,
}

impl LiveState {
    /// The game as a reader sees it now: the clock of the side to move run down, the wait for a claim counted down.
    pub fn view(&self) -> Game {
        let mut g = self.game.clone();
        if let (Some(since), Some(clocks)) = (self.running, g.clocks.as_mut()) {
            let spent = since.elapsed().as_millis() as u64;
            let left = clocks.get_mut(self.turn);
            *left = left.saturating_sub(spent);
        }
        g
    }

    pub fn opponent_gone(&self) -> Option<OpponentGone> {
        self.gone
            .map(|(secs, at)| OpponentGone { claim_in_seconds: secs.saturating_sub(at.elapsed().as_secs() as u32) })
    }
}

/// A game's live part, shared by its task and the readers.
pub(crate) struct Live {
    pub state: Mutex<LiveState>,
    /// Counts changes, so a caller can wait for its move to be on the board.
    pub version: watch::Sender<u64>,
}

impl Live {
    pub fn view(&self) -> Game {
        self.state.lock().unwrap().view()
    }

    pub fn plies(&self) -> usize {
        self.state.lock().unwrap().game.moves.len()
    }

    pub fn is_live(&self) -> bool {
        self.state.lock().unwrap().game.summary.is_live()
    }

    fn bump(&self) {
        self.version.send_modify(|v| *v += 1);
    }
}

/// The game's task's handle on its record.
#[derive(Clone)]
pub(crate) struct Record {
    hub: Arc<Hub>,
    pub id: GameId,
    pub live: Arc<Live>,
}

fn live(game: Game, turn: Side) -> Arc<Live> {
    Arc::new(Live {
        state: Mutex::new(LiveState { game, turn, running: None, gone: None }),
        version: watch::Sender::new(0),
    })
}

impl Record {
    /// Stores a new game (starting) and announces it.
    pub async fn create(hub: &Arc<Hub>, new: NewGame, turn: Side) -> Result<Self, Error> {
        let id = hub.store.insert_game(new.clone()).await?;
        let game = hub.store.game(id).await?.ok_or_else(|| Error::internal("the game just stored is gone"))?;
        let game = Game { clocks: initial_clocks(&game), ..game };
        let record = Self { hub: hub.clone(), id, live: live(game, turn) };
        record.upserted();
        Ok(record)
    }

    /// Carries on a stored game (a Lichess game the app comes back to), with the moves it still agrees with and the
    /// seats as they are now.
    pub async fn resume(
        hub: &Arc<Hub>,
        id: GameId,
        new: &NewGame,
        moves: Vec<Move>,
        turn: Side,
    ) -> Result<Self, Error> {
        hub.store.replace_moves(id, moves).await?;
        let update = GameUpdate {
            status: Some(GameStatus::Starting),
            white: Some(new.white.clone()),
            black: Some(new.black.clone()),
            url: new.url.clone(),
            ..GameUpdate::default()
        };
        hub.store.update_game(id, update).await?;
        let game = hub.store.game(id).await?.ok_or_else(|| Error::not_found(format!("No game {id}")))?;
        let game = Game { clocks: initial_clocks(&game), ..game };
        let record = Self { hub: hub.clone(), id, live: live(game, turn) };
        record.upserted();
        Ok(record)
    }

    fn upserted(&self) {
        let summary = self.live.state.lock().unwrap().game.summary.clone();
        self.hub.bus.emit(Payload::GameUpserted(Box::new(summary)));
        self.live.bump();
    }

    /// The game is under way.
    pub async fn started(&self) {
        match self
            .hub
            .store
            .update_game(self.id, GameUpdate { status: Some(GameStatus::Live), ..Default::default() })
            .await
        {
            Ok(summary) => self.live.state.lock().unwrap().game.summary = summary,
            Err(e) => tracing::warn!("game {}: could not store its start: {e}", self.id),
        }
        self.upserted();
    }

    /// A move was played: stored, announced, and the live part brought along.
    pub async fn moved(&self, mv: Move, fen: String, turn: Side, clocks: Option<SidePair<u64>>, running: bool) {
        if let Err(e) = self.hub.store.append_move(self.id, mv.clone()).await {
            tracing::warn!("game {}: could not store a move: {e}", self.id);
        }
        let ply = {
            let mut s = self.live.state.lock().unwrap();
            s.game.moves.push(mv.clone());
            s.game.summary.plies = s.game.moves.len() as u32;
            if clocks.is_some() {
                s.game.clocks = clocks;
            }
            s.turn = turn;
            s.running = running.then(Instant::now);
            s.game.moves.len() as u32
        };
        self.hub.bus.emit(Payload::GameMoved(GameMoved { id: self.id, ply, mv, clocks, fen }));
        self.live.bump();
    }

    /// Moves were taken back so that `plies` are left.
    pub async fn undone(&self, plies: u32, turn: Side) {
        if let Err(e) = self.hub.store.truncate_moves(self.id, plies).await {
            tracing::warn!("game {}: could not store a takeback: {e}", self.id);
        }
        {
            let mut s = self.live.state.lock().unwrap();
            s.game.moves.truncate(plies as usize);
            s.game.summary.plies = plies;
            s.turn = turn;
        }
        self.hub.bus.emit(Payload::GameUndone(GameUndone { id: self.id, plies }));
        self.live.bump();
    }

    /// Changes the live part with `f` and announces it (`game.changed`).
    pub fn change(&self, f: impl FnOnce(&mut LiveState)) {
        let changed = {
            let mut s = self.live.state.lock().unwrap();
            f(&mut s);
            let view = s.view();
            GameChanged {
                id: self.id,
                offers: view.offers,
                actions: view.actions,
                clocks: view.clocks,
                opponent_gone: s.opponent_gone(),
            }
        };
        self.hub.bus.emit(Payload::GameChanged(changed));
        self.live.bump();
    }

    /// Sets the offers, your actions and the clocks, announcing it when any of them changed.
    pub fn set(&self, offers: Offers, actions: Vec<SeatAction>, clocks: Option<SidePair<u64>>, running: bool) {
        let same = {
            let s = self.live.state.lock().unwrap();
            s.game.offers == offers
                && s.game.actions == actions
                && (clocks.is_none() || s.game.clocks == clocks)
                && s.running.is_some() == running
        };
        if same {
            return;
        }
        self.change(|s| {
            s.game.offers = offers;
            s.game.actions = actions;
            if clocks.is_some() {
                s.game.clocks = clocks;
            }
            if s.running.is_some() != running {
                s.running = running.then(Instant::now);
            }
        });
    }

    /// The game ended: its result stored and announced, the live part at rest.
    pub async fn finish(&self, end: GameEnd) -> GameSummary {
        let stored = self.hub.store.finish_game(self.id, end, now()).await;
        let summary = {
            let mut s = self.live.state.lock().unwrap();
            if let Some(clocks) = s.view().clocks {
                s.game.clocks = Some(clocks);
            }
            s.running = None;
            s.gone = None;
            s.game.offers = Offers::default();
            s.game.actions = Vec::new();
            match stored {
                Ok(summary) => s.game.summary = summary,
                Err(e) => {
                    tracing::warn!("game {}: could not store its end: {e}", self.id);
                    s.game.summary.status = end.status();
                    s.game.summary.outcome = end.outcome;
                    s.game.summary.ending = Some(end.ending);
                }
            }
            s.game.summary.clone()
        };
        self.change(|_| {});
        self.upserted();
        summary
    }
}

/// Both clocks at the start, for a game with a clock: the initial time each.
fn initial_clocks(game: &Game) -> Option<SidePair<u64>> {
    match game.summary.time {
        dameless_core::TimeControl::Clock { initial_ms, .. } => Some(SidePair::both(initial_ms)),
        _ => None,
    }
}
