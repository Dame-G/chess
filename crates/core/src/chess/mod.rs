//! Chess on top of shakmaty: a position that remembers its history (for repetitions), moves in UCI or SAN, the
//! position cache's key, material, and the end of a game. [`pgn`] writes and reads PGN.

pub mod pgn;

use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, KnownOutcome, Move, Outcome as ShakOutcome, Position as _, Role,
    fen::Fen,
    san::{San, SanPlus},
    uci::UciMove,
    zobrist::Zobrist64,
};

use crate::{
    game::{Ending, GameEnd, Outcome},
    ids::Side,
};

/// The standard start position.
pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Why a position or a move does not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ChessError {
    #[error("that FEN does not parse: {0}")]
    BadFen(String),
    #[error("that is not a legal position: {0}")]
    IllegalPosition(String),
    #[error("{0} is not a legal move there")]
    IllegalMove(String),
    #[error("{0}")]
    BadPgn(String),
}

impl From<ChessError> for crate::error::Error {
    fn from(e: ChessError) -> Self {
        Self::BadRequest(e.to_string())
    }
}

/// A move just played: how UCI and SAN write it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Played {
    pub uci: String,
    pub san: String,
}

/// A position and the hash of every position since the game's start (the current one last), so threefold repetition
/// can be told. Standard chess only.
#[derive(Clone, Debug)]
pub struct Position {
    pos: Chess,
    history: Vec<u64>,
}

impl Default for Position {
    fn default() -> Self {
        Self::start()
    }
}

fn hash(pos: &Chess) -> u64 {
    u64::from(pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal))
}

impl Position {
    pub fn start() -> Self {
        Self::from_chess(Chess::default())
    }

    /// A position with no history but itself.
    pub fn from_chess(pos: Chess) -> Self {
        let history = vec![hash(&pos)];
        Self { pos, history }
    }

    /// Reads a FEN; `startpos` is the standard start. Book lines (EPD) of four fields get counters `0 1`.
    pub fn from_fen(fen: &str) -> Result<Self, ChessError> {
        let fen = fen.trim();
        if fen == "startpos" {
            return Ok(Self::start());
        }
        let fields: Vec<&str> = fen.split(';').next().unwrap_or_default().split_whitespace().collect();
        let text = if fields.len() == 4 { format!("{} 0 1", fields.join(" ")) } else { fields.join(" ") };
        let parsed: Fen = text.parse().map_err(|e| ChessError::BadFen(format!("{e}")))?;
        let pos: Chess =
            parsed.into_position(CastlingMode::Standard).map_err(|e| ChessError::IllegalPosition(format!("{e}")))?;
        Ok(Self::from_chess(pos))
    }

    /// The start position of a game: its FEN, or the standard start for none.
    pub fn from_start(start_fen: Option<&str>) -> Result<Self, ChessError> {
        start_fen.map_or_else(|| Ok(Self::start()), Self::from_fen)
    }

    /// The position after `moves` (UCI or SAN) from a game's start.
    pub fn after(start_fen: Option<&str>, moves: &[impl AsRef<str>]) -> Result<Self, ChessError> {
        let mut pos = Self::from_start(start_fen)?;
        for m in moves {
            pos.play(m.as_ref())?;
        }
        Ok(pos)
    }

    pub fn inner(&self) -> &Chess {
        &self.pos
    }

    /// The hash of every position so far, the current one last.
    pub fn history(&self) -> &[u64] {
        &self.history
    }

    pub fn turn(&self) -> Side {
        self.pos.turn().into()
    }

    /// The number of the move now to play, from 1.
    pub fn fullmoves(&self) -> u32 {
        self.pos.fullmoves().get()
    }

    /// Half-moves since the last capture or pawn move.
    pub fn halfmoves(&self) -> u32 {
        self.pos.halfmoves()
    }

    /// The FEN, with an en passant square only when a capture there is legal.
    pub fn fen(&self) -> String {
        Fen::from_position(&self.pos, EnPassantMode::Legal).to_string()
    }

    /// The position cache's key: the FEN without the move counters, so the same position reached by different move
    /// orders or at different moves is searched once.
    pub fn key(&self) -> String {
        position_key(&self.fen())
    }

    pub fn is_standard_start(&self) -> bool {
        self.fen() == START_FEN
    }

    /// The legal move written `uci` (`e2e4`, `e7e8q`, `e1g1`).
    pub fn uci_move(&self, uci: &str) -> Result<Move, ChessError> {
        uci.trim()
            .parse::<UciMove>()
            .ok()
            .and_then(|u| u.to_move(&self.pos).ok())
            .ok_or_else(|| ChessError::IllegalMove(uci.trim().to_owned()))
    }

