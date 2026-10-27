//! bulletformat: bullet's own 32-byte positions (`bulletformat::ChessBoard`), one per position, no games.
//!
//! The board is stored from the side to move's point of view (flipped when Black is to move), with its score and
//! result also from that side. Because positions stand alone, this is the format that per-position filters (checks,
//! captures, score caps) can write into; the chain formats keep every position of a game and leave that filtering to
//! the loader.

use std::io::{Read, Write};

use anyhow::{Result, anyhow};
use bulletformat::ChessBoard;
use shakmaty::{Chess, Color, Position, Role};

use crate::game::GameResult;

pub const SIZE: usize = 32;

/// `score` and `result` are White's.
pub fn encode(pos: &Chess, white_score: i16, result: GameResult) -> Result<[u8; SIZE]> {
    let board = pos.board();
    let bbs = [
        board.white().0,
        board.black().0,
        board.by_role(Role::Pawn).0,
        board.by_role(Role::Knight).0,
        board.by_role(Role::Bishop).0,
        board.by_role(Role::Rook).0,
        board.by_role(Role::Queen).0,
        board.by_role(Role::King).0,
    ];
    let stm = usize::from(pos.turn() == Color::Black);
    let cb = ChessBoard::from_raw(bbs, stm, white_score, f32::from(result.as_u8()) / 2.0).map_err(|e| anyhow!(e))?;
    let mut out = [0u8; SIZE];
    out[0..8].copy_from_slice(&cb.occ.to_le_bytes());
    out[8..24].copy_from_slice(&cb.pcs);
    out[24..26].copy_from_slice(&cb.score.to_le_bytes());
    out[26] = cb.result;
    out[27] = cb.ksq;
    out[28] = cb.opp_ksq;
    Ok(out)
}

/// What stats need from a stored position: its score and result (side to move's), and how many pieces.
#[derive(Copy, Clone, Debug)]
pub struct Decoded {
    pub score: i16,
    /// 0 loss, 1 draw, 2 win, for the side to move.
    pub result: u8,
    pub pieces: u32,
}

pub fn decode(bytes: &[u8; SIZE]) -> Decoded {
    let occ = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    Decoded { score: i16::from_le_bytes([bytes[24], bytes[25]]), result: bytes[26], pieces: occ.count_ones() }
}

pub fn write(out: &mut impl Write, record: &[u8; SIZE]) -> Result<()> {
    out.write_all(record)?;
    Ok(())
}

pub fn read(input: &mut impl Read) -> Result<Option<[u8; SIZE]>> {
    let mut buf = [0u8; SIZE];
    let mut filled = 0;
    while filled < SIZE {
        let n = input.read(&mut buf[filled..])?;
        if n == 0 {
            return if filled == 0 { Ok(None) } else { Err(anyhow!("truncated bulletformat record")) };
        }
        filled += n;
    }
    Ok(Some(buf))
}
