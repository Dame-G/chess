//! The scheduler, versions and builds, ideas and trials (the funnel), tunes (SPSA), promoting.
//!
//! How the server wires it:
//!
//! ```text
//! let scheduler = Scheduler::new(hub.clone());
//! let versions = Versions::new(hub.clone(), scheduler.clone());
//! scheduler.register(versions.executor());          // Build
//! let ideas = Ideas::new(versions.clone());
//! ideas.register();                                 // Trial, Tune, Promote, and the funnel's callback
//! scheduler.register(review_executor);              // from analysis
//! scheduler.register(import_executor);              // from play
//! let harness = Harness::new(ideas.clone());        // Check the harness (its probes: the real machine)
//! let diagnostics = Diagnostics::new(harness.clone()); // GET /api/health
//! scheduler.start().await?;                         // requeues interrupted work, starts the loop
//! ```
//!
//! `play` and `analysis` use [`versions::Versions`] to turn a version into a binary, and [`Scheduler`] to queue work.

pub mod harness;
pub mod health;
pub mod ideas;
pub mod scheduler;
pub mod toolchain;
pub mod versions;

pub use harness::{Harness, HarnessCheckRequest};
pub use health::{Diagnostics, Health, HealthItem, Probes, SystemProbes};
pub use ideas::{
    BatchRequest, BenchLineStatus, BranchCheck, CustomTrial, FixBenchRequest, Ideas, NewIdeaRequest, TuneView,
};
pub use scheduler::{EndHook, NewWork, Scheduler};
pub use toolchain::{Reckless, Toolchain};
pub use versions::{InUse, InUseGuard, PruneReport, PruneRequest, VersionRef, Versions, push, setup};

#[cfg(test)]
mod testing;
