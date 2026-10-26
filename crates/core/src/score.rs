//! Engine scores. Every score that is stored or sent is from **White's point of view**: one rule, no exceptions, and
//! the UI flips for display. Engines report from the side to move; whoever reads them converts with
//! [`Score::from_side`] before the score goes anywhere.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::Side;

/// A score: centipawns, or a forced mate in so many moves. `{"cp": 31}` or `{"mate": -3}`. From White's point of view
/// unless a function says otherwise; `mate: 0` means the side to move is checkmated, from either point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Score {
    Cp(i32),
    Mate(i32),
}

/// Lichess caps centipawns at ±1000 before turning them into win percentages and ACPL; a mate counts as the cap.
pub const CP_CEILING: i32 = 1000;

impl Score {
    /// The same score from the other side.
    pub fn flip(self) -> Self {
        match self {
            Self::Cp(cp) => Self::Cp(-cp),
            Self::Mate(m) => Self::Mate(-m),
        }
    }

    /// A score from `side`'s point of view turned into White's (an engine reports from the side to move).
    pub fn from_side(self, side: Side) -> Self {
        match side {
            Side::White => self,
            Side::Black => self.flip(),
        }
    }

    /// This White score from `side`'s point of view.
    pub fn for_side(self, side: Side) -> Self {
        self.from_side(side)
    }

    pub fn is_mate(self) -> bool {
        matches!(self, Self::Mate(_))
    }

    /// Centipawns, a mate counting as `±(30000 − moves)`: how the tests' adjudication and the draw-offer rule read an
    /// engine's score.
    pub fn centipawns(self) -> i32 {
        match self {
            Self::Cp(cp) => cp,
            Self::Mate(m) if m > 0 => 30_000 - m,
            Self::Mate(m) => -30_000 - m,
        }
    }

    /// Centipawns within ±1000, a mate as the cap, as Lichess reads scores for win percentages. A `mate: 0` here is
    /// read as the side whose view this is being mated.
    pub fn capped(self) -> i32 {
        match self {
            Self::Cp(cp) => cp.clamp(-CP_CEILING, CP_CEILING),
            Self::Mate(m) if m > 0 => CP_CEILING,
            Self::Mate(_) => -CP_CEILING,
        }
    }

    /// The win percentage, 0 to 100, of the side whose view this score is (see [`win_percent`]).
    pub fn win_percent(self) -> f64 {
        win_percent(self.capped())
    }

    /// As PGN comments write it, from whichever side's view it is: `+0.31`, `-1.50`, `+M5`, `-M3`.
    pub fn pgn(self) -> String {
        match self {
            Self::Cp(cp) => format!("{}{:.2}", if cp < 0 { "-" } else { "+" }, f64::from(cp.abs()) / 100.0),
            Self::Mate(m) => format!("{}M{}", if m < 0 { "-" } else { "+" }, m.abs()),
        }
    }

    /// Reads a PGN comment's score (`+0.31`, `-1.5`, `+M5`, `-M3`, `#-3`, `0.00`).
    pub fn parse_pgn(text: &str) -> Option<Self> {
        let t = text.trim();
        let (negative, rest) = match t.as_bytes().first()? {
            b'-' => (true, &t[1..]),
            b'+' => (false, &t[1..]),
            _ => (false, t),
        };
        let mate = rest.strip_prefix('M').or_else(|| rest.strip_prefix('#'));
        if let Some(m) = mate {
            // `#-3` writes the sign after the hash, as Lichess's `%eval` does.
            let n: i32 = m.parse().ok()?;
            return Some(Self::Mate(if negative { -n.abs() } else { n }));
        }
        let pawns: f64 = rest.parse().ok()?;
        if !pawns.is_finite() {
            return None;
        }
        let cp = (pawns * 100.0).round() as i32;
        Some(Self::Cp(if negative { -cp } else { cp }))
    }
}

/// Lichess's winning chances in [-1, 1] from centipawns (lila PR #11148): `2 / (1 + e^(-0.00368208 × cp)) - 1`.
pub fn winning_chances(cp: i32) -> f64 {
    (2.0 / (1.0 + (-0.003_682_08 * f64::from(cp)).exp()) - 1.0).clamp(-1.0, 1.0)
}

/// Lichess's win percentage, 0 to 100, from centipawns capped at ±1000: `50 + 50 × winning_chances`.
pub fn win_percent(cp: i32) -> f64 {
    50.0 + 50.0 * winning_chances(cp.clamp(-CP_CEILING, CP_CEILING))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_serialize_as_lichess_and_the_old_harness_wrote_them() {
        assert_eq!(serde_json::to_value(Score::Cp(31)).unwrap(), serde_json::json!({ "cp": 31 }));
        assert_eq!(serde_json::to_value(Score::Mate(-3)).unwrap(), serde_json::json!({ "mate": -3 }));
        assert_eq!(serde_json::from_value::<Score>(serde_json::json!({ "mate": 2 })).unwrap(), Score::Mate(2));
    }

    #[test]
    fn flips_between_points_of_view() {
        assert_eq!(Score::Cp(31).from_side(Side::Black), Score::Cp(-31));
        assert_eq!(Score::Mate(3).for_side(Side::Black), Score::Mate(-3));
        assert_eq!(Score::Mate(0).flip(), Score::Mate(0));
        assert_eq!(Score::Cp(-5).from_side(Side::White), Score::Cp(-5));
    }

    #[test]
    fn win_percent_follows_lichess() {
        assert_eq!(win_percent(0), 50.0);
        assert!((win_percent(100) - 59.10).abs() < 0.01);
        assert!((win_percent(-100) - 40.90).abs() < 0.01);
        assert_eq!(win_percent(5000), win_percent(1000));
        assert_eq!(Score::Mate(3).win_percent(), win_percent(1000));
        assert_eq!(Score::Mate(-3).win_percent(), win_percent(-1000));
    }

    #[test]
    fn reads_and_writes_pgn_scores() {
        for (text, s) in [("+0.31", Score::Cp(31)), ("-1.50", Score::Cp(-150)), ("+M5", Score::Mate(5))] {
            assert_eq!(s.pgn(), text);
            assert_eq!(Score::parse_pgn(text), Some(s));
        }
        assert_eq!(Score::Mate(-3).pgn(), "-M3");
        assert_eq!(Score::parse_pgn("#-3"), Some(Score::Mate(-3)));
        assert_eq!(Score::parse_pgn("0.17"), Some(Score::Cp(17)));
        assert_eq!(Score::parse_pgn("x"), None);
        assert_eq!(Score::Cp(-450).centipawns(), -450);
        assert_eq!(Score::Mate(2).centipawns(), 29_998);
        assert_eq!(Score::Mate(-2).centipawns(), -29_998);
    }
}
