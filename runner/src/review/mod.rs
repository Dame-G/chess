//! Game review: every position of a stored game searched by one of our builds at a fixed node budget, and each move
//! judged from the evaluations before and after it (accuracy, centipawn loss, and a classification from brilliant to
//! blunder), with a summary per side and per phase. Reviews are jobs in the queue (they load the CPU), stored in
//! `reviews`, one per game, build and node budget; positions are cached in `position_evals`. `compare` aggregates
//! reviewed games per player. The definitions are in `judge` and in `AGENTS.md` ("Game review and comparison").

pub mod compare;
pub mod engine;
pub mod judge;
#[cfg(test)]
mod tests;

use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, ToSql, params, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use self::{
    engine::{Analyser, Factory, Pool, PositionEval, UciAnalyser},
    judge::{Classification, Facts, Judgement, MATE, Phase},
};
use crate::{
    api::{ApiError, AppState},
    engine::{build, uci::Score},
    jobs::{self, Job, JobSpec},
    matches::{self, Match, MatchStatus, MatchTime, rules::Side},
};

/// Nodes searched per position when a request names none.
pub const DEFAULT_NODES: u64 = 200_000;
/// Lines searched per position: the second tells an only move (great) from one of several good ones.
pub const MULTIPV: u32 = 2;
/// Hash per engine, in MiB.
const HASH: u32 = 16;
/// Games a review of an account or player takes when the request gives no `limit`.
const DEFAULT_LIMIT: u32 = 20;

/// A build by id, or a ref meaning its newest build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum BuildRef {
    Id(i64),
    Ref(String),
}

/// Whose games: your account (`me`), the bot's (`bot`), or another Lichess player's imported games (a username).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Who {
    Me,
    Bot,
    Player(String),
}

impl Who {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "me" => Self::Me,
            "bot" => Self::Bot,
            name => Self::Player(name.to_lowercase()),
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Me => "me".into(),
            Self::Bot => "bot".into(),
            Self::Player(p) => p.clone(),
        }
    }

    /// The SQL condition on `m.data` selecting this owner's games, and its parameter.
    fn sql(&self) -> (&'static str, Option<String>) {
        match self {
            Self::Me => ("json_extract(m.data, '$.lichess.account') = 'me'", None),
            Self::Bot => (
                "json_extract(m.data, '$.lichess') IS NOT NULL AND ifnull(json_extract(m.data, '$.lichess.account'), 'bot') = 'bot'",
                None,
            ),
            Self::Player(p) => (
                "json_extract(m.data, '$.lichess.account') = 'player' AND json_extract(m.data, '$.lichess.player') = ?",
                Some(p.clone()),
            ),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ReviewRequest {
    /// These matches, by id. Or, instead, `account` or `player`.
    #[serde(default)]
    pub matches: Vec<i64>,
    /// The newest finished games of `me` (your Lichess account) or `bot` not yet reviewed with this build and budget.
    #[serde(default)]
    pub account: Option<String>,
    /// The newest imported games of this Lichess player not yet reviewed with this build and budget.
    #[serde(default)]
    pub player: Option<String>,
    /// How many games for `account` or `player`. Default 20, at most 1000.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Only these Lichess speeds (`bullet`, `blitz`, `rapid`, `classical`, `correspondence`), for `account` or
    /// `player`.
    #[serde(default)]
    pub speeds: Vec<String>,
    /// Only rated (true) or casual (false) games, for `account` or `player`.
    #[serde(default)]
    pub rated: Option<bool>,
    /// The build to analyse with: an id, or a ref meaning its newest build. Default: the newest build of `main`.
    #[serde(default)]
    pub build: Option<BuildRef>,
    /// Nodes per position. Default 200,000.
    #[serde(default)]
    pub nodes: Option<u64>,
    /// Engines at once, one thread each. Default: half the default test concurrency.
    #[serde(default)]
    pub concurrency: Option<u32>,
}

/// A review job as queued: the request resolved.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ReviewJob {
    pub matches: Vec<i64>,
    pub build: i64,
    pub nodes: u64,
    pub concurrency: u32,
}

// ---------------------------------------------------------------------------------------------------------------------
// What a review is
// ---------------------------------------------------------------------------------------------------------------------

/// One move as the review judged it. Scores are White's (`{cp}` or `{mate}`; `mate: 0` is the side to move
/// checkmated, after the game's last move); win percentages and accuracy are the mover's.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct PlyReview {
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
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
}

