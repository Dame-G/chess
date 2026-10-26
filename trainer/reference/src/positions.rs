//! Deterministic test positions: random playouts from the standard start and from random back ranks.

use shakmaty::{
    Bitboard, Board, CastlingMode, Chess, Color, EnPassantMode, FromSetup, Move, Piece, Position, Role, Setup, Square,
    fen::Fen,
};

/// A small, seedable generator (xorshift64*), so a run can be repeated exactly without a dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// A start with both back ranks shuffled independently and no castling rights (which the network does not see).
/// Shuffles that shakmaty rejects (both bishops of a side on one colour count as too much material) are redrawn.
fn shuffled_start(rng: &mut Rng) -> Chess {
    loop {
        if let Some(position) = try_shuffled_start(rng) {
            return position;
        }
    }
}

fn try_shuffled_start(rng: &mut Rng) -> Option<Chess> {
    let mut board = Board::empty();
    for color in [Color::White, Color::Black] {
        let mut roles =
            [Role::Rook, Role::Knight, Role::Bishop, Role::Queen, Role::King, Role::Bishop, Role::Knight, Role::Rook];
        for i in (1..roles.len()).rev() {
            roles.swap(i, rng.below(i + 1));
        }
        let (back, pawns) = if color == Color::White { (0, 1) } else { (7, 6) };
        for (file, role) in roles.into_iter().enumerate() {
            board.set_piece_at(Square::new(back * 8 + file as u32), Piece { color, role });
            board.set_piece_at(Square::new(pawns * 8 + file as u32), Piece { color, role: Role::Pawn });
        }
    }

    let setup = Setup {
        board,
        promoted: Bitboard::EMPTY,
        pockets: None,
        turn: Color::White,
        castling_rights: Bitboard::EMPTY,
        ep_square: None,
        remaining_checks: None,
        halfmoves: 0,
        fullmoves: std::num::NonZeroU32::MIN,
    };
    Chess::from_setup(setup, CastlingMode::Chess960).ok()
}

/// One position: a random playout of 0 to 199 plies from the standard start (half the time) or a shuffled one. Each
/// playout has a temper, so every output bucket gets positions: a third prefer quiet moves three to one (material
/// lasts), a third pick uniformly, a third prefer captures three to one (down to bare endgames). A playout that ends
/// the game stops at the last position before the end, so every position has a legal move.
pub fn random_position(rng: &mut Rng) -> Chess {
    let mut position = if rng.below(2) == 0 { Chess::default() } else { shuffled_start(rng) };
    let plies = rng.below(200);
    let temper = rng.below(3);

    for _ in 0..plies {
        let moves = position.legal_moves();
        let (captures, quiet): (Vec<&Move>, Vec<&Move>) = moves.iter().partition(|m| m.is_capture());
        let preferred = match temper {
            0 => &quiet,
            2 => &captures,
            _ => &Vec::new(),
        };
        let chosen = if !preferred.is_empty() && rng.below(4) != 0 {
            *preferred[rng.below(preferred.len())]
        } else {
            moves[rng.below(moves.len())]
        };

        let next = position.clone().play(chosen).expect("a legal move plays");
        if next.is_game_over() {
            break;
        }
        position = next;
    }
    position
}

/// `n` positions from `seed`, as FENs (castling as the standard X-FEN letters, en passant only when legal).
pub fn random_fens(seed: u64, n: usize) -> Vec<String> {
    let mut rng = Rng::new(seed);
    (0..n).map(|_| Fen::from_position(&random_position(&mut rng), EnPassantMode::Legal).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_repeatable_and_varied() {
        let a = random_fens(7, 50);
        assert_eq!(a, random_fens(7, 50));
        let distinct: std::collections::HashSet<_> = a.iter().collect();
        assert!(distinct.len() > 45);
        for fen in &a {
            let pos: Chess = fen.parse::<Fen>().unwrap().into_position(CastlingMode::Chess960).unwrap();
            assert!(!pos.legal_moves().is_empty());
        }
    }
}
