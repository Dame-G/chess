//! Viriformat: Viridithas's game chains, which bullet loads (`ViriBinpackLoader`).
//!
//! A game is a 32-byte marlinformat header followed by `(move: u16, eval: i16)` pairs, little-endian, ended by four
//! zero bytes.
//!
//! The header: occupancy (u64), then one nibble per occupied square in ascending order (piece type 0..5 for pawn to
//! king, 6 for a rook that can still castle, bit 3 set for Black), then `stm << 7 | ep square` (64 when none),
//! the halfmove clock (u8), the fullmove number (u16), an eval (i16, 0 here), the result from White's side (u8:
//! 0, 1, 2) and an unused byte.
//!
//! A move is `from | to << 6 | flags`: bits 14-15 are 01 for en passant, 10 for castling (`to` is the rook's square),
//! 11 for a promotion with the piece in bits 12-13 (knight 0 to queen 3). Evals are **from White's side**.

use std::io::{BufRead, Read, Write};

use anyhow::{Context, Result, anyhow, bail};
use shakmaty::{
    Bitboard, Board, ByColor, CastlingSide, Chess, Color, EnPassantMode, Move, Piece, Position, RemainingChecks, Role,
    Setup, Square,
};

use crate::game::{Entry, Game, GameResult};

pub const HEADER_SIZE: usize = 32;
const UNMOVED_ROOK: u8 = 6;

pub fn pack_header(pos: &Chess, result: GameResult) -> [u8; HEADER_SIZE] {
    let board = pos.board();
    let castles = pos.castles();
    let mut out = [0u8; HEADER_SIZE];
    out[0..8].copy_from_slice(&board.occupied().0.to_le_bytes());
    for (i, sq) in board.occupied().into_iter().enumerate() {
        let piece = board.piece_at(sq).expect("occupied");
        let mut code = role_index(piece.role);
        if piece.role == Role::Rook
            && [CastlingSide::KingSide, CastlingSide::QueenSide]
                .iter()
                .any(|&side| castles.rook(piece.color, side) == Some(sq))
        {
            code = UNMOVED_ROOK;
        }
        let nibble = code | (u8::from(piece.color == Color::Black) << 3);
        out[8 + i / 2] |= nibble << ((i % 2) * 4);
    }
    let ep = pos.ep_square(EnPassantMode::Legal).map_or(64, |sq| sq as u8);
    out[24] = (u8::from(pos.turn() == Color::Black) << 7) | ep;
    out[25] = pos.halfmoves().min(255) as u8;
    out[26..28].copy_from_slice(&(pos.fullmoves().get().min(u16::MAX as u32) as u16).to_le_bytes());
    // 28..30: the header's eval, unused by games.
    out[30] = result.as_u8();
    out
}

pub fn unpack_header(bytes: &[u8; HEADER_SIZE]) -> Result<(Chess, GameResult)> {
    let occupancy = Bitboard(u64::from_le_bytes(bytes[0..8].try_into().unwrap()));
    if occupancy.count() > 32 {
        bail!("viriformat header has {} pieces", occupancy.count());
    }
    let mut board = Board::empty();
    let mut castling = Bitboard::EMPTY;
    for (i, sq) in occupancy.into_iter().enumerate() {
        let nibble = (bytes[8 + i / 2] >> ((i % 2) * 4)) & 0xF;
        let color = if nibble & 8 != 0 { Color::Black } else { Color::White };
        let role = match nibble & 7 {
            UNMOVED_ROOK => {
                castling.add(sq);
                Role::Rook
            }
            code => role_from_index(code).ok_or_else(|| anyhow!("invalid piece code {code}"))?,
        };
        board.set_piece_at(sq, Piece { color, role });
    }
    let turn = if bytes[24] & 0x80 != 0 { Color::Black } else { Color::White };
    let ep = bytes[24] & 0x7F;
    let fullmoves = u16::from_le_bytes([bytes[26], bytes[27]]).max(1);
    let setup = Setup {
        board,
        promoted: Bitboard::EMPTY,
        pockets: None,
        turn,
        castling_rights: castling,
        ep_square: if ep < 64 { Some(Square::new(u32::from(ep))) } else { None },
        remaining_checks: None::<ByColor<RemainingChecks>>,
        halfmoves: u32::from(bytes[25]),
        fullmoves: std::num::NonZeroU32::new(u32::from(fullmoves)).unwrap(),
    };
    let pos = crate::game::from_setup(setup, false).context("viriformat header is not a legal position")?;
    Ok((pos, GameResult::from_u8(bytes[30])?))
}

