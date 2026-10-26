//! Stockfish's binpack (the `.binpack` sets, such as the relabelled ones on HuggingFace), through the `sfbinpack`
//! crate, Disservin's port of Stockfish's reader.
//!
//! A binpack is a stream of entries `(position, move, score, ply, result)`, scores and results from the side to
//! move's point of view, stored as chains: a full position, then moves that follow it. A chain becomes one `Game`
//! here, its result taken from the first entry. Chains do not always run to the end of a game (Stockfish's tools cut
//! them), so a game here is "consecutive positions with one result", which is all the chain formats need.
//!
//! Writing is supported for standard chess only (binpacks have no Chess960 castling).

use std::{fs::File, io::BufWriter, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use sfbinpack::{
    CompressedTrainingDataEntryReader, CompressedTrainingDataEntryWriter, TrainingDataEntry,
    chess::{
        color::Color as SfColor,
        coords::Square as SfSquare,
        r#move::{Move as SfMove, MoveType},
        piece::Piece as SfPiece,
        piecetype::PieceType as SfPieceType,
        position::Position as SfPosition,
    },
};
use shakmaty::{CastlingSide, Chess, Color, File as BoardFile, Move, Position, Rank, Role, Square};

use crate::game::{self, Entry, Game, GameResult};

pub struct Reader {
    inner: Option<CompressedTrainingDataEntryReader<File>>,
    /// Chains that could not be read (a position or move that does not convert), skipped.
    pub skipped: u64,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let inner = match CompressedTrainingDataEntryReader::new(file) {
            Ok(reader) => Some(reader),
            Err(sfbinpack::CompressedReaderError::EndOfFile) => None,
            Err(e) => return Err(anyhow!("{}: {e}", path.display())),
        };
        Ok(Self { inner, skipped: 0 })
    }

    pub fn next_game(&mut self) -> Result<Option<Game>> {
        let Some(reader) = self.inner.as_mut() else { return Ok(None) };
        loop {
            if !reader.has_next() {
                return Ok(None);
            }
            let first = reader.next();
            let mut chain = vec![first];
            while reader.has_next() && reader.is_next_entry_continuation() {
                chain.push(reader.next());
            }
            match chain_to_game(&chain) {
                Ok(game) => return Ok(Some(game)),
                Err(_) => self.skipped += 1,
            }
        }
    }
}

fn chain_to_game(chain: &[TrainingDataEntry]) -> Result<Game> {
    let first = &chain[0];
    let fen = first.pos.fen().map_err(|e| anyhow!("binpack position: {e:?}"))?;
    let start = game::parse_fen(&fen)?;
    let stm_result = first.result.signum() as i8;
    let white_result = if start.turn() == Color::White { stm_result } else { -stm_result };
    let result = GameResult::from_u8((white_result + 1) as u8)?;
    let mut game = Game::new(start.clone(), result);
    let mut pos = start;
    for entry in chain {
        let Ok(mv) = convert_move(&pos, entry.mv) else { break };
        game.entries.push(Entry { mv, score: entry.score });
        pos.play_unchecked(mv);
    }
    if game.is_empty() {
        bail!("binpack chain has no readable move");
    }
    Ok(game)
}

fn convert_move(pos: &Chess, mv: SfMove) -> Result<Move> {
    let from = Square::new(mv.from().index());
    let to = Square::new(mv.to().index());
    let promotion = match mv.mtype() {
        MoveType::Promotion => Some(match mv.promoted_piece().piece_type() {
            SfPieceType::Knight => Role::Knight,
            SfPieceType::Bishop => Role::Bishop,
            SfPieceType::Rook => Role::Rook,
            _ => Role::Queen,
        }),
        _ => None,
    };
    pos.legal_moves()
        .into_iter()
        .find(|m| m.from() == Some(from) && m.to() == to && m.promotion() == promotion)
        .ok_or_else(|| anyhow!("binpack move {} is not legal in {}", mv.as_uci(), game::fen(pos)))
}

pub struct Writer {
    inner: CompressedTrainingDataEntryWriter<BufWriter<File>>,
}

impl Writer {
    pub fn create(path: &Path) -> Result<Self> {
        let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
        let inner = CompressedTrainingDataEntryWriter::new(BufWriter::new(file)).map_err(|e| anyhow!("{e:?}"))?;
        Ok(Self { inner })
    }

    pub fn write_game(&mut self, game: &Game) -> Result<()> {
        check_standard(&game.start)?;
        let mut entries = Vec::with_capacity(game.len());
        let mut failed = None;
        game.for_each_position(|pos, entry, _| {
            if failed.is_some() {
                return;
            }
            match to_entry(pos, entry, game.result) {
                Ok(e) => entries.push(e),
                Err(e) => failed = Some(e),
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        for entry in &entries {
            self.inner.write_entry(entry).map_err(|e| anyhow!("{e:?}"))?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.inner.flush_and_end();
        Ok(())
    }
}

fn check_standard(pos: &Chess) -> Result<()> {
    for color in [Color::White, Color::Black] {
        let rank = if color == Color::White { Rank::First } else { Rank::Eighth };
        for (side, file) in [(CastlingSide::KingSide, BoardFile::H), (CastlingSide::QueenSide, BoardFile::A)] {
            if let Some(rook) = pos.castles().rook(color, side) {
                let king = pos.board().king_of(color);
                if rook != Square::from_coords(file, rank) || king != Some(Square::from_coords(BoardFile::E, rank)) {
                    bail!("binpack holds standard chess only, not {}", game::fen(pos));
                }
            }
        }
    }
    Ok(())
}

fn to_entry(pos: &Chess, entry: &Entry, result: GameResult) -> Result<TrainingDataEntry> {
    let sf_pos = SfPosition::from_fen(&game::fen(pos)).map_err(|e| anyhow!("binpack position: {e:?}"))?;
    let stm = if pos.turn() == Color::White { SfColor::White } else { SfColor::Black };
    let from = SfSquare::new(entry.mv.from().expect("no drops") as u32);
    let to = SfSquare::new(entry.mv.to() as u32);
    let mv = match entry.mv {
        Move::Castle { .. } => SfMove::castle(from, to),
        Move::EnPassant { .. } => SfMove::en_passant(from, to),
        Move::Normal { promotion: Some(role), .. } => {
            let piece_type = match role {
                Role::Knight => SfPieceType::Knight,
                Role::Bishop => SfPieceType::Bishop,
                Role::Rook => SfPieceType::Rook,
                _ => SfPieceType::Queen,
            };
            SfMove::promotion(from, to, SfPiece::new(piece_type, stm))
        }
        _ => SfMove::normal(from, to),
    };
    Ok(TrainingDataEntry {
        pos: sf_pos,
        mv,
        score: entry.score,
        ply: game::ply_of(pos).min(u16::MAX as u32) as u16,
        result: i16::from(result.for_color(pos.turn())),
    })
}
