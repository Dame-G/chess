//! The scheduler, the engines list, setup and diagnostics.
//!
//! How the server wires it:
//!
//! ```text
//! let scheduler = Scheduler::new(hub.clone());
//! let engines = Engines::new(hub.clone());          // the engines list, and the EngineSource play and analysis use
//! let setup = Setup::new(engines.clone());
//! scheduler.register(setup.executor());             // Setup (downloading the built-in engines)
//! scheduler.register(review_executor);              // from analysis
//! scheduler.register(import_executor);              // from play
//! scheduler.register(calibrate_executor);           // from play
//! let diagnostics = Diagnostics::new(engines.clone(), Arc::new(SystemProbes));
//! scheduler.start().await?;                         // requeues interrupted work, starts the loop
//! ```

pub mod engines;
pub mod health;
pub mod scheduler;
pub mod setup;

pub use engines::{EngineBinary, EngineSource, Engines, FixedEngines, NewEngineRequest};
pub use health::{Diagnostics, Health, HealthItem, Probes, SystemProbes};
pub use scheduler::{EndHook, NewWork, Scheduler};
pub use setup::{Setup, SetupPackage, SetupPlan, SetupRequest};

#[cfg(test)]
mod testing;
