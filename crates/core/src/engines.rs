//! Engines: the UCI engines the app plays against and analyses with. Any UCI engine can be one: the built-ins that
//! `dameless setup` installs (Stockfish, and Maia's human-like nets on lc0) and engines you add by path.
//!
//! One engine is the **analysis engine** (Stockfish by default): reviews, live evaluation, drills and the ladder of
//! levels run on it. An engine with a fixed node budget (Maia plays one node a move, as it is meant to) plays that
//! budget whatever the clock; every other engine plays on the clock at full strength, or at a level when it is the
//! analysis engine.

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
    /// One of the engines `dameless setup` downloads ([`BUILTINS`]).
    Builtin,
    /// Added by path.
    Custom,
}

/// An engine of the engines list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Engine {
    /// The slug seats and player keys name it by: `stockfish`, `maia-1500`.
    pub id: String,
    /// As shown: `Stockfish`, `Maia 1500`.
    pub name: String,
    pub kind: EngineKind,
    /// The binary; none for a built-in that is not installed yet.
    pub path: Option<String>,
    /// Arguments it is started with (lc0's `--weights=…` for Maia).
    pub args: Vec<String>,
    /// UCI options set after the handshake, before the app's own (`Threads`, `Hash`, `MultiPV`).
    pub options: BTreeMap<String, String>,
    /// What it calls itself (its UCI `id name`), read when it was added or installed: `Stockfish 19`. Reviews and the
    /// position cache are keyed by it, so a new release of an engine never reads an old one's results as its own.
    pub id_name: Option<String>,
    /// Nodes it searches a move whatever the clock: Maia's 1. None plays on the clock.
    pub nodes: Option<u64>,
    /// The rating it plays at, when it has one by design (Maia's nominal rating).
    pub rating: Option<u32>,
    /// Whether it can be started: a binary is there.
    pub installed: bool,
    /// Whether it is the analysis engine.
    pub analysis: bool,
    /// Whether the ladder of levels runs on it (the analysis engine, when it plays on the clock).
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
    /// lc0 with Maia's weights for this rating.
    Maia(u32),
}

/// A built-in engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub package: Package,
}

impl Builtin {
    /// Maia's fixed budget: one node a move, which is how its nets play like people at their rating.
    pub fn nodes(&self) -> Option<u64> {
        match self.package {
            Package::Stockfish => None,
            Package::Maia(_) => Some(1),
        }
    }

    pub fn rating(&self) -> Option<u32> {
        match self.package {
            Package::Stockfish => None,
            Package::Maia(rating) => Some(rating),
        }
    }
}

/// Maia's nets: one per hundred points from 1100 to 1900.
pub const MAIA_RATINGS: [u32; 9] = [1100, 1200, 1300, 1400, 1500, 1600, 1700, 1800, 1900];

/// Every built-in engine: Stockfish, then the Maias by rating.
pub const BUILTINS: [Builtin; 10] = [
    Builtin { id: "stockfish", name: "Stockfish", package: Package::Stockfish },
    Builtin { id: "maia-1100", name: "Maia 1100", package: Package::Maia(1100) },
    Builtin { id: "maia-1200", name: "Maia 1200", package: Package::Maia(1200) },
    Builtin { id: "maia-1300", name: "Maia 1300", package: Package::Maia(1300) },
    Builtin { id: "maia-1400", name: "Maia 1400", package: Package::Maia(1400) },
    Builtin { id: "maia-1500", name: "Maia 1500", package: Package::Maia(1500) },
    Builtin { id: "maia-1600", name: "Maia 1600", package: Package::Maia(1600) },
    Builtin { id: "maia-1700", name: "Maia 1700", package: Package::Maia(1700) },
    Builtin { id: "maia-1800", name: "Maia 1800", package: Package::Maia(1800) },
    Builtin { id: "maia-1900", name: "Maia 1900", package: Package::Maia(1900) },
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
        assert_eq!(SeatSpec::parse("engine:maia-1500"), Ok(SeatSpec::Engine { id: "maia-1500".into(), level: None }));
        assert!(SeatSpec::parse("stockfish@x").is_err());
        assert!(SeatSpec::parse("Big Engine").is_err());
        assert!(SeatSpec::parse("you@3").is_err());
    }

    #[test]
    fn builtins_and_slugs() {
        assert_eq!(builtin("maia-1500").unwrap().nodes(), Some(1));
        assert_eq!(builtin("maia-1900").unwrap().rating(), Some(1900));
        assert_eq!(builtin("stockfish").unwrap().nodes(), None);
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
