//! The Referee's rules for a game played here: whose clock has run out, whether the position ends the game, whether
//! the tests' adjudication would end it early, what each seat may do, and how an engine answers a draw offer. A pure
//! state machine: `play` measures think time on a monotonic clock and feeds it in, and asks the seats for moves.
//!
//! Lichess games are refereed by Lichess; their record mirrors its stream and does not go through this.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chess::{ChessError, Position},
    error::Error,
    game::{Ending, GameEnd, Move, Offers, Outcome, SeatAction, SeatMove},
    ids::{Side, SidePair},
    time::TimeControl,
};

// ---------------------------------------------------------------------------------------------------------------------
// Clocks
// ---------------------------------------------------------------------------------------------------------------------

/// The extra time a move may overrun its clock before the game is lost on time, as the tests allow (fastchess's
/// `timemargin=250`): with no move overhead, pipe latency alone would otherwise lose games.
pub const GRACE_MS: u64 = 250;

/// Both clocks of a game with a time control: what is left, and what each move adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    left: [u64; 2],
    increment: u64,
    grace: u64,
}

/// What charging a move did to its clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spent {
    /// The move was in time; this is what is left now, increment included.
    Left(u64),
    /// The clock ran out before the move arrived.
    Flagged,
}

impl Clock {
    pub fn new(initial_ms: u64, increment_ms: u64, grace_ms: u64) -> Self {
        Self { left: [initial_ms; 2], increment: increment_ms, grace: grace_ms }
    }

    pub fn left(&self, side: Side) -> u64 {
        self.left[side.index()]
    }

    pub fn increment(&self) -> u64 {
        self.increment
    }

    /// Both clocks.
    pub fn both(&self) -> SidePair<u64> {
        SidePair::new(self.left[0], self.left[1])
    }

    /// How long `side` may think before it flags, in ms.
    pub fn allowance(&self, side: Side) -> u64 {
        self.left(side) + self.grace
    }

    /// Charges a move to `side`: the measured think time comes off its clock and the increment goes on. A move that
    /// took longer than the clock held (and the grace) loses on time.
    pub fn spend(&mut self, side: Side, took_ms: u64) -> Spent {
        let left = self.left[side.index()];
        if took_ms > left + self.grace {
            self.left[side.index()] = 0;
            return Spent::Flagged;
        }
        let now = left.saturating_sub(took_ms) + self.increment;
        self.left[side.index()] = now;
        Spent::Left(now)
    }
}

/// `side`'s clock ran out in `pos`: it loses on time, unless the other side has too little material to ever mate, when
/// it is a draw (as on Lichess and in the tests' fastchess).
pub fn flagged(pos: &Position, side: Side) -> GameEnd {
    if !pos.can_mate(side.other()) {
        return GameEnd::draw(Ending::Timeout);
    }
    GameEnd::lost_by(side, Ending::Timeout)
}

// ---------------------------------------------------------------------------------------------------------------------
// Adjudication, as the tests do it (Reckless's fastchess settings)
// ---------------------------------------------------------------------------------------------------------------------

/// What an engine thought of the position it moved from, in centipawns from its own side (a mate counts as a huge
/// score, see [`crate::score::Score::centipawns`]). `None` when it reported no score, which breaks any run.
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
pub fn adjudicate(judged: &[Judged], fullmove: u32) -> Option<GameEnd> {
    for side in Side::BOTH {
        let own: Vec<_> = judged.iter().rev().filter(|j| j.side == side).take(RESIGN_MOVES).collect();
        if own.len() == RESIGN_MOVES && own.iter().all(|j| j.cp.is_some_and(|cp| cp <= -RESIGN_CP)) {
            return Some(GameEnd::lost_by(side, Ending::Adjudication));
        }
    }
    if fullmove >= DRAW_FROM_MOVE && judged.len() >= DRAW_PLIES {
        let recent = &judged[judged.len() - DRAW_PLIES..];
        if recent.iter().all(|j| j.cp.is_some_and(|cp| cp.abs() <= DRAW_CP)) {
            return Some(GameEnd::draw(Ending::Adjudication));
        }
    }
    None
}