    /// The legal move written in SAN (`e4`, `Nf3`, `O-O`, `exd8=Q+`).
    pub fn san_move(&self, san: &str) -> Result<Move, ChessError> {
        let text = san.trim().trim_end_matches(['!', '?']);
        SanPlus::from_ascii(text.as_bytes())
            .ok()
            .and_then(|s| s.san.to_move(&self.pos).ok())
            .or_else(|| San::from_ascii(text.trim_end_matches(['+', '#']).as_bytes()).ok()?.to_move(&self.pos).ok())
            .ok_or_else(|| ChessError::IllegalMove(san.trim().to_owned()))
    }

    /// A move written either way: UCI first, then SAN.
    pub fn parse_move(&self, text: &str) -> Result<Move, ChessError> {
        self.uci_move(text).or_else(|_| self.san_move(text))
    }

    /// The UCI of a move written either way, without playing it.
    pub fn to_uci(&self, text: &str) -> Result<String, ChessError> {
        Ok(uci(self.parse_move(text)?))
    }

    /// The SAN of a legal move given in UCI or SAN, without playing it.
    pub fn san(&self, text: &str) -> Result<String, ChessError> {
        let m = self.parse_move(text)?;
        Ok(SanPlus::from_move(self.pos.clone(), m).to_string())
    }

    /// Plays a move written in UCI or SAN.
    pub fn play(&mut self, text: &str) -> Result<Played, ChessError> {
        let m = self.parse_move(text)?;
        Ok(self.play_move(m))
    }

    /// Plays a legal move.
    pub fn play_move(&mut self, m: Move) -> Played {
        let uci = uci(m);
        let san = SanPlus::from_move_and_play_unchecked(&mut self.pos, m).to_string();
        self.history.push(hash(&self.pos));
        Played { uci, san }
    }

    /// Every legal move, in UCI.
    pub fn legal_moves(&self) -> Vec<String> {
        self.pos.legal_moves().iter().map(|m| uci(*m)).collect()
    }

    pub fn legal_count(&self) -> usize {
        self.pos.legal_moves().len()
    }

    pub fn is_checkmate(&self) -> bool {
        self.pos.is_checkmate()
    }

    /// Whether the game is over in this position by itself: mate, stalemate or bare material. No engine needs to
    /// search it.
    pub fn is_terminal(&self) -> bool {
        self.pos.is_checkmate() || self.pos.is_stalemate() || self.pos.is_insufficient_material()
    }

    /// Whether `side` has enough material to ever mate.
    pub fn can_mate(&self, side: Side) -> bool {
        !self.pos.has_insufficient_material(side.into())
    }

    /// `side`'s material less the other side's, in pawns (N and B 3, R 5, Q 9).
    pub fn material_balance(&self, side: Side) -> i32 {
        balance(&self.pos, side.into())
    }

    /// Whether the position ends the game: checkmate, stalemate, insufficient material, the third repetition or the
    /// fifty-move rule (mate on the hundredth half-move is still mate).
    pub fn ending(&self) -> Option<GameEnd> {
        if let ShakOutcome::Known(known) = self.pos.outcome() {
            let ending = if self.pos.is_checkmate() {
                Ending::Checkmate
            } else if self.pos.is_stalemate() {
                Ending::Stalemate
            } else {
                Ending::InsufficientMaterial
            };
            let outcome = match known {
                KnownOutcome::Decisive { winner } => Outcome::won_by(winner.into()),
                KnownOutcome::Draw => Outcome::Draw,
            };
            return Some(GameEnd::new(outcome, ending));
        }
        if let Some(now) = self.history.last()
            && self.history.iter().filter(|h| *h == now).count() >= 3
        {
            return Some(GameEnd::draw(Ending::Repetition));
        }
        if self.pos.halfmoves() >= 100 {
            return Some(GameEnd::draw(Ending::FiftyMoves));
        }
        None
    }
}

/// Replays `moves` (UCI) from a start position, keeping each position's hash, for a takeback: the moves were legal
/// when played.
pub fn replay(start_fen: Option<&str>, moves: &[String]) -> Result<Position, ChessError> {
    Position::after(start_fen, moves)
}

/// A shakmaty move in UCI, with castling as the king's two-square move.
pub fn uci(m: Move) -> String {
    m.to_uci(CastlingMode::Standard).to_string()
}

/// The position cache's key of a FEN: its first four fields (placement, side to move, castling, en passant).
pub fn position_key(fen: &str) -> String {
    fen.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
}

fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

