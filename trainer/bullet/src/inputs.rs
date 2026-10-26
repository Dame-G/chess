//! The model's inputs, computed with the CPU reference's own feature functions so the indices are Reckless's exactly.
//!
//! bullet's loaders hand out `ChessBoard`s (bulletformat): a position from the side to move's point of view, flipped
//! vertically and colour-swapped when Black is to move, with the score and result also the side to move's. Reckless's
//! features are colour-symmetric (`reference`'s `the_perspectives_of_a_mirrored_position_agree`): Black's view of a
//! position is White's view of the flipped one. So on a `ChessBoard`, the side to move's inputs are White's (`pov` 0)
//! and the other side's are Black's (`pov` 1).

use bullet_lib::game::formats::bulletformat::ChessBoard;
use reference::arch::OUTPUT_BUCKETS_LAYOUT;
use reference::features::{psq_features, threat_features};
use shakmaty::{Board, Color, Piece, Role, Square};

/// Piece-square inputs active at once per perspective: one per piece.
pub const MAX_PSQ: usize = 32;
/// Threat inputs reserved per perspective. A position needs far fewer (the most seen in the parity and smoke sets is
/// reported by `parity`); the rest of the slots are padding. More than this are dropped and counted.
pub const MAX_THREATS: usize = 256;

/// The board of a `ChessBoard` (side to move as White).
pub fn board(position: &ChessBoard) -> Board {
    let mut board = Board::empty();
    for (piece, square) in *position {
        let color = if piece & 8 == 0 { Color::White } else { Color::Black };
        let role = match piece & 7 {
            0 => Role::Pawn,
            1 => Role::Knight,
            2 => Role::Bishop,
            3 => Role::Rook,
            4 => Role::Queen,
            _ => Role::King,
        };
        board.set_piece_at(Square::new(square as u32), Piece { color, role });
    }
    board
}

/// The output bucket, by the number of pieces.
pub fn output_bucket(position: &ChessBoard) -> usize {
    OUTPUT_BUCKETS_LAYOUT[position.occ.count_ones() as usize]
}

/// Writes one perspective's inputs into their slots (padded with -1, bullet's "no input"); returns how many threat
/// inputs did not fit.
pub fn fill(board: &Board, pov: usize, threats: &mut [i32], psq: &mut [i32]) -> usize {
    let mut dropped = 0;
    let features = threat_features(board, pov);
    for (slot, value) in threats.iter_mut().zip(features.iter().map(|&f| f as i32).chain(std::iter::repeat(-1))) {
        *slot = value;
    }
    dropped += features.len().saturating_sub(threats.len());

    let features = psq_features(board, pov);
    for (slot, value) in psq.iter_mut().zip(features.iter().map(|&f| f as i32).chain(std::iter::repeat(-1))) {
        *slot = value;
    }
    dropped + features.len().saturating_sub(psq.len())
}

#[cfg(test)]
mod tests {
    use reference::parity::parse_fen;
    use shakmaty::Position;

    use super::*;

    /// The side to move's and the other side's inputs from a `ChessBoard` equal the reference's on the real board.
    #[test]
    fn a_chessboard_gives_the_reference_s_inputs_for_both_sides() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
            "r3k2r/pp1n1ppp/2p1pn2/q2p4/1bPP4/2N1PN2/PPQ2PPP/R3KB1R w KQkq - 0 1",
            "r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP3PPP/R1BQKB1R b KQ - 0 8",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "8/8/6r1/4B3/3Q3p/N1nq4/5RP1/b3K2k b - - 0 1",
        ] {
            let position = parse_fen(fen).unwrap();
            let stm = if position.turn() == Color::White { 0 } else { 1 };
            let cb: ChessBoard = format!("{fen} | 0 | 0.5").parse().unwrap();
            let flipped = board(&cb);

            for (ours, theirs) in [(0, stm), (1, 1 - stm)] {
                let mut a = threat_features(&flipped, ours);
                let mut b = threat_features(position.board(), theirs);
                a.sort_unstable();
                b.sort_unstable();
                assert_eq!(a, b, "threats of {fen}");

                let mut a = psq_features(&flipped, ours);
                let mut b = psq_features(position.board(), theirs);
                a.sort_unstable();
                b.sort_unstable();
                assert_eq!(a, b, "piece-squares of {fen}");
            }
            assert_eq!(output_bucket(&cb), reference::features::output_bucket(position.board()));
        }
    }

    #[test]
    fn slots_are_padded_with_minus_one() {
        let cb: ChessBoard = "8/8/8/8/8/8/8/K6k w - - 0 1 | 0 | 0.5".parse().unwrap();
        let mut threats = [0; MAX_THREATS];
        let mut psq = [0; MAX_PSQ];
        assert_eq!(fill(&board(&cb), 0, &mut threats, &mut psq), 0);
        assert!(threats.iter().all(|&t| t == -1));
        assert_eq!(psq.iter().filter(|&&p| p >= 0).count(), 2);
    }
}
