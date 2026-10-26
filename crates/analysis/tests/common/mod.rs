//! What the integration tests share: a hub in a temporary directory, a champion version, a fake [`Binaries`] that
//! hands out the fake UCI engine (and counts how often it is asked), and finished games to review.

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
use dameless_analysis::{Binaries, Eval, VersionRef};
use dameless_core::{
    BuildId, Error, GameId, PlayerKey, TimeControl, Timestamp,
    chess::Position,
    game::{Ending, GameStatus, Move, Outcome, Seat, Source},
    version::{Build, BuildStatus, Line, Version},
};
use dameless_hub::{Config, Hub};
use dameless_store::{NewGame, NewVersion, Store};

/// Hands out the fake engine for any version the store knows.
pub struct FakeBinaries {
    store: Store,
    path: PathBuf,
    /// How many binaries were asked for: a search that the cache answers asks for none.
    pub calls: AtomicUsize,
}

impl FakeBinaries {
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Binaries for FakeBinaries {
    async fn resolve(&self, version: &VersionRef) -> Result<Version, Error> {
        let found = match version {
            VersionRef::Champion => self.store.champion().await?,
            VersionRef::Id(id) => self.store.version(*id).await?,
            VersionRef::Name(name) => self.store.version_by_name(name).await?,
        };
        found.ok_or_else(|| Error::not_found(format!("No version {version:?}")))
    }

    async fn binary(&self, version: &VersionRef, features: &[&str]) -> Result<(Build, PathBuf), Error> {
        let v = self.resolve(version).await?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let build = Build {
            id: BuildId(1),
            version: v.id,
            features: features.iter().map(|f| (*f).to_owned()).collect(),
            bench: None,
            nps: None,
            path: self.path.display().to_string(),
            status: BuildStatus::Ready,
        };
        Ok((build, self.path.clone()))
    }
}

pub struct Harness {
    pub hub: Arc<Hub>,
    pub binaries: Arc<FakeBinaries>,
    pub eval: Eval,
    pub champion: Version,
    _dir: tempfile::TempDir,
}

pub async fn harness(cores: u32) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let config =
        Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), dir.path().join("reckless"), cores);
    let hub = Hub::open(config).unwrap();
    let champion =
        hub.store.upsert_version(NewVersion::new("v1", "c0ffee", Line::Champion { number: 1 })).await.unwrap();
    let binaries = Arc::new(FakeBinaries {
        store: hub.store.clone(),
        path: PathBuf::from(env!("CARGO_BIN_EXE_fake-eval")),
        calls: AtomicUsize::new(0),
    });
    let eval = Eval::new(hub.clone(), binaries.clone());
    Harness { hub, binaries, eval, champion, _dir: dir }
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

/// The scholar's mate: You (White) mate Dameless in four.
pub const SCHOLARS_MATE: [&str; 7] = ["e4", "e5", "Bc4", "Nc6", "Qh5", "Nf6", "Qxf7#"];

/// A quiet game Dameless (White) and You (Black) drew.
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

/// Waits until `check` holds, for at most five seconds.
pub async fn eventually(mut check: impl AsyncFnMut() -> bool) {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    while !check().await {
        assert!(tokio::time::Instant::now() < until, "timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
