//! Drills: your weak spots as exercises. A drill is the position before one of your mistakes or blunders; you get one
//! try to find a move as good as the engine's, then the answer. Drills repeat on a spaced schedule (SM-2): a miss
//! comes back tomorrow, a pass in growing intervals. This module is the pure part: making drills from weak spots
//! ([`new_drill`]), checking an answer ([`check`]), scheduling ([`grade`]), picking today's set ([`today`]) and the
//! hit-rate trend ([`trend`]).

use std::{collections::HashSet, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chess::Position,
    error::Error,
    ids::{GameId, Side, Timestamp},
    profile::WeakSpot,
    review::{EvalLine, ReviewedMove, judge::Phase},
    score::Score,
    themes::ThemeKind,
};

/// A drill's row id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct DrillId(pub i64);

impl fmt::Display for DrillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for DrillId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim().parse().map(Self)
    }
}

/// A move counts as correct when it keeps the side to move within this many win percentage points of the best.
pub const CORRECT_WITHIN: f64 = 5.0;
/// New drills started per day when the request names no cap.
pub const NEW_PER_DAY: u32 = 10;
/// Drills in a day's set, at most, when the request names no cap.
pub const DAY_LIMIT: usize = 30;
/// A new drill's ease, as SM-2 starts every item.
pub const START_EASE: f64 = 2.5;
/// SM-2's lowest ease: below it an item would come back more often than it is learned.
pub const MIN_EASE: f64 = 1.3;
/// The quality SM-2 reads a pass and a miss as: drills have one try, so there are only two answers.
const PASS_QUALITY: f64 = 5.0;
const FAIL_QUALITY: f64 = 2.0;
const DAY_MS: i64 = 86_400_000;

/// What a drill is about: the opening family of its game, the phase it came from, and what the mistake was about
/// (`core::themes`; `other` until a drill from before themes is classified).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct Theme {
    pub opening: Option<String>,
    pub phase: Phase,
    pub kind: ThemeKind,
    /// The cause the evidence names (`a knight`, `fork`), when it does.
    pub detail: Option<String>,
}

/// Drills in a session on one theme or line, when the request names no cap.
pub const SESSION_LIMIT: usize = 10;
/// A hint can be asked for twice: what kind of move, then which piece.
pub const HINT_LEVELS: u8 = 2;
/// Half a pass: what a hinted pass counts for in a hit rate.
pub const HINTED_CREDIT: f64 = 0.5;

/// One drill and where its schedule stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Drill {
    pub id: DrillId,
    /// The game and ply the mistake was made at.
    pub game: GameId,
    pub ply: u32,
    /// The position to find a move in, and its key (the FEN without move counters), which drills are unique by.
    pub fen: String,
    pub key: String,
    /// The side to move: yours.
    pub side: Side,
    /// Every move that counts as correct (UCI), the engine's best first: all within [`CORRECT_WITHIN`] of it.
    pub best_moves: Vec<String>,
    /// The same in SAN, for the answer.
    pub best_san: Vec<String>,
    /// The engine's line after its best move (UCI), shown with the answer.
    pub line: Vec<String>,
    /// What you played in the game, UCI and SAN.
    pub played: String,
    pub played_san: String,
    /// Win percentage lost in the game.
    pub swing: f64,
    pub theme: Theme,
    /// When it is next due, and the interval that set it.
    pub due: Timestamp,
    pub interval_days: u32,
    pub ease: f64,
    /// Passes in a row, and misses in all.
    pub reps: u32,
    pub lapses: u32,
    pub created_at: Timestamp,
    /// The last answer; `None` for a drill never tried (a new one).
    pub answered_at: Option<Timestamp>,
}

impl Drill {
    /// A drill never tried.
    pub fn is_new(&self) -> bool {
        self.answered_at.is_none()
    }

    pub fn is_due(&self, now: &Timestamp) -> bool {
        self.due <= *now
    }

    /// The drill as it goes out before it is answered: the position and its theme, without the moves that answer it
    /// or the move you played. [`DrillAnswerResult`] in the analysis crate carries them once it is tried.
    pub fn question(&self) -> Self {
        Self {
            best_moves: Vec::new(),
            best_san: Vec::new(),
            line: Vec::new(),
            played: String::new(),
            played_san: String::new(),
            ..self.clone()
        }
    }
}

