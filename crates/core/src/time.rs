//! Time controls and Lichess's speeds.
//!
//! A clock is written two ways, and the two must not be mixed up:
//!
//! - **Games** (the UI, the CLI, Lichess) write `minutes+seconds`: `3+2` is three minutes plus two seconds a move,
//!   `0.5+0` thirty seconds. See [`TimeControl::parse_minutes`] and [`TimeControl::minutes_text`].
//! - **Test presets** (trials, as the old harness, fastchess and OpenBench write them) write `seconds+seconds`:
//!   `8+0.08` is eight seconds plus 80 ms. See [`TimeControl::parse_seconds`] and [`TimeControl::seconds_text`].

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How long each move may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimeControl {
    /// A clock per side and an increment per move.
    Clock { initial_ms: u64, increment_ms: u64 },
    /// Lichess's correspondence: days per move.
    Correspondence { days: u32 },
    /// A fixed time per move, no clock.
    Movetime { ms: u64 },
    /// A fixed number of nodes per move: no clock, so speed does not count, only search and evaluation.
    Nodes { nodes: u64 },
}

/// Lichess's speeds, from its estimate of a game's length (initial + 40 × increment).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Speed {
    UltraBullet,
    Bullet,
    Blitz,
    Rapid,
    Classical,
    Correspondence,
}

impl Speed {
    pub const ALL: [Self; 6] =
        [Self::UltraBullet, Self::Bullet, Self::Blitz, Self::Rapid, Self::Classical, Self::Correspondence];

    /// The speed of a clock, by Lichess's rule: the estimate `initial + 40 × increment` under 30 s is UltraBullet,
    /// under 3 minutes bullet, under 8 blitz, under 25 rapid, and classical from there.
    pub fn of_clock(initial_ms: u64, increment_ms: u64) -> Self {
        match initial_ms + 40 * increment_ms {
            ..30_000 => Self::UltraBullet,
            30_000..180_000 => Self::Bullet,
            180_000..480_000 => Self::Blitz,
            480_000..1_500_000 => Self::Rapid,
            _ => Self::Classical,
        }
    }

    /// The name Lichess gives the speed, as in a game's `speed`, a challenge's and an account's `perfs`.
    pub fn lichess_key(self) -> &'static str {
        match self {
            Self::UltraBullet => "ultraBullet",
            Self::Bullet => "bullet",
            Self::Blitz => "blitz",
            Self::Rapid => "rapid",
            Self::Classical => "classical",
            Self::Correspondence => "correspondence",
        }
    }

    /// Reads Lichess's name, or ours (`ultra_bullet`).
    pub fn parse(key: &str) -> Option<Self> {
        let key = key.trim();
        Self::ALL.into_iter().find(|s| s.lichess_key().eq_ignore_ascii_case(key) || s.as_str() == key)
    }

    /// Our name, as serde writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UltraBullet => "ultra_bullet",
            Self::Bullet => "bullet",
            Self::Blitz => "blitz",
            Self::Rapid => "rapid",
            Self::Classical => "classical",
            Self::Correspondence => "correspondence",
        }
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a clock does not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct BadClock(pub String);

