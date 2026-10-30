//! Adapter: Lichess's HTTP API with its rate rules, OAuth PKCE, NDJSON streams, the Board API, import, explorer.
//!
//! This crate knows Lichess's wire format and nothing else. Its types are Lichess's JSON as typed structs (times in
//! milliseconds since 1970 as Lichess sends them, scores from White's side as Lichess sends them), and it holds no
//! rules of ours: what the Board API lets you play and fair play belong to
//! `core` and `play`, which map these types to their own.
//!
//! One [`Lichess`] client is made per token (or one anonymous client), and cloned wherever that token is used: the
//! clones share the token's request slot, so only one request is in flight per token as Lichess asks, and a 429 pauses
//! every clone for a minute. Streams (the event stream, game streams, a seek, a games export) hold the slot only while
//! they open.

pub mod account;
pub mod board;

pub mod challenge;
mod client;
mod config;
mod error;
pub mod events;
pub mod explorer;
pub mod game;
pub mod import;
mod ndjson;
pub mod oauth;
pub mod players;
#[cfg(any(test, feature = "mock"))]
pub mod testing;
mod time;

pub use account::{Account, Counts, Perf, TokenInfo, User};
pub use client::Lichess;
pub use config::{Config, LICHESS_URL, Timing};
pub use error::{Error, LichessError, RateLimitInfo};
pub use ndjson::{Backoff, Ndjson};
pub use time::{now_ms, parse_iso};

use serde::{Deserialize, Serialize};

/// A side of the board, as Lichess writes it (`white`, `black`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

/// The colour asked for when challenging or seeking: a side, or Lichess's coin toss.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    #[default]
    Random,
    White,
    Black,
}

impl Color {
    pub fn key(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

/// A game's status as Lichess names it, in game streams, events and exports. Anything new Lichess adds reads as
/// `Other`, so a stream never breaks on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameStatus {
    Created,
    #[default]
    Started,
    Aborted,
    Mate,
    Resign,
    Stalemate,
    /// The opponent left and the win was claimed.
    Timeout,
    Draw,
    /// A flag fell.
    Outoftime,
    Cheat,
    NoStart,
    UnknownFinish,
    InsufficientMaterialClaim,
    VariantEnd,
    #[serde(other)]
    Other,
}

impl GameStatus {
    /// Whether the game has ended (anything but created or started).
    pub fn is_over(self) -> bool {
        !matches!(self, Self::Created | Self::Started)
    }

    /// The name Lichess uses.
    pub fn key(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Started => "started",
            Self::Aborted => "aborted",
            Self::Mate => "mate",
            Self::Resign => "resign",
            Self::Stalemate => "stalemate",
            Self::Timeout => "timeout",
            Self::Draw => "draw",
            Self::Outoftime => "outoftime",
            Self::Cheat => "cheat",
            Self::NoStart => "noStart",
            Self::UnknownFinish => "unknownFinish",
            Self::InsufficientMaterialClaim => "insufficientMaterialClaim",
            Self::VariantEnd => "variantEnd",
            Self::Other => "other",
        }
    }
}

/// A variant as challenges and game streams describe it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Which clients may play a game, as Lichess decides it (`compat` in events): the Bot API, the Board API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compat {
    #[serde(default)]
    pub bot: bool,
    #[serde(default)]
    pub board: bool,
}
