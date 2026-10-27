//! The arithmetic of a review, as plain functions over plain numbers, so it is tested without an engine.
//!
//! - Win percentages, move accuracy, game accuracy and ACPL follow Lichess exactly (lila's `AccuracyPercent.scala`,
//!   `AccuracyCP.scala`, scalachess's `WinPercent`, scalalib's `Maths`).
//! - Inaccuracy, mistake and blunder are Lichess's judgements (lila's `Advice.scala`).
//! - The other classifications (brilliant, great, best, excellent, good, book, miss) are ours, written out in the old
//!   harness's `AGENTS.md` ("Game review and comparison") and carried over unchanged, with chess.com's published
//!   descriptions as the model.
//!
//! A score here is one `i32` from one side's view: centipawns, or a mate as `±(MATE - moves)`, so a mate in 2 for that
//! side is `MATE - 2` and being mated now is `-MATE`. Flipping the view is negation.

use serde::{Deserialize, Serialize};
use shakmaty::{Bitboard, Chess, Color, Move, Position, Rank, uci::UciMove};
use utoipa::ToSchema;

use crate::{chess::balance, score::Score};

/// A forced mate, as a score: `MATE - n` is mate in `n` for the side whose view it is.
pub const MATE: i32 = 100_000;
/// Scores beyond this are mates.
const MATE_BOUND: i32 = MATE - 1000;
/// Lichess caps centipawns at ±1000 before turning them into win percentages and ACPL; a mate counts as the cap.
pub const CEILING: i32 = 1000;

/// A UCI score as one number from the same side's view.
pub fn force_cp(score: Score) -> i32 {
    match score {
        Score::Cp(cp) => cp.clamp(-MATE_BOUND, MATE_BOUND),
        Score::Mate(n) if n > 0 => MATE - n.min(999),
        // Mate 0 (or less) is the side to move mated, now or in -n moves.
        Score::Mate(n) => -MATE - n.max(-999),
    }
}

pub fn is_mate(v: i32) -> bool {
    v.abs() > MATE_BOUND
}

/// Back to a UCI score. A position whose side to move is mated is `Mate(0)` from either view.
pub fn to_score(v: i32) -> Score {
    if v > MATE_BOUND {
        Score::Mate(MATE - v)
    } else if v < -MATE_BOUND {
        Score::Mate(-(MATE + v))
    } else {
        Score::Cp(v)
    }
}

/// Lichess's winning chances in [-1, 1] (lila PR #11148): `2 / (1 + e^(-0.00368208 × cp)) - 1`.
pub fn winning_chances(cp: i32) -> f64 {
    let cp = if is_mate(cp) { cp.signum() * CEILING } else { cp };
    (2.0 / (1.0 + (-0.003_682_08 * f64::from(cp)).exp()) - 1.0).clamp(-1.0, 1.0)
}

/// The win percentage, 0 to 100, of the side whose view `v` is: `50 + 50 × winning_chances`, centipawns capped at
/// ±1000 (a mate is the cap).
pub fn win_percent(v: i32) -> f64 {
    50.0 + 50.0 * winning_chances(v.clamp(-CEILING, CEILING))
}

/// Lichess's accuracy of one move from the mover's win percentage before and after it: 100 when it did not drop, else
/// `103.1668 × e^(-0.04354 × drop) - 3.1669 + 1`, kept within 0 and 100.
pub fn move_accuracy(before: f64, after: f64) -> f64 {
    if after >= before {
        return 100.0;
    }
    let drop = before - after;
    (103.166_810_071_164_9 * (-0.043_544_153_867_539_51 * drop).exp() - 3.166_924_740_191_411 + 1.0).clamp(0.0, 100.0)
}

fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

/// The population standard deviation, as scalalib's `Maths.standardDeviation`.
fn standard_deviation(xs: &[f64]) -> f64 {
    let Some(m) = mean(xs) else { return 0.0 };
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / xs.len() as f64).sqrt()
}

