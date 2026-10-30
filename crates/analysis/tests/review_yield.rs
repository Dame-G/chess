//! A background review takes every core and gives them back within one search: to work of higher priority asking for
//! cores, and when it is paused. Against the fake engine slowed down, so a review lasts long enough to watch.
//!
//! One test, since it sets the fake engine's delay for the whole process.

mod common;

use std::time::{Duration, Instant};

use chess_analysis::{ReviewRequest, ReviewResult, ReviewSpec, Reviews};
use chess_core::{
    PlayerKey, WorkId,
    game::Outcome,
    work::{Priority, WorkKind},
};
use chess_hub::{Control, Executor, WorkContext};
use common::{QUIET_DRAW, SCHOLARS_MATE, eventually, fake, finished_game, harness};
use tokio_util::sync::CancellationToken;

/// Searches by nodes take this long.
const DELAY_MS: u64 = 50;

const RUY_LOPEZ: [&str; 16] =
    ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6", "O-O", "Be7", "Re1", "b5", "Bb3", "d6", "c3", "O-O"];

#[tokio::test]
async fn a_background_review_takes_every_core_and_gives_way_within_one_search() {
    // SAFETY: set before any engine starts, and this file's only test is the only thing running in the process.
    unsafe { std::env::set_var("FAKE_EVAL_DELAY_MS", DELAY_MS.to_string()) };
    let h = harness(2).await;
    let reviews = Reviews::new(h.eval.clone());
    let mut games = Vec::new();
    for (day, san) in [&SCHOLARS_MATE[..], &QUIET_DRAW, &RUY_LOPEZ].into_iter().enumerate() {
        games.push(finished_game(&h.hub, PlayerKey::You, fake(), san, Outcome::Draw, day as i64).await);
    }
    let new = reviews.prepare(ReviewRequest { games, background: true, ..Default::default() }).await.unwrap();
    assert_eq!(new.priority, Priority::Background);
    let spec = new.spec.clone();
    assert_eq!(serde_json::from_value::<ReviewSpec>(spec.clone()).unwrap().concurrency, 2, "every core");
    let work = h.hub.store.insert_work(new).await.unwrap();
    let (control, rx) = tokio::sync::watch::channel(Control::Run);
    let cx = WorkContext::new(h.hub.clone(), work.id, WorkKind::Review, spec, None, CancellationToken::new(), rx)
        .with_priority(Priority::Background);
    let running = {
        let reviews = reviews.clone();
        tokio::spawn(async move { reviews.run(cx).await })
    };
    let ledger = h.hub.ledger.clone();
    let holds = |cores: u32| {
        let ledger = ledger.clone();
        move || ledger.snapshot().grants.iter().any(|g| g.work == work.id && g.cores == cores)
    };
    let all = holds(2);
    eventually(async || all()).await;

    // A review asked for now wants cores: it has them within a search or two, long before the review would end.
    let asked = Instant::now();
    let urgent = ledger.request_cores(WorkId(999), Priority::Review, 2, 2);
    let grant = loop {
        if let Some(grant) = urgent.grant() {
            break grant;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let waited = asked.elapsed();
    assert!(waited < Duration::from_millis(DELAY_MS * 10), "gave way in {waited:?}");
    assert_eq!(grant.cores(), 2);
    assert!(!running.is_finished());
    drop(grant);

    // Paused (for live play): it stops before its next search and holds no cores until resumed.
    let some = holds(2);
    eventually(async || some()).await;
    control.send(Control::Pause { reason: "A live game".into() }).unwrap();
    let store = h.hub.store.clone();
    eventually(async || store.work(work.id).await.unwrap().unwrap().paused_for.is_some()).await;
    assert!(ledger.snapshot().grants.iter().all(|g| g.work != work.id));
    assert!(!running.is_finished());
    control.send(Control::Run).unwrap();

    let result: ReviewResult = serde_json::from_value(running.await.unwrap().unwrap()).unwrap();
    assert_eq!((result.reviewed, result.failed), (3, 0));
}
