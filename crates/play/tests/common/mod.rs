//! What the Referee runtime's tests share: a hub in a temporary directory, the fake engine, and waiting for things.

#![allow(dead_code)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use chess_core::events::{Event, EventKind};
use chess_hub::{Config, Hub, Next, Subscription, TopicFilter};
use chess_play::{Games, Timing};
use chess_work::{EngineSource, FixedEngines};

/// The fake UCI engine (`tests/support/fake_engine.rs`).
pub fn fake() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_play-fake-uci"))
}

pub struct Setup {
    pub hub: Arc<Hub>,
    pub games: Games,
    _dir: tempfile::TempDir,
}

/// A hub with four cores in a fresh data directory, and the games, with the waits shortened.
pub fn setup(engines: Arc<dyn EngineSource>) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), 4);
    let hub = Hub::open(config).unwrap();
    let games = Games::new(hub.clone(), engines, Timing::fast());
    Setup { hub, games, _dir: dir }
}

/// The fake engine as `fake` (the analysis engine, with the ladder), as `dying` (exits on its first search), `slow`
/// (half a second a move) and `illegal`, and as `maia-1500` (one node a move, rated 1500).
pub fn engines() -> Arc<dyn EngineSource> {
    FixedEngines::new()
        .with("fake", fake(), &[])
        .with("dying", fake(), &["--die"])
        .with("slow", fake(), &["--delay", "500"])
        .with("illegal", fake(), &["--illegal"])
        .with_engine("maia-1500", fake(), &[], Some(1), Some(1500))
        .shared()
}

/// Waits (up to fifteen seconds) until `ok` holds, polling.
pub async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let waited = tokio::time::timeout(Duration::from_secs(15), async {
        while !ok() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "timed out waiting for {what}");
}

/// Waits (up to fifteen seconds) until the async check holds.
#[macro_export]
macro_rules! until_async {
    ($what:expr, $check:expr) => {{
        let waited = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                if $check {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(waited.is_ok(), "timed out waiting for {}", $what);
    }};
}

/// Follows every event, game detail included.
pub fn subscribe(hub: &Hub) -> Subscription {
    hub.bus.subscribe(0, TopicFilter::of(["game:*", "games", "policy", "connection:*", "work"]))
}

/// The events received so far.
pub fn drain(sub: &mut Subscription) -> Vec<Event> {
    let mut out = std::mem::take(&mut sub.backlog);
    while let Some(next) = sub.try_recv() {
        if let Next::Event(e) = next {
            out.push(e);
        }
    }
    out
}

pub fn count(events: &[Event], kind: EventKind) -> usize {
    events.iter().filter(|e| e.kind == kind).count()
}
