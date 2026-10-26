//! The one in-memory shape every format reads into and writes from: a game as a starting position, the moves played
//! with the score of the position each was played from, and the result.

use anyhow::{Context, Result, anyhow};
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Move, Position, Role, Setup, fen::Fen, uci::UciMove};

/// Every position is set up in Chess960 mode, which also covers standard chess: castling rights name their rooks, so
/// DFRC starts and standard ones go through the same code. Moves print as king-takes-rook when castling.
pub const MODE: CastlingMode = CastlingMode::Chess960;

/// The score written for a mate (and the cap for every other score): beyond any eval, inside an `i16`.
pub const MATE_SCORE: i16 = 32000;

/// A game's result from White's side, as every format here stores it (0 = Black won, 1 = draw, 2 = White won).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum GameResult {
    BlackWins = 0,
    Draw = 1,
    WhiteWins = 2,
}

impl GameResult {
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::BlackWins),
            1 => Ok(Self::Draw),
            2 => Ok(Self::WhiteWins),
            _ => Err(anyhow!("invalid result {value}, expected 0, 1 or 2")),
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// The result for `color`: 1 a win, 0 a draw, -1 a loss.
    pub fn for_color(self, color: Color) -> i8 {
        let white = self as i8 - 1;
        if color == Color::White { white } else { -white }
    }

    pub fn from_winner(winner: Option<Color>) -> Self {
        match winner {
            Some(Color::White) => Self::WhiteWins,
            Some(Color::Black) => Self::BlackWins,
            None => Self::Draw,
        }
    }

    pub fn pgn(self) -> &'static str {
        match self {
            Self::WhiteWins => "1-0",
            Self::Draw => "1/2-1/2",
            Self::BlackWins => "0-1",
        }
    }
}

/// One position of a game: the move played from it and its score, from the side to move's point of view.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub mv: Move,
    pub score: i16,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub start: Chess,
    pub entries: Vec<Entry>,
    pub result: GameResult,
}

impl PartialEq for Game {
    fn eq(&self, other: &Self) -> bool {
        fen(&self.start) == fen(&other.start) && self.entries == other.entries && self.result == other.result
    }
}

impl Game {
    pub fn new(start: Chess, result: GameResult) -> Self {
        Self { start, entries: Vec::new(), result }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The ply of the starting position, counted from move 1 with White to move.
    pub fn start_ply(&self) -> u32 {
        ply_of(&self.start)
    }

    /// Calls `f` with every position, the entry played from it and its index in the game.
    pub fn for_each_position(&self, mut f: impl FnMut(&Chess, &Entry, usize)) {
        let mut pos = self.start.clone();
        for (index, entry) in self.entries.iter().enumerate() {
            f(&pos, entry, index);
            pos.play_unchecked(entry.mv);
        }
    }

    /// The position after the first `plies` moves.
    pub fn position_after(&self, plies: usize) -> Chess {
        let mut pos = self.start.clone();
        for entry in &self.entries[..plies.min(self.entries.len())] {
            pos.play_unchecked(entry.mv);
        }
        pos
    }

    /// The same game starting `plies` moves later (the first positions dropped).
    pub fn skip(&self, plies: usize) -> Game {
        let plies = plies.min(self.entries.len());
        Game { start: self.position_after(plies), entries: self.entries[plies..].to_vec(), result: self.result }
    }

    /// Checks that every move is legal where it is played.
    pub fn validate(&self) -> Result<()> {
        let mut pos = self.start.clone();
        for (index, entry) in self.entries.iter().enumerate() {
            if !pos.is_legal(entry.mv) {
                return Err(anyhow!("move {index} ({}) is illegal in {}", uci(entry.mv), fen(&pos)));
            }
            pos.play_unchecked(entry.mv);
        }
        Ok(())
    }
}

pub fn ply_of(pos: &Chess) -> u32 {
    (pos.fullmoves().get() - 1) * 2 + u32::from(pos.turn() == Color::Black)
}

pub fn fen(pos: &Chess) -> String {
    Fen::from_position(pos, EnPassantMode::Legal).to_string()
}

pub fn parse_fen(fen: &str) -> Result<Chess> {
    let parsed: Fen = fen.trim().parse().with_context(|| format!("invalid FEN {fen:?}"))?;
    from_setup(parsed.into_setup(), false).with_context(|| format!("illegal position {fen:?}"))
}

/// A position from a setup in Chess960 mode, tolerating an en passant square no capture can use and (with
/// `lenient_castling`) castling rights without their rook.
pub fn from_setup(setup: Setup, lenient_castling: bool) -> Result<Chess> {
    let error = match setup.position::<Chess>(MODE) {
        Ok(pos) => return Ok(pos),
        Err(e) => e,
    };
    let error = match error.ignore_invalid_ep_square() {
        Ok(pos) => return Ok(pos),
        Err(e) => e,
    };
    if !lenient_castling {
        return Err(anyhow!("{error}"));
    }
    match error.ignore_invalid_castling_rights() {
        Ok(pos) => Ok(pos),
        Err(e) => Err(anyhow!("{e}")),
    }
}

pub fn uci(mv: Move) -> String {
    mv.to_uci(MODE).to_string()
}

pub fn parse_uci(pos: &Chess, text: &str) -> Result<Move> {
    let parsed: UciMove = text.parse().with_context(|| format!("invalid UCI move {text:?}"))?;
    // Engines in standard mode write castling as the king's two-square step; accept both.
    parsed
        .to_move(pos)
        .or_else(|_| {
            let standard: Result<Move, _> = parsed.to_move(&with_mode(pos, CastlingMode::Standard));
            standard
        })
        .map_err(|_| anyhow!("illegal move {text} in {}", fen(pos)))
}

fn with_mode(pos: &Chess, mode: CastlingMode) -> Chess {
    pos.to_setup(EnPassantMode::Legal).position(mode).unwrap_or_else(|_| pos.clone())
}

/// A move that captures or promotes: what training filters call tactical.
pub fn is_tactical(mv: Move) -> bool {
    mv.is_capture() || mv.is_promotion()
}

/// Material in Reckless's units (P 1, N B 3, R 5, Q 9), as its score normalisation uses.
pub fn material(pos: &Chess) -> u32 {
    let board = pos.board();
    [(Role::Pawn, 1), (Role::Knight, 3), (Role::Bishop, 3), (Role::Rook, 5), (Role::Queen, 9)]
        .iter()
        .map(|&(role, value)| board.by_role(role).count() as u32 * value)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_for_color() {
        assert_eq!(GameResult::WhiteWins.for_color(Color::White), 1);
        assert_eq!(GameResult::WhiteWins.for_color(Color::Black), -1);
        assert_eq!(GameResult::Draw.for_color(Color::Black), 0);
    }

    #[test]
    fn standard_castling_in_chess960_mode() {
        let pos = parse_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        let a = parse_uci(&pos, "e1g1").unwrap();
        let b = parse_uci(&pos, "e1h1").unwrap();
        assert_eq!(a, b);
        assert_eq!(uci(a), "e1h1");
    }
}