/// A draw offered to an engine is accepted when it has played a fair number of moves and its own last score is not
/// above a tenth of a pawn: it takes a draw it is not winning, and never one it knows it is.
pub fn accepts_draw(engine_cp: Option<i32>, plies: usize) -> bool {
    plies >= 20 && engine_cp.is_some_and(|cp| cp <= 10)
}

// ---------------------------------------------------------------------------------------------------------------------
// The Referee
// ---------------------------------------------------------------------------------------------------------------------

/// Who sits on a side, as far as the rules care: a person answers offers themselves; an engine by
/// [`accepts_draw`] and never asks for takebacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sitter {
    Person,
    Engine,
}

/// What a local game is set up with.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub start_fen: Option<String>,
    pub time: TimeControl,
    pub seats: SidePair<Sitter>,
    /// Apply the tests' adjudication.
    pub adjudicate: bool,
}

/// What a seat's action did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acted {
    /// An offer or request now stands, for the other person to answer.
    Offered,
    /// The engine opponent declined the draw.
    DrawDeclined,
    /// Moves were taken back; this many are left.
    TakenBack(u32),
    /// An offer or request was declined.
    Declined,
    /// The game ended.
    Ended(GameEnd),
}

/// A local game in progress: position, clocks, moves, offers and the end.
#[derive(Clone, Debug)]
pub struct Referee {
    setup: Setup,
    start: Position,
    pos: Position,
    moves: Vec<Move>,
    clock: Option<Clock>,
    judged: Vec<Judged>,
    offers: Offers,
    end: Option<GameEnd>,
}

impl Referee {
    /// A game from its setup. A start position that already ends the game ends it at once.
    pub fn new(setup: Setup) -> Result<Self, ChessError> {
        let start = Position::from_start(setup.start_fen.as_deref())?;
        let clock = match setup.time {
            TimeControl::Clock { initial_ms, increment_ms } => Some(Clock::new(initial_ms, increment_ms, GRACE_MS)),
            _ => None,
        };
        let end = start.ending();
        Ok(Self {
            pos: start.clone(),
            start,
            setup,
            moves: Vec::new(),
            clock,
            judged: Vec::new(),
            offers: Offers::default(),
            end,
        })
    }

