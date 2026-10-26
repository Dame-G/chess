//! Which positions are worth training on, and how games are trimmed.
//!
//! Two kinds:
//!
//! - **Position filters** judge one position: in check, a tactical best move, too early, too few pieces, a score too
//!   large. Chain formats (viriformat, rbinpack, binpack) keep every position of a game, so these apply where
//!   positions stand alone: when writing bulletformat, in `stats` (how many pass), and in the trainer's loader (bullet
//!   filters viriformat as it loads it).
//! - **Game transforms** change games themselves and work for every format: drop the first plies (the game then
//!   starts later), cut the tail once the score is decisive (adjudicated or mating play), drop short games.
//!
//! The defaults are Reckless's (`src/tools/rescore.rs` and `stats.rs` on `nnue-tools-rsn-3`): a score under 4096 in
//! absolute value, no capture or promotion as the move, not in check.

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position};

use crate::game::{Entry, Game, is_tactical, ply_of};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PositionFilter {
    /// Skip positions earlier than this ply of the game (counted from move 1).
    pub min_ply: u32,
    /// Skip positions with fewer pieces (kings included).
    pub min_pieces: u32,
    /// Skip positions whose absolute score is at least this.
    pub max_score: u16,
    /// Skip positions where the move played captures or promotes.
    pub skip_tactical: bool,
    /// Skip positions where the side to move is in check.
    pub skip_check: bool,
}

impl Default for PositionFilter {
    fn default() -> Self {
        Self::RECKLESS
    }
}

impl PositionFilter {
    /// Reckless's filter.
    pub const RECKLESS: Self =
        Self { min_ply: 0, min_pieces: 0, max_score: 4096, skip_tactical: true, skip_check: true };

    /// Keeps everything.
    pub const NONE: Self =
        Self { min_ply: 0, min_pieces: 0, max_score: u16::MAX, skip_tactical: false, skip_check: false };

    pub fn keep(&self, pos: &Chess, entry: &Entry) -> bool {
        if ply_of(pos) < self.min_ply {
            return false;
        }
        if (pos.board().occupied().count() as u32) < self.min_pieces {
            return false;
        }
        if entry.score.unsigned_abs() >= self.max_score {
            return false;
        }
        if self.skip_tactical && is_tactical(entry.mv) {
            return false;
        }
        if self.skip_check && pos.is_check() {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameTransform {
    /// Drop this many plies from the start of every game.
    pub skip_plies: u32,
    /// Cut each game at the first position whose absolute score reaches this (0: never).
    pub cut_tail_at: u16,
    /// Drop games left with fewer positions than this.
    pub min_positions: u32,
}

impl GameTransform {
    pub fn is_identity(&self) -> bool {
        self.skip_plies == 0 && self.cut_tail_at == 0 && self.min_positions == 0
    }

    /// The game transformed, or `None` when it is dropped.
    pub fn apply(&self, game: &Game) -> Option<Game> {
        let mut game = if self.skip_plies > 0 { game.skip(self.skip_plies as usize) } else { game.clone() };
        if self.cut_tail_at > 0
            && let Some(at) = game.entries.iter().position(|e| e.score.unsigned_abs() >= self.cut_tail_at)
        {
            game.entries.truncate(at);
        }
        if game.is_empty() || game.len() < self.min_positions as usize {
            return None;
        }
        Some(game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{GameResult, parse_fen, parse_uci};

    fn game(moves: &[(&str, i16)]) -> Game {
        let start = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mut g = Game::new(start, GameResult::Draw);
        let mut pos = g.start.clone();
        for &(uci, score) in moves {
            let mv = parse_uci(&pos, uci).unwrap();
            g.entries.push(Entry { mv, score });
            pos.play_unchecked(mv);
        }
        g
    }

    #[test]
    fn reckless_filter() {
        let g = game(&[("e2e4", 30), ("d7d5", -25), ("e4d5", 5000), ("d8d5", -40)]);
        let mut kept = Vec::new();
        g.for_each_position(|pos, e, i| {
            if PositionFilter::RECKLESS.keep(pos, e) {
                kept.push(i);
            }
        });
        // e4xd5 is a capture (and its score too large); Qxd5 is a capture.
        assert_eq!(kept, vec![0, 1]);
    }

    #[test]
    fn transforms() {
        let g = game(&[("e2e4", 30), ("d7d5", -25), ("e4d5", 5000), ("d8d5", -40)]);
        let t = GameTransform { skip_plies: 1, cut_tail_at: 3000, min_positions: 0 };
        let out = t.apply(&g).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out.entries[0].score, -25);
        assert!(GameTransform { min_positions: 5, ..Default::default() }.apply(&g).is_none());
    }
}