/// `color`'s material less the other side's, in pawns.
pub fn balance(pos: &Chess, color: Color) -> i32 {
    let board = pos.board();
    let side = |c: Color| {
        let m = board.material_side(c);
        Role::ALL.iter().map(|&r| value(r) * i32::from(*m.get(r))).sum::<i32>()
    };
    side(color) - side(color.other())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(fen: &str) -> Position {
        Position::from_fen(fen).unwrap()
    }

    #[test]
    fn plays_uci_and_san() {
        let mut p = Position::start();
        assert_eq!(p.play("e2e4").unwrap(), Played { uci: "e2e4".into(), san: "e4".into() });
        assert_eq!(p.play("e5").unwrap().uci, "e7e5");
        assert_eq!(p.play("Nf3").unwrap().uci, "g1f3");
        assert_eq!(p.san("b8c6").unwrap(), "Nc6");
        assert_eq!(p.to_uci("Nc6").unwrap(), "b8c6");
        assert!(p.play("e2e4").is_err());
        assert!(matches!(p.play("Qh5"), Err(ChessError::IllegalMove(_))));
        assert_eq!(p.turn(), Side::Black);
        assert_eq!(p.fullmoves(), 2);
        let castle = pos("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        assert_eq!(castle.san("e1g1").unwrap(), "O-O");
        assert_eq!(castle.to_uci("O-O-O").unwrap(), "e1c1");
        let promo = pos("8/4P3/8/8/8/8/k7/4K3 w - - 0 1");
        assert_eq!(promo.san("e7e8q").unwrap(), "e8=Q");
        assert_eq!(promo.to_uci("e8=Q+").unwrap(), "e7e8q");
        assert_eq!(Position::after(None, &["e4", "e5"]).unwrap().legal_count(), 29);
    }

    #[test]
    fn keys_drop_the_counters() {
        let p = pos("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        assert_eq!(p.key(), "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -");
        assert_eq!(position_key(START_FEN), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -");
        assert!(Position::start().is_standard_start());
        assert!(Position::from_fen("startpos").unwrap().is_standard_start());
        // A four-field book line reads with counters.
        assert_eq!(pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -").fen(), START_FEN);
        assert!(Position::from_fen("nonsense").is_err());
        assert!(matches!(Position::from_fen("8/8/8/8/8/8/8/8 w - - 0 1"), Err(ChessError::IllegalPosition(_))));
    }

    #[test]
    fn counts_material() {
        let p = pos("r1bq1rk1/pppn1ppp/4p3/3pP3/1b1P4/2NB1N2/PPP2PPP/R1BQK2R w KQ - 0 1");
        // Black is a knight short in this position.
        assert_eq!(p.material_balance(Side::White), 3);
        let up = pos("4k3/8/8/8/8/8/8/R3K3 w - - 0 1");
        assert_eq!(up.material_balance(Side::White), 5);
        assert_eq!(up.material_balance(Side::Black), -5);
        assert!(!pos("8/8/4k3/8/8/3K4/8/6n1 w - - 0 1").can_mate(Side::Black));
    }

    #[test]
    fn detects_checkmate_and_stalemate() {
        let mate = pos("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(mate.ending(), Some(GameEnd::new(Outcome::BlackWins, Ending::Checkmate)));
        assert!(mate.is_terminal());
        let stale = pos("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        assert_eq!(stale.ending(), Some(GameEnd::draw(Ending::Stalemate)));
        assert_eq!(Position::start().ending(), None);
    }

    #[test]
    fn detects_insufficient_material() {
        assert_eq!(pos("8/8/4k3/8/8/3K4/8/8 w - - 0 1").ending(), Some(GameEnd::draw(Ending::InsufficientMaterial)));
        assert!(pos("8/8/4k3/8/8/3KN3/8/8 w - - 0 1").ending().is_some());
        assert_eq!(pos("8/8/4k3/8/8/3K1R2/8/8 w - - 0 1").ending(), None);
    }

    #[test]
    fn detects_threefold_repetition_by_position() {
        let mut p = pos("4k3/8/8/8/8/8/8/R3K3 w - - 0 1");
        // Both sides shuffle out and back: the start position comes round again after every four plies.
        for round in 0..2 {
            for uci in ["a1a2", "e8e7", "a2a1", "e7e8"] {
                p.play(uci).unwrap();
                let now = *p.history().last().unwrap();
                let seen = p.history().iter().filter(|h| **h == now).count();
                if seen >= 3 {
                    assert_eq!(round, 1);
                    assert_eq!(p.ending(), Some(GameEnd::draw(Ending::Repetition)));
                    return;
                }
                assert_eq!(p.ending(), None, "{uci} in round {round}");
            }
        }
        panic!("the position never came round three times");
    }

    #[test]
    fn detects_the_fifty_move_rule_but_prefers_mate() {
        assert_eq!(pos("4k3/8/8/8/8/8/8/R3K3 w - - 100 80").ending(), Some(GameEnd::draw(Ending::FiftyMoves)));
        assert_eq!(pos("4k3/8/8/8/8/8/8/R3K3 w - - 99 80").ending(), None);
        let mate = pos("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 100 3");
        assert_eq!(mate.ending().unwrap().ending, Ending::Checkmate);
    }

    #[test]
    fn replays_with_history() {
        let moves: Vec<String> = ["a1a2", "e8e7", "a2a1", "e7e8"].map(String::from).to_vec();
        let p = replay(Some("4k3/8/8/8/8/8/8/R3K3 w - - 0 1"), &moves).unwrap();
        assert_eq!(p.history().len(), 5);
        assert_eq!(p.history()[0], p.history()[4]);
    }
}
