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
//! let upstream = Upstream::new(versions.clone());
//! upstream.register();                              // Upstream (taking upstream Reckless as the next champion)
//! let regression = Regression::new(ideas.clone());
//! regression.register();                            // a regression check after every fifth promotion
//! let harness = Harness::new(ideas.clone());        // Check the harness (its probes: the real machine)
//! let diagnostics = Diagnostics::new(harness.clone()); // GET /api/health
//! let probes: Arc<dyn Probes> = Arc::new(SystemProbes::default());
//! let trainer = Arc::new(TrainerBuild::new(TrainerBuild::default_source(), hub.config.trainer_tools_dir()));
//! let nets = Nets::new(hub.clone());
//! let datasets = Datasets::new(versions.clone(), trainer, probes.clone());
//! datasets.register();                              // Datagen, DatasetImport
//! let training = Training::new(ideas.clone(), datasets.clone(), nets.clone(), probes);
//! training.register();                              // Train
//! nets.ensure_shipped(&versions).await.ok();        // the champion's shipped net, when on disk
//! datasets.scan().await.ok();                       // dataset directories already there
//! scheduler.start().await?;                         // requeues interrupted work, starts the loop
//! upstream.spawn_auto(shutdown);                    // the six-hourly sync, while its setting is on (`serve` only)
//! ```
//!
//! `play` and `analysis` use [`versions::Versions`] to turn a version into a binary, and [`Scheduler`] to queue work.

pub mod data;
pub mod harness;
pub mod health;
pub mod ideas;
pub mod nets;
pub mod regression;
pub mod scheduler;
pub mod toolchain;
pub mod train;
pub mod trainer;
pub mod upstream;
pub mod versions;

pub use data::{DataStatus, DatagenRequest, Datasets, ImportRequest};
pub use harness::{Harness, HarnessCheckRequest};
pub use health::{Diagnostics, Health, HealthItem, Probes, SystemProbes};
pub use ideas::{
    BatchRequest, BenchLineStatus, BranchCheck, CustomTrial, FixBenchRequest, Ideas, NewIdeaRequest, TuneView,
};
pub use nets::Nets;
pub use regression::Regression;
pub use scheduler::{EndHook, NewWork, Scheduler};
pub use toolchain::{Reckless, Toolchain};
pub use train::{TrainView, Training};
pub use trainer::{Trainer, TrainerBuild};
pub use upstream::Upstream;
pub use versions::{InUse, InUseGuard, PruneReport, PruneRequest, VersionRef, Versions, push, setup};

#[cfg(test)]
mod fake_trainer;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod train_tests;
