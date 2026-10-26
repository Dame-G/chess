//! Reviews without an engine: Lichess's accuracy against lila's own test values, the judgements and classifications
//! on hand-made scores, a brilliant sacrifice from recorded evaluations, the cache, the comparison, and importing
//! another player's games from a tiny Lichess.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, Position, fen::Fen};
use tokio_util::sync::CancellationToken;

use super::{judge::*, *};
use crate::{config::Config, lichess::me::import::to_match_as, matches::LichessAccount, store::Store};

fn close(a: f64, b: f64, delta: f64) -> bool {
    (a - b).abs() <= delta
}

// ---------------------------------------------------------------------------------------------------------------------
// Lichess's arithmetic
// ---------------------------------------------------------------------------------------------------------------------

/// lila's `AccuracyPercentTest`: White moves first, the start scored +15 (`evals.legacyInitial`).
fn compute(cps: &[i32], white_first: bool) -> (Option<f64>, Option<f64>) {
    let mut all = vec![15];
    all.extend_from_slice(cps);
    game_accuracy(white_first, &all)
}

#[test]
fn game_accuracy_matches_lilas_tests() {
    assert_eq!(compute(&[], true), (None, None));
    assert_eq!(compute(&[15], true).1, None);
    let check = |cps: Vec<i32>, white: f64, wd: f64, black: f64, bd: f64| {
        let (w, b) = compute(&cps, true);
        let (w, b) = (w.unwrap(), b.unwrap());
        assert!(close(w, white, wd) && close(b, black, bd), "{cps:?}: white {w}, black {b}");
    };
    check(vec![15, 15], 100.0, 1.0, 100.0, 1.0);
    check(vec![-900, -900], 10.0, 5.0, 100.0, 1.0);
    check(vec![15, 900], 100.0, 1.0, 10.0, 5.0);
    check(vec![-900, 0], 10.0, 5.0, 10.0, 5.0);
    check(vec![15; 20], 100.0, 1.0, 100.0, 1.0);
    check([vec![15; 20], vec![-900]].concat(), 50.0, 5.0, 100.0, 1.0);
    check([vec![15; 21], vec![900]].concat(), 100.0, 1.0, 50.0, 5.0);
    check([-50, 15].repeat(5), 76.0, 8.0, 76.0, 8.0);
    check([-50, 15].repeat(50), 76.0, 8.0, 76.0, 8.0);
    check([-135, 15].repeat(50), 54.0, 8.0, 54.0, 8.0);
    check([-435, 15].repeat(50), 20.0, 8.0, 20.0, 8.0);

    // Black moves first.
    let black_first = |cps: Vec<i32>, black: f64, bd: f64, white: f64, wd: f64| {
        let (w, b) = compute(&cps, false);
        let (w, b) = (w.unwrap(), b.unwrap());
        assert!(close(w, white, wd) && close(b, black, bd), "{cps:?}: white {w}, black {b}");
    };
    assert_eq!(compute(&[15], false).0, None);
    black_first(vec![15, 15], 100.0, 1.0, 100.0, 1.0);
    black_first(vec![900, 900], 10.0, 5.0, 100.0, 1.0);
    black_first(vec![15, -900], 100.0, 1.0, 10.0, 5.0);
    black_first(vec![900, 0], 10.0, 5.0, 10.0, 5.0);
}

#[test]
fn win_percent_and_move_accuracy_follow_lichess() {
    assert_eq!(win_percent(0), 50.0);
    assert!(close(win_percent(100), 59.10, 0.01), "{}", win_percent(100));
    assert!(close(win_percent(-100), 40.90, 0.01));
    // Capped at ±1000, and a mate is the cap.
    assert_eq!(win_percent(5000), win_percent(1000));
    assert_eq!(win_percent(force_cp(Score::Mate(3))), win_percent(1000));
    assert_eq!(win_percent(force_cp(Score::Mate(-3))), win_percent(-1000));
    assert_eq!(move_accuracy(50.0, 50.0), 100.0);
    assert_eq!(move_accuracy(50.0, 70.0), 100.0);
    assert!(close(move_accuracy(60.0, 50.0), 64.58, 0.01), "{}", move_accuracy(60.0, 50.0));
    assert_eq!(move_accuracy(100.0, 0.0), 0.0);
    assert_eq!(cp_loss(30, -2000), 1030);
    assert_eq!(cp_loss(30, 80), 0);
}

