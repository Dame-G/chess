//! Levels: games at a level choose among the engine's lines from a seeded RNG, are recorded for the fit, and the
//! ladder is calibrated by games between levels.

mod common;

use std::{sync::Arc, time::Duration};

use common::*;
use dameless_core::{
    PlayerKey, Side, TimeControl,
    events::{EventKind, LevelsUpdated},
    game::{GameStatus, SeatAction},
    levels::{Entrant, Level},
    work::{Priority, WorkKind},
};
use dameless_hub::{Config, Executor, Hub, Next, TopicFilter, WorkContext};
use dameless_play::{Calibrate, CalibrateRequest, CalibrateResult, Games, Levels, NewGameRequest, SeatRequest, Timing};

fn request(white: &str, black: &str, time: TimeControl, seed: Option<u64>) -> NewGameRequest {
    NewGameRequest {
        white: white.parse().unwrap(),
        black: black.parse().unwrap(),
        time,
        from: None,
        hash: None,
        threads: None,
        adjudicate: false,
        seed,
    }
}

/// Two small levels: a choosy one and one that plays the engine's move.
fn ladder() -> Vec<Level> {
    vec![
        Level { id: 1, name: "Level 1".into(), nodes: Some(100), multipv: 4, temperature: 15.0, nominal: 800 },
        Level { id: 2, name: "Level 2".into(), nodes: Some(200), multipv: 1, temperature: 0.0, nominal: 1200 },
    ]
}

struct LevelSetup {
    hub: Arc<Hub>,
    games: Games,
    _dir: tempfile::TempDir,
}

fn level_setup() -> LevelSetup {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), 4);
    let hub = Hub::open(config).unwrap();
    let levels = Levels::with_ladder(hub.clone(), ladder());
    let games = Games::with_levels(hub.clone(), engines(), Timing::fast(), levels);
    LevelSetup { hub, games, _dir: dir }
}

fn player_events(hub: &Hub) -> dameless_hub::Subscription {
    hub.bus.subscribe(0, TopicFilter::of(["engines", "work"]))
}

fn levels_updated(sub: &mut dameless_hub::Subscription) -> Vec<LevelsUpdated> {
    let mut out = Vec::new();
    for e in std::mem::take(&mut sub.backlog) {
        if e.kind == EventKind::LevelsUpdated {
            out.push(serde_json::from_value(e.data).unwrap());
        }
    }
    while let Some(next) = sub.try_recv() {
        if let Next::Event(e) = next
            && e.kind == EventKind::LevelsUpdated
        {
            out.push(serde_json::from_value(e.data).unwrap());
        }
    }
    out
}

#[test]
fn level_seats_read_and_write() {
    assert_eq!("fake@5".parse::<SeatRequest>().unwrap(), SeatRequest::at_level("fake", 5));
    let s: SeatRequest = "engine:stockfish@3".parse().unwrap();
    assert_eq!((s.engine_id(), s.level()), (Some("stockfish"), Some(3)));
    assert_eq!(s.to_string(), "stockfish@3");
    assert!("fake@".parse::<SeatRequest>().is_err());
    assert!("you@3".parse::<SeatRequest>().is_err());
    let req: NewGameRequest = serde_json::from_value(serde_json::json!({
        "white": "you", "black": { "player": "fake", "level": 5 }, "time": { "type": "movetime", "ms": 100 }
    }))
    .unwrap();
    assert_eq!(req.black, SeatRequest::at_level("fake", 5));
    assert_eq!(req.seed, None);
    assert_eq!(serde_json::to_value(&req.black).unwrap(), "fake@5");
}

#[tokio::test]
async fn a_level_chooses_among_its_lines_the_same_way_for_the_same_seed() {
    let s = level_setup();
    let play = |seed: u64| {
        let games = s.games.clone();
        async move {
            let g =
                games.start(request("fake@1", "fake@1", TimeControl::Nodes { nodes: 1000 }, Some(seed))).await.unwrap();
            games.wait_finished(g.summary.id, Duration::from_secs(60)).await.unwrap()
        }
    };
    let first = play(42).await;
    assert_eq!(first.summary.status, GameStatus::Finished);
    assert_eq!(first.summary.white.name, "Fake 800");
    assert_eq!(first.summary.white.rating, Some(800));
    let again = play(42).await;
    let moves = |g: &dameless_core::game::Game| g.moves.iter().map(|m| m.uci.clone()).collect::<Vec<_>>();
    assert_eq!(moves(&first), moves(&again), "the same seed plays the same game");
    let other = play(43).await;
    assert_ne!(moves(&first), moves(&other), "another seed, another game");

    // The engine alone always plays its first line; at level 1 the softmax often picks another.
    let plain = s.games.start(request("fake", "fake", TimeControl::Nodes { nodes: 1000 }, None)).await.unwrap();
    let plain = s.games.wait_finished(plain.summary.id, Duration::from_secs(60)).await.unwrap();
    assert_ne!(moves(&first)[..8], moves(&plain)[..8]);
    // A level's move carries its own line's score: some are worse than the best line's 20 cp.
    assert!(first.moves.iter().any(|m| m.eval.is_some_and(|e| e.centipawns().abs() != 20)));

    // Both seats were levels: the games count for the fit.
    let seats = s.hub.store.level_seats(first.summary.id).await.unwrap();
    assert_eq!(seats, Some((Entrant::Level(1), Entrant::Level(1))));
    assert_eq!(s.hub.store.level_seats(plain.summary.id).await.unwrap(), None);
}