/// Lichess's own counts: misses count as the mistake or blunder they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct JudgementCounts {
    pub inaccuracy: u32,
    pub mistake: u32,
    pub blunder: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct PhaseReview {
    pub moves: u32,
    /// Lichess's game accuracy over this phase's moves, as lila's `phaseAccuracies`.
    pub accuracy: Option<f64>,
    pub acpl: Option<u32>,
    /// Moves with a measured think time (games with a clock, after the first move of each side), and their total.
    pub timed_moves: u32,
    pub time_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
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
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
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

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ReviewSummary {
    pub build: i64,
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
}

/// A stored review: the summary and every move.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Review {
    pub id: i64,
    pub match_id: i64,
    pub build: i64,
    pub nodes: u64,
    pub created_at: String,
    pub summary: ReviewSummary,
    pub plies: Vec<PlyReview>,
}

/// What lists show of a side's review.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SideBrief {
    pub accuracy: Option<f64>,
    pub acpl: Option<u32>,
    pub counts: ClassCounts,
}

/// What match lists show of a game's newest review.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ReviewBrief {
    /// The review's id.
    pub id: i64,
    pub build: i64,
    pub nodes: u64,
    pub white: SideBrief,
    pub black: SideBrief,
}

impl ReviewBrief {
    pub fn of(id: i64, s: &ReviewSummary) -> Self {
        let side = |r: &SideReview| SideBrief { accuracy: r.accuracy, acpl: r.acpl, counts: r.counts };
        Self { id, build: s.build, nodes: s.nodes, white: side(&s.white), black: side(&s.black) }
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------------------------------------------------
// Judging a game
// ---------------------------------------------------------------------------------------------------------------------

/// The start position of a game and every position after each move.
pub fn positions(game: &Match) -> Result<Vec<Chess>> {
    let mut pos: Chess = if game.start_fen == "startpos" {
        Chess::default()
    } else {
        game.start_fen
            .parse::<Fen>()
            .ok()
            .and_then(|f| f.into_position(CastlingMode::Standard).ok())
            .context("the game's start position does not read")?
    };
    let mut out = vec![pos.clone()];
    for mv in &game.moves {
        let m = mv
            .uci
            .parse::<UciMove>()
            .ok()
            .and_then(|u| u.to_move(&pos).ok())
            .with_context(|| format!("move {} ({}) is not legal", mv.ply, mv.uci))?;
        pos.play_unchecked(m);
        out.push(pos.clone());
    }
    Ok(out)
}

pub fn fen(pos: &Chess) -> String {
    Fen::from_position(pos, EnPassantMode::Legal).to_string()
}

/// A position no engine needs to search: the game is over in it.
pub fn terminal(pos: &Chess) -> bool {
    pos.is_checkmate() || pos.is_stalemate() || pos.is_insufficient_material()
}

/// The side to move's score of each position: the engine's best line, or for a finished position mated (`-MATE`) or
/// drawn (0).
fn values(positions: &[Chess], evals: &[Option<PositionEval>]) -> Result<Vec<i32>> {
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
                    .with_context(|| format!("no evaluation of position {k}"))?;
                Ok(judge::force_cp(line.score))
            }
        })
        .collect()
}

fn white_view(v: i32, side: Color) -> i32 {
    if side == Color::White { v } else { -v }
}

fn san(pos: &Chess, uci: &str) -> String {
    uci.parse::<UciMove>()
        .ok()
        .and_then(|u| u.to_move(pos).ok())
        .map(|m| SanPlus::from_move(pos.clone(), m).to_string())
        .unwrap_or_else(|| uci.to_owned())
}

