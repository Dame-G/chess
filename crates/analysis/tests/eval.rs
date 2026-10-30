//! Live evaluation sessions and cached evaluation, against the fake engine.

mod common;

use std::time::Duration;

use common::{eventually, harness};
use dameless_analysis::{Budget, EvalRequest};
use dameless_core::{
    ErrorCode, Score,
    chess::{Position, position_key},
    events::{EvalInfo, EventKind},
    review::ToSearch,
};
use dameless_hub::{Next, Subscription, TopicFilter};

fn request(client: &str, moves: &[&str]) -> EvalRequest {
    EvalRequest {
        fen: None,
        moves: moves.iter().map(|m| (*m).to_owned()).collect(),
        engine: None,
        multipv: 2,
        budget: Budget::Infinite,
        threads: None,
        client: client.to_owned(),
    }
}

/// The `eval.info` events waiting on a subscription.
fn infos(sub: &mut Subscription) -> Vec<EvalInfo> {
    std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            Next::Event(e) if e.kind == EventKind::EvalInfo => Some(serde_json::from_value(e.data).unwrap()),
            _ => None,
        })
        .collect()
}

fn cp(score: Score) -> i32 {
    match score {
        Score::Cp(cp) => cp,
        Score::Mate(_) => panic!("a mate"),
    }
}

#[tokio::test]
async fn a_session_streams_whites_scores_a_few_times_a_second() {
    let h = harness(2).await;
    let mut sub = h.hub.bus.subscribe(0, TopicFilter::of(["eval:*"]));
    // After 1. e4 Black is to move: the engine's second line is 25 worse for Black, so 25 better for White.
    let started = h.eval.start_session(request("tab-1", &["e4"])).await.unwrap();
    assert_eq!(started.engine, "Fake 1.0");
    assert_eq!(started.fen, Position::after(None, &["e4"]).unwrap().fen());
    assert_eq!(h.hub.ledger.snapshot().reserved, 1);
    // Listen for at least 600 ms, and until the first report has come however slow the machine is.
    let t0 = std::time::Instant::now();
    let mut got = Vec::new();
    while got.is_empty() || t0.elapsed() < Duration::from_millis(600) {
        assert!(t0.elapsed() < Duration::from_secs(10), "no eval.info");
        tokio::time::sleep(Duration::from_millis(20)).await;
        got.extend(infos(&mut sub));
    }
    // The engine reports every 10 ms; the board hears at most five a second (and a first one at once).
    let allowed = 2 + (t0.elapsed().as_millis() / 200) as usize;
    assert!(got.len() <= allowed, "{} events in {:?}", got.len(), t0.elapsed());
    let last = got.last().unwrap();
    assert_eq!(last.session, started.session);
    assert_eq!(last.lines.len(), 2);
    assert_eq!(cp(last.lines[1].score) - cp(last.lines[0].score), 25);
    assert_eq!(last.lines[0].pv.first().map(String::as_str), Some("a7a5"));
    assert!(got.windows(2).all(|w| w[0].depth <= w[1].depth));

    assert!(h.eval.stop_session(&started.session).await);
    assert!(h.eval.sessions().is_empty());
    assert_eq!(h.hub.ledger.snapshot().reserved, 0);
    assert!(!h.eval.stop_session(&started.session).await);

    // White to move: the second line is worse for White.
    let started = h.eval.start_session(request("tab-1", &[])).await.unwrap();
    eventually(async || !infos(&mut sub).is_empty()).await;
    // Wait for a report of both lines (the first ones may hold only one), however long it takes.
    let t0 = std::time::Instant::now();
    let last = loop {
        assert!(t0.elapsed() < Duration::from_secs(10), "no report of two lines");
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(last) = infos(&mut sub).into_iter().rfind(|i| i.lines.len() == 2) {
            break last;
        }
    };
    h.eval.stop_session(&started.session).await;
    assert_eq!(cp(last.lines[1].score) - cp(last.lines[0].score), -25);
}

#[tokio::test]
async fn one_session_per_client() {
    let h = harness(4).await;
    let first = h.eval.start_session(request("tab-1", &[])).await.unwrap();
    let other = h.eval.start_session(request("tab-2", &["d4"])).await.unwrap();
    let second = h.eval.start_session(request("tab-1", &["e4", "e5"])).await.unwrap();
    let mut running: Vec<_> = h.eval.sessions().into_iter().map(|s| (s.client, s.session)).collect();
    running.sort();
    assert_eq!(running, [("tab-1".to_owned(), second.session.clone()), ("tab-2".to_owned(), other.session.clone())]);
    assert_ne!(first.session, second.session);
    assert_eq!(h.hub.ledger.snapshot().reservations.len(), 2);
    h.eval.stop_all().await;
    assert!(h.eval.sessions().is_empty());
    assert_eq!(h.hub.ledger.snapshot().reserved, 0);
}