#[test]
fn scores_round_trip_and_mate_zero_is_the_mated_side() {
    for s in [Score::Cp(-37), Score::Mate(4), Score::Mate(-2), Score::Mate(0)] {
        assert_eq!(to_score(force_cp(s)), s);
    }
    assert_eq!(force_cp(Score::Mate(0)), -MATE);
    assert_eq!(to_score(MATE), Score::Mate(0));
    assert!(force_cp(Score::Mate(1)) > force_cp(Score::Mate(5)));
}

#[test]
fn judgements_follow_lilas_advice() {
    // Drops in winning chances of 0.18, 0.27 and 0.50.
    assert_eq!(judgement(0, -100), Some(Judgement::Inaccuracy));
    assert_eq!(judgement(0, -150), Some(Judgement::Mistake));
    assert_eq!(judgement(0, -300), Some(Judgement::Blunder));
    assert_eq!(judgement(0, -40), None);
    assert_eq!(judgement(-50, 200), None);
    // Walking into a mate: the worse off already, the milder.
    let mated = force_cp(Score::Mate(-3));
    assert_eq!(judgement(0, mated), Some(Judgement::Blunder));
    assert_eq!(judgement(-800, mated), Some(Judgement::Mistake));
    assert_eq!(judgement(-1200, mated), Some(Judgement::Inaccuracy));
    // Letting one's own mate go.
    let mating = force_cp(Score::Mate(2));
    assert_eq!(judgement(mating, 1200), Some(Judgement::Inaccuracy));
    assert_eq!(judgement(mating, 800), Some(Judgement::Mistake));
    assert_eq!(judgement(mating, 100), Some(Judgement::Blunder));
    assert_eq!(judgement(mating, mated), Some(Judgement::Blunder));
    // A slower mate is no judgement.
    assert_eq!(judgement(mating, force_cp(Score::Mate(6))), None);
}

#[test]
fn classifications_on_hand_made_scores() {
    let base = Facts { legal_moves: 30, ..Facts::default() };
    let c = |f: Facts| classify(&f);
    assert_eq!(c(Facts { before: 30, after: 30, is_best: true, ..base.clone() }), Classification::Best);
    assert_eq!(c(Facts { before: 30, after: 20, ..base.clone() }), Classification::Excellent);
    assert_eq!(c(Facts { before: 30, after: -20, ..base.clone() }), Classification::Good);
    let inaccurate = Facts { before: 30, after: -80, judgement: Some(Judgement::Inaccuracy), ..base.clone() };
    assert_eq!(c(inaccurate.clone()), Classification::Inaccuracy);
    assert_eq!(c(Facts { book: true, ..base.clone() }), Classification::Book);
    // Book never hides an inaccuracy.
    assert_eq!(c(Facts { book: true, ..inaccurate }), Classification::Inaccuracy);
    // Great: the only move, and it holds.
    let only = Facts { before: 0, after: 0, is_best: true, second: Some(-400), ..base.clone() };
    assert_eq!(c(only.clone()), Classification::Great);
    assert_eq!(c(Facts { recapture: true, ..only.clone() }), Classification::Best);
    assert_eq!(c(Facts { legal_moves: 1, ..only.clone() }), Classification::Best);
    assert_eq!(c(Facts { second: Some(-50), ..only.clone() }), Classification::Best);
    // An only move that still loses is no great move.
    assert_eq!(c(Facts { before: -500, after: -500, second: Some(-1500), ..only }), Classification::Best);
    // Brilliant: a sound sacrifice, not when already winning, not when it leaves the mover worse.
    let sac = Facts { before: 150, after: 150, is_best: true, sacrifice: 3, ..base.clone() };
    assert_eq!(c(sac.clone()), Classification::Brilliant);
    assert_eq!(c(Facts { sacrifice: 1, ..sac.clone() }), Classification::Best);
    assert_eq!(c(Facts { before: 900, after: 900, ..sac.clone() }), Classification::Best);
    assert_eq!(c(Facts { before: -100, after: -100, ..sac }), Classification::Best);
    // Miss: the opponent's blunder answered with a mistake that gives the chance back.
    let missed = Facts {
        before: 300,
        after: 60,
        judgement: Some(Judgement::Mistake),
        previous: Some(Judgement::Blunder),
        ..base.clone()
    };
    assert_eq!(c(missed.clone()), Classification::Miss);
    assert_eq!(c(Facts { previous: Some(Judgement::Inaccuracy), ..missed.clone() }), Classification::Mistake);
    // From winning to lost is a blunder, not a miss.
    assert_eq!(c(Facts { after: -600, judgement: Some(Judgement::Blunder), ..missed }), Classification::Blunder);
}

