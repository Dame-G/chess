//! Engines: the UCI engines the app plays against and analyses with. Any UCI engine can be one: the built-ins that
//! `chess setup` installs (Stockfish and Reckless) and engines you add by path.
//!
//! One engine is the **analysis engine** (Stockfish by default): reviews, live evaluation and drills run on it. Every
//! engine that plays on the clock has its own ladder of levels (`stockfish@5`, `reckless@3`), each with its own
//! calibrated ratings. An engine you add with a fixed node budget plays that budget whatever the clock, and has no
//! levels.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{PlayerKey, Timestamp, valid_engine_id};

/// The analysis engine when none was chosen.
pub const DEFAULT_ANALYSIS: &str = "stockfish";

/// Where an engine comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    /// One of the engines `chess setup` downloads ([`BUILTINS`]).
    Builtin,
    /// Added by path.
    Custom,
}

/// An engine of the engines list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Engine {
    /// The slug seats and player keys name it by: `stockfish`, `reckless`, `my-engine`.
    pub id: String,
    /// As shown: `Stockfish`, `Reckless`.
    pub name: String,
    pub kind: EngineKind,
    /// The binary; none for a built-in that is not installed yet.
    pub path: Option<String>,
    /// Arguments it is started with.
    pub args: Vec<String>,
    /// UCI options set after the handshake, before the app's own (`Threads`, `Hash`, `MultiPV`).
    pub options: BTreeMap<String, String>,
    /// What it calls itself (its UCI `id name`), read when it was added or installed: `Stockfish 19`. Reviews and the
    /// position cache are keyed by it, so a new release of an engine never reads an old one's results as its own.
    pub id_name: Option<String>,
    /// Nodes it searches a move whatever the clock (set when you add it). None plays on the clock.
    pub nodes: Option<u64>,
    /// The rating it plays at, when you gave it one (an engine made to play at a rating).
    pub rating: Option<u32>,
    /// Whether it declares UCI's `MultiPV` (read when it was added or installed). Without it, levels 1 to 6 play
    /// their node budget only, the engine's best move at that budget, rather than choosing among its lines.
    pub multipv: bool,
    /// Whether it can be started: a binary is there.
    pub installed: bool,
    /// Whether it is the analysis engine.
    pub analysis: bool,
    /// Whether it has a ladder of levels: installed, and plays on the clock (no fixed node budget).
    pub levels: bool,
    pub created_at: Option<Timestamp>,
}

impl Engine {
    /// Its player key: `engine:<id>`.
    pub fn player(&self) -> PlayerKey {
        PlayerKey::Engine(self.id.clone())
    }

    /// What reviews and the position cache are keyed by: the id and what the engine calls itself.
    pub fn cache_key(&self) -> String {
        format!("{}|{}", self.id, self.id_name.as_deref().unwrap_or(&self.name))
    }

    /// How reviews and seats label it: what it calls itself, else its name.
    pub fn label(&self) -> &str {
        self.id_name.as_deref().unwrap_or(&self.name)
    }
}

/// `GET /api/engines` and the `engines.changed` event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EnginesOverview {
    /// The analysis engine's id.
    pub analysis: String,
    /// Built-ins first (installed or not), then the engines you added, by name.
    pub engines: Vec<Engine>,
}

/// What a built-in engine is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Package {
    /// Stockfish's release binary.
    Stockfish,
    /// Reckless's release binary.
    Reckless,
}

/// A built-in engine. Both play on the clock at full strength and can analyse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub package: Package,
}

/// Every built-in engine: Stockfish (the analysis engine by default), then Reckless.
pub const BUILTINS: [Builtin; 2] = [
    Builtin { id: "stockfish", name: "Stockfish", package: Package::Stockfish },
    Builtin { id: "reckless", name: "Reckless", package: Package::Reckless },
];

/// The built-in engine with this id.
pub fn builtin(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.id == id)
}

/// A seat of a local game: `you`, `<engine id>` (full strength, or its fixed budget), or `<engine id>@<level>` (a level
/// of the ladder).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatSpec {
    You,
    Engine { id: String, level: Option<u32> },
}

impl SeatSpec {
    /// Reads a seat as requests write it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text == "you" {
            return Ok(Self::You);
        }
        let (id, level) = match text.split_once('@') {
            Some((id, level)) => {
                let level =
                    level.parse::<u32>().map_err(|_| format!("{text:?}: a level is a number, as in stockfish@5"))?;
                (id, Some(level))
            }
            None => (text, None),
        };
        let id = id.strip_prefix("engine:").unwrap_or(id);
        if !valid_engine_id(id) || id == "you" {
            return Err(format!(
                "{text:?} is not a seat: use you, an engine's id (stockfish), or stockfish@5 for a level"
            ));
        }
        Ok(Self::Engine { id: id.to_owned(), level })
    }
}

/// An engine id made from a name for an engine you add: `My Engine 2` becomes `my-engine-2`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    out.chars().take(40).collect::<String>().trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seats_read() {
        assert_eq!(SeatSpec::parse("you"), Ok(SeatSpec::You));
        assert_eq!(SeatSpec::parse("stockfish"), Ok(SeatSpec::Engine { id: "stockfish".into(), level: None }));
        assert_eq!(SeatSpec::parse("stockfish@5"), Ok(SeatSpec::Engine { id: "stockfish".into(), level: Some(5) }));
        assert_eq!(SeatSpec::parse("engine:reckless"), Ok(SeatSpec::Engine { id: "reckless".into(), level: None }));
        assert!(SeatSpec::parse("stockfish@x").is_err());
        assert!(SeatSpec::parse("Big Engine").is_err());
        assert!(SeatSpec::parse("you@3").is_err());
    }

    #[test]
    fn builtins_and_slugs() {
        assert_eq!(builtin("reckless").unwrap().package, Package::Reckless);
        assert_eq!(builtin("stockfish").unwrap().package, Package::Stockfish);
        assert!(builtin("maia-1500").is_none());
        assert!(BUILTINS.iter().all(|b| valid_engine_id(b.id)));
        assert_eq!(slug("My Engine 2"), "my-engine-2");
        assert_eq!(slug("  Reckless (dev)  "), "reckless-dev");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn engines_key_their_results_by_what_they_report() {
        let e = Engine {
            id: "stockfish".into(),
            name: "Stockfish".into(),
            kind: EngineKind::Builtin,
            path: Some("sf".into()),
            args: Vec::new(),
            options: BTreeMap::new(),
            id_name: Some("Stockfish 19".into()),
            nodes: None,
            rating: None,
            multipv: true,
            installed: true,
            analysis: true,
            levels: true,
            created_at: None,
        };
        assert_eq!(e.cache_key(), "stockfish|Stockfish 19");
        assert_eq!(e.label(), "Stockfish 19");
        assert_eq!(e.player().to_string(), "engine:stockfish");
    }
}