#[tokio::test]
async fn a_session_with_a_budget_ends_by_itself() {
    let h = harness(2).await;
    let mut sub = h.hub.bus.subscribe(0, TopicFilter::of(["eval:*"]));
    let started =
        h.eval.start_session(EvalRequest { budget: Budget::Depth { depth: 3 }, ..request("tab", &[]) }).await.unwrap();
    eventually(async || h.eval.sessions().is_empty()).await;
    let got = infos(&mut sub);
    // The last report always goes out, however soon after the one before it came.
    assert_eq!(got.last().map(|i| (i.session.as_str(), i.depth)), Some((started.session.as_str(), 3)));
    assert_eq!(h.hub.ledger.snapshot().reserved, 0);
}

#[tokio::test]
async fn sessions_stop_when_you_play_on_lichess_and_none_start() {
    let h = harness(2).await;
    h.eval.start_session(request("tab", &[])).await.unwrap();
    // A Lichess game of yours starts.
    h.hub.policy.set_live_games(&[false]);
    eventually(async || h.eval.sessions().is_empty()).await;
    assert_eq!(h.hub.ledger.snapshot().reserved, 0);
    let refused = h.eval.start_session(request("tab", &[])).await.unwrap_err();
    assert_eq!((refused.code(), refused.status()), (ErrorCode::FairPlay, 409));
    // A correspondence game still forbids engines.
    h.hub.policy.set_live_games(&[true]);
    assert_eq!(h.eval.start_session(request("tab", &[])).await.unwrap_err().status(), 409);
    h.hub.policy.set_live_games(&[]);
    let again = h.eval.start_session(request("tab", &[])).await.unwrap();
    assert!(h.eval.stop_session(&again.session).await);
}

#[tokio::test]
async fn bad_requests_are_refused() {
    let h = harness(2).await;
    let illegal = h.eval.start_session(request("tab", &["e5"])).await.unwrap_err();
    assert_eq!(illegal.status(), 400);
    let lines = h.eval.start_session(EvalRequest { multipv: 0, ..request("tab", &[]) }).await.unwrap_err();
    assert_eq!(lines.status(), 400);
    assert_eq!(h.eval.start_session(request(" ", &[])).await.unwrap_err().status(), 400);
    assert!(h.eval.sessions().is_empty());
    assert_eq!(h.hub.ledger.snapshot().reserved, 0);
}

#[tokio::test]
async fn evaluations_are_cached_by_position_engine_nodes_and_multipv() {
    let h = harness(2).await;
    let after_e4 = Position::after(None, &["e4"]).unwrap().fen();
    let first = h.eval.evaluate(&after_e4, &h.engine, 10_000, 2).await.unwrap();
    // White's view: Black's second line is 25 better for White.
    assert_eq!(cp(first.lines[1].score) - cp(first.lines[0].score), 25);
    assert_eq!(first.lines[0].uci, "a7a5");
    // The same position at another move number is the same entry: no engine is asked for.
    let later = after_e4.replace(" 0 1", " 0 7");
    assert_eq!(h.eval.evaluate(&later, &h.engine, 10_000, 2).await.unwrap(), first);
    let one = |fen: &str| ToSearch { fen: fen.to_owned(), key: position_key(fen), terminal: false };
    let again = h.eval.evaluate_positions(&[one(&later)], &h.engine, 10_000, 2, 1).await.unwrap();
    assert_eq!((again.searched, again.cached), (0, 1));
    // Another budget is another search.
    let other = h.eval.evaluate_positions(&[one(&after_e4)], &h.engine, 20_000, 2, 1).await.unwrap();
    assert_eq!((other.searched, other.cached), (1, 0));
    // An engine with a fixed budget cannot analyse.
    let maia = dameless_analysis::EvalRequest { engine: Some("maia-1500".into()), ..request("tab-9", &[]) };
    assert_eq!(h.eval.start_session(maia).await.unwrap_err().status(), 400);

    // Many at once: the cached one is read, a repeated one searched once, the finished one not at all.
    let start = Position::start().fen();
    let mated = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
    let at = |fen: &str, terminal| ToSearch { fen: fen.to_owned(), key: position_key(fen), terminal };
    let found = h
        .eval
        .evaluate_positions(
            &[at(&start, false), at(&after_e4, false), at(&start, false), at(mated, true)],
            &h.engine,
            10_000,
            2,
            2,
        )
        .await
        .unwrap();
    assert_eq!((found.searched, found.cached), (1, 1));
    assert_eq!(found.evals[1].as_ref(), Some(&first));
    assert_eq!(found.evals[0], found.evals[2]);
    assert!(found.evals[3].is_none());
    assert_eq!(h.eval.evaluate(mated, &h.engine, 10_000, 2).await.unwrap_err().status(), 400);
}