/// A drill before it is stored: what [`new_drill`] makes, and the store gives an id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewDrill {
    pub game: GameId,
    pub ply: u32,
    pub fen: String,
    pub key: String,
    pub side: Side,
    pub best_moves: Vec<String>,
    pub best_san: Vec<String>,
    pub line: Vec<String>,
    pub played: String,
    pub played_san: String,
    pub swing: f64,
    pub theme: Theme,
}

impl NewDrill {
    /// The drill as stored with `id`: new, due at once, with SM-2's starting schedule.
    pub fn into_drill(self, id: DrillId, now: Timestamp) -> Drill {
        Drill {
            id,
            game: self.game,
            ply: self.ply,
            fen: self.fen,
            key: self.key,
            side: self.side,
            best_moves: self.best_moves,
            best_san: self.best_san,
            line: self.line,
            played: self.played,
            played_san: self.played_san,
            swing: self.swing,
            theme: self.theme,
            due: now.clone(),
            interval_days: 0,
            ease: START_EASE,
            reps: 0,
            lapses: 0,
            created_at: now,
            answered_at: None,
        }
    }
}

/// The searched lines a reviewed move carries: the best and, when there was one, the second best (White's scores).
pub fn lines_of(m: &ReviewedMove) -> Vec<EvalLine> {
    let mut lines = vec![EvalLine { uci: m.best_uci.clone(), score: m.eval_before, pv: m.best_pv.clone() }];
    if let (Some(uci), Some(score)) = (&m.second_uci, m.eval_second) {
        lines.push(EvalLine { uci: uci.clone(), score, pv: vec![uci.clone()] });
    }
    lines
}

/// A drill from a weak spot and the searched lines of its position (White's scores, best first: a [`PositionEval`]'s
/// lines from the position cache, or [`lines_of`] the reviewed move). Every line within [`CORRECT_WITHIN`] of the
/// best counts as correct, except the move played. Without lines, the weak spot's best move is the only answer.
///
/// [`PositionEval`]: crate::review::PositionEval
pub fn new_drill(spot: &WeakSpot, lines: &[EvalLine]) -> Result<NewDrill, Error> {
    let pos = Position::from_fen(&spot.fen)?;
    let side = pos.turn();
    let played = pos.to_uci(&spot.played)?;
    let mut best_moves = Vec::new();
    let mut line = Vec::new();
    if let Some(best) = lines.first() {
        let top = best.score.for_side(side).win_percent();
        for l in lines {
            let Ok(uci) = pos.to_uci(&l.uci) else { continue };
            let close = top - l.score.for_side(side).win_percent() <= CORRECT_WITHIN;
            if close && uci != played && !best_moves.contains(&uci) {
                best_moves.push(uci);
            }
        }
        line = best.pv.iter().skip(1).cloned().collect();
    }
    if best_moves.is_empty() {
        let uci = pos.to_uci(&spot.best)?;
        if uci == played {
            return Err(Error::bad_request("the weak spot's best move is the move played"));
        }
        best_moves.push(uci);
    }
    let best_san = best_moves.iter().map(|m| pos.san(m)).collect::<Result<Vec<_>, _>>()?;
    Ok(NewDrill {
        game: spot.game,
        ply: spot.ply,
        fen: pos.fen(),
        key: pos.key(),
        side,
        best_moves,
        best_san,
        line,
        played_san: pos.san(&played)?,
        played,
        swing: spot.swing,
        theme: Theme { opening: spot.opening.clone(), phase: spot.phase, kind: spot.kind, detail: spot.detail.clone() },
    })
}

/// New drills that are not drills yet: one per position key, the biggest swing kept when several share one, and none
/// whose key is in `existing`. In the order given otherwise.
pub fn dedupe(existing: &HashSet<String>, candidates: Vec<NewDrill>) -> Vec<NewDrill> {
    let mut out: Vec<NewDrill> = Vec::new();
    for c in candidates {
        if existing.contains(&c.key) {
            continue;
        }
        match out.iter_mut().find(|d| d.key == c.key) {
            Some(d) if c.swing > d.swing => *d = c,
            Some(_) => {}
            None => out.push(c),
        }
    }
    out
}

/// One try at a drill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrillOutcome {
    Pass,
    Fail,
}

