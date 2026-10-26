//! The rules of a played game that the engines do not decide: whose clock has run out, whether the position ends
//! the game, and whether the same adjudication as the tests would end it early. All of it is plain functions over
//! plain data, so it is tested without an engine.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, EnPassantMode, KnownOutcome, Outcome, Position, zobrist::Zobrist64};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

impl From<Color> for Side {
    fn from(c: Color) -> Self {
        match c {
            Color::White => Self::White,
            Color::Black => Self::Black,
        }
    }
}

/// How a game came out, as PGN writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum GameResult {
    #[serde(rename = "1-0")]
    White,
    #[serde(rename = "0-1")]
    Black,
    #[serde(rename = "1/2-1/2")]
    Draw,
    /// Not finished, or stopped before it finished.
    #[serde(rename = "*")]
    Unfinished,
}

impl GameResult {
    pub fn won_by(side: Side) -> Self {
        match side {
            Side::White => Self::White,
            Side::Black => Self::Black,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::White => "1-0",
            Self::Black => "0-1",
            Self::Draw => "1/2-1/2",
            Self::Unfinished => "*",
        }
    }
}

/// Why a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Checkmate,
    Stalemate,
    /// The same position for the third time.
    Repetition,
    /// Fifty moves each without a capture or a pawn move.
    FiftyMoves,
    InsufficientMaterial,
    /// A clock ran out.
    Time,
    /// An engine exited or stopped answering.
    Crash,
    /// An engine answered with a move that is not legal.
    IllegalMove,
    /// Adjudicated: one side was losing by a wide margin for several moves.
    Resignation,
    /// Adjudicated: the evaluation sat at zero for many moves.
    DrawAdjudication,
    /// Stopped by the person watching, or by the runner restarting; on Lichess, aborted before it got going.
    Aborted,
    /// A player resigned (a Lichess game).
    Resigned,
    /// The players agreed a draw (a Lichess game).
    Agreement,
    /// A player left and the other claimed the game, or a draw (Lichess's `timeout`).
    Abandoned,
    /// A player did not make a first move in time (Lichess's `noStart`).
    NoStart,
    /// Lichess ended the game for a breach of its fair-play rules.
    Cheat,
    /// The variant's own ending, such as a king reaching the centre.
    VariantEnd,
    /// Lichess ended the game for a reason it did not name.
    Other,
}

