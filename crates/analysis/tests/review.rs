//! Reviews as work, end to end against the fake engine: queued, run, stored, announced, skipped, paused and resumed.

mod common;

use std::time::Duration;

use common::{Harness, QUIET_DRAW, SCHOLARS_MATE, eventually, fake, finished_game, harness};
use dameless_analysis::{ReviewCheckpoint, ReviewRequest, ReviewResult, ReviewSpec, Reviews};
use dameless_core::{
    GameId, PlayerKey, Score, Side,
    events::{EventKind, ReviewUpserted},
    game::Outcome,
    review::judge::Classification,
    work::{Priority, Subject, Work, WorkKind},
};
use dameless_hub::{Control, Executor, TopicFilter, WorkContext};
use tokio_util::sync::CancellationToken;

async fn queue(h: &Harness, reviews: &Reviews, req: ReviewRequest) -> (Work, serde_json::Value) {
    let new = reviews.prepare(req).await.unwrap();
    let spec = new.spec.clone();
    (h.hub.store.insert_work(new).await.unwrap(), spec)
}

async fn run(h: &Harness, reviews: &Reviews, work: &Work, spec: serde_json::Value) -> ReviewResult {
    let (cx, _cancel) = WorkContext::detached(h.hub.clone(), work.id, WorkKind::Review, spec);
    serde_json::from_value(reviews.run(cx).await.unwrap()).unwrap()
}

#[tokio::test]
async fn a_game_is_reviewed_stored_and_announced_then_skipped() {
    let h = harness(2).await;
    let reviews = Reviews::new(h.eval.clone());
    let game = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let mut sub = h.hub.bus.subscribe(0, TopicFilter::of(["games", "work"]));

    let (work, spec) = queue(&h, &reviews, ReviewRequest { games: vec![game], ..Default::default() }).await;
    assert_eq!((work.kind, work.title.as_str()), (WorkKind::Review, "Reviewing you vs engine:fake"));
    assert_eq!(work.subject, Subject::Game { id: game });
    let resolved: ReviewSpec = serde_json::from_value(spec.clone()).unwrap();
    assert_eq!(
        (resolved.engine.as_str(), resolved.nodes, resolved.multipv, resolved.concurrency),
        ("fake", 200_000, 2, 1)
    );

    let result = run(&h, &reviews, &work, spec).await;
    // Seven moves, eight positions, the last one mate: seven searched, none cached.
    assert_eq!((result.reviewed, result.skipped, result.failed), (1, 0, 0));
    assert_eq!((result.positions_searched, result.positions_cached), (7, 0));

    let review = h.hub.store.review(game, None, None).await.unwrap().unwrap();
    assert_eq!((review.engine.as_str(), review.nodes, review.plies.len()), ("Fake 1.0", 200_000, 7));
    assert_eq!(result.engine, "Fake 1.0");
    let mate = review.plies.last().unwrap();
    assert_eq!((mate.san.as_str(), mate.side, mate.eval_after), ("Qxf7#", Side::White, Score::Mate(0)));
    // The fake engine's best move is the alphabetically first legal one; 1. e4 is not it.
    assert_eq!(review.plies[0].best_uci, "a2a3");
    assert_ne!(review.plies[0].classification, Classification::Best);
    assert!(review.summary.white.accuracy.is_some() && review.summary.black.accuracy.is_some());

    let events: Vec<_> = std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            dameless_hub::Next::Event(e) => Some(e),
            dameless_hub::Next::Lagged(_) => None,
        })
        .collect();
    let upserted: Vec<ReviewUpserted> = events
        .iter()
        .filter(|e| e.kind == EventKind::ReviewUpserted)
        .map(|e| serde_json::from_value(e.data.clone()).unwrap())
        .collect();
    assert_eq!(upserted.len(), 1);
    assert_eq!(upserted[0].game, game);
    assert_eq!(upserted[0].accuracy, review.summary.accuracy());
    assert!(events.iter().any(|e| e.kind == EventKind::GameUpserted));
    let stored = h.hub.store.work(work.id).await.unwrap().unwrap();
    let progress = stored.progress.unwrap();
    assert_eq!((progress.done, progress.total, progress.unit.as_str()), (1, Some(1), "games"));
    let checkpoint: ReviewCheckpoint =
        serde_json::from_value(h.hub.store.work_checkpoint(work.id).await.unwrap().unwrap()).unwrap();
    assert_eq!((checkpoint.next, checkpoint.reviewed), (1, 1));

    // Again with the same engine and budget: nothing to do.
    let (work, spec) = queue(&h, &reviews, ReviewRequest { games: vec![game], ..Default::default() }).await;
    let again = run(&h, &reviews, &work, spec).await;
    assert_eq!((again.reviewed, again.skipped, again.positions_searched), (0, 1, 0));

    // Another budget reviews it again, every position searched afresh.
    let (work, spec) =
        queue(&h, &reviews, ReviewRequest { games: vec![game], nodes: Some(5_000), ..Default::default() }).await;
    let other = run(&h, &reviews, &work, spec).await;
    assert_eq!((other.reviewed, other.positions_searched), (1, 7));
}