#[test]
fn phases_go_forward_only() {
    let start = Chess::default();
    assert_eq!(phase_of(&start, 0), Phase::Opening);
    assert_eq!(phase_of(&start, 30), Phase::Middlegame);
    let ending: Chess =
        "8/5k2/8/3R4/8/2B5/5K2/8 w - - 0 1".parse::<Fen>().unwrap().into_position(CastlingMode::Standard).unwrap();
    assert_eq!(phase_of(&ending, 10), Phase::Endgame);
    assert_eq!(phases(&[start.clone(), ending, start]), [Phase::Opening, Phase::Endgame, Phase::Endgame]);
}

// ---------------------------------------------------------------------------------------------------------------------
// A game judged from recorded evaluations
// ---------------------------------------------------------------------------------------------------------------------

/// A Lichess export of one game, as `GET /api/games/user/{name}` sends it.
fn exported(id: &str, created: i64, white: &str, black: &str, moves: &str, winner: Option<&str>) -> Value {
    json!({
        "id": id, "rated": true, "variant": "standard", "speed": "blitz", "createdAt": created,
        "lastMoveAt": created + 60_000, "status": if winner.is_some() { "resign" } else { "draw" }, "winner": winner,
        "players": {
            "white": { "user": { "name": white, "id": white.to_lowercase() }, "rating": 1800 },
            "black": { "user": { "name": black, "id": black.to_lowercase() }, "rating": 1700 }
        },
        "opening": { "eco": "C20", "name": "King's Pawn Game: Busch-Gass Gambit", "ply": 2 },
        "moves": moves,
        "clocks": vec![18000; moves.split_whitespace().count()],
        "clock": { "initial": 180, "increment": 0, "totalTime": 180 }
    })
}

fn line(uci: &str, cp: i32, pv: &[&str]) -> engine::Line {
    engine::Line { uci: uci.into(), score: Score::Cp(cp), pv: pv.iter().map(|s| (*s).to_owned()).collect() }
}

fn eval(lines: Vec<engine::Line>) -> Option<PositionEval> {
    Some(PositionEval { lines, depth: 12, nodes: 200_000 })
}

const GREEK_GIFT: &str = "r1bq1rk1/pppn1ppp/4p3/3pP3/1b1P4/2NB1N2/PPP2PPP/R1BQK2R w KQ - 0 1";

fn greek_gift() -> Match {
    let mut g = exported("greek001", 1_700_000_000_000, "Dame", "Bob", "Bxh7+", None);
    g["variant"] = json!("fromPosition");
    g["initialFen"] = json!(GREEK_GIFT);
    g["opening"] = Value::Null;
    g["status"] = json!("resign");
    g["winner"] = json!("white");
    to_match_as(&g, "dame", LichessAccount::Me).unwrap().unwrap()
}

