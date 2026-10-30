//! The player side: drills from your reviewed mistakes, answered on a schedule; your repertoire; both players'
//! progress.

mod common;

use common::{SCHOLARS_MATE, fake, finished_game, harness, moves};
use dameless_analysis::{Drills, Profiles, RepertoireRequest, ReviewRequest, Reviews, SessionRequest};
use dameless_core::{
    GameId, PlayerKey, Side, TimeControl, Timestamp,
    drills::DrillOutcome,
    events::{Event, EventKind},
    game::{Ending, GameStatus, Opening, Outcome, Seat, Source},
    prep::PrepRequest,
    progress::Metric,
    themes::ThemeKind,
    work::WorkKind,
};
use dameless_hub::{Executor, Next, Subscription, TopicFilter, WorkContext};
use dameless_store::NewGame;

/// You (White) give your queen away on move 3.
const QUEEN_BLUNDER: [&str; 6] = ["e4", "e5", "Qh5", "Nc6", "Qxe5+", "Nxe5"];

fn events(sub: &mut Subscription) -> Vec<Event> {
    let mut out = std::mem::take(&mut sub.backlog);
    while let Some(next) = sub.try_recv() {
        if let Next::Event(e) = next {
            out.push(e);
        }
    }
    out
}

async fn review(h: &common::Harness, reviews: &Reviews, games: Vec<GameId>) {
    let new = reviews.prepare(ReviewRequest { games, ..Default::default() }).await.unwrap();
    let spec = new.spec.clone();
    let work = h.hub.store.insert_work(new).await.unwrap();
    let (cx, _cancel) = WorkContext::detached(h.hub.clone(), work.id, WorkKind::Review, spec);
    reviews.run(cx).await.unwrap();
}