/// Judges every move of `game` from the evaluations of its positions (`evals[k]` for position `k`, `None` only where
/// the game is over). Pure: the engine work is done before.
pub fn judge_game(
    game: &Match,
    positions: &[Chess],
    evals: &[Option<PositionEval>],
    build: i64,
    nodes: u64,
) -> Result<(ReviewSummary, Vec<PlyReview>)> {
    if positions.len() != game.moves.len() + 1 || evals.len() != positions.len() {
        bail!("a review needs an evaluation of every position");
    }
    let values = values(positions, evals)?;
    let white_cps: Vec<i32> = values.iter().zip(positions).map(|(v, p)| white_view(*v, p.turn())).collect();
    let phases = judge::phases(positions);
    let opening_ply = game.lichess.as_ref().and_then(|l| l.opening_ply);
    let initial_ms = match game.time {
        MatchTime::Clock { seconds, .. } if seconds > 0.0 => Some((seconds * 1000.0) as u64),
        _ => None,
    };

    let mut plies: Vec<PlyReview> = Vec::with_capacity(game.moves.len());
    let mut judgements: Vec<Option<Judgement>> = Vec::with_capacity(game.moves.len());
    for (i, mv) in game.moves.iter().enumerate() {
        let from = &positions[i];
        let mover = from.turn();
        let side = Side::from(mover);
        let (before, after) = (values[i], -values[i + 1]);
        let eval = evals[i].as_ref().with_context(|| format!("no evaluation before move {}", i + 1))?;
        let best = &eval.lines[0];
        let is_best = best.uci == mv.uci;
        let judgement = if is_best { None } else { judge::judgement(before, after) };
        let played = mv
            .uci
            .parse::<UciMove>()
            .ok()
            .and_then(|u| u.to_move(from).ok())
            .with_context(|| format!("move {} is not legal", i + 1))?;
        let reply_pv: &[String] = evals[i + 1].as_ref().and_then(|e| e.lines.first()).map_or(&[], |l| &l.pv[..]);
        let sacrifice = judge::sacrifice(from, played, reply_pv);
        let recapture = i > 0
            && game.moves[i - 1]
                .uci
                .parse::<UciMove>()
                .ok()
                .and_then(|u| u.to_move(&positions[i - 1]).ok())
                .is_some_and(|prev| prev.is_capture() && prev.to() == played.to() && played.is_capture());
        let second = eval.lines.get(1);
        let facts = Facts {
            before,
            after,
            second: second.map(|l| judge::force_cp(l.score)),
            is_best,
            legal_moves: from.legal_moves().len(),
            sacrifice,
            recapture,
            book: opening_ply.is_some_and(|p| (i as u32) < p),
            judgement,
            previous: i.checked_sub(1).and_then(|j| judgements[j]),
        };
        let classification = judge::classify(&facts);
        judgements.push(judgement);
        let (win_before, win_after) = (judge::win_percent(before), judge::win_percent(after));
        let clock_before_ms = initial_ms.and_then(|initial| match i.checked_sub(1) {
            None => Some(initial),
            Some(j) => match side {
                Side::White => game.moves[j].clock_white_ms,
                Side::Black => game.moves[j].clock_black_ms,
            }
            .or(if i == 1 { Some(initial) } else { None }),
        });
        plies.push(PlyReview {
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
            eval_second: second.map(|l| judge::to_score(white_view(judge::force_cp(l.score), mover))),
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
    for side in [Side::White, Side::Black] {
        let mine: Vec<&PlyReview> = plies.iter().filter(|p| p.side == side).collect();
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
            let in_phase: Vec<&&PlyReview> = mine.iter().filter(|p| p.phase == phase).collect();
            let r = s.phases.get_mut(phase);
            r.moves = in_phase.len() as u32;
            r.acpl = mean(&in_phase.iter().map(|p| p.cp_loss).collect::<Vec<_>>());
            for p in &in_phase {
                let mv = &game.moves[p.ply as usize - 1];
                if initial_ms.is_some() && p.ply > 2 {
                    r.timed_moves += 1;
                    r.time_ms += mv.time_ms;
                }
            }
        }
    }
    // Each phase's accuracy is the game accuracy of the positions from the one before its first move to its last.
    for phase in Phase::ALL {
        let moves: Vec<usize> = (0..plies.len()).filter(|&i| phases[i] == phase).collect();
        let (Some(&first), Some(&last)) = (moves.first(), moves.last()) else { continue };
        let slice = &white_cps[first..=last + 1];
        let (w, b) = judge::game_accuracy(positions[first].turn() == Color::White, slice);
        sides[0].phases.get_mut(phase).accuracy = w.map(round1);
        sides[1].phases.get_mut(phase).accuracy = b.map(round1);
    }
    let starts = |p: Phase| (0..plies.len()).find(|&i| phases[i] == p).map(|i| i as u32 + 1);
    let [white, black] = sides;
    let summary = ReviewSummary {
        build,
        nodes,
        white,
        black,
        middlegame_ply: starts(Phase::Middlegame),
        endgame_ply: starts(Phase::Endgame),
    };
    Ok((summary, plies))
}

// ---------------------------------------------------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------------------------------------------------

fn exists(c: &Connection, match_id: i64, build: i64, nodes: u64) -> Result<bool> {
    Ok(c.query_row(
        "SELECT EXISTS (SELECT 1 FROM reviews WHERE match_id = ?1 AND build_id = ?2 AND nodes = ?3)",
        params![match_id, build, nodes as i64],
        |r| r.get(0),
    )?)
}

fn insert(c: &Connection, match_id: i64, summary: &ReviewSummary, plies: &[PlyReview]) -> Result<i64> {
    c.execute(
        "INSERT OR IGNORE INTO reviews (match_id, build_id, nodes, summary, plies) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            match_id,
            summary.build,
            summary.nodes as i64,
            serde_json::to_string(summary)?,
            serde_json::to_string(plies)?
        ],
    )?;
    Ok(c.query_row(
        "SELECT id FROM reviews WHERE match_id = ?1 AND build_id = ?2 AND nodes = ?3",
        params![match_id, summary.build, summary.nodes as i64],
        |r| r.get(0),
    )?)
}

const REVIEW_COLUMNS: &str = "id, match_id, build_id, nodes, created_at, summary, plies";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<(i64, i64, i64, i64, String, String, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
}

fn to_review(row: (i64, i64, i64, i64, String, String, String)) -> Result<Review> {
    let (id, match_id, build, nodes, created_at, summary, plies) = row;
    Ok(Review {
        id,
        match_id,
        build,
        nodes: nodes as u64,
        created_at,
        summary: serde_json::from_str(&summary).context("read a stored review")?,
        plies: serde_json::from_str(&plies).context("read a stored review")?,
    })
}

/// The newest review of a match, or the one with this build and node budget.
pub fn load(c: &Connection, match_id: i64, build: Option<i64>, nodes: Option<u64>) -> Result<Option<Review>> {
    let row = c
        .query_row(
            &format!(
                "SELECT {REVIEW_COLUMNS} FROM reviews WHERE match_id = ?1 AND (?2 IS NULL OR build_id = ?2)
                 AND (?3 IS NULL OR nodes = ?3) ORDER BY id DESC LIMIT 1"
            ),
            params![match_id, build, nodes.map(|n| n as i64)],
            from_row,
        )
        .optional()?;
    row.map(to_review).transpose()
}

/// The newest review of each of these matches, by match id.
pub fn load_many(c: &Connection, ids: &[i64]) -> Result<HashMap<i64, Review>> {
    let mut out = HashMap::new();
    let mut stmt = c.prepare_cached(&format!(
        "SELECT {REVIEW_COLUMNS} FROM reviews WHERE match_id = ?1 ORDER BY id DESC LIMIT 1"
    ))?;
    for id in ids {
        if let Some(row) = stmt.query_row([id], from_row).optional()? {
            out.insert(*id, to_review(row)?);
        }
    }
    Ok(out)
}

pub async fn get(state: &AppState, match_id: i64, build: Option<i64>, nodes: Option<u64>) -> Result<Option<Review>> {
    state.store.call(move |c| load(c, match_id, build, nodes)).await
}

// ---------------------------------------------------------------------------------------------------------------------
// Queueing
// ---------------------------------------------------------------------------------------------------------------------

pub const SPEEDS: [&str; 6] = ["ultraBullet", "bullet", "blitz", "rapid", "classical", "correspondence"];

pub fn check_speeds(speeds: &[String]) -> Result<(), ApiError> {
    match speeds.iter().find(|s| !SPEEDS.contains(&s.as_str())) {
        Some(s) => Err(ApiError::bad_request(&format!("{s} is not a Lichess speed ({})", SPEEDS.join(", ")))),
        None => Ok(()),
    }
}

/// A SQL condition on `m.data` and its parameters, for speeds and rated.
pub fn game_filters(speeds: &[String], rated: Option<bool>) -> (String, Vec<String>) {
    let mut sql = String::new();
    let mut args = Vec::new();
    if !speeds.is_empty() {
        sql += &format!(" AND json_extract(m.data, '$.lichess.speed') IN ({})", vec!["?"; speeds.len()].join(", "));
        args.extend(speeds.iter().cloned());
    }
    if let Some(r) = rated {
        sql += &format!(" AND json_extract(m.data, '$.lichess.rated') = {}", i32::from(r));
    }
    (sql, args)
}

async fn resolve_build(state: &AppState, spec: Option<BuildRef>) -> Result<build::Build, ApiError> {
    let spec = spec.unwrap_or(BuildRef::Ref("main".into()));
    let found = state
        .store
        .call({
            let spec = spec.clone();
            move |c| match spec {
                BuildRef::Id(id) => build::get(c, id),
                BuildRef::Ref(r) => build::newest_of(c, &r),
            }
        })
        .await?;
    match (found, spec) {
        (Some(b), _) if b.deleted_at.is_some() => {
            Err(ApiError::conflict(&format!("build {} was deleted: build its commit again", b.id)))
        }
        (Some(b), _) => Ok(b),
        (None, BuildRef::Id(_)) => Err(ApiError::not_found("no such build")),
        (None, BuildRef::Ref(r)) => Err(ApiError::conflict(&format!("no build of {r} yet: `chess build {r}` first"))),
    }
}

/// Queues a review. 409 while you play on Lichess, or without a build to review with.
pub async fn submit(state: &Arc<AppState>, req: ReviewRequest) -> Result<Job, ApiError> {
    crate::lichess::me::check_fair_play(state)?;
    check_speeds(&req.speeds)?;
    let b = resolve_build(state, req.build.clone()).await?;
    let nodes = req.nodes.unwrap_or(DEFAULT_NODES);
    if !(1_000..=100_000_000).contains(&nodes) {
        return Err(ApiError::bad_request("nodes must be between 1,000 and 100,000,000"));
    }
    let cores = state.machine.cores.max(1) as u32;
    let concurrency =
        req.concurrency.unwrap_or_else(|| (state.machine.default_concurrency(1) / 2).max(1)).clamp(1, cores);
    let who = match (&req.account, &req.player) {
        (Some(a), None) if a == "me" || a == "bot" => Some(Who::parse(a)),
        (Some(_), None) => return Err(ApiError::bad_request("account is `me` or `bot`")),
        (None, Some(p)) if !p.trim().is_empty() => Some(Who::Player(p.trim().to_lowercase())),
        (None, None) => None,
        _ => return Err(ApiError::bad_request("give matches, or account, or player")),
    };
    let ids = match who {
        Some(who) if req.matches.is_empty() => {
            let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 1000);
            let (build_id, speeds, rated) = (b.id, req.speeds.clone(), req.rated);
            state.store.call(move |c| unreviewed(c, &who, &speeds, rated, build_id, nodes, limit)).await?
        }
        None if !req.matches.is_empty() => {
            let ids = req.matches.clone();
            let missing = state
                .store
                .call(move |c| {
                    let mut stmt = c.prepare_cached("SELECT EXISTS (SELECT 1 FROM matches WHERE id = ?1)")?;
                    let mut missing = Vec::new();
                    for id in ids {
                        if !stmt.query_row([id], |r| r.get::<_, bool>(0))? {
                            missing.push(id);
                        }
                    }
                    Ok(missing)
                })
                .await?;
            if let Some(id) = missing.first() {
                return Err(ApiError::not_found(&format!("no match {id}")));
            }
            req.matches.clone()
        }
        _ => return Err(ApiError::bad_request("give matches, or account, or player")),
    };
    let spec = JobSpec::Review { review: ReviewJob { matches: ids, build: b.id, nodes, concurrency } };
    // Nothing to search needs no place in the queue.
    let empty = matches!(&spec, JobSpec::Review { review } if review.matches.is_empty());
    Ok(if empty { jobs::run_now(state, spec).await? } else { jobs::submit(state, spec).await? })
}

