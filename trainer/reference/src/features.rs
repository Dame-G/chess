//! Input features: which of the network's inputs a position switches on, per perspective.
//!
//! Numbering follows Reckless: squares are 0 = a1 … 63 = h8, a piece is `2 × type + colour` (type: pawn 0 … king 5;
//! colour: white 0, black 1), and every index is computed from one perspective (`pov`), whose side is "ours".
//!
//! Both kinds of input are mirrored horizontally when the perspective's king stands on files e–h, and flipped
//! vertically for Black, so the perspective always sees its king on files a–d of its own first ranks.

use std::sync::OnceLock;

use shakmaty::{Bitboard, Board, Color, Square, attacks};

use crate::arch::{INPUT_BUCKETS_LAYOUT, OUTPUT_BUCKETS_LAYOUT, PSQ_INPUTS_PER_BUCKET, THREAT_INPUTS};

/// Piece types in Reckless's order.
pub const PAWN: usize = 0;
pub const KNIGHT: usize = 1;
pub const BISHOP: usize = 2;
pub const ROOK: usize = 3;
pub const QUEEN: usize = 4;
pub const KING: usize = 5;

/// A piece in Reckless's numbering: `2 × type + colour`.
pub const fn piece_index(color: usize, piece_type: usize) -> usize {
    2 * piece_type + color
}

fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

fn reckless_piece(piece: shakmaty::Piece) -> usize {
    piece_index(color_index(piece.color), piece.role as usize - 1)
}

fn shakmaty_piece(piece: usize) -> shakmaty::Piece {
    let color = if piece & 1 == 0 { Color::White } else { Color::Black };
    let role = match piece / 2 {
        PAWN => shakmaty::Role::Pawn,
        KNIGHT => shakmaty::Role::Knight,
        BISHOP => shakmaty::Role::Bishop,
        ROOK => shakmaty::Role::Rook,
        QUEEN => shakmaty::Role::Queen,
        _ => shakmaty::Role::King,
    };
    shakmaty::Piece { color, role }
}

const fn is_kingside(square: usize) -> bool {
    square % 8 >= 4
}

/// The king bucket (0..10) a perspective's king square selects.
pub fn king_bucket(king: usize, pov: usize) -> usize {
    let flip = (7 * is_kingside(king) as usize) ^ (56 * pov);
    INPUT_BUCKETS_LAYOUT[king ^ flip] as usize
}

/// The piece-square input for `piece` (Reckless numbering) on `square`, seen from `pov` with its king on `king`.
///
/// `bucket × 768 + 384 × (piece is theirs) + 64 × type + square`, with the square mirrored and flipped as the king's.
pub fn psq_index(piece: usize, square: usize, king: usize, pov: usize) -> usize {
    let flip = (7 * is_kingside(king) as usize) ^ (56 * pov);
    let (color, piece_type) = (piece & 1, piece / 2);

    INPUT_BUCKETS_LAYOUT[king ^ flip] as usize * PSQ_INPUTS_PER_BUCKET
        + 384 * (color != pov) as usize
        + 64 * piece_type
        + (square ^ flip)
}

/// The tables behind [`threat_index`], built as Reckless's `threats::initialize` builds them.
struct ThreatTables {
    /// Per (attacking, attacked) piece: the base index, or `None` when that pair is never an input.
    pair_base: [[Option<usize>; 12]; 12],
    /// Per (attacking, attacked) piece: whether the pair is counted once only (same type), from the higher square.
    pair_semi_excluded: [[bool; 12]; 12],
    /// Per piece and square: how many (square, target) slots come before it.
    piece_offset: [[usize; 64]; 12],
    /// Per piece, square and target: the target's rank among the piece's empty-board attacks from that square.
    attack_index: Box<[[[u8; 64]; 64]; 12]>,
    /// How many threat inputs there are (66,864).
    total: usize,
}

/// Which attacked piece types each attacking type can see (-1: never an input), in pawn … king order.
#[rustfmt::skip]
const PIECE_INTERACTION_MAP: [[i32; 6]; 6] = [
    [0,  1, -1,  2, -1, -1],
    [0,  1,  2,  3,  4, -1],
    [0,  1,  2,  3, -1, -1],
    [0,  1,  2,  3, -1, -1],
    [0,  1,  2,  3,  4, -1],
    [0,  1,  2,  3, -1, -1],
];

/// How many (colour, attacked type) targets each attacking type has: twice the non-negative entries of its row.
const PIECE_TARGET_COUNT: [usize; 6] = [6, 10, 8, 8, 10, 8];

