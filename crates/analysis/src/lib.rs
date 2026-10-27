//! The evaluation service, the position cache, reviews, profiles and comparisons.
//!
//! - [`Eval`]: live evaluation sessions for the board (`POST /api/eval`, events on `eval:<session>`), and cached
//!   searches of positions ([`Eval::evaluate`], [`Eval::evaluate_positions`]) for reviews.
//! - [`Reviews`]: queues reviews ([`Reviews::prepare`]) and runs them as the `review` work's executor.
//! - [`Profiles`]: a player's games added up, and players compared; your repertoire ([`Profiles::repertoire`]) and
//!   both players' progress ([`Profiles::progress`]).
//! - [`Drills`]: drills from your mistakes, today's set, answers on a spaced schedule, the hit rate.
//!
//! Every score analysis stores or sends is White's: engine scores are flipped in [`search`] before they go anywhere.
//! A version becomes a binary through [`Binaries`], which `work`'s versions implement.

pub mod drills;
pub mod eval;
pub mod profiles;
pub mod progress;
pub mod repertoire;
pub mod review;
pub mod search;
pub mod versions;

pub use drills::{AnswerRequest, DrillAnswerResult, Drills, DrillsToday, MakeDrillsRequest};
pub use eval::{Budget, Eval, EvalRequest, EvalSession, Evaluated, SessionInfo};
pub use profiles::Profiles;
pub use repertoire::RepertoireRequest;
pub use review::{ReviewCheckpoint, ReviewRequest, ReviewResult, ReviewSpec, Reviews};
pub use versions::{Binaries, VersionRef};
