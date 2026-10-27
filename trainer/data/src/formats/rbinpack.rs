//! Reckless's rbinpack, as written by the tools on its `nnue-tools-rsn-3` branch (`src/tools/binpack.rs`).
//!
//! A game is a record: its length (u32, the bytes that follow), a 72-byte position, then 4 bytes per move.
//!
//! The position is Reckless's `#[repr(C)]` struct, little-endian: six piece bitboards (pawn to king, u64 each), two
//! colour bitboards (White, Black), the side to move (u8: 0 White), the en passant square (u8, 64 for none), castling
//! rights (u8: 1 White king side, 2 White queen side, 4 Black king side, 8 Black queen side), the halfmove clock
//! (u8), the fullmove number (u16), the result from White's side (u8: 0, 1, 2) and an unused byte.
//!
//! A move is Reckless's `Move`: `from | to << 6 | kind << 12`, kind 0 quiet, 1 double push, 2 castling (`to` is the
//! king's landing square, G or C), 4 capture, 5 en passant, 8..11 promotion to N B R Q, 12..15 the same capturing.
//! Then the score (i16) **from the side to move's point of view**.
//!
//! Castling rooks are found the way Reckless finds them: from the corner inwards, the first rook of that colour.

use std::io::{BufRead, Read, Write};

use anyhow::{Context, Result, anyhow, bail};
use shakmaty::{
    Bitboard, Board, ByColor, CastlingSide, Chess, Color, EnPassantMode, File, Move, Piece, Position, Rank,
    RemainingChecks, Role, Setup, Square,
};

use crate::game::{Entry, Game, GameResult};

pub const POSITION_SIZE: usize = 72;
pub const ENTRY_SIZE: usize = 4;

const WHITE_KINGSIDE: u8 = 1;
const WHITE_QUEENSIDE: u8 = 2;
const BLACK_KINGSIDE: u8 = 4;
const BLACK_QUEENSIDE: u8 = 8;

fn castling_flag(color: Color, side: CastlingSide) -> u8 {
    match (color, side) {
        (Color::White, CastlingSide::KingSide) => WHITE_KINGSIDE,
        (Color::White, CastlingSide::QueenSide) => WHITE_QUEENSIDE,
        (Color::Black, CastlingSide::KingSide) => BLACK_KINGSIDE,
        (Color::Black, CastlingSide::QueenSide) => BLACK_QUEENSIDE,
    }
}

pub fn pack_position(pos: &Chess, result: GameResult) -> [u8; POSITION_SIZE] {
    let board = pos.board();
    let mut out = [0u8; POSITION_SIZE];
    let roles = [Role::Pawn, Role::Knight, Role::Bishop, Role::Rook, Role::Queen, Role::King];
    for (i, role) in roles.into_iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&board.by_role(role).0.to_le_bytes());
    }
    out[48..56].copy_from_slice(&board.white().0.to_le_bytes());
    out[56..64].copy_from_slice(&board.black().0.to_le_bytes());
    out[64] = u8::from(pos.turn() == Color::Black);
    out[65] = pos.ep_square(EnPassantMode::Legal).map_or(64, |sq| sq as u8);
    let mut castling = 0;
    for color in [Color::White, Color::Black] {
        for side in [CastlingSide::KingSide, CastlingSide::QueenSide] {
            if pos.castles().has(color, side) {
                castling |= castling_flag(color, side);
            }
        }
    }
    out[66] = castling;
    out[67] = pos.halfmoves().min(255) as u8;
    out[68..70].copy_from_slice(&(pos.fullmoves().get().min(u16::MAX as u32) as u16).to_le_bytes());
    out[70] = result.as_u8();
    out
}

pub fn unpack_position(bytes: &[u8]) -> Result<(Chess, GameResult)> {
    if bytes.len() < POSITION_SIZE {
        bail!("short rbinpack position");
    }
    let bb = |i: usize| Bitboard(u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap()));
    let roles = [Role::Pawn, Role::Knight, Role::Bishop, Role::Rook, Role::Queen, Role::King];
    let (white, black) = (bb(6), bb(7));
    let mut board = Board::empty();
    for (i, role) in roles.into_iter().enumerate() {
        for sq in bb(i) {
            let color = if white.contains(sq) {
                Color::White
            } else if black.contains(sq) {
                Color::Black
            } else {
                bail!("rbinpack piece on {sq} has no colour");
            };
            board.set_piece_at(sq, Piece { color, role });
        }
    }
    let flags = bytes[66];
    let mut castling = Bitboard::EMPTY;
    for color in [Color::White, Color::Black] {
        let rank = if color == Color::White { Rank::First } else { Rank::Eighth };
        let rook = Piece { color, role: Role::Rook };
        for side in [CastlingSide::KingSide, CastlingSide::QueenSide] {
            if flags & castling_flag(color, side) == 0 {
                continue;
            }
            let files: Vec<u32> =
                if side == CastlingSide::KingSide { (0..8).rev().collect() } else { (0..8).collect() };
            if let Some(sq) = files
                .into_iter()
                .map(|f| Square::from_coords(File::new(f), rank))
                .find(|&sq| board.piece_at(sq) == Some(rook))
            {
                castling.add(sq);
            }
        }
    }
    let ep = bytes[65];
    let setup = Setup {
        board,
        promoted: Bitboard::EMPTY,
        pockets: None,
        turn: if bytes[64] == 0 { Color::White } else { Color::Black },
        castling_rights: castling,
        ep_square: if ep < 64 { Some(Square::new(u32::from(ep))) } else { None },
        remaining_checks: None::<ByColor<RemainingChecks>>,
        halfmoves: u32::from(bytes[67]),
        fullmoves: std::num::NonZeroU32::new(u32::from(u16::from_le_bytes([bytes[68], bytes[69]]).max(1))).unwrap(),
    };
    let pos = crate::game::from_setup(setup, true).context("rbinpack position is not legal")?;
    Ok((pos, GameResult::from_u8(bytes[70])?))
}

