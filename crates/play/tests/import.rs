//! Importing history from the mock Lichess: your games (through a broken export, then again with nothing new), stored
//! once; your whole history, new games after it, a history cut short and filled in, an import resumed after a
//! restart; another player's, which waits while you play on Lichess.

mod common;

use std::time::Duration;

use chess_core::{
    ConnectionKey, PlayerKey, WorkId,
    game::GameStatus,
    work::{Priority, Subject, WorkKind},
};
use chess_hub::{Executor, WorkContext};
use chess_lichess::testing::{MockAccount, MockLichess, exported_game};
use chess_play::{Connections, ImportExecutor, ImportSpec};
use chess_store::{GameFilter, NewWork, SourceKind};
use common::*;

async fn work(s: &Setup, spec: &ImportSpec) -> (WorkContext, tokio_util::sync::CancellationToken) {
    let spec = serde_json::to_value(spec).unwrap();
    let w = s
        .hub
        .store
        .insert_work(NewWork {
            kind: WorkKind::Import,
            priority: Priority::Review,
            title: "Importing".into(),
            subject: Subject::Player { key: PlayerKey::You },
            spec: spec.clone(),
        })
        .await
        .unwrap();
    WorkContext::detached(s.hub.clone(), w.id, WorkKind::Import, spec)
}

fn spec(player: PlayerKey) -> ImportSpec {
    ImportSpec { player, max: None, since: None, speeds: Vec::new(), rated: None }
}

async fn imported(s: &Setup) -> Vec<chess_core::game::GameSummary> {
    s.hub
        .store
        .games(GameFilter { sources: vec![SourceKind::Import], limit: 100, ..GameFilter::default() })
        .await
        .unwrap()
}

#[tokio::test]
async fn your_history_is_imported_once_through_a_broken_export() {
    let s = setup(engines());
    let mock = MockLichess::start().await;
    mock.add_account("you-tok", MockAccount::user("Dame"));
    let games =
        (0..6).map(|i| exported_game(&format!("g{i}"), 1_700_000_000_000 + i * 100_000, "dame", "bob")).collect();
    mock.set_games_export("dame", games);
    let connections = Connections::new(s.hub.clone(), s.games.clone(), mock.config());
    connections.start().await.unwrap();
    let executor = ImportExecutor::new(connections.clone());
    assert_eq!(executor.kind(), WorkKind::Import);

    // Not logged in: nothing to import from.
    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    assert_eq!(executor.run(cx).await.unwrap_err().status(), 409);

    connections.set_token(ConnectionKey::You, "you-tok").await.unwrap();
    mock.break_export_after(2);
    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    let id = cx.id;
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["skipped"].as_u64()), (Some(6), Some(0)));
    let stored = imported(&s).await;
    assert_eq!(stored.len(), 6);
    assert!(stored.iter().all(|g| g.status == GameStatus::Finished && g.you.is_some() && g.plies == 7));
    assert_eq!(mock.requests_to("/api/games/user/Dame").len(), 2, "the broken export was reopened");
    let checkpoint = s.hub.store.work_checkpoint(id).await.unwrap().unwrap();
    assert_eq!(checkpoint["read"], 6);
    let full = s.hub.store.game(stored[0].id).await.unwrap().unwrap();
    assert!(full.moves.iter().any(|m| m.eval.is_some()));

    // Again: only newer games are asked for, and there are none.
    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    let result = executor.run(cx).await.unwrap();
    assert_eq!(result["imported"], 0);
    let asked = mock.requests_to("/api/games/user/Dame");
    assert_eq!(asked.last().unwrap().query["since"], (1_700_000_500_000i64 + 1).to_string());

    // From the start: every game is here already.
    let (cx, _) = work(&s, &ImportSpec { since: Some(0), ..spec(PlayerKey::You) }).await;
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["skipped"].as_u64()), (Some(0), Some(6)));
    assert_eq!(imported(&s).await.len(), 6);
}

