//! What the integration tests share: a hub in a temporary directory, a fake engines list that hands out the fake UCI
//! engine as the analysis engine (and counts how often it is asked), and finished games to review.

#![allow(dead_code)]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chess_analysis::Eval;
use chess_core::{
    Error, GameId, PlayerKey, TimeControl, Timestamp,
    chess::Position,
    game::{Ending, GameStatus, Move, Outcome, Seat, Source},
};
use chess_hub::{Config, Hub};
use chess_store::NewGame;
use chess_work::{EngineBinary, EngineSource, FixedEngines};

/// The fake engine as `fake` (the analysis engine), counting how often an engine is asked for.
pub struct FakeEngines {
    engines: FixedEngines,
    pub calls: AtomicUsize,
}

impl FakeEngines {
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl EngineSource for FakeEngines {
    async fn ready(&self, id: Option<&str>) -> Result<EngineBinary, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.engines.ready(id).await
    }
}

pub struct Harness {
    pub hub: Arc<Hub>,
    pub engines: Arc<FakeEngines>,
    pub eval: Eval,
    /// The analysis engine.
    pub engine: EngineBinary,
    _dir: tempfile::TempDir,
}

pub async fn harness(cores: u32) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), cores);
    let hub = Hub::open(config).unwrap();
    let path = PathBuf::from(env!("CARGO_BIN_EXE_fake-eval"));
    let fixed = FixedEngines::new().with("fake", &path, &[]).with_engine("maia-1500", &path, &[], Some(1), Some(1500));
    let engines = Arc::new(FakeEngines { engines: fixed, calls: AtomicUsize::new(0) });
    let engine = engines.engines.ready(None).await.unwrap();
    let eval = Eval::new(hub.clone(), engines.clone());
    Harness { hub, engines, eval, engine, _dir: dir }
}

/// Moves (SAN) as a game records them.
pub fn moves(san: &[&str]) -> Vec<Move> {
    let mut pos = Position::start();
    san.iter()
        .map(|m| {
            let played = pos.play(m).unwrap();
            Move::new(played.uci, played.san)
        })
        .collect()
}

/// The engine every test game is played against.
pub fn fake() -> PlayerKey {
    PlayerKey::Engine("fake".into())
}

/// The scholar's mate: You (White) mate the engine in four.
pub const SCHOLARS_MATE: [&str; 7] = ["e4", "e5", "Bc4", "Nc6", "Qh5", "Nf6", "Qxf7#"];

/// A quiet game the engine (White) and You (Black) drew.
pub const QUIET_DRAW: [&str; 8] = ["d4", "d5", "Nf3", "Nf6", "e3", "e6", "Bd3", "Bd6"];

/// Stores a finished local game between these players.
pub async fn finished_game(
    hub: &Hub,
    white: PlayerKey,
    black: PlayerKey,
    san: &[&str],
    outcome: Outcome,
    day: i64,
) -> GameId {
    let seat = |p: PlayerKey| {
        let name = p.to_string();
        Seat { rating: Some(1500), ..Seat::new(p, name) }
    };
    let mut game = NewGame::starting(
        seat(white),
        seat(black),
        Source::Local,
        TimeControl::clock(180_000, 2_000),
        Timestamp::from_unix_ms(1_700_000_000_000 + day * 86_400_000),
    );
    game.status = GameStatus::Finished;
    game.outcome = Some(outcome);
    game.ending = Some(if outcome == Outcome::Draw { Ending::Agreement } else { Ending::Checkmate });
    game.moves = moves(san);
    game.ended_at = Some(Timestamp::from_unix_ms(1_700_000_600_000 + day * 86_400_000));
    hub.store.insert_game(game).await.unwrap()
}

/// Waits until `check` holds, for at most fifteen seconds (a loaded CI runner is slow).
pub async fn eventually(mut check: impl AsyncFnMut() -> bool) {
    let until = tokio::time::Instant::now() + Duration::from_secs(15);
    while !check().await {
        assert!(tokio::time::Instant::now() < until, "timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
