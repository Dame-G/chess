//! `uci` and `pool` against the fake engine in `tests/support/fake_uci.rs`, so they run without a real engine.

use std::{path::Path, time::Duration};

use chess_engine::{
    pool::{EngineSpec, Pool, Reset},
    uci::{Bound, Engine, Limits, Position, Score, SearchEvent},
};

fn fake() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_fake-uci"))
}

/// The value of an `info string <prefix> …` line the fake engine answers a `fake …` command with.
async fn ask(engine: &mut Engine, command: &str, prefix: &str) -> String {
    engine.send(command).await.unwrap();
    loop {
        let line = engine.read_line().await.unwrap();
        if let Some(rest) = line.strip_prefix(&format!("info string {prefix}")) {
            return rest.trim().to_owned();
        }
    }
}

#[tokio::test]
async fn handshakes_and_searches() {
    let mut engine = Engine::start(fake()).await.unwrap();
    assert_eq!((engine.name.as_str(), engine.author.as_str()), ("Fake 1.0", "The tests"));
    assert!(engine.options.iter().any(|o| o.name == "Clear Hash" && o.kind == "button"));
    engine.configure(&[("MultiPV".into(), "2".into())]).await.unwrap();

    let mut seen = 0;
    let result = engine.search(&Position::startpos(), &Limits::depth(4), |_| seen += 1).await.unwrap();
    assert_eq!(seen, 8);
    assert_eq!(result.best.best.as_deref(), Some("e2e4"));
    assert_eq!(result.best.ponder.as_deref(), Some("e7e5"));
    assert_eq!(result.lines.len(), 2);
    assert_eq!((result.lines[0].depth, result.lines[0].score, result.lines[0].bound), (4, Score::Cp(30), Bound::Exact));
    assert_eq!(result.lines[1].score, Score::Cp(10));
    assert_eq!(result.nodes(), 1000);

    let result = engine.search(&Position::startpos().with_moves(["e2e4"]), &Limits::nodes(500), |_| {}).await.unwrap();
    assert_eq!(result.best.best.as_deref(), Some("e7e5"));
    engine.quit().await;
}

#[tokio::test]
async fn stops_an_infinite_search() {
    let mut engine = Engine::start(fake()).await.unwrap();
    engine.ready().await.unwrap();
    engine.go(&Position::fen("8/8/8/8/8/8/8/K1k5 w - - 0 1"), &Limits::infinite()).await.unwrap();
    assert!(matches!(engine.next().await.unwrap(), SearchEvent::Info(_)));
    assert!(engine.searching());
    let best = engine.finish(Duration::from_secs(5)).await.unwrap().unwrap();
    assert_eq!(best.best.as_deref(), Some("e2e4"));
    assert!(!engine.searching());
    // A finished engine searches again cleanly.
    let r = engine.search(&Position::startpos(), &Limits::movetime(50), |_| {}).await.unwrap();
    assert!(r.best.best.is_some());
}

#[tokio::test]
async fn pools_warm_engines() {
    let pool = Pool::new(2);
    let spec = EngineSpec::new(fake()).option("Hash", 32).option("MultiPV", 2);

    let mut a = pool.checkout(&spec).await.unwrap();
    assert_eq!(ask(&mut a, "fake option Hash", "option Hash").await, "32");
    // Left searching: the pool stops it before it is handed out again.
    a.go(&Position::startpos(), &Limits::infinite()).await.unwrap();
    drop(a);
    assert_eq!(pool.stats(&spec).idle, 1);

    let mut a = pool.checkout(&spec).await.unwrap();
    assert!(!a.searching());
    assert_eq!(ask(&mut a, "fake newgames", "newgames").await, "1", "a reused engine gets ucinewgame");
    let r = a.search(&Position::startpos(), &Limits::depth(2), |_| {}).await.unwrap();
    assert_eq!(r.lines.len(), 2);
    drop(a);

    let mut kept = pool.checkout_with(&spec, Reset::Keep).await.unwrap();
    assert_eq!(ask(&mut kept, "fake newgames", "newgames").await, "1", "kept state: no ucinewgame");
    drop(kept);
    assert_eq!(pool.stats(&spec).idle, 1);
}

#[tokio::test]
async fn waits_at_the_limit_and_resizes() {
    let pool = Pool::new(1);
    let spec = EngineSpec::new(fake());
    let first = pool.checkout(&spec).await.unwrap();
    // The limit is one: a second checkout waits until the first comes back.
    let waiting = tokio::time::timeout(Duration::from_millis(300), pool.checkout(&spec)).await;
    assert!(waiting.is_err());

    pool.resize(&spec, 2);
    let second = tokio::time::timeout(Duration::from_secs(5), pool.checkout(&spec)).await.unwrap().unwrap();
    assert_eq!(pool.stats(&spec).checked_out, 2);

    // Shrinking kills engines over the limit as they come back.
    pool.resize(&spec, 1);
    drop(first);
    drop(second);
    assert_eq!(pool.stats(&spec), chess_engine::pool::PoolStats { limit: 1, checked_out: 0, idle: 1 });
    pool.kill_idle();
    assert_eq!(pool.stats(&spec).idle, 0);
}

#[tokio::test]
async fn replaces_an_engine_that_died() {
    let pool = Pool::new(1);
    let spec = EngineSpec::new(fake());
    let mut a = pool.checkout(&spec).await.unwrap();
    a.send("fake exit").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(a);
    // Dead on return: not kept.
    assert_eq!(pool.stats(&spec).idle, 0);
    let mut b = pool.checkout(&spec).await.unwrap();
    assert!(b.search(&Position::startpos(), &Limits::depth(1), |_| {}).await.is_ok());
    b.discard();
    assert_eq!(pool.stats(&spec), chess_engine::pool::PoolStats { limit: 1, checked_out: 0, idle: 0 });
}

#[tokio::test]
async fn warms_engines_ahead() {
    let pool = Pool::new(3);
    let spec = EngineSpec::new(fake());
    pool.warm(&spec, 2).await.unwrap();
    assert_eq!(pool.stats(&spec).idle, 2);
    pool.kill_idle_older_than(Duration::from_secs(3600));
    assert_eq!(pool.stats(&spec).idle, 2);
}