#[test]
fn a_sound_bishop_sacrifice_is_brilliant() {
    let game = greek_gift();
    let positions = positions(&game).unwrap();
    let evals = vec![
        eval(vec![line("d3h7", 250, &["d3h7", "g8h7", "f3g5", "h7g8", "d1h5"]), line("e1g1", 40, &["e1g1"])]),
        eval(vec![line("g8h7", -240, &["g8h7", "f3g5", "h7g8", "d1h5"]), line("g8h8", -500, &["g8h8"])]),
    ];
    let (summary, plies) = judge_game(&game, &positions, &evals, 1, 200_000).unwrap();
    let p = &plies[0];
    assert_eq!(p.sacrifice, 2, "a bishop for a pawn");
    assert_eq!(p.classification, Classification::Brilliant);
    assert_eq!(p.best_san, "Bxh7+");
    assert_eq!(p.eval_before, Score::Cp(250));
    assert_eq!(p.eval_after, Score::Cp(240));
    assert_eq!(summary.white.counts.brilliant, 1);
    assert_eq!(summary.black.moves, 0);

    // The same move when the engine says it loses: a blunder, and the best move is named.
    let evals = vec![
        eval(vec![line("e1g1", 40, &["e1g1"]), line("d3h7", -200, &["d3h7", "g8h7"])]),
        eval(vec![line("g8h7", 200, &["g8h7", "f3g5", "h7g8"])]),
    ];
    let (summary, plies) = judge_game(&game, &positions, &evals, 1, 200_000).unwrap();
    assert_eq!(plies[0].classification, Classification::Blunder);
    assert_eq!(plies[0].judgement, Some(Judgement::Blunder));
    assert_eq!(plies[0].best_san, "O-O");
    assert_eq!(summary.white.judgements.blunder, 1);
    assert!(close(summary.white.accuracy.unwrap(), 38.7, 0.5), "{:?}", summary.white.accuracy);
}

#[test]
fn checkmate_ends_the_review_with_mate_zero() {
    let g = exported("mate0001", 1_700_000_000_000, "Dame", "Bob", "f3 e5 g4 Qh4#", Some("black"));
    let game = to_match_as(&g, "dame", LichessAccount::Me).unwrap().unwrap();
    let positions = positions(&game).unwrap();
    assert!(terminal(&positions[4]));
    let evals = vec![
        eval(vec![line("e2e4", 20, &["e2e4"])]),
        eval(vec![line("e7e5", -60, &["e7e5"])]),
        eval(vec![line("b1c3", -100, &["b1c3"])]),
        eval(vec![engine::Line { uci: "d8h4".into(), score: Score::Mate(1), pv: vec!["d8h4".into()] }]),
        None,
    ];
    let (summary, plies) = judge_game(&game, &positions, &evals, 1, 200_000).unwrap();
    assert_eq!(plies[3].eval_after, Score::Mate(0));
    assert_eq!(plies[3].classification, Classification::Best);
    assert_eq!(plies[3].win_after, round1(win_percent(MATE)));
    // g4 walks into mate in one.
    assert_eq!(plies[2].judgement, Some(Judgement::Blunder));
    assert_eq!(summary.white.judgements.blunder, 1);
    // The first two plies are the named opening.
    assert_eq!(plies[0].classification, Classification::Book);
    assert_eq!(plies[1].classification, Classification::Book);
}

// ---------------------------------------------------------------------------------------------------------------------
// Jobs, the cache and the comparison, against a store
// ---------------------------------------------------------------------------------------------------------------------

fn runner(lichess_url: String) -> Arc<AppState> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).unwrap();
    let data = std::env::temp_dir().join(format!("chess-review-test-{}", u64::from_le_bytes(bytes)));
    std::fs::create_dir_all(&data).unwrap();
    let config = Config {
        data,
        reckless: ".".into(),
        listen: "127.0.0.1:7878".parse().unwrap(),
        token: "runner-token".into(),
        lichess_url,
    };
    Arc::new(AppState::new(config, Store::memory().unwrap(), crate::lichess::engines()))
}