/// Whether `answer` (UCI or SAN) is one of the drill's correct moves. An answer that is not a legal move there fails.
pub fn check(drill: &Drill, answer: &str) -> DrillOutcome {
    let correct =
        Position::from_fen(&drill.fen).and_then(|p| p.to_uci(answer)).is_ok_and(|uci| drill.best_moves.contains(&uci));
    if correct { DrillOutcome::Pass } else { DrillOutcome::Fail }
}

/// The quality SM-2 reads a pass found with a hint as: right, but not on your own.
const HINTED_QUALITY: f64 = 3.0;

/// The drill after an answer at `now`, by SM-2 with two answers: a pass is quality 5, a miss quality 2.
///
/// - A miss: passes in a row back to 0, one more lapse, due again in 1 day.
/// - A pass: one more in a row; the interval is 1 day after the first, 6 after the second, then the last interval
///   times the ease, rounded.
/// - A pass with a hint (`hinted`) is half a miss: quality 3, it neither counts as a pass in a row nor breaks the
///   run, and it comes back in 1 day.
/// - The ease moves by SM-2's `0.1 − (5 − q)(0.08 + (5 − q)·0.02)` on every answer (+0.10 a pass, −0.14 with a hint,
///   −0.32 a miss) and never drops under [`MIN_EASE`]. The new interval uses the ease from before the answer, as SM-2
///   does.
pub fn grade(drill: &Drill, outcome: DrillOutcome, hinted: bool, now: &Timestamp) -> Drill {
    let mut d = drill.clone();
    let hinted = hinted && outcome == DrillOutcome::Pass;
    let q = match outcome {
        _ if hinted => HINTED_QUALITY,
        DrillOutcome::Pass => PASS_QUALITY,
        DrillOutcome::Fail => FAIL_QUALITY,
    };
    match outcome {
        DrillOutcome::Fail => {
            d.reps = 0;
            d.lapses += 1;
            d.interval_days = 1;
        }
        DrillOutcome::Pass if hinted => d.interval_days = 1,
        DrillOutcome::Pass => {
            d.reps += 1;
            d.interval_days = match d.reps {
                1 => 1,
                2 => 6,
                _ => ((f64::from(d.interval_days.max(1)) * drill.ease).round() as u32).max(1),
            };
        }
    }
    d.ease = (drill.ease + 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02)).max(MIN_EASE);
    let at = now.unix_ms().unwrap_or_default();
    d.due = Timestamp::from_unix_ms(at + i64::from(d.interval_days) * DAY_MS);
    d.answered_at = Some(now.clone());
    d
}

/// The start of `now`'s day (UTC): new drills started since then count against the day's cap.
pub fn day_start(now: &Timestamp) -> Timestamp {
    let ms = now.unix_ms().unwrap_or_default();
    Timestamp::from_unix_ms(ms - ms.rem_euclid(DAY_MS))
}

/// Today's drills, in the order to play them: every tried drill that is due, the most overdue first, then new drills
/// (never tried) while fewer than `new_per_day` have been started today (`started_today` already), biggest swing first;
/// at most `limit` in all. Drills that are not due are left out.
pub fn today(drills: &[Drill], now: &Timestamp, new_per_day: u32, started_today: u32, limit: usize) -> Vec<DrillId> {
    let mut due: Vec<&Drill> = drills.iter().filter(|d| !d.is_new() && d.is_due(now)).collect();
    due.sort_by(|a, b| a.due.cmp(&b.due).then(a.id.cmp(&b.id)));
    let mut fresh: Vec<&Drill> = drills.iter().filter(|d| d.is_new() && d.is_due(now)).collect();
    fresh.sort_by(|a, b| b.swing.total_cmp(&a.swing).then(a.id.cmp(&b.id)));
    let room = new_per_day.saturating_sub(started_today) as usize;
    due.into_iter().chain(fresh.into_iter().take(room)).take(limit).map(|d| d.id).collect()
}

/// One stored answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DrillAnswer {
    pub drill: DrillId,
    pub at: Timestamp,
    /// The move given, UCI (or as typed, when it was not a legal move).
    pub answer: String,
    pub outcome: DrillOutcome,
    /// A hint was asked for before the answer: a pass then counts for [`HINTED_CREDIT`].
    #[serde(default)]
    pub hinted: bool,
}

impl DrillAnswer {
    /// What the answer counts for in a hit rate: 1 a clean pass, [`HINTED_CREDIT`] a pass with a hint, 0 a miss.
    pub fn credit(&self) -> f64 {
        match (self.outcome, self.hinted) {
            (DrillOutcome::Pass, false) => 1.0,
            (DrillOutcome::Pass, true) => HINTED_CREDIT,
            (DrillOutcome::Fail, _) => 0.0,
        }
    }
}