#[tokio::test]
async fn a_players_newest_unreviewed_games_are_found() {
    let h = harness(4).await;
    let reviews = Reviews::new(h.eval.clone());
    let mate = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let draw = finished_game(&h.hub, fake(), PlayerKey::You, &QUIET_DRAW, Outcome::Draw, 2).await;
    finished_game(&h.hub, fake(), PlayerKey::Engine("maia-1500".into()), &QUIET_DRAW, Outcome::Draw, 3).await;

    // All of them in the background: at the lowest priority, newest first, with no cap.
    let all = reviews.prepare(ReviewRequest { player: Some(PlayerKey::You), background: true, ..Default::default() });
    let all = all.await.unwrap();
    assert_eq!((all.priority, all.title.as_str()), (Priority::Background, "Reviewing all 2 of your games"));
    assert_eq!(serde_json::from_value::<ReviewSpec>(all.spec).unwrap().games, [draw, mate]);
    // The review bar's count is that same query, whatever a profile's window holds.
    assert_eq!(reviews.unreviewed_count(&PlayerKey::You).await.unwrap(), 2);

    let req = ReviewRequest { player: Some(PlayerKey::You), concurrency: Some(3), ..Default::default() };
    let (work, spec) = queue(&h, &reviews, req.clone()).await;
    assert_eq!(work.priority, Priority::Review);
    assert_eq!(work.title, "Reviewing 2 of your games");
    let resolved: ReviewSpec = serde_json::from_value(spec.clone()).unwrap();
    assert_eq!(resolved.games, [draw, mate]);
    assert_eq!(resolved.concurrency, 3);
    // Both games open the same way: the start position is searched once.
    let result = run(&h, &reviews, &work, spec).await;
    assert_eq!((result.reviewed, result.positions_searched, result.positions_cached), (2, 15, 1));
    assert_eq!(reviews.unreviewed_count(&PlayerKey::You).await.unwrap(), 0);

    let (_, spec) = queue(&h, &reviews, req).await;
    assert!(serde_json::from_value::<ReviewSpec>(spec).unwrap().games.is_empty());

    // A game another engine reviewed (Reckless, before the engines list) counts as reviewed: no review all again.
    let older = finished_game(&h.hub, PlayerKey::You, fake(), &QUIET_DRAW, Outcome::Draw, 4).await;
    assert_eq!(reviews.unreviewed_count(&PlayerKey::You).await.unwrap(), 1);
    let summary = h.hub.store.review(mate, None, None).await.unwrap().unwrap().summary;
    h.hub.store.put_review(older, "reckless|Reckless abcdef1".into(), summary, Vec::new()).await.unwrap();
    assert_eq!(reviews.unreviewed_count(&PlayerKey::You).await.unwrap(), 0);

    let missing = reviews.prepare(ReviewRequest { games: vec![GameId(999)], ..Default::default() }).await;
    assert_eq!(missing.unwrap_err().status(), 404);
    assert_eq!(reviews.prepare(ReviewRequest::default()).await.unwrap_err().status(), 400);
    let tiny = ReviewRequest { games: vec![mate], nodes: Some(10), ..Default::default() };
    assert_eq!(reviews.prepare(tiny).await.unwrap_err().status(), 400);
}

