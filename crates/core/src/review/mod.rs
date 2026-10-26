//! Game review: every position of a finished game searched by one of our versions at a fixed node budget, and each
//! move judged from the evaluations before and after it (accuracy, centipawn loss, and a classification from brilliant
//! to blunder), with a summary per side and per phase. The searching is `analysis`'s; this is everything after it.
//! The formulas are in [`judge`]. Every score stored here is White's.

pub mod judge;

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, Position as _};
use utoipa::ToSchema;

use self::judge::{Classification, Facts, Judgement, MATE, Phase};
use crate::{
    chess::{Position, position_key},
    error::Error,
    game::{Game, Move},
    ids::{GameId, ReviewId, Side, SidePair, Timestamp, VersionId},
    score::Score,
    time::TimeControl,
};

/// Nodes searched per position when a request names none.
pub const DEFAULT_NODES: u64 = 200_000;
/// Lines searched per position: the second tells an only move (great) from one of several good ones.
pub const MULTIPV: u32 = 2;
/// Games a review of a player takes when the request gives no `limit`.
pub const DEFAULT_LIMIT: u32 = 20;

/// One line of a searched position. `score` is White's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EvalLine {
    /// The line's first move.
    pub uci: String,
    pub score: Score,
    /// The line, its first move included, in UCI.
    pub pv: Vec<String>,
}

/// A searched position, as the position cache keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PositionEval {
    /// The best line first, then the second best when MultiPV was 2 and there was a second legal move.
    pub lines: Vec<EvalLine>,
    pub depth: u32,
    pub nodes: u64,
}

/// One move as the review judged it. Scores are White's (`{cp}` or `{mate}`; `mate: 0` is the side to move
/// checkmated, after the game's last move); win percentages and accuracy are the mover's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewedMove {
    pub ply: u32,
    pub side: Side,
    pub uci: String,
    pub san: String,
    /// The engine's best score before the move.
    pub eval_before: Score,
    /// The engine's best score after it.
    pub eval_after: Score,
    /// The engine's best move here, and its line (at most 8 moves, UCI).
    pub best_uci: String,
    pub best_san: String,
    pub best_pv: Vec<String>,
    /// The second-best move and its score, when there was a second legal move.
    pub second_uci: Option<String>,
    pub eval_second: Option<Score>,
    pub win_before: f64,
    pub win_after: f64,
    /// Lichess's accuracy of this move, 0 to 100.
    pub accuracy: f64,
    /// Centipawns lost, each score capped at ±1000.
    pub cp_loss: u32,
    pub classification: Classification,
    /// Lichess's judgement, which a `miss` also carries.
    pub judgement: Option<Judgement>,
    /// The phase of the position the move was played from.
    pub phase: Phase,
    /// Pawns of material given up along the engine's best reply (a brilliant move gives up 2 or more).
    pub sacrifice: i32,
    /// The mover's clock before the move, in a game with a clock.
    pub clock_before_ms: Option<u64>,
    /// The depth of the search of the position before the move.
    pub depth: u32,
}

/// How many moves got each classification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClassCounts {
    pub brilliant: u32,
    pub great: u32,
    pub best: u32,
    pub excellent: u32,
    pub good: u32,
    pub book: u32,
    pub inaccuracy: u32,
    pub mistake: u32,
    pub blunder: u32,
    pub miss: u32,
}

impl ClassCounts {
    pub fn get(&self, c: Classification) -> u32 {
        match c {
            Classification::Brilliant => self.brilliant,
            Classification::Great => self.great,
            Classification::Best => self.best,
            Classification::Excellent => self.excellent,
            Classification::Good => self.good,
            Classification::Book => self.book,
            Classification::Inaccuracy => self.inaccuracy,
            Classification::Mistake => self.mistake,
            Classification::Blunder => self.blunder,
            Classification::Miss => self.miss,
        }
    }

    fn slot(&mut self, c: Classification) -> &mut u32 {
        match c {
            Classification::Brilliant => &mut self.brilliant,
            Classification::Great => &mut self.great,
            Classification::Best => &mut self.best,
            Classification::Excellent => &mut self.excellent,
            Classification::Good => &mut self.good,
            Classification::Book => &mut self.book,
            Classification::Inaccuracy => &mut self.inaccuracy,
            Classification::Mistake => &mut self.mistake,
            Classification::Blunder => &mut self.blunder,
            Classification::Miss => &mut self.miss,
        }
    }

    pub fn add(&mut self, c: Classification, n: u32) {
        *self.slot(c) += n;
    }

    /// Both counts added up.
    pub fn plus(mut self, other: &Self) -> Self {
        for c in Classification::ALL {
            self.add(c, other.get(c));
        }
        self
    }