pub fn encode_move(mv: Move) -> u16 {
    let from = mv.from().expect("not a drop") as u16;
    let to = mv.to() as u16;
    let flags = match mv {
        Move::EnPassant { .. } => 0b01 << 14,
        Move::Castle { .. } => 0b10 << 14,
        Move::Normal { promotion: Some(role), .. } => (0b11 << 14) | ((u16::from(role_index(role)) - 1) << 12),
        _ => 0,
    };
    from | (to << 6) | flags
}

pub fn decode_move(pos: &Chess, raw: u16) -> Result<Move> {
    pos.legal_moves()
        .into_iter()
        .find(|&mv| encode_move(mv) == raw)
        .ok_or_else(|| anyhow!("viriformat move {raw:#06x} is not legal in {}", crate::game::fen(pos)))
}

pub fn write_game(out: &mut impl Write, game: &Game) -> Result<()> {
    out.write_all(&encode_game(game))?;
    Ok(())
}

pub fn encode_game(game: &Game) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE + 4 * (game.len() + 1));
    buf.extend_from_slice(&pack_header(&game.start, game.result));
    let mut white_to_move = game.start.turn() == Color::White;
    for entry in &game.entries {
        let white_score = if white_to_move { entry.score } else { entry.score.saturating_neg() };
        buf.extend_from_slice(&encode_move(entry.mv).to_le_bytes());
        buf.extend_from_slice(&white_score.to_le_bytes());
        white_to_move = !white_to_move;
    }
    buf.extend_from_slice(&[0; 4]);
    buf
}

/// Reads the next game, or `None` at a clean end of the input.
pub fn read_game(input: &mut impl BufRead) -> Result<Option<Game>> {
    let Some(raw) = read_raw(input)? else { return Ok(None) };
    decode_game(&raw).map(Some)
}

pub fn decode_game(raw: &[u8]) -> Result<Game> {
    let header: &[u8; HEADER_SIZE] = raw[..HEADER_SIZE].try_into().context("short viriformat game")?;
    let (start, result) = unpack_header(header)?;
    let mut game = Game::new(start.clone(), result);
    let mut pos = start;
    for pair in raw[HEADER_SIZE..raw.len() - 4].as_chunks::<4>().0 {
        let mv = decode_move(&pos, u16::from_le_bytes([pair[0], pair[1]]))?;
        let white_score = i16::from_le_bytes([pair[2], pair[3]]);
        let score = if pos.turn() == Color::White { white_score } else { white_score.saturating_neg() };
        game.entries.push(Entry { mv, score });
        pos.play_unchecked(mv);
    }
    Ok(game)
}

/// The bytes of the next game, header to terminator, without decoding them.
pub fn read_raw(input: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    if input.fill_buf()?.is_empty() {
        return Ok(None);
    }
    let mut raw = vec![0u8; HEADER_SIZE];
    input.read_exact(&mut raw).context("truncated viriformat header")?;
    loop {
        let mut pair = [0u8; 4];
        input.read_exact(&mut pair).context("truncated viriformat game")?;
        raw.extend_from_slice(&pair);
        if pair == [0; 4] {
            return Ok(Some(raw));
        }
    }
}

/// Positions and games of a viriformat file, from the bytes alone.
pub fn count(input: &mut impl Read) -> Result<(u64, u64)> {
    let mut input = std::io::BufReader::with_capacity(1 << 20, input);
    let (mut positions, mut games) = (0, 0);
    while let Some(raw) = read_raw(&mut input)? {
        positions += ((raw.len() - HEADER_SIZE) / 4 - 1) as u64;
        games += 1;
    }
    Ok((positions, games))
}

pub(crate) fn role_index(role: Role) -> u8 {
    role as u8 - 1
}

pub(crate) fn role_from_index(index: u8) -> Option<Role> {
    match index {
        0 => Some(Role::Pawn),
        1 => Some(Role::Knight),
        2 => Some(Role::Bishop),
        3 => Some(Role::Rook),
        4 => Some(Role::Queen),
        5 => Some(Role::King),
        _ => None,
    }
}