/// scalalib's `Maths.harmonicMean`, which counts values under 1 as 1.
fn harmonic_mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.len() as f64 / xs.iter().map(|v| 1.0 / v.max(1.0)).sum::<f64>())
}

fn weighted_mean(xs: &[(f64, f64)]) -> Option<f64> {
    let w: f64 = xs.iter().map(|(_, w)| w).sum();
    (w != 0.0).then(|| xs.iter().map(|(v, w)| v * w).sum::<f64>() / w)
}

/// Lichess's game accuracy of each side, `(white, black)`: the mean of a volatility-weighted mean and the harmonic
/// mean of its move accuracies. `cps` are White's scores of every position, the start included; `white_first` says
/// who made the first move. A side without a move has none.
///
/// Each move is weighted by the standard deviation of the win percentages in a window around it (between 0.5 and 12),
/// the window being a tenth of the game, at least 2 and at most 8 positions; the first windows repeat the first one.
pub fn game_accuracy(white_first: bool, cps: &[i32]) -> (Option<f64>, Option<f64>) {
    if cps.len() < 2 {
        return (None, None);
    }
    let wins: Vec<f64> = cps.iter().map(|&cp| win_percent(cp)).collect();
    let size = ((cps.len() - 1) / 10).clamp(2, 8);
    let first = size.min(wins.len());
    let mut windows: Vec<&[f64]> = vec![&wins[..first]; first - 2];
    if wins.len() <= size {
        windows.push(&wins[..]);
    } else {
        windows.extend(wins.windows(size));
    }
    let mut sides: [Vec<(f64, f64)>; 2] = [Vec::new(), Vec::new()];
    for (i, (pair, window)) in wins.windows(2).zip(windows).enumerate() {
        let white = (i % 2 == 0) == white_first;
        let weight = standard_deviation(window).clamp(0.5, 12.0);
        let accuracy = if white { move_accuracy(pair[0], pair[1]) } else { move_accuracy(pair[1], pair[0]) };
        sides[usize::from(!white)].push((accuracy, weight));
    }
    let side = |moves: &[(f64, f64)]| {
        let weighted = weighted_mean(moves)?;
        let harmonic = harmonic_mean(&moves.iter().map(|(a, _)| *a).collect::<Vec<_>>())?;
        Some((weighted + harmonic) / 2.0)
    };
    (side(&sides[0]), side(&sides[1]))
}

/// Centipawns lost by a move, from the mover's scores before and after it, each capped at ±1000 (Lichess's ACPL).
pub fn cp_loss(before: i32, after: i32) -> u32 {
    (before.clamp(-CEILING, CEILING) - after.clamp(-CEILING, CEILING)).max(0) as u32
}

/// Lichess's judgement of a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Judgement {
    Inaccuracy,
    Mistake,
    Blunder,
}

/// Lichess's judgement of a move that is not the engine's best, from the mover's scores before (the best line) and
/// after it:
///
/// - between two centipawn scores, by the drop in winning chances: 0.3 or more a blunder, 0.2 a mistake, 0.1 an
///   inaccuracy (15, 10 and 5 points of win percentage);
/// - walking into a forced mate: an inaccuracy if the mover was already 10 pawns down, a mistake if 7, else a blunder;
/// - letting a forced mate of one's own go: an inaccuracy if still 10 pawns up after, a mistake if 7, else a blunder.
pub fn judgement(before: i32, after: i32) -> Option<Judgement> {
    match (is_mate(before), is_mate(after)) {
        (false, false) => {
            let drop = winning_chances(before) - winning_chances(after);
            if drop >= 0.3 {
                Some(Judgement::Blunder)
            } else if drop >= 0.2 {
                Some(Judgement::Mistake)
            } else if drop >= 0.1 {
                Some(Judgement::Inaccuracy)
            } else {
                None
            }
        }
        // Mate created: the mover is now being mated.
        (false, true) if after < 0 => Some(if before < -999 {
            Judgement::Inaccuracy
        } else if before < -700 {
            Judgement::Mistake
        } else {
            Judgement::Blunder
        }),
        // Mate lost: the mover had a mate and now has none (or is mated).
        (true, _) if before > 0 && (after < 0 || !is_mate(after)) => {
            let cp = if is_mate(after) { 0 } else { after };
            Some(if cp > 999 {
                Judgement::Inaccuracy
            } else if cp > 700 {
                Judgement::Mistake
            } else {
                Judgement::Blunder
            })
        }
        _ => None,
    }
}