impl Reason {
    /// The `Termination` header of the PGN.
    pub fn termination(self) -> &'static str {
        match self {
            Self::Checkmate
            | Self::Stalemate
            | Self::Repetition
            | Self::FiftyMoves
            | Self::InsufficientMaterial
            | Self::Resigned
            | Self::Agreement
            | Self::VariantEnd
            | Self::Other => "normal",
            Self::Time => "time forfeit",
            Self::Crash | Self::IllegalMove | Self::Cheat => "rules infraction",
            Self::Resignation | Self::DrawAdjudication => "adjudication",
            Self::Aborted | Self::Abandoned | Self::NoStart => "abandoned",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ending {
    pub result: GameResult,
    pub reason: Reason,
}

impl Ending {
    pub fn lost_by(side: Side, reason: Reason) -> Self {
        Self { result: GameResult::won_by(side.other()), reason }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Clocks
// ---------------------------------------------------------------------------------------------------------------------

/// The extra time a move may overrun its clock before the game is lost on time, as the tests allow (fastchess's
/// `timemargin=250`): with no move overhead, pipe latency alone would otherwise lose games.
pub const GRACE: Duration = Duration::from_millis(250);

/// Both clocks of a game with a time control: what is left, and what each move adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    left: [u64; 2],
    increment: u64,
    grace: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spent {
    /// The move was in time; this is what is left now, increment included.
    Left(u64),
    /// The clock ran out before the move arrived.
    Flagged,
}

impl Clock {
    pub fn new(base_ms: u64, increment_ms: u64, grace: Duration) -> Self {
        Self { left: [base_ms; 2], increment: increment_ms, grace: grace.as_millis() as u64 }
    }

    pub fn left(&self, side: Side) -> u64 {
        self.left[side.index()]
    }

    pub fn increment(&self) -> u64 {
        self.increment
    }

    /// How long `side` may think before it flags.
    pub fn allowance(&self, side: Side) -> Duration {
        Duration::from_millis(self.left(side) + self.grace)
    }

    /// Charges a move to `side`: the measured think time comes off its clock and the increment goes on. A move that
    /// took longer than the clock held (and the grace) loses on time.
    pub fn spend(&mut self, side: Side, took: Duration) -> Spent {
        let took = took.as_millis() as u64;
        let left = self.left[side.index()];
        if took > left + self.grace {
            self.left[side.index()] = 0;
            return Spent::Flagged;
        }
        let now = left.saturating_sub(took) + self.increment;
        self.left[side.index()] = now;
        Spent::Left(now)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The end of a game
// ---------------------------------------------------------------------------------------------------------------------

pub fn hash(pos: &Chess) -> u64 {
    u64::from(pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal))
}

/// Whether the position ends the game. `history` holds the hash of every position since the start, the current one last.
pub fn detect(pos: &Chess, history: &[u64]) -> Option<Ending> {
    if let Outcome::Known(known) = pos.outcome() {
        let reason = if pos.is_checkmate() {
            Reason::Checkmate
        } else if pos.is_stalemate() {
            Reason::Stalemate
        } else {
            Reason::InsufficientMaterial
        };
        let result = match known {
            KnownOutcome::Decisive { winner } => GameResult::won_by(winner.into()),
            KnownOutcome::Draw => GameResult::Draw,
        };
        return Some(Ending { result, reason });
    }
    if let Some(now) = history.last()
        && history.iter().filter(|h| *h == now).count() >= 3
    {
        return Some(Ending { result: GameResult::Draw, reason: Reason::Repetition });
    }
    if pos.halfmoves() >= 100 {
        return Some(Ending { result: GameResult::Draw, reason: Reason::FiftyMoves });
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------------
// Adjudication, as the tests do it (Reckless's fastchess settings)
// ---------------------------------------------------------------------------------------------------------------------

/// What an engine thought of the position it moved from, in centipawns from its own side (a mate counts as a huge
/// score). `None` when it reported no score, which breaks any run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judged {
    pub side: Side,
    pub cp: Option<i32>,
}

/// A side whose last three moves all scored at least 4 pawns against it has lost.
const RESIGN_CP: i32 = 400;
const RESIGN_MOVES: usize = 3;
/// From move 30, a run of six moves each (twelve plies) all within 0.10 of even is a draw.
const DRAW_CP: i32 = 10;
const DRAW_PLIES: usize = 12;
const DRAW_FROM_MOVE: u32 = 30;

/// Decides the game early, if the recent scores say it is decided. `fullmove` is the number of the move now to play.
pub fn adjudicate(judged: &[Judged], fullmove: u32) -> Option<Ending> {
    for side in [Side::White, Side::Black] {
        let own: Vec<_> = judged.iter().rev().filter(|j| j.side == side).take(RESIGN_MOVES).collect();
        if own.len() == RESIGN_MOVES && own.iter().all(|j| j.cp.is_some_and(|cp| cp <= -RESIGN_CP)) {
            return Some(Ending::lost_by(side, Reason::Resignation));
        }
    }
    if fullmove >= DRAW_FROM_MOVE && judged.len() >= DRAW_PLIES {
        let recent = &judged[judged.len() - DRAW_PLIES..];
        if recent.iter().all(|j| j.cp.is_some_and(|cp| cp.abs() <= DRAW_CP)) {
            return Some(Ending { result: GameResult::Draw, reason: Reason::DrawAdjudication });
        }
    }
    None
}

/// A draw offered to an engine is accepted when it has played a fair number of moves and its own last score is not
/// above a tenth of a pawn: it takes a draw it is not winning, and never one it knows it is.
pub fn accepts_draw(engine_cp: Option<i32>, plies: usize) -> bool {
    plies >= 20 && engine_cp.is_some_and(|cp| cp <= 10)
}

#[cfg(test)]
mod tests {
    use shakmaty::{CastlingMode, fen::Fen, uci::UciMove};

    use super::*;

    fn clock(base: u64, inc: u64) -> Clock {
        Clock::new(base, inc, Duration::ZERO)
    }

    #[test]
    fn a_move_costs_its_time_and_earns_the_increment() {
        let mut c = clock(10_000, 100);
        assert_eq!(c.spend(Side::White, Duration::from_millis(1_500)), Spent::Left(8_600));
        assert_eq!(c.left(Side::White), 8_600);
        // Black's clock is its own.
        assert_eq!(c.left(Side::Black), 10_000);
        assert_eq!(c.spend(Side::Black, Duration::from_millis(0)), Spent::Left(10_100));
    }

    #[test]
    fn clocks_flag_when_they_run_below_zero() {
        let mut c = clock(1_000, 50);
        assert_eq!(c.spend(Side::White, Duration::from_millis(1_000)), Spent::Left(50));
        assert_eq!(c.spend(Side::White, Duration::from_millis(51)), Spent::Flagged);
        assert_eq!(c.left(Side::White), 0);
        // Exactly the whole clock is still in time; one millisecond more is not.
        let mut c = clock(1_000, 0);
        assert_eq!(c.spend(Side::Black, Duration::from_millis(1_001)), Spent::Flagged);
    }

    #[test]
    fn the_grace_covers_pipe_latency_but_not_more() {
        let mut c = Clock::new(1_000, 100, GRACE);
        assert_eq!(c.allowance(Side::White), Duration::from_millis(1_250));
        // 100 ms over the clock is forgiven and leaves just the increment.
        assert_eq!(c.spend(Side::White, Duration::from_millis(1_100)), Spent::Left(100));
        assert_eq!(c.spend(Side::White, Duration::from_millis(351)), Spent::Flagged);
    }

    fn pos(fen: &str) -> Chess {
        fen.parse::<Fen>().unwrap().into_position(CastlingMode::Standard).unwrap()
    }

    #[test]
    fn detects_checkmate_and_stalemate() {
        // The fool's mate: White is mated.
        let mate = pos("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(
            detect(&mate, &[hash(&mate)]),
            Some(Ending { result: GameResult::Black, reason: Reason::Checkmate })
        );
        let stale = pos("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        assert_eq!(
            detect(&stale, &[hash(&stale)]),
            Some(Ending { result: GameResult::Draw, reason: Reason::Stalemate })
        );
        let start = Chess::default();
        assert_eq!(detect(&start, &[hash(&start)]), None);
    }

    #[test]
    fn detects_insufficient_material() {
        let bare = pos("8/8/4k3/8/8/3K4/8/8 w - - 0 1");
        assert_eq!(
            detect(&bare, &[hash(&bare)]),
            Some(Ending { result: GameResult::Draw, reason: Reason::InsufficientMaterial })
        );
        let knight = pos("8/8/4k3/8/8/3KN3/8/8 w - - 0 1");
        assert!(detect(&knight, &[hash(&knight)]).is_some());
        let rook = pos("8/8/4k3/8/8/3K1R2/8/8 w - - 0 1");
        assert_eq!(detect(&rook, &[hash(&rook)]), None);
    }

    #[test]
    fn detects_threefold_repetition_by_position() {
        let mut p = pos("4k3/8/8/8/8/8/8/R3K3 w - - 0 1");
        let mut history = vec![hash(&p)];
        // Both sides shuffle out and back: the start position comes round again after every four plies.
        for round in 0..2 {
            for uci in ["a1a2", "e8e7", "a2a1", "e7e8"] {
                let m = uci.parse::<UciMove>().unwrap().to_move(&p).unwrap();
                p.play_unchecked(m);
                history.push(hash(&p));
                let seen = history.iter().filter(|h| **h == *history.last().unwrap()).count();
                let ended = detect(&p, &history);
                if seen >= 3 {
                    assert_eq!(round, 1);
                    assert_eq!(ended, Some(Ending { result: GameResult::Draw, reason: Reason::Repetition }));
                    return;
                }
                assert_eq!(ended, None, "{uci} in round {round}");
            }
        }
        panic!("the position never came round three times");
    }

    #[test]
    fn detects_the_fifty_move_rule_but_prefers_mate() {
        let drawn = pos("4k3/8/8/8/8/8/8/R3K3 w - - 100 80");
        assert_eq!(
            detect(&drawn, &[hash(&drawn)]),
            Some(Ending { result: GameResult::Draw, reason: Reason::FiftyMoves })
        );
        let almost = pos("4k3/8/8/8/8/8/8/R3K3 w - - 99 80");
        assert_eq!(detect(&almost, &[hash(&almost)]), None);
        // Mate on the hundredth half-move is mate, not a draw.
        let mate = pos("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 100 3");
        assert_eq!(detect(&mate, &[hash(&mate)]).unwrap().reason, Reason::Checkmate);
    }

    /// Scores in order, the first by `side`, the sides alternating.
    fn run(side: Side, cps: &[Option<i32>]) -> Vec<Judged> {
        let mut s = side;
        cps.iter()
            .map(|cp| {
                let j = Judged { side: s, cp: *cp };
                s = s.other();
                j
            })
            .collect()
    }

    #[test]
    fn adjudicates_a_lost_game_after_three_losing_moves() {
        // White keeps seeing -4.50 and Black +5: White's last three moves are all lost ones.
        let scores = run(Side::White, &[Some(-450), Some(500), Some(-450), Some(500), Some(-450), Some(500)]);
        assert_eq!(adjudicate(&scores, 4), Some(Ending { result: GameResult::Black, reason: Reason::Resignation }));
        // Two are not enough, and a gap in the run breaks it.
        assert_eq!(adjudicate(&scores[..4], 3), None);
        let gap = run(Side::White, &[Some(-450), Some(500), None, Some(500), Some(-450), Some(500)]);
        assert_eq!(adjudicate(&gap, 4), None);
        // Black on the receiving end loses too.
        let black = run(Side::Black, &[Some(-900), Some(900), Some(-900), Some(900), Some(-900), Some(900)]);
        assert_eq!(adjudicate(&black, 4).unwrap().result, GameResult::White);
    }

    #[test]
    fn an_engine_takes_a_draw_only_when_it_is_not_winning() {
        assert!(accepts_draw(Some(0), 30));
        assert!(accepts_draw(Some(-150), 30));
        assert!(!accepts_draw(Some(80), 30));
        assert!(!accepts_draw(None, 30));
        assert!(!accepts_draw(Some(0), 10));
    }

    #[test]
    fn adjudicates_a_dead_draw_only_from_move_thirty() {
        let level = run(Side::White, &[Some(5); 12]);
        assert_eq!(adjudicate(&level, 29), None);
        assert_eq!(adjudicate(&level, 30), Some(Ending { result: GameResult::Draw, reason: Reason::DrawAdjudication }));
        let mut off = level.clone();
        off[3].cp = Some(11);
        assert_eq!(adjudicate(&off, 40), None);
        assert_eq!(adjudicate(&level[..11], 40), None);
    }
}