fn empty_board_attacks(piece: usize, square: usize) -> Bitboard {
    attacks::attacks(Square::new(square as u32), shakmaty_piece(piece), Bitboard::EMPTY)
}

impl ThreatTables {
    fn build() -> Self {
        let mut offset = 0;
        let mut piece_count = [0usize; 12];
        let mut piece_start = [0usize; 12];
        let mut piece_offset = [[0usize; 64]; 12];

        for color in 0..2 {
            for (piece_type, &targets) in PIECE_TARGET_COUNT.iter().enumerate() {
                let piece = piece_index(color, piece_type);
                let mut count = 0;

                for (square, entry) in piece_offset[piece].iter_mut().enumerate() {
                    *entry = count;
                    if piece_type != PAWN || (8..56).contains(&square) {
                        count += empty_board_attacks(piece, square).count();
                    }
                }

                piece_count[piece] = count;
                piece_start[piece] = offset;
                offset += targets * count;
            }
        }

        let mut pair_base = [[None; 12]; 12];
        let mut pair_semi_excluded = [[false; 12]; 12];

        for attacking in 0..12 {
            for attacked in 0..12 {
                let (attacking_type, attacking_color) = (attacking / 2, attacking & 1);
                let (attacked_type, attacked_color) = (attacked / 2, attacked & 1);

                let map = PIECE_INTERACTION_MAP[attacking_type][attacked_type];
                if map >= 0 {
                    let slot = attacked_color * (PIECE_TARGET_COUNT[attacking_type] / 2) + map as usize;
                    pair_base[attacking][attacked] = Some(piece_start[attacking] + slot * piece_count[attacking]);
                }

                let enemy = attacking_color != attacked_color;
                pair_semi_excluded[attacking][attacked] =
                    attacking_type == attacked_type && (enemy || attacking_type != PAWN);
            }
        }

        let mut attack_index = Box::new([[[0u8; 64]; 64]; 12]);
        for (piece, table) in attack_index.iter_mut().enumerate() {
            for (from, row) in table.iter_mut().enumerate() {
                let attacks = empty_board_attacks(piece, from);
                for (to, entry) in row.iter_mut().enumerate() {
                    *entry = (Bitboard((1u64 << to) - 1) & attacks).count() as u8;
                }
            }
        }

        Self { pair_base, pair_semi_excluded, piece_offset, attack_index, total: offset }
    }
}

fn tables() -> &'static ThreatTables {
    static TABLES: OnceLock<ThreatTables> = OnceLock::new();
    TABLES.get_or_init(ThreatTables::build)
}

/// The number of threat inputs as the tables count them; equals [`THREAT_INPUTS`].
pub fn threat_input_count() -> usize {
    tables().total
}

/// The threat input for `piece` on `from` attacking `attacked` on `to`, seen from `pov` (whose king is on the e–h
/// files when `mirrored`). `None` when the pair is not an input: an excluded pair of types (a pawn attacking a
/// bishop, anything attacking a king, …), or the second of a symmetric pair of the same type, which is counted only
/// from the higher square (after flipping and mirroring).
pub fn threat_index(
    piece: usize,
    from: usize,
    attacked: usize,
    to: usize,
    mirrored: bool,
    pov: usize,
) -> Option<usize> {
    let transform = (56 * pov) ^ (7 * mirrored as usize);
    let (from, to) = (from ^ transform, to ^ transform);
    let (attacking, attacked) = (piece ^ pov, attacked ^ pov);

    let tables = tables();
    let base = tables.pair_base[attacking][attacked]?;
    if tables.pair_semi_excluded[attacking][attacked] && from < to {
        return None;
    }

    let index = base + tables.piece_offset[attacking][from] + tables.attack_index[attacking][from][to] as usize;
    debug_assert!(index < THREAT_INPUTS);
    Some(index)
}

/// A board in Reckless's terms: a piece (or none) per square.
fn mailbox(board: &Board) -> [Option<usize>; 64] {
    let mut squares = [None; 64];
    for (square, piece) in board.iter() {
        squares[square.to_usize()] = Some(reckless_piece(piece));
    }
    squares
}

/// The perspective's king square, or `None` when it has no king.
fn king_square(board: &Board, pov: usize) -> Option<usize> {
    let color = if pov == 0 { Color::White } else { Color::Black };
    board.king_of(color).map(Square::to_usize)
}