async fn with_build(state: &AppState) {
    state
        .store
        .call(|c| {
            c.execute(
                "INSERT INTO builds (key, ref, commit_sha, subject, features, bench, nps)
                 VALUES ('k', 'main', 'abcdef0123', 's', 'no-syzygy', 1, 1)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
}

/// An engine that knows nothing: the first legal move is best at +0.10, the second next at 0.00. It counts searches.
struct Fake {
    searches: Arc<AtomicUsize>,
}

impl Analyser for Fake {
    fn analyse<'a>(&'a mut self, fen: &'a str) -> futures::future::BoxFuture<'a, anyhow::Result<PositionEval>> {
        Box::pin(async move {
            self.searches.fetch_add(1, Ordering::SeqCst);
            let pos: Chess = fen.parse::<Fen>()?.into_position(CastlingMode::Standard)?;
            let moves: Vec<String> =
                pos.legal_moves().iter().map(|m| m.to_uci(CastlingMode::Standard).to_string()).collect();
            let mut lines = vec![line(&moves[0], 10, &[&moves[0]])];
            if let Some(m) = moves.get(1) {
                lines.push(line(m, 0, &[m]));
            }
            Ok(PositionEval { lines, depth: 1, nodes: 1 })
        })
    }
}

fn fake(searches: Arc<AtomicUsize>) -> Factory {
    Arc::new(move || {
        let searches = searches.clone();
        Box::pin(async move { Ok(Box::new(Fake { searches }) as Box<dyn Analyser>) })
    })
}

async fn import(state: &AppState, games: Vec<Value>, who: &str, account: LichessAccount) -> Vec<i64> {
    let ms: Vec<Match> = games.iter().map(|g| to_match_as(g, who, account).unwrap().unwrap()).collect();
    matches::import(state, ms).await.unwrap();
    let player = (account == LichessAccount::Player).then(|| who.to_owned());
    let account = (account != LichessAccount::Player).then_some(account);
    let mut ids: Vec<i64> =
        matches::list(state, 100, None, account, player).await.unwrap().into_iter().map(|m| m.id).collect();
    ids.sort_unstable();
    ids
}

#[tokio::test]
async fn positions_are_searched_once_and_a_second_review_is_a_no_op() {
    let state = runner(String::new());
    with_build(&state).await;
    let ids = import(
        &state,
        vec![
            exported("cache001", 1_700_000_000_000, "Dame", "Bob", "e4 e5 Nf3 Nc6", Some("white")),
            exported("cache002", 1_700_000_100_000, "Bob", "Dame", "e4 e5 Nf3 Nf6", None),
        ],
        "dame",
        LichessAccount::Me,
    )
    .await;
    let searches = Arc::new(AtomicUsize::new(0));
    let job = ReviewJob { matches: ids.clone(), build: 1, nodes: 1000, concurrency: 2 };
    let cancel = CancellationToken::new();
    let result = run_with(&state, 0, &cancel, &|_| {}, &job, fake(searches.clone())).await.unwrap();
    // Five positions each, the first four shared.
    assert_eq!(searches.load(Ordering::SeqCst), 6);
    assert_eq!(result["reviewed"], 2);
    assert_eq!(result["positions_cached"], 4);

    let again = run_with(&state, 0, &cancel, &|_| {}, &job, fake(searches.clone())).await.unwrap();
    assert_eq!(again["skipped"], 2);
    assert_eq!(searches.load(Ordering::SeqCst), 6);

    // Another budget searches again; the review is stored beside the first.
    let deeper = ReviewJob { nodes: 2000, ..job };
    run_with(&state, 0, &cancel, &|_| {}, &deeper, fake(searches.clone())).await.unwrap();
    assert_eq!(searches.load(Ordering::SeqCst), 12);
    let newest = super::get(&state, ids[0], None, None).await.unwrap().unwrap();
    assert_eq!(newest.nodes, 2000);
    assert_eq!(super::get(&state, ids[0], None, Some(1000)).await.unwrap().unwrap().nodes, 1000);
    assert_eq!(newest.plies.len(), 4);
    assert_eq!(newest.plies[0].classification, Classification::Book);

    // Lists carry the newest review.
    let list = matches::list(&state, 10, None, Some(LichessAccount::Me), None).await.unwrap();
    assert!(list.iter().all(|m| m.review.as_ref().is_some_and(|r| r.nodes == 2000)));
}

#[tokio::test]
async fn a_review_waits_while_you_play_on_lichess() {
    let state = runner(String::new());
    with_build(&state).await;
    state.me.set_playing_for_test(true);
    let req = ReviewRequest { account: Some("me".into()), ..ReviewRequest::default() };
    let refused = submit(&state, req.clone()).await.unwrap_err();
    assert_eq!(refused.status, StatusCode::CONFLICT);

    // One already running pauses until the game ends.
    let ids = import(
        &state,
        vec![exported("pause001", 1_700_000_000_000, "Dame", "Bob", "e4 e5", None)],
        "dame",
        LichessAccount::Me,
    )
    .await;
    let searches = Arc::new(AtomicUsize::new(0));
    let job = ReviewJob { matches: ids, build: 1, nodes: 1000, concurrency: 1 };
    let (state2, s2) = (state.clone(), searches.clone());
    let running =
        tokio::spawn(
            async move { run_with(&state2, 0, &CancellationToken::new(), &|_| {}, &job, fake(s2)).await.unwrap() },
        );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(searches.load(Ordering::SeqCst), 0);
    assert!(!running.is_finished());
    state.me.set_playing_for_test(false);
    let result = tokio::time::timeout(Duration::from_secs(10), running).await.unwrap().unwrap();
    assert_eq!(result["reviewed"], 1);
    assert_eq!(searches.load(Ordering::SeqCst), 3);

    // And with the lock off, the request is queued.
    let job = submit(&state, req).await.unwrap();
    assert!(matches!(job.spec, JobSpec::Review { .. }));
}

#[tokio::test]
async fn the_comparison_counts_results_for_all_games_and_engine_figures_for_reviewed_ones() {
    let state = runner(String::new());
    with_build(&state).await;
    let mine = import(
        &state,
        vec![
            exported("cmp00001", 1_700_000_000_000, "Dame", "Bob", "e4 e5 Nf3 Nc6", Some("white")),
            exported("cmp00002", 1_700_000_100_000, "Bob", "Dame", "d4 d5", Some("black")),
            exported("cmp00003", 1_700_000_200_000, "Dame", "Carl", "c4 e5", Some("black")),
        ],
        "dame",
        LichessAccount::Me,
    )
    .await;
    let theirs = import(
        &state,
        vec![exported("cmp00004", 1_700_000_300_000, "Alice", "Bob", "e4 c5", None)],
        "alice",
        LichessAccount::Player,
    )
    .await;
    // Review two of mine.
    let job = ReviewJob { matches: mine[..2].to_vec(), build: 1, nodes: 1000, concurrency: 1 };
    run_with(&state, 0, &CancellationToken::new(), &|_| {}, &job, fake(Arc::default())).await.unwrap();

    let (me, alice) = state
        .store
        .call(|c| {
            let me = compare::games_of(c, &Who::Me, &[], None, None, 100)?;
            let alice = compare::games_of(c, &Who::parse("Alice"), &[], None, None, 100)?;
            Ok((compare::aggregate("me", &me), compare::aggregate("alice", &alice)))
        })
        .await
        .unwrap();
    assert_eq!((me.games, me.reviewed, me.pending_review), (3, 2, 1));
    assert_eq!((me.results.overall.wins, me.results.overall.draws, me.results.overall.losses), (2, 0, 1));
    assert_eq!(me.results.white.wins, 1);
    assert_eq!(me.results.black.wins, 1);
    assert_eq!(me.longest_win_streak, 2);
    assert_eq!(me.username.as_deref(), Some("Dame"));
    assert!(me.accuracy.overall.is_some());
    assert_eq!(me.openings[0].name, "King's Pawn Game");
    assert_eq!(me.openings[0].results.games, 3);
    // Opponents averaged 1733.3 (Bob 1700, Bob 1800, Carl 1700); +1 net over 3 games.
    assert_eq!(me.opponents.average_rating, Some(1733.3));
    assert_eq!(me.opponents.performance, Some(round1(1733.333 + 400.0 / 3.0)));
    let total: u32 = Classification::ALL.iter().map(|c| me.counts.get(*c)).sum();
    assert_eq!(total, 3, "my moves in the two reviewed games");
    assert_eq!((alice.games, alice.reviewed, alice.results.overall.draws), (1, 0, 1));
    assert_eq!(theirs.len(), 1);
    // Speeds and dates filter.
    let none = state.store.call(|c| compare::games_of(c, &Who::Me, &["rapid".into()], None, None, 100)).await.unwrap();
    assert!(none.is_empty());
    let late = state
        .store
        .call(|c| compare::games_of(c, &Who::Me, &[], Some(true), Some("2023-11-14T22:15:00"), 100))
        .await
        .unwrap();
    assert_eq!(late.len(), 2);
}

#[test]
fn opening_families_drop_the_code_and_the_variation() {
    assert_eq!(compare::opening_family("C20 King's Pawn Game: Busch-Gass Gambit"), "King's Pawn Game");
    assert_eq!(compare::opening_family("B01 Scandinavian Defense"), "Scandinavian Defense");
    assert_eq!(compare::opening_family("Englund Gambit"), "Englund Gambit");
}

// ---------------------------------------------------------------------------------------------------------------------
// Other players, from a tiny Lichess
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Mock {
    exports: std::sync::Mutex<Vec<String>>,
}

async fn export(
    State(m): State<Arc<Mock>>,
    Path(name): Path<String>,
    uri: axum::http::Uri,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    m.exports.lock().unwrap().push(uri.query().unwrap_or_default().to_owned());
    if name != "alice" {
        return (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "Not found" }))).into_response();
    }
    [
        exported("plr00001", 1_700_000_000_000, "Alice", "Bob", "e4 e5", Some("white")),
        exported("plr00002", 1_700_000_100_000, "Carl", "Alice", "d4 d5", None),
    ]
    .iter()
    .map(|g| format!("{g}\n"))
    .collect::<String>()
    .into_response()
}