#[tokio::test]
async fn your_games_against_a_level_refit_the_ladder() {
    let s = level_setup();
    s.games.levels().set_anchor(Arc::new(|| Some((1000, None))));
    let mut events = player_events(&s.hub);
    let unknown = s.games.start(request("you", "fake@9", TimeControl::Movetime { ms: 50 }, None)).await;
    assert_eq!(unknown.unwrap_err().status(), 400);

    let g = s.games.start(request("you", "fake@2", TimeControl::Movetime { ms: 50 }, None)).await.unwrap();
    let id = g.summary.id;
    assert_eq!(g.summary.black.name, "Fake 1200");
    assert_eq!(s.hub.store.level_seats(id).await.unwrap(), Some((Entrant::You, Entrant::Level(2))));
    s.games.act(id, SeatAction::Move, Some("e4".into())).await.unwrap();
    until_async!("the level's reply", s.games.game(id).await.unwrap().moves.len() == 2);
    s.games.act(id, SeatAction::Resign, None).await.unwrap();
    s.games.wait_finished(id, Duration::from_secs(5)).await.unwrap();

    until_async!("the refit", s.hub.store.level_ratings().await.unwrap().1.is_some());
    let (ratings, _) = s.hub.store.level_ratings().await.unwrap();
    let you = ratings.iter().find(|r| r.entrant == Entrant::You).unwrap();
    assert_eq!(you.games, 1);
    let two = ratings.iter().find(|r| r.entrant == Entrant::Level(2)).unwrap();
    assert!(two.estimate.elo > 1200.0, "level 2 won");
    // The refit is stored before its event is published: wait for the event too.
    let mut updated = Vec::new();
    until_async!("the levels event", {
        updated.extend(levels_updated(&mut events));
        !updated.is_empty()
    });
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].ratings, ratings);

    let overview = s.games.levels().overview().await.unwrap();
    assert_eq!(overview.levels.len(), 2);
    assert_eq!(overview.levels[1].games, 1);
    assert!(overview.levels[0].rating.is_none(), "level 1 has no games yet");
    assert_eq!(overview.you.map(|y| y.games), Some(1));
    assert_eq!(overview.suggested, Some(1), "your rating is nearer level 1's");
    assert!(overview.updated_at.is_some());
    let _ = Side::White;
}

#[tokio::test]
async fn calibration_plays_levels_against_each_other_and_fits_them() {
    let s = level_setup();
    let mut events = player_events(&s.hub);
    let levels = s.games.levels().clone();
    let plan = levels.plan(3).await.unwrap();
    assert_eq!(plan.len(), 1, "two levels make one pair");
    let engines = engines();
    let bad = CalibrateRequest { games_per_pair: Some(0), ..Default::default() };
    assert_eq!(levels.prepare_calibration(engines.as_ref(), bad).await.unwrap_err().status(), 400);
    let new = levels
        .prepare_calibration(engines.as_ref(), CalibrateRequest { games_per_pair: Some(3), ..Default::default() })
        .await
        .unwrap();
    assert_eq!((new.kind, new.priority), (WorkKind::Calibrate, Priority::Background));
    let spec = new.spec.clone();
    let work = s.hub.store.insert_work(new).await.unwrap();
    let (cx, _cancel) = WorkContext::detached(s.hub.clone(), work.id, WorkKind::Calibrate, spec);
    let calibrate = Calibrate::new(s.games.clone());
    assert_eq!(calibrate.kind(), WorkKind::Calibrate);
    let result: CalibrateResult = serde_json::from_value(calibrate.run(cx).await.unwrap()).unwrap();
    assert_eq!((result.games, result.failed), (3, 0));

    let games = s.hub.store.level_games().await.unwrap();
    assert_eq!(games.len(), 3);
    assert_eq!(games.iter().filter(|g| g.white == Entrant::Level(1)).count(), 2, "colours alternate");
    let (ratings, updated_at) = s.hub.store.level_ratings().await.unwrap();
    assert!(updated_at.is_some());
    assert!(ratings.iter().all(|r| r.games == 3));
    assert_eq!(levels_updated(&mut events).len(), 1, "one refit after the match");
    // The games are stored, finished, with the levels at their seats.
    let any = s.hub.store.games(Default::default()).await.unwrap();
    assert_eq!(any.len(), 3);
    assert!(any.iter().all(|g| g.status == GameStatus::Finished && g.white.player == PlayerKey::Engine("fake".into())));
    assert!(any.iter().all(|g| g.white.level.is_some() && g.white.name.starts_with("Fake ")));
    // Every engine was let go, and none reserved cores outside the grant.
    assert_eq!(s.hub.ledger.snapshot().reserved, 0);
}