/// The active piece-square inputs of `board` from `pov` (0 = White, 1 = Black), in square order.
pub fn psq_features(board: &Board, pov: usize) -> Vec<usize> {
    let king = king_square(board, pov).expect("a position needs both kings");
    board.iter().map(|(square, piece)| psq_index(reckless_piece(piece), square.to_usize(), king, pov)).collect()
}

/// The active threat inputs of `board` from `pov`: for every piece and every occupied square it attacks (with the
/// board's occupancy, so sliders stop at the first piece), the pair's input when it has one. Kings attack and are
/// attacked like any piece; attacks on kings are never inputs.
pub fn threat_features(board: &Board, pov: usize) -> Vec<usize> {
    let king = king_square(board, pov).expect("a position needs both kings");
    let mirrored = is_kingside(king);
    let squares = mailbox(board);
    let occupied = board.occupied();

    let mut features = Vec::with_capacity(128);
    for (square, piece) in board.iter() {
        let threats = attacks::attacks(square, piece, occupied) & occupied;
        for target in threats {
            let attacked = squares[target.to_usize()].expect("an occupied square holds a piece");
            if let Some(index) =
                threat_index(reckless_piece(piece), square.to_usize(), attacked, target.to_usize(), mirrored, pov)
            {
                features.push(index);
            }
        }
    }
    features
}

/// The output bucket: by the number of pieces on the board, kings included.
pub fn output_bucket(board: &Board) -> usize {
    OUTPUT_BUCKETS_LAYOUT[board.occupied().count()]
}

#[cfg(test)]
mod tests {
    use shakmaty::{CastlingMode, Chess, Position, fen::Fen};

    use super::*;
    use crate::arch::PSQ_INPUTS;

    fn board(fen: &str) -> Board {
        let pos: Chess = fen.parse::<Fen>().unwrap().into_position(CastlingMode::Standard).unwrap();
        pos.board().clone()
    }

    #[test]
    fn the_tables_count_the_threat_inputs() {
        assert_eq!(threat_input_count(), THREAT_INPUTS);
    }

    #[test]
    fn every_pair_of_squares_maps_into_range_and_without_collisions() {
        // Every (attacking piece, from, attacked piece, to) an empty-board attack allows gives a distinct index.
        let mut seen = vec![false; THREAT_INPUTS];
        let mut count = 0;
        for piece in 0..12 {
            for from in 0..64 {
                if piece / 2 == PAWN && !(8..56).contains(&from) {
                    continue;
                }
                for to in empty_board_attacks(piece, from) {
                    for attacked in 0..12 {
                        if let Some(index) = threat_index(piece, from, attacked, to.to_usize(), false, 0) {
                            assert!(!seen[index], "index {index} given twice");
                            seen[index] = true;
                            count += 1;
                        }
                    }
                }
            }
        }
        // Semi-excluded pairs share the slots of their mirror image, so not every slot is used.
        assert!(count > THREAT_INPUTS / 2 && count <= THREAT_INPUTS);
    }

    #[test]
    fn the_start_position_switches_on_one_input_per_piece() {
        let board = board("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        for pov in 0..2 {
            let features = psq_features(&board, pov);
            assert_eq!(features.len(), 32);
            assert!(features.iter().all(|&f| f < PSQ_INPUTS));
        }
    }

    #[test]
    fn the_perspectives_of_a_mirrored_position_agree() {
        // The same position with colours swapped: White's view of one is Black's view of the other.
        let a = board("r3k2r/pp1n1ppp/2p1pn2/q2p4/1bPP4/2N1PN2/PPQ2PPP/R3KB1R w KQkq - 0 1");
        let mut b = Board::empty();
        for (square, piece) in a.iter() {
            b.set_piece_at(square.flip_vertical(), shakmaty::Piece { color: !piece.color, role: piece.role });
        }
        for pov in 0..2 {
            let mut fa = psq_features(&a, pov);
            let mut fb = psq_features(&b, 1 - pov);
            fa.sort_unstable();
            fb.sort_unstable();
            assert_eq!(fa, fb);

            let mut ta = threat_features(&a, pov);
            let mut tb = threat_features(&b, 1 - pov);
            ta.sort_unstable();
            tb.sort_unstable();
            assert_eq!(ta, tb);
        }
    }

    #[test]
    fn kings_are_bucketed_from_their_own_side() {
        assert_eq!(king_bucket(4, 0), 3); // e1, mirrored to d1
        assert_eq!(king_bucket(60, 1), 3); // e8 for Black
        assert_eq!(king_bucket(6, 0), 1); // g1 → b1
        assert_eq!(king_bucket(36, 0), 9);
    }
}