/// What a review calls a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Classification {
    /// A sound sacrifice: the best move (or within 2 points of win percentage), giving up at least 2 points of
    /// material along the engine's best reply, while not already winning and staying at least level.
    Brilliant,
    /// The only good move: every other move is at least 15 points of win percentage worse, and it keeps the mover at
    /// 40 or better. Not a recapture, not the only legal move.
    Great,
    /// The engine's best move.
    Best,
    /// Within 2 points of win percentage of the best.
    Excellent,
    /// Worse than that, but not an inaccuracy.
    Good,
    /// A move of the opening as Lichess names it (`opening_ply`), and not an inaccuracy or worse.
    Book,
    /// Lichess's inaccuracy.
    Inaccuracy,
    /// Lichess's mistake.
    Mistake,
    /// Lichess's blunder.
    Blunder,
    /// A mistake or blunder right after the opponent's mistake or blunder, from a position of 60 or better to one of
    /// 30 or better: the chance was let go rather than the game lost.
    Miss,
}

impl Classification {
    pub const ALL: [Self; 10] = [
        Self::Brilliant,
        Self::Great,
        Self::Best,
        Self::Excellent,
        Self::Good,
        Self::Book,
        Self::Inaccuracy,
        Self::Mistake,
        Self::Blunder,
        Self::Miss,
    ];
}

/// Win percentage within which a move counts as as good as the best (excellent, and a candidate for brilliant or great).
pub const EXCELLENT_DROP: f64 = 2.0;
/// Material, in pawns (N and B 3, R 5, Q 9), a brilliant move gives up along the engine's best reply: an exchange, or a
/// piece for a pawn.
pub const SACRIFICE: i32 = 2;
/// A brilliant move is not played from a position already this good (about +4.7): a sacrifice when winning anyway
/// does not count.
pub const BRILLIANT_BEFORE_MAX: f64 = 85.0;
/// ...and leaves the mover at least level.
pub const BRILLIANT_AFTER_MIN: f64 = 50.0;
/// A great move's second-best alternative is at least this much worse (a blunder's worth).
pub const GREAT_GAP: f64 = 15.0;
/// ...and the great move keeps the mover at this or better.
pub const GREAT_AFTER_MIN: f64 = 40.0;
/// A miss is played from at least this...
pub const MISS_BEFORE_MIN: f64 = 60.0;
/// ...and leaves the mover at this or better (worse is a plain mistake or blunder).
pub const MISS_AFTER_MIN: f64 = 30.0;

/// What [`classify`] needs to know about a move. Scores are the mover's.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The engine's best score before the move.
    pub before: i32,
    /// The engine's best score after it.
    pub after: i32,
    /// The second-best line's score before the move, when there was one.
    pub second: Option<i32>,
    /// Whether the move is the engine's best.
    pub is_best: bool,
    /// How many legal moves the mover had.
    pub legal_moves: usize,
    /// Material given up along the engine's best reply ([`sacrifice`]).
    pub sacrifice: i32,
    /// Whether the move takes back on the square the opponent just captured on.
    pub recapture: bool,
    /// Whether the move is within the named opening.
    pub book: bool,
    /// Lichess's judgement of this move (none when it is the best).
    pub judgement: Option<Judgement>,
    /// Lichess's judgement of the opponent's move just before.
    pub previous: Option<Judgement>,
}

