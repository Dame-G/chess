//! Reckless's network in bullet: the model ([`model`]), its inputs with the CPU reference's exact indices
//! ([`inputs`]), loading and saving Reckless's net file ([`net`]), parity with the reference ([`parity`]) and the
//! training runner ([`train`]). See `trainer/README.md`.

pub mod inputs;
pub mod model;
pub mod net;
pub mod parity;
pub mod train;