    pub fn total(&self) -> u32 {
        Classification::ALL.iter().map(|c| self.get(*c)).sum()
    }
}

/// Lichess's own counts: misses count as the mistake or blunder they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JudgementCounts {
    pub inaccuracy: u32,
    pub mistake: u32,
    pub blunder: u32,
}

impl JudgementCounts {
    pub fn plus(self, other: &Self) -> Self {
        Self {
            inaccuracy: self.inaccuracy + other.inaccuracy,
            mistake: self.mistake + other.mistake,
            blunder: self.blunder + other.blunder,
        }
    }
}

/// One side's play in one phase.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PhaseReview {
    pub moves: u32,
    /// Lichess's game accuracy over this phase's moves, as lila's `phaseAccuracies`.
    pub accuracy: Option<f64>,
    pub acpl: Option<u32>,
    /// Moves with a known think time (games with a clock, after the first move of each side), and their total.
    pub timed_moves: u32,
    pub time_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PhaseReviews {
    pub opening: PhaseReview,
    pub middlegame: PhaseReview,
    pub endgame: PhaseReview,
}

impl PhaseReviews {
    pub fn get(&self, p: Phase) -> &PhaseReview {
        match p {
            Phase::Opening => &self.opening,
            Phase::Middlegame => &self.middlegame,
            Phase::Endgame => &self.endgame,
        }
    }

    fn get_mut(&mut self, p: Phase) -> &mut PhaseReview {
        match p {
            Phase::Opening => &mut self.opening,
            Phase::Middlegame => &mut self.middlegame,
            Phase::Endgame => &mut self.endgame,
        }
    }
}

/// One side's game.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SideReview {
    pub moves: u32,
    /// Lichess's game accuracy, 0 to 100.
    pub accuracy: Option<f64>,
    /// Average centipawn loss, as Lichess computes it.
    pub acpl: Option<u32>,
    pub counts: ClassCounts,
    pub judgements: JudgementCounts,
    pub phases: PhaseReviews,
    /// Blunders (Lichess's judgement) played with less than a tenth of the starting clock left.
    pub time_trouble_blunders: u32,
}

/// A review's summary per side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewSummary {
    pub version: VersionId,
    pub nodes: u64,
    pub white: SideReview,
    pub black: SideReview,
    /// The ply whose move starts the middlegame and the endgame (1 = the first move), when the game got there.
    pub middlegame_ply: Option<u32>,
    pub endgame_ply: Option<u32>,
}

impl ReviewSummary {
    pub fn side(&self, side: Side) -> &SideReview {
        match side {
            Side::White => &self.white,
            Side::Black => &self.black,
        }
    }

    /// Each side's accuracy, for lists and the `review.upserted` event.
    pub fn accuracy(&self) -> SidePair<Option<f32>> {
        SidePair::new(self.white.accuracy.map(|a| a as f32), self.black.accuracy.map(|a| a as f32))
    }

    /// Both accuracies, when both sides moved: what a game summary shows.
    pub fn accuracy_pair(&self) -> Option<SidePair<f32>> {
        let a = self.accuracy();
        Some(SidePair::new(a.white?, a.black?))
    }
}

/// A stored review: the summary and every move.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Review {
    pub id: ReviewId,
    pub game: GameId,
    pub version: VersionId,
    pub nodes: u64,
    pub summary: ReviewSummary,
    pub plies: Vec<ReviewedMove>,
    pub created_at: Timestamp,
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------------------------------------------------
// Judging a game
// ---------------------------------------------------------------------------------------------------------------------

/// What a review reads of a game.
#[derive(Clone, Copy, Debug)]
pub struct ReviewInput<'a> {
    pub start_fen: Option<&'a str>,
    pub moves: &'a [Move],
    pub time: TimeControl,
    /// How many plies the named opening covers: those moves are `book`.
    pub opening_ply: Option<u32>,
}

impl<'a> From<&'a Game> for ReviewInput<'a> {
    fn from(g: &'a Game) -> Self {
        Self {
            start_fen: g.start_fen.as_deref(),
            moves: &g.moves,
            time: g.summary.time,
            opening_ply: g.summary.opening.as_ref().and_then(|o| o.ply),
        }
    }
}

/// A position a review needs searched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToSearch {
    pub fen: String,
    /// The position cache's key.
    pub key: String,
    /// The game is over in it: no engine searches it.
    pub terminal: bool,
}

/// The start position of a game and every position after each move.
pub fn positions(start_fen: Option<&str>, moves: &[Move]) -> Result<Vec<Chess>, Error> {
    let mut pos = Position::from_start(start_fen)?;
    let mut out = vec![pos.inner().clone()];
    for (i, mv) in moves.iter().enumerate() {
        pos.play(&mv.uci).map_err(|_| Error::bad_request(format!("move {} ({}) is not legal", i + 1, mv.uci)))?;
        out.push(pos.inner().clone());
    }
    Ok(out)
}