/// One day of the hit-rate trend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HitRatePoint {
    /// `2026-10-25` (UTC).
    pub date: String,
    pub answers: u32,
    /// Passes found without a hint.
    pub passes: u32,
    /// Passes found with a hint, each worth half in the rates.
    pub hinted: u32,
    /// Credit per answer that day, 0 to 1.
    pub rate: f64,
    /// Credit per answer over the last `window` answers up to the end of that day: the line the UI draws.
    pub rolling: f64,
}

/// The hit rate per day with answers, oldest first, with a rolling rate over the last `window` answers.
pub fn trend(answers: &[DrillAnswer], window: usize) -> Vec<HitRatePoint> {
    let mut sorted: Vec<&DrillAnswer> = answers.iter().collect();
    sorted.sort_by(|a, b| a.at.cmp(&b.at));
    let window = window.max(1);
    let mut out: Vec<HitRatePoint> = Vec::new();
    let mut credit_today = 0.0;
    for (i, a) in sorted.iter().enumerate() {
        let date = a.at.date().to_owned();
        if out.last().is_none_or(|p| p.date != date) {
            out.push(HitRatePoint { date, answers: 0, passes: 0, hinted: 0, rate: 0.0, rolling: 0.0 });
            credit_today = 0.0;
        }
        let p = out.last_mut().expect("pushed above");
        p.answers += 1;
        credit_today += a.credit();
        match (a.outcome, a.hinted) {
            (DrillOutcome::Pass, false) => p.passes += 1,
            (DrillOutcome::Pass, true) => p.hinted += 1,
            (DrillOutcome::Fail, _) => {}
        }
        p.rate = credit_today / f64::from(p.answers);
        let recent = &sorted[(i + 1).saturating_sub(window)..=i];
        p.rolling = recent.iter().map(|a| a.credit()).sum::<f64>() / recent.len() as f64;
    }
    out
}

/// Which drills a session is made of, besides today's schedule: one theme, the games of one opening line, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFilter {
    pub theme: Option<ThemeKind>,
    /// Only drills from these games (empty for any).
    pub games: Vec<GameId>,
    /// Only mistakes made by this ply.
    pub max_ply: Option<u32>,
}

impl SessionFilter {
    pub fn is_empty(&self) -> bool {
        self.theme.is_none() && self.games.is_empty() && self.max_ply.is_none()
    }

    pub fn matches(&self, d: &Drill) -> bool {
        self.theme.is_none_or(|t| d.theme.kind == t)
            && (self.games.is_empty() || self.games.contains(&d.game))
            && self.max_ply.is_none_or(|p| d.ply <= p)
    }
}

/// A session on a theme or a line: the drills that match, in the order to play them. Tried drills that are due first
/// (most overdue first), then new ones (biggest swing first), then tried ones not due yet (soonest first), because
/// asking for a theme means practising it today. At most `limit`; the day's cap on new drills does not apply.
pub fn session(drills: &[Drill], filter: &SessionFilter, now: &Timestamp, limit: usize) -> Vec<DrillId> {
    let mine: Vec<&Drill> = drills.iter().filter(|d| filter.matches(d)).collect();
    let mut due: Vec<&Drill> = mine.iter().copied().filter(|d| !d.is_new() && d.is_due(now)).collect();
    due.sort_by(|a, b| a.due.cmp(&b.due).then(a.id.cmp(&b.id)));
    let mut fresh: Vec<&Drill> = mine.iter().copied().filter(|d| d.is_new()).collect();
    fresh.sort_by(|a, b| b.swing.total_cmp(&a.swing).then(a.id.cmp(&b.id)));
    let mut later: Vec<&Drill> = mine.iter().copied().filter(|d| !d.is_new() && !d.is_due(now)).collect();
    later.sort_by(|a, b| a.due.cmp(&b.due).then(a.id.cmp(&b.id)));
    due.into_iter().chain(fresh).chain(later).take(limit).map(|d| d.id).collect()
}

/// A hint at a drill: a nudge that names a property of the right move, never the move.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DrillHint {
    /// 1 says what kind of move it is, 2 which piece moves.
    pub level: u8,
    pub text: String,
    /// At level 2: the square the piece stands on, when all the right moves start there.
    pub from: Option<String>,
}