#[tokio::test]
async fn drills_come_from_your_reviewed_mistakes_and_repeat_on_a_schedule() {
    let h = harness(2).await;
    let mut sub = h.hub.bus.subscribe(0, TopicFilter::of(["player:*"]));
    let blunder = finished_game(
        &h.hub,
        PlayerKey::You,
        PlayerKey::lichess("Bob").unwrap(),
        &QUEEN_BLUNDER,
        Outcome::BlackWins,
        1,
    )
    .await;
    let theirs =
        finished_game(&h.hub, fake(), PlayerKey::Engine("maia-1500".into()), &QUEEN_BLUNDER, Outcome::BlackWins, 2)
            .await;
    let drills = Drills::new(h.hub.clone());
    // The review makes drills from your game, and only yours.
    let reviews = Reviews::new(h.eval.clone()).with_drills(drills.clone());
    review(&h, &reviews, vec![blunder, theirs]).await;
    let questions = drills.list().await.unwrap();
    assert!(!questions.is_empty(), "the queen blunder is a drill");
    // The list gives questions: no answers go out before a drill is tried.
    assert!(questions.iter().all(|d| d.best_moves.is_empty() && d.best_san.is_empty() && d.played.is_empty()));
    let mut all = Vec::new();
    for q in &questions {
        all.push(drills.get(q.id).await.unwrap());
    }
    assert!(all.iter().all(|d| d.game == blunder && d.side == Side::White));
    let queen = all.iter().find(|d| d.played_san == "Qxe5+").expect("a drill of the blunder");
    assert_eq!(queen.ply, 5);
    assert!(!queen.best_moves.is_empty() && !queen.best_moves.contains(&queen.played));
    let made = events(&mut sub);
    assert_eq!(made.iter().filter(|e| e.kind == EventKind::DrillUpserted).count(), all.len());
    assert!(made.iter().all(|e| e.topic == "player:you"));

    // Making them again finds nothing new: one drill per position.
    assert!(drills.make_from_weak_spots(None).await.unwrap().is_empty());
    assert!(drills.make_from_game(theirs).await.unwrap().is_empty());

    // Today: every drill is new and due.
    let now = Timestamp::from_unix_ms(1_800_000_000_000);
    let today = drills.today(&now, &SessionRequest::default()).await.unwrap();
    assert_eq!((today.new as usize, today.due, today.started_today), (all.len(), 0, 0));

    // A pass comes back tomorrow; a miss resets the run.
    let right = queen.best_moves[0].clone();
    let pass = drills.answer(queen.id, &right, now.clone()).await.unwrap();
    assert_eq!(pass.outcome, DrillOutcome::Pass);
    assert_eq!((pass.drill.reps, pass.drill.interval_days, pass.drill.lapses), (1, 1, 0));
    assert_eq!(pass.best_moves, queen.best_moves);
    assert!(pass.drill.due > now);
    let later = Timestamp::from_unix_ms(1_800_000_000_000 + 2 * 86_400_000);
    let miss = drills.answer(queen.id, &queen.played_san, later.clone()).await.unwrap();
    assert_eq!(miss.outcome, DrillOutcome::Fail);
    assert_eq!((miss.drill.reps, miss.drill.lapses, miss.drill.interval_days), (0, 1, 1));
    assert_eq!(drills.today(&now, &SessionRequest::default()).await.unwrap().started_today, 1);
    let answered = events(&mut sub);
    assert_eq!(answered.iter().filter(|e| e.kind == EventKind::DrillAnswered).count(), 2);
    assert_eq!(answered.iter().filter(|e| e.kind == EventKind::DrillUpserted).count(), 2);

    let trend = drills.trend(None).await.unwrap();
    assert_eq!(trend.iter().map(|p| p.answers).sum::<u32>(), 2);
    assert_eq!(trend.iter().map(|p| p.passes).sum::<u32>(), 1);

    assert_eq!(drills.answer(dameless_core::drills::DrillId(999), "e4", now.clone()).await.unwrap_err().status(), 404);
    drills.delete(queen.id).await.unwrap();
    assert_eq!(drills.delete(queen.id).await.unwrap_err().status(), 404);
    assert!(events(&mut sub).iter().any(|e| e.kind == EventKind::DrillDeleted));
    // Deleted, it can be made again.
    let again = drills.make_from_weak_spots(Some(5)).await.unwrap();
    assert_eq!(again.len(), 1);

    // Giving the queen away is a hanging piece, and a session on that theme finds it, whatever the day's cap says.
    assert_eq!(again[0].theme.kind, ThemeKind::HangingPiece);
    assert_eq!(again[0].theme.detail.as_deref(), Some("a queen"));
    let session = SessionRequest { theme: Some(ThemeKind::HangingPiece), ..SessionRequest::default() };
    let hanging = drills.today(&now, &SessionRequest { new_per_day: Some(0), ..session.clone() }).await.unwrap();
    assert_eq!(hanging.drills.iter().map(|d| d.id).collect::<Vec<_>>(), [again[0].id]);
    assert!(hanging.drills.iter().all(|d| d.best_moves.is_empty()));
    let none = SessionRequest { theme: Some(ThemeKind::TimeTrouble), ..SessionRequest::default() };
    assert!(drills.today(&now, &none).await.unwrap().drills.is_empty());

    // A hint names no move, and a pass found with it is half a miss: not a pass in a row, back tomorrow.
    let id = again[0].id;
    let first = drills.hint(id).await.unwrap();
    assert_eq!(first.level, 1);
    let second = drills.hint(id).await.unwrap();
    assert_eq!((second.level, drills.hint(id).await.unwrap().level), (2, 2));
    let right = drills.get(id).await.unwrap().best_moves[0].clone();
    assert!(!first.text.contains(&right) && !second.text.contains(&right));
    let hinted = drills.answer(id, &right, now.clone()).await.unwrap();
    assert!(hinted.hinted && hinted.outcome == DrillOutcome::Pass);
    assert_eq!((hinted.drill.reps, hinted.drill.interval_days, hinted.drill.lapses), (0, 1, 0));
    // The hint is spent with the answer.
    let plain = drills.answer(id, &right, now.clone()).await.unwrap();
    assert!(!plain.hinted);
    let trend = drills.trend(None).await.unwrap();
    assert_eq!(trend.iter().map(|p| p.hinted).sum::<u32>(), 1);
    assert_eq!(drills.hint(dameless_core::drills::DrillId(999)).await.unwrap_err().status(), 404);
}