async fn top() -> axum::Json<Value> {
    axum::Json(json!({ "users": [
        { "id": "cutemouse83", "username": "cutemouse83", "perfs": { "blitz": { "rating": 2988, "progress": 6 } }, "title": "GM" },
        { "id": "tarzansmurf", "username": "tarzansmurf", "perfs": { "blitz": { "rating": 2966, "progress": 56 } } }
    ]}))
}

async fn user(Path(name): Path<String>) -> axum::response::Response {
    use axum::response::IntoResponse;
    if name != "alice" {
        return (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "Not found" }))).into_response();
    }
    axum::Json(json!({
        "id": "alice", "username": "Alice", "title": "FM", "createdAt": 1_576_514_685_030i64,
        "perfs": { "blitz": { "games": 19608, "rating": 2988, "rd": 46, "prog": 6 },
                   "rapid": { "games": 0, "rating": 1500, "rd": 500, "prog": 0, "prov": true } },
        "count": { "all": 19660, "rated": 19616, "draw": 3129, "loss": 4250, "win": 12281 },
        "profile": { "flag": "IL" }
    }))
    .into_response()
}

async fn mock() -> (String, Arc<Mock>) {
    let m = Arc::new(Mock::default());
    let app = Router::new()
        .route("/api/games/user/{name}", get(export))
        .route("/api/player/top/{nb}/{perf}", get(top))
        .route("/api/user/{name}", get(user))
        .with_state(m.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, m)
}