/// The hint at `level` (1 or 2; more is 2) for a drill: from the moves that count as right, what they have in common.
///
/// - Level 1: "A check" or "A capture" when every right move is one, "A forcing move" when each is a capture or a
///   check, "A quiet move" when none is, else a nudge that names nothing.
/// - Level 2: "Move the knight" when every right move is made by the same kind of piece, else the kinds ("a knight or
///   a bishop"); with the square the piece stands on when it is the same for all of them.
pub fn hint(drill: &Drill, level: u8) -> Result<DrillHint, Error> {
    use shakmaty::{Position as _, Role};

    let pos = Position::from_fen(&drill.fen)?;
    let moves = drill.best_moves.iter().map(|m| pos.uci_move(m)).collect::<Result<Vec<_>, _>>()?;
    if moves.is_empty() {
        return Err(Error::bad_request("the drill has no answer to hint at"));
    }
    let level = level.clamp(1, HINT_LEVELS);
    if level == 1 {
        let checks: Vec<bool> = moves
            .iter()
            .map(|m| {
                let mut next = pos.inner().clone();
                next.play_unchecked(*m);
                next.is_check()
            })
            .collect();
        let all = |f: &dyn Fn(usize) -> bool| (0..moves.len()).all(f);
        let text = if all(&|i| checks[i]) {
            "A check"
        } else if all(&|i| moves[i].is_capture()) {
            "A capture"
        } else if all(&|i| checks[i] || moves[i].is_capture()) {
            "A forcing move"
        } else if all(&|i| !checks[i] && !moves[i].is_capture()) {
            "A quiet move"
        } else {
            "Look at what your opponent is threatening"
        };
        return Ok(DrillHint { level, text: text.to_owned(), from: None });
    }
    let mut roles: Vec<Role> = Vec::new();
    for m in &moves {
        if !roles.contains(&m.role()) {
            roles.push(m.role());
        }
    }
    roles.sort_by_key(|r| *r as u8);
    let word = |r: &Role| match r {
        Role::Pawn => "pawn",
        Role::Knight => "knight",
        Role::Bishop => "bishop",
        Role::Rook => "rook",
        Role::Queen => "queen",
        Role::King => "king",
    };
    let text = match roles.as_slice() {
        [one] => format!("Move the {}", word(one)),
        many => format!("Move {}", many.iter().map(|r| format!("a {}", word(r))).collect::<Vec<_>>().join(" or ")),
    };
    let first = moves[0].from();
    let from = first.filter(|f| moves.iter().all(|m| m.from() == Some(*f))).map(|f| f.to_string());
    Ok(DrillHint { level, text, from })
}