/// A finished Lichess-like game of yours with a named opening covering `book` plies.
async fn opening_game(h: &common::Harness, san: &[&str], you: Side, outcome: Outcome, book: u32, day: i64) -> GameId {
    let (white, black) = match you {
        Side::White => (PlayerKey::You, PlayerKey::lichess("Bob").unwrap()),
        Side::Black => (PlayerKey::lichess("Bob").unwrap(), PlayerKey::You),
    };
    let mut g = NewGame::starting(
        Seat::new(white.clone(), white.to_string()),
        Seat::new(black.clone(), black.to_string()),
        Source::Local,
        TimeControl::clock(300_000, 0),
        Timestamp::from_unix_ms(1_700_000_000_000 + day * 86_400_000),
    );
    g.status = GameStatus::Finished;
    g.outcome = Some(outcome);
    g.ending = Some(Ending::Resignation);
    g.moves = moves(san);
    g.opening = Some(Opening { eco: Some("C20".into()), name: "King's Pawn Game".into(), ply: Some(book) });
    h.hub.store.insert_game(g).await.unwrap()
}

#[tokio::test]
async fn your_repertoire_is_built_from_your_games_and_off_while_you_play() {
    let h = harness(2).await;
    opening_game(&h, &["e4", "e5", "Nf3", "Nc6", "Bb5"], Side::White, Outcome::WhiteWins, 2, 1).await;
    opening_game(&h, &["e4", "e5", "Nf3", "Nc6", "Bc4"], Side::White, Outcome::Draw, 2, 2).await;
    opening_game(&h, &["e4", "e5", "Qh5", "Nc6"], Side::White, Outcome::BlackWins, 2, 3).await;
    opening_game(&h, &["d4", "d5"], Side::Black, Outcome::Draw, 2, 4).await;
    let profiles = Profiles::new(h.hub.clone());
    let req = RepertoireRequest { side: Side::White, max_plies: None, min_games: Some(1), limit: None };
    let white = profiles.repertoire(req).await.unwrap();
    assert_eq!(white.games, 3);
    assert_eq!(white.main_line()[..2], ["e2e4".to_owned(), "e7e5".to_owned()]);
    let root = &white.nodes.iter().find(|n| n.ply == 0).unwrap();
    assert_eq!(root.moves[0].games, 3);
    // Theory is the named opening's two plies: every game leaves it on move 2, by your hand.
    assert!(!white.departures.is_empty());
    assert!(white.departures.iter().all(|d| d.ply == 2 && d.by_you));

    let black = profiles.repertoire(RepertoireRequest { side: Side::Black, ..req }).await.unwrap();
    assert_eq!(black.games, 1);

    // Prep reads the same games into lines: the named opening, here two plies, with its moves from the newest game.
    let prep_req = PrepRequest { side: Side::White, limit: None };
    let prep = profiles.prep(prep_req).await.unwrap();
    assert_eq!((prep.games, prep.lines.len()), (3, 1));
    let line = &prep.lines[0];
    assert_eq!((line.name.as_str(), line.games, line.pgn.as_str()), ("King's Pawn Game", 3, "1. e4 e5"));
    assert_eq!((line.wins, line.draws, line.losses), (1, 1, 1));

    h.hub.policy.set_live_games(&[false]);
    assert_eq!(profiles.prep(prep_req).await.unwrap_err().status(), 409);
    assert_eq!(profiles.repertoire(req).await.unwrap_err().status(), 409);
    h.hub.policy.set_live_games(&[true]);
    assert!(profiles.repertoire(req).await.is_ok(), "correspondence allows the explorer, and this");
    h.hub.policy.set_live_games(&[]);
}

#[tokio::test]
async fn progress_has_your_lines() {
    let h = harness(2).await;
    let mate = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let drills = Drills::new(h.hub.clone());
    let reviews = Reviews::new(h.eval.clone()).with_drills(drills.clone());
    review(&h, &reviews, vec![mate]).await;
    let profiles = Profiles::new(h.hub.clone());
    let p = profiles.progress(None).await.unwrap();
    let metric = |player: PlayerKey, m: Metric| p.series.iter().find(|s| s.player == player && s.metric == m);
    assert_eq!(metric(PlayerKey::You, Metric::Rating).unwrap().points.len(), 1, "your seat's rating");
    assert_eq!(metric(PlayerKey::You, Metric::Accuracy).unwrap().points.len(), 1);
    assert!(metric(PlayerKey::You, Metric::DrillHitRate).is_none(), "no drills answered yet");
    assert!(p.series.iter().all(|s| s.player == PlayerKey::You));
}
