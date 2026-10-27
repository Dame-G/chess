//! The Dameless app: the HTTP API (axum, documented with utoipa), the event stream, the wiring of every service, and
//! the `dameless` command line.
//!
//! - [`app`]: opens the hub and wires the services together as their crates document it, and runs the server.
//! - [`api`]: the routes. They translate HTTP to service calls and hold no domain logic.
//! - [`cli`]: every other subcommand of `dameless`, each a client of a running app over HTTP.

pub mod api;
pub mod app;
pub mod cli;

pub use app::{App, Options};