/// Reads a non-negative decimal number of some unit into thousandths of it, exactly (no float rounding surprises for
/// `0.08` or `0.1`): `"8"` is 8000, `"0.08"` 80. At most three decimals.
fn thousandths(text: &str) -> Option<u64> {
    let t = text.trim();
    let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
    if whole.is_empty() && frac.is_empty() || frac.len() > 3 {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let frac: u64 = format!("{frac:0<3}").parse().ok()?;
    whole.checked_mul(1000)?.checked_add(frac)
}

/// Thousandths as the shortest decimal: 8000 is `8`, 80 is `0.08`, 24910 is `24.91`.
fn decimal(thousandths: u64) -> String {
    let (whole, frac) = (thousandths / 1000, thousandths % 1000);
    if frac == 0 {
        return whole.to_string();
    }
    let frac = format!("{frac:03}");
    format!("{whole}.{}", frac.trim_end_matches('0'))
}

impl TimeControl {
    /// A clock of `initial_ms` and `increment_ms`.
    pub fn clock(initial_ms: u64, increment_ms: u64) -> Self {
        Self::Clock { initial_ms, increment_ms }
    }

    /// The speed Lichess would file a game at this time control under. Movetime and node limits have none.
    pub fn speed(&self) -> Option<Speed> {
        match *self {
            Self::Clock { initial_ms, increment_ms } => Some(Speed::of_clock(initial_ms, increment_ms)),
            Self::Correspondence { .. } => Some(Speed::Correspondence),
            Self::Movetime { .. } | Self::Nodes { .. } => None,
        }
    }

    /// Whether games at this time control run a clock that can run out.
    pub fn is_clock(&self) -> bool {
        matches!(self, Self::Clock { .. })
    }

    /// Reads a game clock as Lichess writes it, `minutes+seconds`: `3+2`, `0.5+0`, `1.5+1`, `15+10`.
    pub fn parse_minutes(text: &str) -> Result<Self, BadClock> {
        let bad = || BadClock(format!("a clock looks like 3+2 (minutes plus increment seconds), not {text:?}"));
        let (minutes, increment) = text.trim().split_once('+').ok_or_else(bad)?;
        let minutes = thousandths(minutes).ok_or_else(bad)?;
        let increment_ms = thousandths(increment).ok_or_else(bad)?;
        // Thousandths of a minute are 60 ms each.
        Ok(Self::Clock { initial_ms: minutes * 60, increment_ms })
    }

    /// Reads a test clock as fastchess and OpenBench write it, `seconds+seconds`: `8+0.08`, `40+0.4`, `11+0.11`.
    pub fn parse_seconds(text: &str) -> Result<Self, BadClock> {
        let bad = || BadClock(format!("a test clock looks like 8+0.08 (seconds plus increment seconds), not {text:?}"));
        let (initial, increment) = text.trim().split_once('+').ok_or_else(bad)?;
        Ok(Self::Clock {
            initial_ms: thousandths(initial).ok_or_else(bad)?,
            increment_ms: thousandths(increment).ok_or_else(bad)?,
        })
    }

    /// As games write it: `3+2`, `0.5+0`; `2 days`, `1s/move` and `25000 nodes` for the others.
    pub fn minutes_text(&self) -> String {
        match *self {
            Self::Clock { initial_ms, increment_ms } => {
                // Whole minutes and the usual fractions print exactly; anything else to the millisecond.
                let minutes = if initial_ms % 60 == 0 {
                    decimal(initial_ms / 60)
                } else {
                    format!("{}", initial_ms as f64 / 60_000.0)
                };
                format!("{minutes}+{}", decimal(increment_ms))
            }
            other => other.other_text(),
        }
    }

    /// As test presets and fastchess's `tc=` write it: `8+0.08`, `24.91+0.25`; the others as [`Self::minutes_text`].
    pub fn seconds_text(&self) -> String {
        match *self {
            Self::Clock { initial_ms, increment_ms } => format!("{}+{}", decimal(initial_ms), decimal(increment_ms)),
            other => other.other_text(),
        }
    }

    fn other_text(&self) -> String {
        match *self {
            Self::Correspondence { days } => format!("{days} day{}", if days == 1 { "" } else { "s" }),
            Self::Movetime { ms } => format!("{}s/move", decimal(ms)),
            Self::Nodes { nodes } => format!("{nodes} nodes"),
            Self::Clock { .. } => unreachable!("clocks are written by the callers"),
        }
    }

    /// The PGN `TimeControl` header: `180+2` (seconds, as the PGN standard writes it), `-` for none, `1/86400` for
    /// correspondence (a move a day).
    pub fn pgn_header(&self) -> String {
        match *self {
            Self::Clock { initial_ms, increment_ms } => format!("{}+{}", decimal(initial_ms), decimal(increment_ms)),
            Self::Correspondence { days } => format!("1/{}", u64::from(days) * 86_400),
            Self::Movetime { ms } => format!("{}/move", decimal(ms)),
            Self::Nodes { nodes } => format!("nodes {nodes}"),
        }
    }

    /// Reads a PGN `TimeControl` header (`180+2`, `600`, `-`, `1/86400`): seconds, as the standard writes it.
    pub fn parse_pgn_header(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.is_empty() || t == "-" || t == "?" {
            return None;
        }
        if let Some(per_move) = t.strip_prefix("1/") {
            let seconds: u64 = per_move.parse().ok()?;
            return Some(Self::Correspondence { days: (seconds / 86_400).max(1) as u32 });
        }
        if let Some(nodes) = t.strip_prefix("nodes ") {
            return Some(Self::Nodes { nodes: nodes.trim().parse().ok()? });
        }
        if let Some(ms) = t.strip_suffix("/move") {
            return Some(Self::Movetime { ms: thousandths(ms)? });
        }
        let (initial, increment) = t.split_once('+').unwrap_or((t, "0"));
        Some(Self::Clock { initial_ms: thousandths(initial)?, increment_ms: thousandths(increment)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_follow_lichess() {
        let s = |initial_s: u64, inc_s: u64| Speed::of_clock(initial_s * 1000, inc_s * 1000);
        assert_eq!(s(15, 0), Speed::UltraBullet);
        assert_eq!(s(30, 0), Speed::Bullet);
        assert_eq!(s(60, 0), Speed::Bullet);
        // 120 + 40 × 1 = 160 s: still bullet.
        assert_eq!(s(120, 1), Speed::Bullet);
        assert_eq!(s(180, 0), Speed::Blitz);
        assert_eq!(s(180, 2), Speed::Blitz);
        assert_eq!(s(300, 3), Speed::Blitz);
        assert_eq!(s(480, 0), Speed::Rapid);
        assert_eq!(s(600, 5), Speed::Rapid);
        assert_eq!(s(1500, 0), Speed::Classical);
        assert_eq!(TimeControl::Correspondence { days: 2 }.speed(), Some(Speed::Correspondence));
        assert_eq!(TimeControl::Nodes { nodes: 1 }.speed(), None);
        assert_eq!(Speed::parse("ultraBullet"), Some(Speed::UltraBullet));
        assert_eq!(Speed::parse("ultra_bullet"), Some(Speed::UltraBullet));
        assert_eq!(serde_json::to_value(Speed::UltraBullet).unwrap(), serde_json::json!("ultra_bullet"));
    }

    #[test]
    fn game_clocks_are_minutes_plus_seconds() {
        assert_eq!(TimeControl::parse_minutes("3+2").unwrap(), TimeControl::clock(180_000, 2_000));
        assert_eq!(TimeControl::parse_minutes("0.5+0").unwrap(), TimeControl::clock(30_000, 0));
        assert_eq!(TimeControl::parse_minutes(" 1.5+1 ").unwrap(), TimeControl::clock(90_000, 1_000));
        assert_eq!(TimeControl::parse_minutes("0.25+0").unwrap(), TimeControl::clock(15_000, 0));
        for bad in ["3", "x+2", "3+", "-1+2", "3+2+1", "0.0001+0"] {
            assert!(TimeControl::parse_minutes(bad).is_err(), "{bad}");
        }
        for text in ["3+2", "0.5+0", "1.5+1", "15+10", "0.25+0", "0.75+0", "0+1"] {
            assert_eq!(TimeControl::parse_minutes(text).unwrap().minutes_text(), text);
        }
    }

    #[test]
    fn test_clocks_are_seconds_plus_seconds() {
        assert_eq!(TimeControl::parse_seconds("8+0.08").unwrap(), TimeControl::clock(8_000, 80));
        assert_eq!(TimeControl::parse_seconds("40+0.4").unwrap(), TimeControl::clock(40_000, 400));
        for text in ["8+0.08", "4+0.04", "40+0.4", "11+0.11", "24.91+0.25"] {
            assert_eq!(TimeControl::parse_seconds(text).unwrap().seconds_text(), text);
        }
        assert_eq!(TimeControl::Nodes { nodes: 25_000 }.seconds_text(), "25000 nodes");
        assert_eq!(TimeControl::Movetime { ms: 1_000 }.minutes_text(), "1s/move");
    }

    #[test]
    fn pgn_headers_are_seconds() {
        let tc = TimeControl::clock(180_000, 2_000);
        assert_eq!(tc.pgn_header(), "180+2");
        assert_eq!(TimeControl::parse_pgn_header("180+2"), Some(tc));
        assert_eq!(TimeControl::parse_pgn_header("10+0.1"), Some(TimeControl::clock(10_000, 100)));
        assert_eq!(TimeControl::parse_pgn_header("600"), Some(TimeControl::clock(600_000, 0)));
        assert_eq!(TimeControl::parse_pgn_header("-"), None);
        assert_eq!(TimeControl::parse_pgn_header("1/172800"), Some(TimeControl::Correspondence { days: 2 }));
        assert_eq!(serde_json::to_value(tc).unwrap()["type"], "clock");
    }
}
