//! The evaluation service, the position cache, reviews, profiles and comparisons.
//!
//! - [`Eval`]: live evaluation sessions for the board (`POST /api/eval`, events on `eval:<session>`), and cached
//!   searches of positions ([`Eval::evaluate`], [`Eval::evaluate_positions`]) for reviews.
//! - [`Reviews`]: queues reviews ([`Reviews::prepare`]) and runs them as the `review` work's executor.
//! - [`Profiles`]: a player's games added up, and players compared.
//!
//! Every score analysis stores or sends is White's: engine scores are flipped in [`search`] before they go anywhere.
//! A version becomes a binary through [`Binaries`], which `work`'s versions implement.

pub mod eval;
pub mod profiles;
pub mod review;
pub mod search;
pub mod versions;

pub use eval::{Budget, Eval, EvalRequest, EvalSession, Evaluated, SessionInfo};
pub use profiles::Profiles;
pub use review::{ReviewCheckpoint, ReviewRequest, ReviewResult, ReviewSpec, Reviews};
pub use versions::{Binaries, VersionRef};