/// The newest finished games of `who` not yet reviewed with this build and budget, newest first.
fn unreviewed(
    c: &Connection,
    who: &Who,
    speeds: &[String],
    rated: Option<bool>,
    build: i64,
    nodes: u64,
    limit: u32,
) -> Result<Vec<i64>> {
    let (owner, owner_arg) = who.sql();
    let (filters, filter_args) = game_filters(speeds, rated);
    let sql = format!(
        "SELECT m.id FROM matches m WHERE m.status = 'finished' AND {owner}{filters}
         AND json_array_length(m.data, '$.moves') > 0
         AND NOT EXISTS (SELECT 1 FROM reviews r WHERE r.match_id = m.id AND r.build_id = {build} AND r.nodes = {})
         ORDER BY m.created_at DESC, m.id DESC LIMIT {limit}",
        nodes as i64
    );
    let args: Vec<String> = owner_arg.into_iter().chain(filter_args).collect();
    let mut stmt = c.prepare(&sql)?;
    let ids = stmt
        .query_map(params_from_iter(args.iter().map(|a| a as &dyn ToSql)), |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

// ---------------------------------------------------------------------------------------------------------------------
// The job
// ---------------------------------------------------------------------------------------------------------------------

/// Engines for a review: the build's binary, one thread each.
fn uci_factory(binary: std::path::PathBuf, nodes: u64) -> Factory {
    Arc::new(move || {
        let binary = binary.clone();
        Box::pin(async move {
            let a = UciAnalyser::start(&binary, nodes, MULTIPV, HASH).await?;
            Ok(Box::new(a) as Box<dyn Analyser>)
        })
    })
}

/// What reviewing one game came to.
enum Outcome {
    Reviewed { review: ReviewBrief, searched: usize, cached: usize },
    Skipped(&'static str),
}

/// The engines, started when a game first needs a search.
struct Engines {
    factory: Factory,
    size: usize,
    gate: engine::Gate,
    pool: Option<Pool>,
}

impl Engines {
    async fn pool(&mut self) -> Result<&Pool> {
        if self.pool.is_none() {
            self.pool = Some(Pool::start(self.factory.clone(), self.size, self.gate.clone()).await?);
        }
        Ok(self.pool.as_ref().expect("just started"))
    }
}

async fn review_one(
    state: &AppState,
    engines: &mut Engines,
    cancel: &CancellationToken,
    id: i64,
    build: i64,
    nodes: u64,
) -> Result<Outcome> {
    let Some(game) = matches::get(state, id).await? else { return Ok(Outcome::Skipped("no such match")) };
    if game.status == MatchStatus::Running {
        return Ok(Outcome::Skipped("still being played"));
    }
    if game.moves.is_empty() {
        return Ok(Outcome::Skipped("no moves"));
    }
    if state.store.call(move |c| exists(c, id, build, nodes)).await? {
        return Ok(Outcome::Skipped("already reviewed"));
    }
    let positions = positions(&game)?;
    let fens: Vec<String> = positions.iter().map(fen).collect();
    let keys: Vec<Option<String>> =
        positions.iter().zip(&fens).map(|(p, f)| (!terminal(p)).then(|| engine::cache_key(f))).collect();
    let wanted: Vec<String> = keys.iter().flatten().cloned().collect();
    let mut known = state.store.call(move |c| engine::cached(c, &wanted, build, nodes, MULTIPV)).await?;
    let cached = keys.iter().flatten().filter(|k| known.contains_key(*k)).count();
    let mut missing: Vec<(String, String)> = Vec::new();
    for (key, fen) in keys.iter().zip(&fens) {
        if let Some(key) = key
            && !known.contains_key(key)
            && !missing.iter().any(|(k, _)| k == key)
        {
            missing.push((key.clone(), fen.clone()));
        }
    }
    if !missing.is_empty() {
        let search: Vec<String> = missing.iter().map(|(_, f)| f.clone()).collect();
        let pool = engines.pool().await?;
        let found = tokio::select! {
            _ = cancel.cancelled() => bail!("cancelled"),
            found = pool.analyse_all(&search) => found?,
        };
        let fresh: Vec<(String, PositionEval)> = missing.iter().map(|(k, _)| k.clone()).zip(found).collect();
        known.extend(fresh.iter().cloned());
        state.store.call(move |c| engine::store(c, &fresh, build, nodes, MULTIPV)).await?;
    }
    let evals: Vec<Option<PositionEval>> =
        keys.iter().map(|k| k.as_ref().and_then(|k| known.get(k).cloned())).collect();
    let (summary, plies) = judge_game(&game, &positions, &evals, build, nodes)?;
    let review_id = state.store.call(move |c| insert(c, id, &summary, &plies).map(|rid| (rid, summary))).await?;
    Ok(Outcome::Reviewed { review: ReviewBrief::of(review_id.0, &review_id.1), searched: missing.len(), cached })
}

/// Waits while you play on Lichess: a review is analysis, which Lichess forbids during a game.
async fn wait_for_fair_play(
    state: &AppState,
    cancel: &CancellationToken,
    log: &(dyn Fn(&str) + Sync),
    progress: &(dyn Fn(bool) + Sync),
) -> Result<()> {
    if !state.me.locked() {
        return Ok(());
    }
    log("paused: you're playing on Lichess; the review goes on when the game ends");
    progress(true);
    while state.me.locked() {
        tokio::select! {
            _ = cancel.cancelled() => bail!("cancelled"),
            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
        }
    }
    log("resumed");
    progress(false);
    Ok(())
}

pub async fn run(
    state: &Arc<AppState>,
    job_id: i64,
    cancel: &CancellationToken,
    log: &(dyn Fn(&str) + Sync),
    job: &ReviewJob,
) -> Result<Value> {
    let factory = if job.matches.is_empty() {
        // Nothing to search: no engine is ever started.
        Arc::new(|| -> futures::future::BoxFuture<'static, Result<Box<dyn Analyser>>> {
            Box::pin(async { bail!("no engine") })
        }) as Factory
    } else {
        let b = state.store.call({
            let id = job.build;
            move |c| build::get(c, id)
        });
        let b = b.await?.with_context(|| format!("build {} is gone", job.build))?;
        if b.deleted_at.is_some() {
            bail!("build {} was deleted", b.id);
        }
        uci_factory(build::binary(&state.config.data, &b), job.nodes)
    };
    run_with(state, job_id, cancel, log, job, factory).await
}

/// The job with any analyser, so tests can run it without an engine.
pub async fn run_with(
    state: &Arc<AppState>,
    job_id: i64,
    cancel: &CancellationToken,
    log: &(dyn Fn(&str) + Sync),
    job: &ReviewJob,
    factory: Factory,
) -> Result<Value> {
    let total = job.matches.len();
    log(&format!(
        "reviewing {total} game{} with build {} at {} nodes a position, {} engine{}",
        if total == 1 { "" } else { "s" },
        job.build,
        job.nodes,
        job.concurrency,
        if job.concurrency == 1 { "" } else { "s" }
    ));
    let gate_state = state.clone();
    let gate: engine::Gate = Arc::new(move || gate_state.me.locked());
    let mut engines = Engines { factory, size: job.concurrency as usize, gate, pool: None };
    let (mut reviewed, mut skipped, mut failed, mut searched, mut cached) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let emit = |done: usize, id: Option<i64>, review: Option<&ReviewBrief>, paused: bool| {
        state.events.emit(
            "review.progress",
            json!({ "job": job_id, "done": done, "total": total, "match": id, "review": review, "paused": paused }),
        );
    };
    for (i, &id) in job.matches.iter().enumerate() {
        if cancel.is_cancelled() {
            bail!("cancelled");
        }
        wait_for_fair_play(state, cancel, log, &|paused| emit(i, None, None, paused)).await?;
        let started = std::time::Instant::now();
        match review_one(state, &mut engines, cancel, id, job.build, job.nodes).await {
            Ok(Outcome::Reviewed { review, searched: s, cached: k }) => {
                reviewed += 1;
                searched += s;
                cached += k;
                log(&format!(
                    "{}/{total} match {id}: accuracy {} / {}, {s} positions searched, {k} cached, {:.1} s",
                    i + 1,
                    review.white.accuracy.map_or("-".into(), |a| format!("{a:.1}")),
                    review.black.accuracy.map_or("-".into(), |a| format!("{a:.1}")),
                    started.elapsed().as_secs_f64()
                ));
                emit(i + 1, Some(id), Some(&review), false);
            }
            Ok(Outcome::Skipped(why)) => {
                skipped += 1;
                log(&format!("{}/{total} match {id}: skipped ({why})", i + 1));
                emit(i + 1, Some(id), None, false);
            }
            Err(e) if cancel.is_cancelled() => return Err(e),
            Err(e) => {
                failed += 1;
                log(&format!("{}/{total} match {id}: failed: {e:#}", i + 1));
                emit(i + 1, Some(id), None, false);
            }
        }
    }
    log(&format!("done: {reviewed} reviewed, {skipped} skipped, {failed} failed"));
    if total > 0 && failed == total {
        bail!("every game failed");
    }
    Ok(json!({
        "reviewed": reviewed, "skipped": skipped, "failed": failed,
        "positions_searched": searched, "positions_cached": cached,
        "build": job.build, "nodes": job.nodes,
    }))
}