/// Every position of the game with its FEN and cache key, in order: what `analysis` searches (all but the terminal
/// ones) before [`judge_game`].
pub fn to_search(input: ReviewInput<'_>) -> Result<Vec<ToSearch>, Error> {
    Ok(positions(input.start_fen, input.moves)?
        .into_iter()
        .map(|p| {
            let fen = Position::from_chess(p.clone()).fen();
            ToSearch { key: position_key(&fen), fen, terminal: terminal(&p) }
        })
        .collect())
}

/// A position no engine needs to search: the game is over in it.
pub fn terminal(pos: &Chess) -> bool {
    pos.is_checkmate() || pos.is_stalemate() || pos.is_insufficient_material()
}

/// The side to move's score of each position: the engine's best line, or for a finished position mated (`-MATE`) or
/// drawn (0).
fn values(positions: &[Chess], evals: &[Option<PositionEval>]) -> Result<Vec<i32>, Error> {
    positions
        .iter()
        .zip(evals)
        .enumerate()
        .map(|(k, (pos, eval))| {
            if pos.is_checkmate() {
                Ok(-MATE)
            } else if terminal(pos) {
                Ok(0)
            } else {
                let line = eval
                    .as_ref()
                    .and_then(|e| e.lines.first())
                    .ok_or_else(|| Error::bad_request(format!("no evaluation of position {k}")))?;
                Ok(judge::force_cp(line.score.for_side(pos.turn().into())))
            }
        })
        .collect()
}

fn white_view(v: i32, side: Color) -> i32 {
    if side == Color::White { v } else { -v }
}

fn san(pos: &Chess, uci: &str) -> String {
    Position::from_chess(pos.clone()).san(uci).unwrap_or_else(|_| uci.to_owned())
}

/// What a move took: its think time, else what its clock says (the clock before, less the clock after, plus the
/// increment), in a game with a clock.
fn think_ms(moves: &[Move], i: usize, increment_ms: u64) -> Option<u64> {
    moves[i].think_ms.or_else(|| {
        let before = moves.get(i.checked_sub(2)?)?.clock_ms?;
        let after = moves[i].clock_ms?;
        Some((before + increment_ms).saturating_sub(after))
    })
}