pub fn classify(f: &Facts) -> Classification {
    let (before, after) = (win_percent(f.before), win_percent(f.after));
    let drop = (before - after).max(0.0);
    let serious = |j: Option<Judgement>| matches!(j, Some(Judgement::Mistake | Judgement::Blunder));
    if let Some(j) = f.judgement {
        if serious(Some(j)) && serious(f.previous) && before >= MISS_BEFORE_MIN && after >= MISS_AFTER_MIN {
            return Classification::Miss;
        }
        return match j {
            Judgement::Inaccuracy => Classification::Inaccuracy,
            Judgement::Mistake => Classification::Mistake,
            Judgement::Blunder => Classification::Blunder,
        };
    }
    if f.book {
        return Classification::Book;
    }
    let near_best = f.is_best || drop <= EXCELLENT_DROP;
    let choice = f.legal_moves > 1;
    if near_best && choice && f.sacrifice >= SACRIFICE && before < BRILLIANT_BEFORE_MAX && after >= BRILLIANT_AFTER_MIN
    {
        return Classification::Brilliant;
    }
    if near_best
        && choice
        && !f.recapture
        && after >= GREAT_AFTER_MIN
        && f.second.is_some_and(|s| win_percent(f.before) - win_percent(s) >= GREAT_GAP)
    {
        return Classification::Great;
    }
    if f.is_best {
        Classification::Best
    } else if drop <= EXCELLENT_DROP {
        Classification::Excellent
    } else {
        Classification::Good
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Material and phases
// ---------------------------------------------------------------------------------------------------------------------

/// How much material the mover gives up by playing `played` from `before`, along the engine's best reply `pv` (the
/// line from the position after the move): the mover's balance before, less the balance after the move and six more
/// plies of the line (longer while it keeps capturing, at most ten), never below zero.
pub fn sacrifice(before: &Chess, played: Move, pv: &[String]) -> i32 {
    let mover = before.turn();
    let start = balance(before, mover);
    let mut pos = before.clone();
    let mut capturing = played.is_capture();
    pos.play_unchecked(played);
    for (k, uci) in pv.iter().enumerate() {
        if k >= 10 || (k >= 6 && !capturing) {
            break;
        }
        let Some(m) = uci.parse::<UciMove>().ok().and_then(|u| u.to_move(&pos).ok()) else { break };
        capturing = m.is_capture();
        pos.play_unchecked(m);
    }
    (start - balance(&pos, mover)).max(0)
}

/// A phase of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Opening,
    Middlegame,
    Endgame,
}

impl Phase {
    pub const ALL: [Self; 3] = [Self::Opening, Self::Middlegame, Self::Endgame];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// The phase a position is in by itself (see [`phases`] for a game's, which never goes back):
///
/// - endgame: at most 6 queens, rooks, bishops and knights left on the board (lichess's `Divider` rule);
/// - middlegame: at most 10 of them, or a side's back rank holding fewer than 4 of its own pieces, or 30 plies played;
/// - opening otherwise.
pub fn phase_of(pos: &Chess, ply: usize) -> Phase {
    let board = pos.board();
    let pieces = (board.occupied() & !board.pawns() & !board.kings()).count();
    if pieces <= 6 {
        return Phase::Endgame;
    }
    let back = |color: Color, rank: Rank| (board.by_color(color) & Bitboard::from_rank(rank)).count();
    if pieces <= 10 || back(Color::White, Rank::First) < 4 || back(Color::Black, Rank::Eighth) < 4 || ply >= 30 {
        return Phase::Middlegame;
    }
    Phase::Opening
}

/// The phase of each position of a game, in order: [`phase_of`], but a game never goes back to an earlier phase.
pub fn phases(positions: &[Chess]) -> Vec<Phase> {
    let mut current = Phase::Opening;
    positions
        .iter()
        .enumerate()
        .map(|(ply, pos)| {
            let p = phase_of(pos, ply);
            if p.index() > current.index() {
                current = p;
            }
            current
        })
        .collect()
}