/// A White score as the win percentage of `side`: a helper for callers showing a drill's lines.
pub fn win_for(score: Score, side: Side) -> f64 {
    score.for_side(side).win_percent()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After 1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4?: White to move; Nxe5 is the mistake, Nxd4 and c3 are fine.
    const FEN: &str = "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";

    fn spot() -> WeakSpot {
        WeakSpot {
            game: GameId(7),
            ply: 7,
            fen: FEN.into(),
            played: "Nxe5".into(),
            best: "Nxd4".into(),
            swing: 18.0,
            opening: Some("Italian Game".into()),
            phase: Phase::Opening,
            recurring: 2,
            kind: ThemeKind::OpeningError,
            detail: None,
        }
    }

    fn line(uci: &str, cp: i32) -> EvalLine {
        EvalLine { uci: uci.into(), score: Score::Cp(cp), pv: vec![uci.into(), "e5d4".into()] }
    }

    fn drill() -> Drill {
        new_drill(&spot(), &[line("f3d4", 60), line("c2c3", 30), line("b1c3", -200)])
            .unwrap()
            .into_drill(DrillId(1), Timestamp::from_unix_ms(0))
    }

    #[test]
    fn a_weak_spot_becomes_a_drill_with_every_good_enough_move() {
        let d = drill();
        // 60 cp is 55.5 win%, 30 cp 52.8: within 5 points. -200 cp is far off.
        assert_eq!(d.best_moves, ["f3d4", "c2c3"]);
        assert_eq!(d.best_san, ["Nxd4", "c3"]);
        assert_eq!((d.played.as_str(), d.played_san.as_str()), ("f3e5", "Nxe5"));
        assert_eq!(d.line, ["e5d4"]);
        assert_eq!(d.side, Side::White);
        assert_eq!(d.key, "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq -");
        assert_eq!(
            d.theme,
            Theme {
                opening: Some("Italian Game".into()),
                phase: Phase::Opening,
                kind: ThemeKind::OpeningError,
                detail: None
            }
        );
        assert!(d.is_new() && d.is_due(&Timestamp::from_unix_ms(0)));
        assert_eq!((d.ease, d.reps, d.lapses, d.interval_days), (START_EASE, 0, 0, 0));
    }

    #[test]
    fn black_to_move_reads_scores_from_black() {
        // After 1. e4: Black to move. White's -20 is Black's +20, the best; White's +150 is far worse for Black.
        let pos = Position::after(None, &["e2e4"]).unwrap();
        let s = WeakSpot { fen: pos.fen(), played: "g5".into(), best: "e5".into(), ..spot() };
        let d = new_drill(&s, &[line("e7e5", -20), line("c7c5", -10), line("f7f6", 150)]).unwrap();
        assert_eq!(d.best_moves, ["e7e5", "c7c5"]);
        assert_eq!(d.side, Side::Black);
    }

    #[test]
    fn without_lines_the_best_move_is_the_answer() {
        let d = new_drill(&spot(), &[]).unwrap();
        assert_eq!(d.best_moves, ["f3d4"]);
        assert!(d.line.is_empty());
        let same = WeakSpot { best: "Nxe5".into(), ..spot() };
        assert!(new_drill(&same, &[]).is_err());
        assert!(new_drill(&WeakSpot { fen: "nonsense".into(), ..spot() }, &[]).is_err());
    }

    #[test]
    fn a_reviewed_move_gives_its_two_lines() {
        let m: ReviewedMove = serde_json::from_value(serde_json::json!({
            "ply": 7, "side": "white", "uci": "f3e5", "san": "Nxe5", "eval_before": {"cp": 60}, "eval_after": {"cp": -150},
            "best_uci": "f3d4", "best_san": "Nxd4", "best_pv": ["f3d4", "e5d4"], "second_uci": "c2c3",
            "eval_second": {"cp": 30}, "win_before": 55.5, "win_after": 36.0, "accuracy": 40.0, "cp_loss": 210,
            "classification": "mistake", "judgement": "mistake", "phase": "opening", "sacrifice": 0,
            "clock_before_ms": null, "depth": 14
        }))
        .unwrap();
        let lines = lines_of(&m);
        assert_eq!(lines.len(), 2);
        assert_eq!(new_drill(&spot(), &lines).unwrap().best_moves, ["f3d4", "c2c3"]);
    }

    #[test]
    fn drills_are_unique_by_position() {
        let a = new_drill(&spot(), &[]).unwrap();
        let b = NewDrill { swing: 30.0, game: GameId(8), ..a.clone() };
        let other = new_drill(
            &WeakSpot {
                fen: Position::after(None, &["e2e4"]).unwrap().fen(),
                played: "g5".into(),
                best: "e5".into(),
                ..spot()
            },
            &[],
        )
        .unwrap();
        let out = dedupe(&HashSet::new(), vec![a.clone(), b, other.clone()]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].game, GameId(8));
        let existing: HashSet<String> = [a.key.clone()].into();
        assert_eq!(dedupe(&existing, vec![a, other.clone()]), [other]);
    }

    #[test]
    fn answers_are_checked_in_uci_or_san() {
        let d = drill();
        assert_eq!(check(&d, "f3d4"), DrillOutcome::Pass);
        assert_eq!(check(&d, "c3"), DrillOutcome::Pass);
        assert_eq!(check(&d, "Nxe5"), DrillOutcome::Fail);
        assert_eq!(check(&d, "e1e8"), DrillOutcome::Fail);
        assert_eq!(check(&d, "zz"), DrillOutcome::Fail);
    }

    fn day(n: i64) -> Timestamp {
        Timestamp::from_unix_ms(n * DAY_MS)
    }

    #[test]
    fn passes_grow_the_interval_and_a_miss_starts_over() {
        let d = drill();
        let d1 = grade(&d, DrillOutcome::Pass, false, &day(0));
        assert_eq!((d1.reps, d1.interval_days, d1.due.clone()), (1, 1, day(1)));
        assert!((d1.ease - 2.6).abs() < 1e-9);
        let d2 = grade(&d1, DrillOutcome::Pass, false, &day(1));
        assert_eq!((d2.reps, d2.interval_days, d2.due.clone()), (2, 6, day(7)));
        let d3 = grade(&d2, DrillOutcome::Pass, false, &day(7));
        // 6 days × the ease before this answer (2.7), rounded.
        assert_eq!((d3.reps, d3.interval_days), (3, 16));
        assert_eq!(d3.due, day(23));
        let miss = grade(&d3, DrillOutcome::Fail, false, &day(23));
        assert_eq!((miss.reps, miss.lapses, miss.interval_days, miss.due.clone()), (0, 1, 1, day(24)));
        assert!((miss.ease - (2.8 - 0.32)).abs() < 1e-9);
        assert_eq!(miss.answered_at, Some(day(23)));
        // The ease never drops under SM-2's floor.
        let mut d = drill();
        for i in 0..10 {
            d = grade(&d, DrillOutcome::Fail, false, &day(i));
        }
        assert_eq!(d.ease, MIN_EASE);
        assert_eq!(d.lapses, 10);
    }

    #[test]
    fn today_puts_due_drills_first_and_caps_new_ones() {
        let base = drill();
        let mk = |id: i64, due: i64, answered: bool, swing: f64| Drill {
            id: DrillId(id),
            due: day(due),
            swing,
            answered_at: answered.then(|| day(0)),
            ..base.clone()
        };
        let drills = [
            mk(1, 3, true, 5.0),   // due, less overdue
            mk(2, 1, true, 5.0),   // due, most overdue
            mk(3, 9, true, 5.0),   // not due
            mk(4, 0, false, 10.0), // new
            mk(5, 0, false, 30.0), // new, bigger swing
            mk(6, 0, false, 20.0), // new
        ];
        let now = day(5);
        assert_eq!(today(&drills, &now, 2, 0, 30), [DrillId(2), DrillId(1), DrillId(5), DrillId(6)]);
        assert_eq!(today(&drills, &now, 2, 1, 30), [DrillId(2), DrillId(1), DrillId(5)]);
        assert_eq!(today(&drills, &now, 2, 5, 30), [DrillId(2), DrillId(1)]);
        assert_eq!(today(&drills, &now, 10, 0, 3), [DrillId(2), DrillId(1), DrillId(5)]);
        assert_eq!(day_start(&Timestamp::from_unix_ms(5 * DAY_MS + 12_345)), day(5));
    }

    #[test]
    fn the_trend_is_a_rate_per_day_and_a_rolling_rate() {
        let a = |ms: i64, pass: bool| DrillAnswer {
            drill: DrillId(1),
            at: Timestamp::from_unix_ms(ms),
            answer: "e2e4".into(),
            outcome: if pass { DrillOutcome::Pass } else { DrillOutcome::Fail },
            hinted: false,
        };
        let answers = [a(DAY_MS + 5, true), a(10, false), a(20, true), a(DAY_MS + 9, true), a(2 * DAY_MS, false)];
        let t = trend(&answers, 3);
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].date.as_str(), t[0].answers, t[0].passes, t[0].rate), ("1970-01-01", 2, 1, 0.5));
        assert_eq!((t[1].answers, t[1].rate), (2, 1.0));
        // Rolling over the last three answers: to day 2's end pass, pass, pass; to day 3's pass, pass, fail.
        assert_eq!(t[1].rolling, 1.0);
        assert!((t[2].rolling - 2.0 / 3.0).abs() < 1e-9 && t[2].rate == 0.0);
        assert!(trend(&[], 10).is_empty());
        assert!((win_for(Score::Cp(-100), Side::Black) - 59.1).abs() < 0.01);
    }

    #[test]
    fn a_hinted_pass_is_half_a_miss() {
        let d = grade(&drill(), DrillOutcome::Pass, false, &day(0));
        let h = grade(&d, DrillOutcome::Pass, true, &day(1));
        // The run neither grows nor breaks, it is due tomorrow, and the ease takes SM-2's quality 3.
        assert_eq!((h.reps, h.lapses, h.interval_days, h.due.clone()), (1, 0, 1, day(2)));
        assert!((h.ease - (d.ease - 0.14)).abs() < 1e-9);
        // A hint does not excuse a miss.
        let miss = grade(&d, DrillOutcome::Fail, true, &day(1));
        assert_eq!((miss.reps, miss.lapses), (0, 1));
        assert!((miss.ease - (d.ease - 0.32)).abs() < 1e-9);
    }

    #[test]
    fn hints_count_for_half_in_the_hit_rate() {
        let a = |ms: i64, outcome, hinted| DrillAnswer {
            drill: DrillId(1),
            at: Timestamp::from_unix_ms(ms),
            answer: "e2e4".into(),
            outcome,
            hinted,
        };
        let answers = [
            a(10, DrillOutcome::Pass, false),
            a(20, DrillOutcome::Pass, true),
            a(30, DrillOutcome::Fail, true),
            a(40, DrillOutcome::Pass, false),
        ];
        let t = trend(&answers, 10);
        assert_eq!((t[0].answers, t[0].passes, t[0].hinted), (4, 2, 1));
        assert!((t[0].rate - 0.625).abs() < 1e-9 && (t[0].rolling - 0.625).abs() < 1e-9);
    }

    #[test]
    fn a_session_on_a_theme_takes_due_then_new_then_later() {
        let base = drill();
        let mk = |id: i64, kind: ThemeKind, due: i64, answered: bool, swing: f64, game: i64| Drill {
            id: DrillId(id),
            due: day(due),
            swing,
            game: GameId(game),
            theme: Theme { kind, ..base.theme.clone() },
            answered_at: answered.then(|| day(0)),
            ..base.clone()
        };
        let drills = [
            mk(1, ThemeKind::HangingPiece, 3, true, 5.0, 1),   // due
            mk(2, ThemeKind::HangingPiece, 9, true, 5.0, 1),   // tried, not due
            mk(3, ThemeKind::HangingPiece, 0, false, 10.0, 2), // new
            mk(4, ThemeKind::HangingPiece, 0, false, 30.0, 2), // new, bigger swing
            mk(5, ThemeKind::MissedTactic, 0, false, 99.0, 1), // another theme
        ];
        let now = day(5);
        let theme = SessionFilter { theme: Some(ThemeKind::HangingPiece), ..SessionFilter::default() };
        assert_eq!(session(&drills, &theme, &now, 10), [DrillId(1), DrillId(4), DrillId(3), DrillId(2)]);
        assert_eq!(session(&drills, &theme, &now, 2), [DrillId(1), DrillId(4)]);
        let line = SessionFilter { games: vec![GameId(2)], max_ply: Some(7), ..SessionFilter::default() };
        assert_eq!(session(&drills, &line, &now, 10), [DrillId(4), DrillId(3)]);
        let early = SessionFilter { max_ply: Some(6), ..SessionFilter::default() };
        assert!(session(&drills, &early, &now, 10).is_empty());
        assert!(SessionFilter::default().is_empty() && !early.is_empty());
    }

    #[test]
    fn a_hint_names_a_property_never_the_move() {
        let d = drill();
        // Nxd4 and c3: a capture and a quiet move, by different pieces.
        let one = hint(&d, 1).unwrap();
        assert_eq!(one, DrillHint { level: 1, text: "Look at what your opponent is threatening".into(), from: None });
        let two = hint(&d, 2).unwrap();
        assert_eq!(two.text, "Move a pawn or a knight");
        assert_eq!(two.from, None);
        // One capture by a knight.
        let only = Drill { best_moves: vec!["f3d4".into()], best_san: vec!["Nxd4".into()], ..d.clone() };
        assert_eq!(hint(&only, 1).unwrap().text, "A capture");
        let knight = hint(&only, 2).unwrap();
        assert_eq!((knight.text.as_str(), knight.from.as_deref()), ("Move the knight", Some("f3")));
        // Past the last level stays at it; a quiet move reads as one.
        let quiet = Drill { best_moves: vec!["c2c3".into()], best_san: vec!["c3".into()], ..d.clone() };
        assert_eq!(hint(&quiet, 5).unwrap().level, 2);
        assert_eq!(hint(&quiet, 1).unwrap().text, "A quiet move");
        assert!(hint(&Drill { best_moves: Vec::new(), ..d }, 1).is_err());
    }

    #[test]
    fn a_question_carries_no_answer() {
        let q = drill().question();
        assert!(q.best_moves.is_empty() && q.best_san.is_empty() && q.line.is_empty() && q.played.is_empty());
        assert_eq!((q.fen.as_str(), q.side), (FEN, Side::White));
        assert_eq!(q.theme, drill().theme);
    }
}
