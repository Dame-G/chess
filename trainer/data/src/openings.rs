//! Where datagen games start: the standard position, a Chess960 position, a DFRC one (White and Black each get their
//! own Chess960 back rank), or a line of an EPD/FEN book; then a number of random legal plies.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position};

use crate::{game, rng::Rng};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "path")]
pub enum Opening {
    Startpos,
    Chess960,
    Dfrc,
    /// One FEN or EPD per line.
    Book(std::path::PathBuf),
}

/// The back rank (files a to h) of Chess960 position `n` (0..960), by Scharnagl's numbering; 518 is the standard one.
pub fn chess960_back_rank(n: u32) -> [char; 8] {
    assert!(n < 960);
    let mut rank = [' '; 8];
    let (n2, b1) = (n / 4, n % 4);
    rank[(2 * b1 + 1) as usize] = 'B';
    let (n3, b2) = (n2 / 4, n2 % 4);
    rank[(2 * b2) as usize] = 'B';
    let (n4, q) = (n3 / 6, n3 % 6);
    let empty = |rank: &[char; 8]| (0..8).filter(|&i| rank[i] == ' ').collect::<Vec<_>>();
    rank[empty(&rank)[q as usize]] = 'Q';
    const KNIGHTS: [(usize, usize); 10] =
        [(0, 1), (0, 2), (0, 3), (0, 4), (1, 2), (1, 3), (1, 4), (2, 3), (2, 4), (3, 4)];
    let (k1, k2) = KNIGHTS[n4 as usize];
    let free = empty(&rank);
    rank[free[k1]] = 'N';
    rank[free[k2]] = 'N';
    let free = empty(&rank);
    rank[free[0]] = 'R';
    rank[free[1]] = 'K';
    rank[free[2]] = 'R';
    rank
}

/// The FEN of a DFRC start with White on Chess960 position `white` and Black on `black`; Shredder castling rights.
pub fn dfrc_fen(white: u32, black: u32) -> String {
    let w = chess960_back_rank(white);
    let b = chess960_back_rank(black);
    let rooks = |rank: &[char; 8]| -> Vec<char> {
        let files: Vec<char> =
            rank.iter().enumerate().filter(|(_, c)| **c == 'R').map(|(i, _)| (b'a' + i as u8) as char).collect();
        // King side first, as Shredder-FEN usually writes it.
        files.into_iter().rev().collect()
    };
    let castling: String = rooks(&w).iter().map(|c| c.to_ascii_uppercase()).chain(rooks(&b)).collect();
    let white_rank: String = w.iter().collect();
    let black_rank: String = b.iter().map(|c| c.to_ascii_lowercase()).collect();
    format!("{black_rank}/pppppppp/8/8/8/8/PPPPPPPP/{white_rank} w {castling} - 0 1")
}

pub struct Openings {
    kind: Opening,
    book: Vec<String>,
}

impl Openings {
    pub fn new(kind: &Opening) -> Result<Self> {
        let book = match kind {
            Opening::Book(path) => read_book(path)?,
            _ => Vec::new(),
        };
        Ok(Self { kind: kind.clone(), book })
    }

    /// A starting position before the random plies.
    pub fn base(&self, rng: &mut Rng) -> Result<Chess> {
        match &self.kind {
            Opening::Startpos => Ok(game::parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")?),
            Opening::Chess960 => {
                let n = rng.below(960) as u32;
                game::parse_fen(&dfrc_fen(n, n))
            }
            Opening::Dfrc => game::parse_fen(&dfrc_fen(rng.below(960) as u32, rng.below(960) as u32)),
            Opening::Book(_) => game::parse_fen(&self.book[rng.below(self.book.len() as u64) as usize]),
        }
    }

    /// A start: the base position and `plies` random legal moves, retried until the position is not over.
    pub fn start(&self, rng: &mut Rng, plies: u32) -> Result<Chess> {
        'attempt: for _ in 0..1000 {
            let mut pos = self.base(rng)?;
            for _ in 0..plies {
                let moves = pos.legal_moves();
                if moves.is_empty() {
                    continue 'attempt;
                }
                let mv = moves[rng.below(moves.len() as u64) as usize];
                pos.play_unchecked(mv);
            }
            if !pos.legal_moves().is_empty() && !pos.is_insufficient_material() {
                return Ok(pos);
            }
        }
        bail!("no playable opening after 1000 tries")
    }
}

fn read_book(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut lines = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        // EPD has four fields then operations; FEN has six.
        let counters = if fields.len() >= 6 && fields[4].parse::<u32>().is_ok() && fields[5].parse::<u32>().is_ok() {
            format!("{} {}", fields[4], fields[5])
        } else {
            "0 1".to_string()
        };
        let fen = format!("{} {} {} {} {counters}", fields[0], fields[1], fields[2], fields[3]);
        if game::parse_fen(&fen).is_ok() {
            lines.push(fen);
        }
    }
    if lines.is_empty() {
        bail!("{} has no usable position", path.display());
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_is_518() {
        assert_eq!(chess960_back_rank(518).iter().collect::<String>(), "RNBQKBNR");
        assert_eq!(chess960_back_rank(0).iter().collect::<String>(), "BBQNNRKR");
    }

    #[test]
    fn every_dfrc_start_is_legal() {
        for n in 0..960 {
            let pos = game::parse_fen(&dfrc_fen(n, 959 - n)).unwrap();
            assert!(pos.legal_moves().len() >= 18, "{n}");
            assert!(pos.castles().any());
        }
    }

    #[test]
    fn random_plies() {
        let openings = Openings::new(&Opening::Dfrc).unwrap();
        let mut rng = Rng::new(1);
        let pos = openings.start(&mut rng, 8).unwrap();
        assert_eq!(game::ply_of(&pos), 8);
    }
}
