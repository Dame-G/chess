//! The Referee runtime: game tasks and seat controllers (engine, local, Lichess board, Lichess bot).
//!
//! Every game that is played (not imported, not a trial's, which fastchess plays) runs here, one task per game:
//!
//! - [`Games`]: starts local games, routes your actions to the game they belong to, and answers what a game looks like
//!   now, with its live clocks, offers and actions laid over the stored record.
//! - [`seat`]: the [`SeatController`] each seat is played through: an engine, you at the board, and the Lichess seats.
//! - [`Connections`]: the two Lichess accounts (yours and Dameless's BOT account), one client each, their tokens and
//!   status, logging in, the explorer and the tablebase behind the fair-play policy. [`you::You`] follows your account
//!   (your games, seeks and challenges); [`bot::Bot`] plays as the bot (challenges, seeking, the daily bot limit).
//! - [`import::ImportExecutor`]: the `Import` work: your Lichess history, or another player's.
//! - [`Levels`]: Dameless at a strength you choose (`dameless@5` at a seat), the ladder's calibrated ratings and their
//!   refit; [`Calibrate`]: the `Calibrate` work, levels playing each other in the background.
//!
//! Engines are binaries of versions: [`Binaries`] turns a version into one, implemented for `work::Versions`.

pub mod binaries;
pub mod bot;
pub mod calibrate;
mod clients;
pub mod connections;
mod engine_seat;
mod error;
pub mod games;
pub mod import;
pub mod levels;
mod lichess_game;
mod local;
mod record;
pub mod seat;
mod timing;
pub mod you;

pub use binaries::{Binaries, EngineBinary, FixedBinaries};
pub use calibrate::{Calibrate, CalibrateCheckpoint, CalibrateResult};
pub use connections::{Connections, LoginFinished, LoginStarted};
pub use engine_seat::{EngineOptions, EngineSeat};
pub use games::{Games, NewGameRequest, SeatRequest, StartFrom};
pub use import::{ImportExecutor, ImportSpec};
pub use levels::{CalibrateRequest, CalibrateSpec, LevelRating, Levels, LevelsOverview, your_anchor};
pub use seat::{LocalSeat, SeatController, SeatKind};
pub use timing::Timing;