#[tokio::test]
async fn another_players_games_import_once_and_stay_out_of_your_lists() {
    let (url, mock) = mock().await;
    let state = runner(url);
    let req = crate::lichess::players::PlayerImportRequest {
        max: Some(10),
        speeds: vec!["blitz".into(), "rapid".into()],
        rated: Some(true),
    };
    let cancel = CancellationToken::new();
    let first = crate::lichess::players::run_import(&state, &cancel, &|_| {}, "alice", &req).await.unwrap();
    assert_eq!(first["imported"], 2);
    let second = crate::lichess::players::run_import(&state, &cancel, &|_| {}, "alice", &req).await.unwrap();
    assert_eq!((second["imported"].as_u64(), second["skipped"].as_u64()), (Some(0), Some(2)));
    let query = mock.exports.lock().unwrap()[0].clone();
    assert!(
        query.contains("max=10") && query.contains("perfType=blitz,rapid") && query.contains("rated=true"),
        "{query}"
    );

    let theirs = matches::list(&state, 10, None, None, Some("Alice".into())).await.unwrap();
    assert_eq!(theirs.len(), 2);
    let l = theirs[0].lichess.as_ref().unwrap();
    assert_eq!((l.account, l.player.as_deref(), l.opening_ply), (LichessAccount::Player, Some("alice"), Some(2)));
    // Alice is Black in the newest game; nobody is "you".
    assert_eq!(l.our_side, Side::Black);
    assert!(!theirs[0].white.human && !theirs[0].black.human);
    assert!(matches::list(&state, 10, None, None, None).await.unwrap().is_empty());
    assert!(matches::list(&state, 10, None, Some(LichessAccount::Me), None).await.unwrap().is_empty());
    assert!(matches::list(&state, 10, None, Some(LichessAccount::Bot), None).await.unwrap().is_empty());

    let missing = crate::lichess::players::run_import(&state, &cancel, &|_| {}, "nobody", &req).await;
    assert!(missing.unwrap_err().to_string().contains("no player"));

    // Profiles and the leaderboard.
    let p = crate::lichess::players::profile(&state, "Alice").await.unwrap();
    assert_eq!((p.title.as_deref(), p.imported, p.reviewed), (Some("FM"), 2, 0));
    assert_eq!(p.ratings.blitz.as_ref().unwrap().rating, 2988);
    assert!(p.ratings.rapid.as_ref().unwrap().provisional);
    assert_eq!(p.counts.wins, 12281);
    assert_eq!(crate::lichess::players::profile(&state, "nobody").await.unwrap_err().status, StatusCode::NOT_FOUND);
    let top = crate::lichess::players::top(&state, "blitz", 2).await.unwrap();
    assert_eq!((top[0].username.as_str(), top[0].rating, top[0].title.as_deref()), ("cutemouse83", 2988, Some("GM")));
    assert_eq!(crate::lichess::players::top(&state, "chess960", 2).await.unwrap_err().status, StatusCode::BAD_REQUEST);
}

