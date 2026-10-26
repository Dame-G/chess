//! Pure domain: types, chess, statistics, review rules, referee rules, SPSA and scaling maths, events. No I/O: no
//! tokio, no filesystem, no network, no clock reads. Timestamps, durations and randomness are passed in, so every
//! rule here is a plain function and is unit tested.
//!
//! `docs/design.md` is the contract for every type here.

pub mod chess;
pub mod error;
pub mod events;
pub mod game;
pub mod ideas;
pub mod ids;
pub mod lichess;
pub mod referee;
pub mod scaling;
pub mod score;
pub mod spsa;
pub mod stats;
pub mod time;
pub mod version;
pub mod work;

pub use error::{Error, ErrorCode, Result};
pub use ids::{
    BuildId, Color, ConnectionKey, GameId, IdeaId, PlayerKey, ReviewId, Side, SidePair, Timestamp, TrialId, VersionId,
    WorkId,
};
pub use score::Score;
pub use time::{Speed, TimeControl};