/// Judges every move of a game from the evaluations of its positions (`evals[k]` for position `k`, `None` only where
/// the game is over). Pure: the engine work is done before.
pub fn judge_game(
    input: ReviewInput<'_>,
    evals: &[Option<PositionEval>],
    version: VersionId,
    nodes: u64,
) -> Result<(ReviewSummary, Vec<ReviewedMove>), Error> {
    let moves = input.moves;
    let positions = positions(input.start_fen, moves)?;
    if evals.len() != positions.len() {
        return Err(Error::bad_request("a review needs an evaluation of every position"));
    }
    let values = values(&positions, evals)?;
    let white_cps: Vec<i32> = values.iter().zip(&positions).map(|(v, p)| white_view(*v, p.turn())).collect();
    let phases = judge::phases(&positions);
    let (initial_ms, increment_ms) = match input.time {
        TimeControl::Clock { initial_ms, increment_ms } if initial_ms > 0 => (Some(initial_ms), increment_ms),
        _ => (None, 0),
    };

    let mut plies: Vec<ReviewedMove> = Vec::with_capacity(moves.len());
    let mut judgements: Vec<Option<Judgement>> = Vec::with_capacity(moves.len());
    for (i, mv) in moves.iter().enumerate() {
        let from = &positions[i];
        let mover = from.turn();
        let side = Side::from(mover);
        let (before, after) = (values[i], -values[i + 1]);
        let eval =
            evals[i].as_ref().ok_or_else(|| Error::bad_request(format!("no evaluation before move {}", i + 1)))?;
        let best = eval.lines.first().ok_or_else(|| Error::bad_request(format!("no line before move {}", i + 1)))?;
        let is_best = best.uci == mv.uci;
        let judgement = if is_best { None } else { judge::judgement(before, after) };
        let wrap = Position::from_chess(from.clone());
        let played = wrap.uci_move(&mv.uci).map_err(|_| Error::bad_request(format!("move {} is not legal", i + 1)))?;
        let reply_pv: &[String] = evals[i + 1].as_ref().and_then(|e| e.lines.first()).map_or(&[], |l| &l.pv[..]);
        let sacrifice = judge::sacrifice(from, played, reply_pv);
        let recapture = i > 0
            && Position::from_chess(positions[i - 1].clone())
                .uci_move(&moves[i - 1].uci)
                .is_ok_and(|prev| prev.is_capture() && prev.to() == played.to() && played.is_capture());
        let second = eval.lines.get(1);
        let second_value = second.map(|l| judge::force_cp(l.score.for_side(side)));
        let facts = Facts {
            before,
            after,
            second: second_value,
            is_best,
            legal_moves: from.legal_moves().len(),
            sacrifice,
            recapture,
            book: input.opening_ply.is_some_and(|p| (i as u32) < p),
            judgement,
            previous: i.checked_sub(1).and_then(|j| judgements[j]),
        };
        let classification = judge::classify(&facts);
        judgements.push(judgement);
        let (win_before, win_after) = (judge::win_percent(before), judge::win_percent(after));
        let clock_before_ms = initial_ms.and_then(|initial| match i.checked_sub(2) {
            None => Some(initial),
            Some(j) => moves[j].clock_ms,
        });
        plies.push(ReviewedMove {
            ply: i as u32 + 1,
            side,
            uci: mv.uci.clone(),
            san: mv.san.clone(),
            eval_before: judge::to_score(white_cps[i]),
            eval_after: judge::to_score(white_cps[i + 1]),
            best_uci: best.uci.clone(),
            best_san: san(from, &best.uci),
            best_pv: best.pv.iter().take(8).cloned().collect(),
            second_uci: second.map(|l| l.uci.clone()),
            eval_second: second_value.map(|v| judge::to_score(white_view(v, mover))),
            win_before: round1(win_before),
            win_after: round1(win_after),
            accuracy: round1(judge::move_accuracy(win_before, win_after)),
            cp_loss: judge::cp_loss(before, after),
            classification,
            judgement,
            phase: phases[i],
            sacrifice,
            clock_before_ms,
            depth: eval.depth,
        });
    }

    let white_first = positions[0].turn() == Color::White;
    let (white_acc, black_acc) = judge::game_accuracy(white_first, &white_cps);
    let mut sides = [SideReview::default(), SideReview::default()];
    sides[0].accuracy = white_acc.map(round1);
    sides[1].accuracy = black_acc.map(round1);
    let mean = |xs: &[u32]| (!xs.is_empty()).then(|| (xs.iter().sum::<u32>() as f64 / xs.len() as f64).round() as u32);
    for side in Side::BOTH {
        let mine: Vec<&ReviewedMove> = plies.iter().filter(|p| p.side == side).collect();
        let s = &mut sides[side.index()];
        s.moves = mine.len() as u32;
        s.acpl = mean(&mine.iter().map(|p| p.cp_loss).collect::<Vec<_>>());
        for p in &mine {
            s.counts.add(p.classification, 1);
            match p.judgement {
                Some(Judgement::Inaccuracy) => s.judgements.inaccuracy += 1,
                Some(Judgement::Mistake) => s.judgements.mistake += 1,
                Some(Judgement::Blunder) => {
                    s.judgements.blunder += 1;
                    if let (Some(initial), Some(left)) = (initial_ms, p.clock_before_ms)
                        && left * 10 < initial
                    {
                        s.time_trouble_blunders += 1;
                    }
                }
                None => {}
            }
        }
        for phase in Phase::ALL {
            let in_phase: Vec<&&ReviewedMove> = mine.iter().filter(|p| p.phase == phase).collect();
            let r = s.phases.get_mut(phase);
            r.moves = in_phase.len() as u32;
            r.acpl = mean(&in_phase.iter().map(|p| p.cp_loss).collect::<Vec<_>>());
            for p in &in_phase {
                // Lichess records no time for each side's first move.
                if initial_ms.is_some()
                    && p.ply > 2
                    && let Some(ms) = think_ms(moves, p.ply as usize - 1, increment_ms)
                {
                    r.timed_moves += 1;
                    r.time_ms += ms;
                }
            }
        }
    }
    // Each phase's accuracy is the game accuracy of the positions from the one before its first move to its last.
    for phase in Phase::ALL {
        let in_phase: Vec<usize> = (0..plies.len()).filter(|&i| phases[i] == phase).collect();
        let (Some(&first), Some(&last)) = (in_phase.first(), in_phase.last()) else { continue };
        let slice = &white_cps[first..=last + 1];
        let (w, b) = judge::game_accuracy(positions[first].turn() == Color::White, slice);
        sides[0].phases.get_mut(phase).accuracy = w.map(round1);
        sides[1].phases.get_mut(phase).accuracy = b.map(round1);
    }
    let starts = |p: Phase| (0..plies.len()).find(|&i| phases[i] == p).map(|i| i as u32 + 1);
    let [white, black] = sides;
    let summary = ReviewSummary {
        version,
        nodes,
        white,
        black,
        middlegame_ply: starts(Phase::Middlegame),
        endgame_ply: starts(Phase::Endgame),
    };
    Ok((summary, plies))
}

#[cfg(test)]
mod tests;