    pub fn position(&self) -> &Position {
        &self.pos
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// The moves in UCI, for `position ... moves ...`.
    pub fn uci_moves(&self) -> Vec<String> {
        self.moves.iter().map(|m| m.uci.clone()).collect()
    }

    pub fn turn(&self) -> Side {
        self.pos.turn()
    }

    /// The ply the next move will be, from 1.
    pub fn next_ply(&self) -> u32 {
        self.moves.len() as u32 + 1
    }

    pub fn clocks(&self) -> Option<SidePair<u64>> {
        self.clock.map(|c| c.both())
    }

    pub fn clock(&self) -> Option<&Clock> {
        self.clock.as_ref()
    }

    pub fn offers(&self) -> Offers {
        self.offers
    }

    /// How long the side to move may think before it flags, in ms; none without a clock.
    pub fn allowance(&self) -> Option<u64> {
        self.clock.map(|c| c.allowance(self.turn()))
    }

    /// How the game ended, once it has.
    pub fn ending(&self) -> Option<GameEnd> {
        self.end
    }

    /// Plays the side to move's move, which took `think_ms`: charges its clock (a flag ends the game instead), plays
    /// it (an illegal move from an engine loses; from a person it is refused), records its score for adjudication,
    /// and decides whether the game is over. Answers the move as recorded.
    pub fn apply(&mut self, mv: &SeatMove, think_ms: u64) -> Result<Move, Error> {
        if self.end.is_some() {
            return Err(Error::conflict("the game is over"));
        }
        let side = self.turn();
        let Ok(m) = self.pos.parse_move(&mv.uci) else {
            if self.setup.seats.get(side) == &Sitter::Engine {
                self.end = Some(GameEnd::lost_by(side, Ending::IllegalMove));
            }
            return Err(Error::bad_request(format!("{} is not a legal move there", mv.uci)));
        };
        let clock_ms = match self.clock.as_mut().map(|c| c.spend(side, think_ms)) {
            Some(Spent::Flagged) => {
                let end = flagged(&self.pos, side);
                self.end = Some(end);
                return Err(Error::conflict("the clock ran out"));
            }
            Some(Spent::Left(left)) => Some(left),
            None => None,
        };
        let played = self.pos.play_move(m);
        self.judged.push(Judged { side, cp: mv.eval.map(|s| s.for_side(side).centipawns()) });
        let recorded = Move {
            uci: played.uci,
            san: played.san,
            clock_ms,
            think_ms: Some(think_ms),
            eval: mv.eval,
            depth: mv.depth,
            nodes: mv.nodes,
        };
        self.moves.push(recorded.clone());
        // A move answers any offer the other side made (as on Lichess), and withdraws the mover's own takeback ask.
        self.offers = Offers::default();
        self.end = self
            .pos
            .ending()
            .or_else(|| self.setup.adjudicate.then(|| adjudicate(&self.judged, self.pos.fullmoves())).flatten());
        Ok(recorded)
    }

    /// The side to move's clock ran out without a move.
    pub fn flag(&mut self) -> GameEnd {
        let end = flagged(&self.pos, self.turn());
        if let Some(c) = self.clock.as_mut() {
            c.left[self.pos.turn().index()] = 0;
        }
        self.end = Some(end);
        end
    }

    /// An engine exited or stopped answering: it loses.
    pub fn engine_failed(&mut self, side: Side) -> GameEnd {
        if let Some(end) = self.end {
            return end;
        }
        let end = GameEnd::lost_by(side, Ending::EngineFailure);
        self.end = Some(end);
        end
    }

    /// The game is stopped from outside (the stop route, the app shutting down): a person who stops a game that has
    /// got going resigns it, and anything else (an engine game, a game before each side has moved) is aborted.
    pub fn stop(&mut self, by: Option<Side>) -> GameEnd {
        if let Some(end) = self.end {
            return end;
        }
        let person = by.filter(|s| self.setup.seats.get(*s) == &Sitter::Person);
        let end = match person {
            Some(side) if self.moves.len() >= 2 => GameEnd::lost_by(side, Ending::Resignation),
            _ => GameEnd::aborted(),
        };
        self.end = Some(end);
        end
    }

    /// What `side` may do now. Only people act; engines only move.
    pub fn actions_for(&self, side: Side) -> Vec<SeatAction> {
        let seats = &self.setup.seats;
        if self.end.is_some() || seats.get(side) != &Sitter::Person {
            return Vec::new();
        }
        let mine = self.turn() == side;
        let other = side.other();
        let mut actions = Vec::new();
        if mine {
            actions.push(SeatAction::Move);
        }
        let can_offer = match seats.get(other) {
            // An engine answers at once, on your turn.
            Sitter::Engine => mine,
            Sitter::Person => self.offers.draw.is_none(),
        };
        if can_offer && self.offers.draw != Some(other) {
            actions.push(SeatAction::OfferDraw);
        }
        if self.offers.draw == Some(other) {
            actions.extend([SeatAction::AcceptDraw, SeatAction::DeclineDraw]);
        }
        if self.can_take_back(side) {
            actions.push(SeatAction::Takeback);
        }
        if self.offers.takeback == Some(other) {
            actions.extend([SeatAction::AcceptTakeback, SeatAction::DeclineTakeback]);
        }
        actions.push(if self.moves.len() < 2 { SeatAction::Abort } else { SeatAction::Resign });
        actions
    }

    /// Takebacks are for games without a clock, against an engine on your own turn once each side has moved (your
    /// move and its reply go), or between people as a request when you have moved.
    fn can_take_back(&self, side: Side) -> bool {
        if self.clock.is_some() || self.offers.takeback.is_some() {
            return false;
        }
        match self.setup.seats.get(side.other()) {
            Sitter::Engine => self.turn() == side && self.moves.len() >= 2,
            Sitter::Person => self.moves_by(side) > 0,
        }
    }

    fn moves_by(&self, side: Side) -> usize {
        let first = self.start.turn();
        (0..self.moves.len()).filter(|i| (if i % 2 == 0 { first } else { first.other() }) == side).count()
    }

    /// `side` does `action` (a move goes through [`Self::apply`]). Anything not in [`Self::actions_for`] is refused.
    pub fn act(&mut self, side: Side, action: SeatAction) -> Result<Acted, Error> {
        if action == SeatAction::Move {
            return Err(Error::bad_request("a move is played with its UCI"));
        }
        if !self.actions_for(side).contains(&action) {
            return Err(Error::conflict(format!("you cannot {} now", action.as_str().replace('_', " "))));
        }
        let other = side.other();
        let acted = match action {
            SeatAction::OfferDraw => match self.setup.seats.get(other) {
                Sitter::Engine => {
                    let engine = self.judged.iter().rev().find(|j| j.side == other).and_then(|j| j.cp);
                    if accepts_draw(engine, self.moves.len()) {
                        self.finish(GameEnd::draw(Ending::Agreement))
                    } else {
                        Acted::DrawDeclined
                    }
                }
                Sitter::Person => {
                    self.offers.draw = Some(side);
                    Acted::Offered
                }
            },
            SeatAction::AcceptDraw => self.finish(GameEnd::draw(Ending::Agreement)),
            SeatAction::DeclineDraw => {
                self.offers.draw = None;
                Acted::Declined
            }
            SeatAction::Takeback => match self.setup.seats.get(other) {
                Sitter::Engine => Acted::TakenBack(self.take_back(2)),
                Sitter::Person => {
                    self.offers.takeback = Some(side);
                    Acted::Offered
                }
            },
            SeatAction::AcceptTakeback => {
                // The asker's last move goes, and the reply to it when there is one.
                let n = if self.turn() == other { 2 } else { 1 };
                self.offers.takeback = None;
                Acted::TakenBack(self.take_back(n))
            }
            SeatAction::DeclineTakeback => {
                self.offers.takeback = None;
                Acted::Declined
            }
            SeatAction::Resign => self.finish(GameEnd::lost_by(side, Ending::Resignation)),
            SeatAction::Abort => self.finish(GameEnd::aborted()),
            SeatAction::Move | SeatAction::ClaimVictory | SeatAction::ClaimDraw => {
                return Err(Error::bad_request("that is only for a game on Lichess"));
            }
        };
        Ok(acted)
    }

    fn finish(&mut self, end: GameEnd) -> Acted {
        self.end = Some(end);
        Acted::Ended(end)
    }

    /// Takes back the last `n` moves and answers how many are left.
    fn take_back(&mut self, n: usize) -> u32 {
        let keep = self.moves.len().saturating_sub(n);
        self.moves.truncate(keep);
        self.judged.truncate(keep);
        self.offers = Offers::default();
        // The moves were legal when they were played, so the replay cannot fail.
        let mut pos = self.start.clone();
        for m in &self.moves {
            if let Ok(mv) = pos.uci_move(&m.uci) {
                pos.play_move(mv);
            }
        }
        self.pos = pos;
        keep as u32
    }
}

/// The result as seen by `side`: 1, ½ or 0, none for an abort.
pub fn points(end: &GameEnd, side: Side) -> Option<f64> {
    end.outcome.map(|o: Outcome| o.points(side))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Score;

    fn clock(base: u64, inc: u64) -> Clock {
        Clock::new(base, inc, 0)
    }

    #[test]
    fn a_move_costs_its_time_and_earns_the_increment() {
        let mut c = clock(10_000, 100);
        assert_eq!(c.spend(Side::White, 1_500), Spent::Left(8_600));
        assert_eq!(c.left(Side::White), 8_600);
        // Black's clock is its own.
        assert_eq!(c.left(Side::Black), 10_000);
        assert_eq!(c.spend(Side::Black, 0), Spent::Left(10_100));
    }

    #[test]
    fn clocks_flag_when_they_run_below_zero() {
        let mut c = clock(1_000, 50);
        assert_eq!(c.spend(Side::White, 1_000), Spent::Left(50));
        assert_eq!(c.spend(Side::White, 51), Spent::Flagged);
        assert_eq!(c.left(Side::White), 0);
        // Exactly the whole clock is still in time; one millisecond more is not.
        let mut c = clock(1_000, 0);
        assert_eq!(c.spend(Side::Black, 1_001), Spent::Flagged);
    }

    #[test]
    fn the_grace_covers_pipe_latency_but_not_more() {
        let mut c = Clock::new(1_000, 100, GRACE_MS);
        assert_eq!(c.allowance(Side::White), 1_250);
        // 100 ms over the clock is forgiven and leaves just the increment.
        assert_eq!(c.spend(Side::White, 1_100), Spent::Left(100));
        assert_eq!(c.spend(Side::White, 351), Spent::Flagged);
    }

    fn pos(fen: &str) -> Position {
        Position::from_fen(fen).unwrap()
    }

    #[test]
    fn a_flag_against_a_side_that_cannot_mate_is_a_draw() {
        // White flags; Black has only a knight: it could never mate, so the game is drawn.
        assert_eq!(flagged(&pos("8/8/4k3/8/8/3K4/8/6n1 w - - 0 1"), Side::White), GameEnd::draw(Ending::Timeout));
        // With a rook it could, so White loses.
        assert_eq!(
            flagged(&pos("8/8/4k3/8/8/3K4/8/6r1 w - - 0 1"), Side::White),
            GameEnd::new(Outcome::BlackWins, Ending::Timeout)
        );
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
        assert_eq!(adjudicate(&scores, 4), Some(GameEnd::new(Outcome::BlackWins, Ending::Adjudication)));
        // Two are not enough, and a gap in the run breaks it.
        assert_eq!(adjudicate(&scores[..4], 3), None);
        let gap = run(Side::White, &[Some(-450), Some(500), None, Some(500), Some(-450), Some(500)]);
        assert_eq!(adjudicate(&gap, 4), None);
        // Black on the receiving end loses too.
        let black = run(Side::Black, &[Some(-900), Some(900), Some(-900), Some(900), Some(-900), Some(900)]);
        assert_eq!(adjudicate(&black, 4).unwrap().outcome, Some(Outcome::WhiteWins));
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
        assert_eq!(adjudicate(&level, 30), Some(GameEnd::draw(Ending::Adjudication)));
        let mut off = level.clone();
        off[3].cp = Some(11);
        assert_eq!(adjudicate(&off, 40), None);
        assert_eq!(adjudicate(&level[..11], 40), None);
    }

    fn referee(time: TimeControl, white: Sitter, black: Sitter) -> Referee {
        Referee::new(Setup { start_fen: None, time, seats: SidePair::new(white, black), adjudicate: false }).unwrap()
    }

    const NO_CLOCK: TimeControl = TimeControl::Movetime { ms: 100 };

    #[test]
    fn a_game_between_people_ends_in_mate() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Person);
        for m in ["f2f3", "e7e5", "g2g4"] {
            r.apply(&SeatMove::bare(m), 10).unwrap();
            assert_eq!(r.ending(), None);
        }
        let mv = r.apply(&SeatMove::bare("d8h4"), 10).unwrap();
        assert_eq!(mv.san, "Qh4#");
        assert_eq!(r.ending(), Some(GameEnd::new(Outcome::BlackWins, Ending::Checkmate)));
        assert!(r.actions_for(Side::White).is_empty());
        assert!(r.apply(&SeatMove::bare("e2e4"), 10).is_err());
    }

    #[test]
    fn a_persons_illegal_move_is_refused_and_an_engines_loses() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Engine);
        assert_eq!(r.apply(&SeatMove::bare("e2e5"), 10).unwrap_err().status(), 400);
        assert_eq!(r.ending(), None);
        r.apply(&SeatMove::bare("e4"), 10).unwrap();
        assert!(r.apply(&SeatMove::bare("e7e4"), 10).is_err());
        assert_eq!(r.ending(), Some(GameEnd::new(Outcome::WhiteWins, Ending::IllegalMove)));
    }

    #[test]
    fn the_clock_is_charged_and_flags() {
        let mut r = referee(TimeControl::clock(1_000, 100), Sitter::Person, Sitter::Engine);
        let m = r.apply(&SeatMove::bare("e2e4"), 400).unwrap();
        assert_eq!(m.clock_ms, Some(700));
        assert_eq!(r.clocks(), Some(SidePair::new(700, 1_000)));
        assert_eq!(r.allowance(), Some(1_250));
        assert!(r.apply(&SeatMove::bare("e7e5"), 1_300).is_err());
        assert_eq!(r.ending(), Some(GameEnd::new(Outcome::WhiteWins, Ending::Timeout)));
        let mut r = referee(TimeControl::clock(1_000, 0), Sitter::Person, Sitter::Engine);
        assert_eq!(r.flag(), GameEnd::new(Outcome::BlackWins, Ending::Timeout));
    }

    #[test]
    fn takebacks_go_without_a_clock_on_your_turn() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Engine);
        assert!(!r.actions_for(Side::White).contains(&SeatAction::Takeback));
        r.apply(&SeatMove::bare("e2e4"), 10).unwrap();
        // The engine's turn: you may do nothing but stop.
        assert_eq!(r.actions_for(Side::White), [SeatAction::Abort]);
        r.apply(&SeatMove::bare("e7e5"), 10).unwrap();
        assert!(r.actions_for(Side::White).contains(&SeatAction::Takeback));
        assert_eq!(r.act(Side::White, SeatAction::Takeback).unwrap(), Acted::TakenBack(0));
        assert_eq!(r.position().fen(), Position::start().fen());
        // With a clock there are none.
        let mut r = referee(TimeControl::clock(60_000, 0), Sitter::Person, Sitter::Engine);
        r.apply(&SeatMove::bare("e2e4"), 10).unwrap();
        r.apply(&SeatMove::bare("e7e5"), 10).unwrap();
        assert!(r.act(Side::White, SeatAction::Takeback).is_err());
        // Engines never act.
        assert!(r.actions_for(Side::Black).is_empty());
    }

    #[test]
    fn people_offer_and_answer() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Person);
        r.apply(&SeatMove::bare("e2e4"), 10).unwrap();
        assert_eq!(r.act(Side::White, SeatAction::Takeback).unwrap(), Acted::Offered);
        assert!(r.actions_for(Side::Black).contains(&SeatAction::AcceptTakeback));
        assert_eq!(r.act(Side::Black, SeatAction::AcceptTakeback).unwrap(), Acted::TakenBack(0));
        r.apply(&SeatMove::bare("d2d4"), 10).unwrap();
        assert_eq!(r.act(Side::White, SeatAction::OfferDraw).unwrap(), Acted::Offered);
        assert!(!r.actions_for(Side::White).contains(&SeatAction::OfferDraw));
        assert_eq!(r.act(Side::Black, SeatAction::DeclineDraw).unwrap(), Acted::Declined);
        r.act(Side::White, SeatAction::OfferDraw).unwrap();
        // A move answers the offer.
        r.apply(&SeatMove::bare("d7d5"), 10).unwrap();
        assert_eq!(r.offers(), Offers::default());
        r.act(Side::White, SeatAction::OfferDraw).unwrap();
        assert_eq!(r.act(Side::Black, SeatAction::AcceptDraw).unwrap(), Acted::Ended(GameEnd::draw(Ending::Agreement)));
    }

    #[test]
    fn an_engine_answers_a_draw_offer_by_its_score_from_ply_twenty() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Engine);
        let shuffle = ["g1f3", "g8f6", "f3g1", "f6g8"];
        for i in 0..20 {
            let side = r.turn();
            // The engine (Black) thinks it is a little worse: +0.05 for White.
            let eval = (side == Side::Black).then_some(Score::Cp(5));
            r.apply(&SeatMove { uci: shuffle[i % 4].into(), eval, depth: Some(1), nodes: None }, 1).unwrap();
            if i == 3 {
                assert_eq!(r.act(Side::White, SeatAction::OfferDraw).unwrap(), Acted::DrawDeclined);
            }
            if r.ending().is_some() {
                break;
            }
        }
        // The shuffle repeats: the third time ends it before the offer matters.
        assert_eq!(r.ending(), Some(GameEnd::draw(Ending::Repetition)));
    }

    #[test]
    fn an_engine_takes_a_draw_it_is_not_winning_late_enough() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Engine);
        let line = [
            "e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6", "b5a4", "g8f6", "e1g1", "f8e7", "f1e1", "b7b5", "a4b3",
            "d7d6", "c2c3", "e8g8", "h2h3", "c8b7", "d2d4", "f8e8",
        ];
        for m in line {
            let eval = (r.turn() == Side::Black).then_some(Score::Cp(20));
            r.apply(&SeatMove { uci: m.into(), eval, depth: Some(1), nodes: None }, 1).unwrap();
        }
        assert_eq!(r.act(Side::White, SeatAction::OfferDraw).unwrap(), Acted::Ended(GameEnd::draw(Ending::Agreement)));
    }

    #[test]
    fn stopping_resigns_a_game_that_has_got_going_and_aborts_one_that_has_not() {
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Engine);
        r.apply(&SeatMove::bare("e2e4"), 1).unwrap();
        assert_eq!(r.stop(Some(Side::White)), GameEnd::aborted());
        let mut r = referee(NO_CLOCK, Sitter::Person, Sitter::Engine);
        r.apply(&SeatMove::bare("e2e4"), 1).unwrap();
        r.apply(&SeatMove::bare("e7e5"), 1).unwrap();
        assert!(r.actions_for(Side::White).contains(&SeatAction::Resign));
        assert_eq!(r.stop(Some(Side::White)), GameEnd::lost_by(Side::White, Ending::Resignation));
        let mut engines = referee(NO_CLOCK, Sitter::Engine, Sitter::Engine);
        engines.apply(&SeatMove::bare("e2e4"), 1).unwrap();
        engines.apply(&SeatMove::bare("e7e5"), 1).unwrap();
        assert_eq!(engines.stop(None), GameEnd::aborted());
        assert_eq!(engines.engine_failed(Side::White).ending, Ending::Aborted, "an ended game stays ended");
    }

    #[test]
    fn adjudication_ends_a_lost_engine_game() {
        let mut r = Referee::new(Setup {
            start_fen: None,
            time: NO_CLOCK,
            seats: SidePair::both(Sitter::Engine),
            adjudicate: true,
        })
        .unwrap();
        // White is busted by its own account: -5.00 on each of its three moves, Black agrees.
        for m in ["f2f3", "e7e5", "e1f2", "d8h4", "f2e3"] {
            assert_eq!(r.ending(), None);
            let eval = Some(Score::Cp(-500));
            r.apply(&SeatMove { uci: m.into(), eval, depth: Some(1), nodes: None }, 1).unwrap();
        }
        assert_eq!(r.ending(), Some(GameEnd::lost_by(Side::White, Ending::Adjudication)));
    }
}