#[tokio::test]
async fn another_players_history_waits_while_you_play() {
    let s = setup(engines());
    let mock = MockLichess::start().await;
    let games = (0..3).map(|i| exported_game(&format!("b{i}"), 1_700_000_000_000 + i, "bob", "carol")).collect();
    mock.set_games_export("bob", games);
    let connections = Connections::new(s.hub.clone(), s.games.clone(), mock.config());
    connections.start().await.unwrap();
    let executor = std::sync::Arc::new(ImportExecutor::new(connections));

    s.hub.policy.set_live_games(&[false]);
    let (cx, _) = work(&s, &ImportSpec { max: Some(2), ..spec(PlayerKey::Lichess("bob".into())) }).await;
    let running = {
        let executor = executor.clone();
        tokio::spawn(async move { executor.run(cx).await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!running.is_finished(), "an import of another player waits while you play");
    assert!(mock.requests_to("/api/games/user/bob").is_empty());
    s.hub.policy.set_live_games(&[]);
    let result = tokio::time::timeout(Duration::from_secs(10), running).await.unwrap().unwrap().unwrap();
    assert_eq!(result["imported"], 2);
    let stored = imported(&s).await;
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|g| g.you.is_none() && g.white.player == PlayerKey::Lichess("bob".into())));
    // Anonymous: none of your token without a login.
    assert_eq!(mock.requests_to("/api/games/user/bob")[0].token, None);

    // Stopped: it says so and keeps where it was.
    let (cx, cancel) = work(&s, &spec(PlayerKey::Lichess("bob".into()))).await;
    cancel.cancel();
    let e = executor.run(cx).await.unwrap_err();
    assert!(WorkContext::is_stopped(&e));
    let _ = WorkId(0);
}

/// `n` games of yours on Lichess, one a minute apart, the newest last; ids `{prefix}{i}`.
fn history(prefix: &str, from: usize, n: usize) -> Vec<serde_json::Value> {
    (from..from + n).map(|i| exported_game(&format!("{prefix}{i}"), at(i), "dame", "bob")).collect()
}

fn at(i: usize) -> i64 {
    1_700_000_000_000 + i as i64 * 60_000
}

/// The mock, you logged in, and an import executor.
async fn yours(s: &Setup, games: Vec<serde_json::Value>) -> (MockLichess, ImportExecutor) {
    let mock = MockLichess::start().await;
    mock.add_account("you-tok", MockAccount::user("Dame"));
    mock.set_games_export("dame", games);
    let connections = Connections::new(s.hub.clone(), s.games.clone(), mock.config());
    connections.start().await.unwrap();
    connections.set_token(ConnectionKey::You, "you-tok").await.unwrap();
    (mock, ImportExecutor::new(connections))
}

async fn stored(s: &Setup) -> u32 {
    s.hub.store.imported_history(PlayerKey::You).await.unwrap().0
}

async fn complete(s: &Setup) -> bool {
    s.hub.store.meta::<bool>(chess_play::import::COMPLETE_KEY).await.unwrap().unwrap_or(false)
}

#[tokio::test]
async fn the_first_import_brings_in_everything_and_the_next_only_whats_new() {
    let s = setup(engines());
    let (mock, executor) = yours(&s, history("h", 0, 250)).await;

    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    let id = cx.id;
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["complete"].as_bool()), (Some(250), Some(true)));
    assert_eq!(stored(&s).await, 250);
    assert!(complete(&s).await);
    // Read from the start, all of it: nothing older to go back for.
    assert_eq!(mock.requests_to("/api/games/user/Dame").len(), 1);
    let progress = s.hub.store.work(id).await.unwrap().unwrap().progress.unwrap();
    assert_eq!((progress.done, progress.total), (250, Some(250)));
    assert_eq!(progress.text.as_deref(), Some("250 of about 250 games, 250 new"));

    // Two new games: only they are asked for and stored.
    let mut games = history("h", 0, 250);
    games.extend(history("n", 250, 2));
    mock.set_games_export("dame", games);
    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["skipped"].as_u64()), (Some(2), Some(0)));
    let asked = mock.requests_to("/api/games/user/Dame");
    assert_eq!(asked.len(), 2, "a complete history is not gone back over");
    assert_eq!(asked[1].query["since"], (at(249) + 1).to_string());
    assert!(!asked[1].query.contains_key("until"));
    assert_eq!(stored(&s).await, 252);
}