/// A real game reviewed by a real build, to measure the cost: `REVIEW_ENGINE` (a build's binary), `REVIEW_DB` (a
/// runner database, opened read-only), `REVIEW_MATCH` (default: your newest game), `REVIEW_NODES`, `REVIEW_ENGINES`.
#[tokio::test]
#[ignore = "needs an engine binary and a runner database"]
async fn review_a_real_game() {
    let env = |k: &str| std::env::var(k).ok();
    let binary = std::path::PathBuf::from(env("REVIEW_ENGINE").expect("REVIEW_ENGINE"));
    let db = rusqlite::Connection::open_with_flags(
        env("REVIEW_DB").expect("REVIEW_DB"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let nodes: u64 = env("REVIEW_NODES").map_or(DEFAULT_NODES, |n| n.parse().unwrap());
    let engines: usize = env("REVIEW_ENGINES").map_or(3, |n| n.parse().unwrap());
    let data: String = match env("REVIEW_MATCH") {
        Some(id) => db.query_row("SELECT data FROM matches WHERE id = ?1", [id], |r| r.get(0)).unwrap(),
        None => db
            .query_row(
                "SELECT data FROM matches WHERE json_extract(data, '$.lichess.account') = 'me' AND status = 'finished'
                 ORDER BY created_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap(),
    };
    let game: Match = serde_json::from_str(&data).unwrap();
    let positions = positions(&game).unwrap();
    let fens: Vec<String> = positions.iter().filter(|p| !terminal(p)).map(fen).collect();
    let started = std::time::Instant::now();
    let gate: engine::Gate = Arc::new(|| false);
    let pool = Pool::start(uci_factory(binary, nodes), engines, gate).await.unwrap();
    let found = pool.analyse_all(&fens).await.unwrap();
    let took = started.elapsed();
    let mut by_fen: std::collections::HashMap<String, PositionEval> = fens.into_iter().zip(found).collect();
    let evals: Vec<Option<PositionEval>> =
        positions.iter().map(|p| if terminal(p) { None } else { by_fen.remove(&fen(p)) }).collect();
    let (summary, plies) = judge_game(&game, &positions, &evals, 0, nodes).unwrap();
    println!(
        "match {} ({} plies): {} positions at {nodes} nodes on {engines} engines in {:.1} s",
        game.id,
        plies.len(),
        evals.iter().flatten().count(),
        took.as_secs_f64()
    );
    println!("white {:?}\nblack {:?}", summary.white, summary.black);
    for p in &plies {
        println!(
            "{:>3} {:<8} {:?} {:.1}->{:.1} best {} d{}",
            p.ply, p.san, p.classification, p.win_before, p.win_after, p.best_san, p.depth
        );
    }
}