pub fn encode_move(mv: Move) -> u16 {
    let (from, to, kind): (Square, Square, u16) = match mv {
        Move::Normal { role, from, capture, to, promotion } => {
            let kind = match (promotion, capture.is_some()) {
                (Some(p), capture) => 8 + (p as u16 - 2) + if capture { 4 } else { 0 },
                (None, true) => 4,
                (None, false) if role == Role::Pawn && from.distance(to) == 2 => 1,
                (None, false) => 0,
            };
            (from, to, kind)
        }
        Move::EnPassant { from, to } => (from, to, 5),
        Move::Castle { king, rook } => {
            let file = if rook > king { File::G } else { File::C };
            (king, Square::from_coords(file, king.rank()), 2)
        }
        Move::Put { .. } => unreachable!("no drops in chess"),
    };
    from as u16 | ((to as u16) << 6) | (kind << 12)
}

pub fn decode_move(pos: &Chess, raw: u16) -> Result<Move> {
    pos.legal_moves()
        .into_iter()
        .find(|&mv| encode_move(mv) == raw)
        .ok_or_else(|| anyhow!("rbinpack move {raw:#06x} is not legal in {}", crate::game::fen(pos)))
}

pub fn encode_game(game: &Game) -> Vec<u8> {
    let length = POSITION_SIZE + ENTRY_SIZE * game.len();
    let mut buf = Vec::with_capacity(4 + length);
    buf.extend_from_slice(&(length as u32).to_le_bytes());
    buf.extend_from_slice(&pack_position(&game.start, game.result));
    for entry in &game.entries {
        buf.extend_from_slice(&encode_move(entry.mv).to_le_bytes());
        buf.extend_from_slice(&entry.score.to_le_bytes());
    }
    buf
}

pub fn write_game(out: &mut impl Write, game: &Game) -> Result<()> {
    out.write_all(&encode_game(game))?;
    Ok(())
}

/// The bytes of the next record, length prefix included.
pub fn read_raw(input: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    if input.fill_buf()?.is_empty() {
        return Ok(None);
    }
    let mut header = [0u8; 4];
    input.read_exact(&mut header).context("truncated rbinpack record")?;
    let length = u32::from_le_bytes(header) as usize;
    if length < POSITION_SIZE || !(length - POSITION_SIZE).is_multiple_of(ENTRY_SIZE) {
        bail!("invalid rbinpack record length {length}");
    }
    let mut raw = vec![0u8; 4 + length];
    raw[..4].copy_from_slice(&header);
    input.read_exact(&mut raw[4..]).context("truncated rbinpack record")?;
    Ok(Some(raw))
}

pub fn decode_game(raw: &[u8]) -> Result<Game> {
    let body = &raw[4..];
    let (start, result) = unpack_position(&body[..POSITION_SIZE])?;
    let mut game = Game::new(start.clone(), result);
    let mut pos = start;
    for pair in body[POSITION_SIZE..].as_chunks::<ENTRY_SIZE>().0 {
        let mv = decode_move(&pos, u16::from_le_bytes([pair[0], pair[1]]))?;
        game.entries.push(Entry { mv, score: i16::from_le_bytes([pair[2], pair[3]]) });
        pos.play_unchecked(mv);
    }
    Ok(game)
}

pub fn read_game(input: &mut impl BufRead) -> Result<Option<Game>> {
    let Some(raw) = read_raw(input)? else { return Ok(None) };
    decode_game(&raw).map(Some)
}

pub fn count(input: &mut impl Read) -> Result<(u64, u64)> {
    let mut input = std::io::BufReader::with_capacity(1 << 20, input);
    let (mut positions, mut games) = (0, 0);
    while let Some(raw) = read_raw(&mut input)? {
        positions += ((raw.len() - 4 - POSITION_SIZE) / ENTRY_SIZE) as u64;
        games += 1;
    }
    Ok((positions, games))
}
