//! A CPU reference of Reckless's network, in plain safe Rust.
//!
//! It loads a net file exactly as the engine embeds it and computes the same evaluation, bit for bit: the threat and
//! king-bucketed piece-square inputs, the pairwise feature transformer, and 768 → 16 → 32 → 1 with eight output
//! buckets, with the engine's integer and float arithmetic in the engine's order. It is the yardstick the bullet model
//! is checked against. See `trainer/README.md` for the architecture.
//!
//! ```no_run
//! use reference::{Network, parity::parse_fen};
//!
//! let network = Network::load("v60-7f587dfb.nnue")?;
//! let position = parse_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1")?;
//! let eval = network.evaluate(&position); // from the side to move, as Reckless's `eval` prints it × 100
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod arch;
pub mod features;
pub mod network;
pub mod parity;
pub mod positions;

pub use network::{Forward, Network};
