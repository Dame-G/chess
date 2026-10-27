//! Importing history from the mock Lichess: your games (through a broken export, then again with nothing new), stored
//! once; another player's, which waits while you play on Lichess.

mod common;

use std::time::Duration;

use common::*;
use dameless_core::{
    ConnectionKey, PlayerKey, WorkId,
    game::GameStatus,
    work::{Priority, Subject, WorkKind},
};
use dameless_hub::{Executor, WorkContext};
use dameless_lichess::testing::{MockAccount, MockLichess, exported_game};
use dameless_play::{Connections, ImportExecutor, ImportSpec};
use dameless_store::{GameFilter, NewWork, SourceKind};

async fn work(s: &Setup, spec: &ImportSpec) -> (WorkContext, tokio_util::sync::CancellationToken) {
    let spec = serde_json::to_value(spec).unwrap();
    let w = s
        .hub
        .store
        .insert_work(NewWork {
            kind: WorkKind::Import,
            priority: Priority::Interactive,
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

async fn imported(s: &Setup) -> Vec<dameless_core::game::GameSummary> {
    s.hub
        .store
        .games(GameFilter { sources: vec![SourceKind::Import], limit: 100, ..GameFilter::default() })
        .await
        .unwrap()
}

#[tokio::test]
async fn your_history_is_imported_once_through_a_broken_export() {
    let s = setup(binaries());
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
    let s = setup(binaries());
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
    // Anonymous: never the bot's token, and none of yours without a login.
    assert_eq!(mock.requests_to("/api/games/user/bob")[0].token, None);

    // Stopped: it says so and keeps where it was.
    let (cx, cancel) = work(&s, &spec(PlayerKey::Lichess("bob".into()))).await;
    cancel.cancel();
    let e = executor.run(cx).await.unwrap_err();
    assert!(WorkContext::is_stopped(&e));
    let _ = WorkId(0);
}
