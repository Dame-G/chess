//! Reviews without an engine: Lichess's accuracy against lila's own test values, the judgements and classifications
//! on hand-made scores, a brilliant sacrifice and a mate from recorded evaluations.

use super::{judge::*, *};

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
    // The score module's win% agrees.
    assert_eq!(crate::score::win_percent(100), win_percent(100));
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
        ..base
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
    let ending = Position::from_fen("8/5k2/8/3R4/8/2B5/5K2/8 w - - 0 1").unwrap().inner().clone();
    assert_eq!(phase_of(&ending, 10), Phase::Endgame);
    assert_eq!(phases(&[start.clone(), ending, start]), [Phase::Opening, Phase::Endgame, Phase::Endgame]);
}

// ---------------------------------------------------------------------------------------------------------------------
// A game judged from recorded evaluations
// ---------------------------------------------------------------------------------------------------------------------

/// A line with White's score.
fn line(uci: &str, cp: i32, pv: &[&str]) -> EvalLine {
    EvalLine { uci: uci.into(), score: Score::Cp(cp), pv: pv.iter().map(|s| (*s).to_owned()).collect() }
}

fn eval(lines: Vec<EvalLine>) -> Option<PositionEval> {
    Some(PositionEval { lines, depth: 12, nodes: 200_000 })
}

const GREEK_GIFT: &str = "r1bq1rk1/pppn1ppp/4p3/3pP3/1b1P4/2NB1N2/PPP2PPP/R1BQK2R w KQ - 0 1";

fn blitz() -> TimeControl {
    TimeControl::clock(180_000, 0)
}

#[test]
fn a_sound_bishop_sacrifice_is_brilliant() {
    let moves = [Move::new("d3h7", "Bxh7+")];
    let input = ReviewInput { start_fen: Some(GREEK_GIFT), moves: &moves, time: blitz(), opening_ply: None };
    let evals = vec![
        eval(vec![line("d3h7", 250, &["d3h7", "g8h7", "f3g5", "h7g8", "d1h5"]), line("e1g1", 40, &["e1g1"])]),
        // Black to move, but scores are White's: +2.40 after the sacrifice.
        eval(vec![line("g8h7", 240, &["g8h7", "f3g5", "h7g8", "d1h5"]), line("g8h8", 500, &["g8h8"])]),
    ];
    let (summary, plies) = judge_game(input, &evals, VersionId(1), 200_000).unwrap();
    let p = &plies[0];
    assert_eq!(p.sacrifice, 2, "a bishop for a pawn");
    assert_eq!(p.classification, Classification::Brilliant);
    assert_eq!(p.best_san, "Bxh7+");
    assert_eq!(p.eval_before, Score::Cp(250));
    assert_eq!(p.eval_after, Score::Cp(240));
    assert_eq!(p.eval_second, Some(Score::Cp(40)));
    assert_eq!(summary.white.counts.brilliant, 1);
    assert_eq!(summary.black.moves, 0);

    // The same move when the engine says it loses: a blunder, and the best move is named.
    let evals = vec![
        eval(vec![line("e1g1", 40, &["e1g1"]), line("d3h7", -200, &["d3h7", "g8h7"])]),
        eval(vec![line("g8h7", -200, &["g8h7", "f3g5", "h7g8"])]),
    ];
    let (summary, plies) = judge_game(input, &evals, VersionId(1), 200_000).unwrap();
    assert_eq!(plies[0].classification, Classification::Blunder);
    assert_eq!(plies[0].judgement, Some(Judgement::Blunder));
    assert_eq!(plies[0].best_san, "O-O");
    assert_eq!(summary.white.judgements.blunder, 1);
    assert!(close(summary.white.accuracy.unwrap(), 38.7, 0.5), "{:?}", summary.white.accuracy);
}

#[test]
fn checkmate_ends_the_review_with_mate_zero() {
    let mut moves: Vec<Move> =
        [("f2f3", "f3"), ("e7e5", "e5"), ("g2g4", "g4"), ("d8h4", "Qh4#")].map(|(u, s)| Move::new(u, s)).to_vec();
    for m in &mut moves {
        m.clock_ms = Some(170_000);
    }
    // White had ten seconds left when it walked into mate: a blunder in time trouble.
    moves[0].clock_ms = Some(10_000);
    let input = ReviewInput { start_fen: None, moves: &moves, time: blitz(), opening_ply: Some(2) };
    let searched = to_search(input).unwrap();
    assert_eq!(searched.len(), 5);
    assert!(searched[4].terminal && !searched[3].terminal);
    assert_eq!(searched[0].key, "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -");
    let evals = vec![
        eval(vec![line("e2e4", 20, &["e2e4"])]),
        eval(vec![line("e7e5", 60, &["e7e5"])]),
        eval(vec![line("b1c3", -100, &["b1c3"])]),
        // Black mates in one: White's view is mated in one.
        eval(vec![EvalLine { uci: "d8h4".into(), score: Score::Mate(-1), pv: vec!["d8h4".into()] }]),
        None,
    ];
    let (summary, plies) = judge_game(input, &evals, VersionId(1), 200_000).unwrap();
    assert_eq!(plies[3].eval_after, Score::Mate(0));
    assert_eq!(plies[3].eval_before, Score::Mate(-1));
    assert_eq!(plies[3].classification, Classification::Best);
    assert_eq!(plies[3].win_after, round1(win_percent(MATE)));
    // g4 walks into mate in one.
    assert_eq!(plies[2].judgement, Some(Judgement::Blunder));
    assert_eq!(plies[2].clock_before_ms, Some(10_000));
    assert_eq!(summary.white.judgements.blunder, 1);
    assert_eq!(summary.white.time_trouble_blunders, 1);
    // The first two plies are the named opening.
    assert_eq!(plies[0].classification, Classification::Book);
    assert_eq!(plies[1].classification, Classification::Book);
    // Think times come from the clocks, from each side's second move on (Lichess times no first move).
    assert_eq!(summary.black.phases.opening.timed_moves, 1);
    assert_eq!(summary.white.phases.opening.timed_moves, 1);
    assert_eq!(summary.accuracy().white.is_some(), summary.accuracy_pair().is_some());
}

#[test]
fn a_review_needs_every_position() {
    let moves = [Move::new("e2e4", "e4")];
    let input = ReviewInput { start_fen: None, moves: &moves, time: blitz(), opening_ply: None };
    assert!(judge_game(input, &[eval(vec![line("e2e4", 20, &["e2e4"])])], VersionId(1), 1).is_err());
    assert!(judge_game(input, &[None, None], VersionId(1), 1).is_err());
    let bad = [Move::new("e2e5", "e5")];
    assert!(to_search(ReviewInput { moves: &bad, ..input }).is_err());
}