#[tokio::test]
async fn a_history_cut_short_is_filled_in() {
    let s = setup(engines());
    let (mock, executor) = yours(&s, history("h", 0, 10)).await;

    // The newest three, as the sheet's old button asked for.
    let (cx, _) = work(&s, &ImportSpec { max: Some(3), ..spec(PlayerKey::You) }).await;
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["complete"].as_bool()), (Some(3), Some(false)));
    assert!(!complete(&s).await);

    // Then everything: nothing newer, and the seven older ones.
    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["skipped"].as_u64()), (Some(7), Some(0)));
    assert_eq!(stored(&s).await, 10);
    assert!(complete(&s).await);
    let asked = mock.requests_to("/api/games/user/Dame");
    assert_eq!(asked.len(), 3);
    assert_eq!(asked[1].query["since"], (at(9) + 1).to_string());
    assert_eq!(asked[2].query["until"], (at(7) - 1).to_string());
    assert!(!asked[2].query.contains_key("since"));

    // Once complete, only what is new.
    let (cx, _) = work(&s, &spec(PlayerKey::You)).await;
    assert_eq!(executor.run(cx).await.unwrap()["imported"], 0);
    assert_eq!(mock.requests_to("/api/games/user/Dame").len(), 4);
}

#[tokio::test]
async fn an_import_cut_off_resumes_after_a_restart() {
    let s = setup(engines());
    let mock = MockLichess::start().await;
    mock.add_account("you-tok", MockAccount::user("Dame"));
    mock.set_games_export("dame", history("h", 0, 250));
    let mut config = mock.config();
    // A long wait before a broken export is reopened: the app stops in it.
    config.timing.import_break = Duration::from_secs(20);
    let connections = Connections::new(s.hub.clone(), s.games.clone(), config);
    connections.start().await.unwrap();
    connections.set_token(ConnectionKey::You, "you-tok").await.unwrap();
    let executor = std::sync::Arc::new(ImportExecutor::new(connections));

    // The export breaks after 150 games, and the app stops while it waits to reopen it: what was read is stored.
    mock.break_export_after(150);
    let (cx, cancel) = work(&s, &spec(PlayerKey::You)).await;
    let id = cx.id;
    let running = {
        let executor = executor.clone();
        tokio::spawn(async move { executor.run(cx).await })
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        while s.hub.store.work_checkpoint(id).await.unwrap().is_none_or(|c| c["read"] != 100) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    cancel.cancel();
    let e = running.await.unwrap().unwrap_err();
    assert!(WorkContext::is_stopped(&e));
    assert_eq!(stored(&s).await, 150);
    assert!(!complete(&s).await);
    let checkpoint = s.hub.store.work_checkpoint(id).await.unwrap().unwrap();
    assert_eq!(checkpoint["read"], 150);

    // The app starts again: the same work, from its checkpoint, carries on below the oldest game read.
    let spec_json = s.hub.store.work_spec(id).await.unwrap().unwrap();
    let (_control, rx) = tokio::sync::watch::channel(chess_hub::Control::Run);
    let cx = WorkContext::new(
        s.hub.clone(),
        id,
        WorkKind::Import,
        spec_json,
        Some(checkpoint),
        tokio_util::sync::CancellationToken::new(),
        rx,
    );
    let result = executor.run(cx).await.unwrap();
    assert_eq!((result["imported"].as_u64(), result["skipped"].as_u64()), (Some(250), Some(0)));
    assert_eq!(stored(&s).await, 250);
    assert!(complete(&s).await);
    let asked = mock.requests_to("/api/games/user/Dame");
    assert_eq!(asked.last().unwrap().query["until"], (at(100) - 1).to_string());
}