#[tokio::test]
async fn a_review_waits_while_you_play_on_lichess() {
    let h = harness(2).await;
    let reviews = Reviews::new(h.eval.clone());
    let game = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let (work, spec) = queue(&h, &reviews, ReviewRequest { games: vec![game], ..Default::default() }).await;

    h.hub.policy.set_live_games(&[false]);
    assert_eq!(
        reviews.prepare(ReviewRequest { games: vec![game], ..Default::default() }).await.unwrap_err().status(),
        409
    );
    let running = {
        let (hub, reviews, id) = (h.hub.clone(), reviews.clone(), work.id);
        tokio::spawn(async move {
            let (cx, _cancel) = WorkContext::detached(hub, id, WorkKind::Review, spec);
            reviews.run(cx).await
        })
    };
    let store = h.hub.store.clone();
    eventually(async || store.work(work.id).await.unwrap().unwrap().paused_for.is_some()).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!running.is_finished());
    assert!(h.hub.store.review(game, None, None).await.unwrap().is_none());

    h.hub.policy.set_live_games(&[]);
    let result: ReviewResult = serde_json::from_value(running.await.unwrap().unwrap()).unwrap();
    assert_eq!(result.reviewed, 1);
    assert!(h.hub.store.work(work.id).await.unwrap().unwrap().paused_for.is_none());
}

#[tokio::test]
async fn a_review_carries_on_from_its_checkpoint_and_stops_when_asked() {
    let h = harness(2).await;
    let reviews = Reviews::new(h.eval.clone());
    let first = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let second = finished_game(&h.hub, fake(), PlayerKey::You, &QUIET_DRAW, Outcome::Draw, 2).await;
    let (work, spec) = queue(&h, &reviews, ReviewRequest { games: vec![first, second], ..Default::default() }).await;

    // Stopped before it began: it saves where it stands and says it stopped.
    let (cx, cancel) = WorkContext::detached(h.hub.clone(), work.id, WorkKind::Review, spec.clone());
    cancel.cancel();
    let stopped = reviews.run(cx).await.unwrap_err();
    assert!(WorkContext::is_stopped(&stopped));
    let saved = h.hub.store.work_checkpoint(work.id).await.unwrap().unwrap();
    assert_eq!(serde_json::from_value::<ReviewCheckpoint>(saved).unwrap(), ReviewCheckpoint::default());

    // After a restart with the first game done: only the second is reviewed.
    let checkpoint = ReviewCheckpoint { next: 1, reviewed: 1, searched: 7, ..Default::default() };
    let (_tx, rx) = tokio::sync::watch::channel(Control::Run);
    let cx = WorkContext::new(
        h.hub.clone(),
        work.id,
        WorkKind::Review,
        spec,
        Some(serde_json::to_value(&checkpoint).unwrap()),
        CancellationToken::new(),
        rx,
    );
    let result: ReviewResult = serde_json::from_value(reviews.run(cx).await.unwrap()).unwrap();
    assert_eq!((result.reviewed, result.positions_searched), (2, 16));
    assert!(h.hub.store.review(first, None, None).await.unwrap().is_none());
    assert!(h.hub.store.review(second, None, None).await.unwrap().is_some());
}

#[tokio::test]
async fn the_scheduler_runs_a_submitted_review() {
    let h = harness(2).await;
    let reviews = Reviews::new(h.eval.clone());
    let scheduler = dameless_work::Scheduler::new(h.hub.clone());
    scheduler.register(std::sync::Arc::new(reviews.clone()));
    scheduler.start().await.unwrap();
    let game = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let work = reviews.submit(&scheduler, ReviewRequest { games: vec![game], ..Default::default() }).await.unwrap();
    let store = h.hub.store.clone();
    eventually(async || store.work(work.id).await.unwrap().unwrap().status == dameless_core::work::WorkStatus::Done)
        .await;
    assert!(reviews.get(game, None, None).await.unwrap().is_some());
    let named = reviews.get(game, Some("fake"), Some(200_000)).await;
    assert!(named.unwrap().is_some());
    scheduler.shutdown();
}
