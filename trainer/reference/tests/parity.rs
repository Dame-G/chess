//! Parity with a running engine on 10,000 random positions. Ignored by default: it needs a Reckless binary.
//!
//! ```sh
//! RECKLESS_BIN=/path/to/reckless RECKLESS_NET=/path/to/v60-7f587dfb.nnue cargo test --release -- --ignored parity
//! ```
//!
//! The binary must be built from a commit that embeds the same net. `PARITY_POSITIONS` and `PARITY_SEED` change the
//! run (default 10,000 and 1).

mod common;

use std::path::PathBuf;

use reference::{Network, parity, positions};

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
#[ignore = "needs a Reckless binary: RECKLESS_BIN"]
fn parity_with_the_engine() {
    let engine = PathBuf::from(std::env::var("RECKLESS_BIN").expect("RECKLESS_BIN: a Reckless binary"));
    let net = common::main_net().expect("RECKLESS_NET: the net the binary embeds");
    let network = Network::load(net).unwrap();

    let fens = positions::random_fens(env_or("PARITY_SEED", 1), env_or("PARITY_POSITIONS", 10_000));
    let engine_evals = parity::engine_evals(&engine, &fens).unwrap();
    let report = parity::compare(&network, &fens, &engine_evals).unwrap();

    for m in report.mismatches.iter().take(10) {
        eprintln!("{}: engine {:?}, reference {:?}", m.fen, m.engine, m.reference);
    }
    assert!(report.per_bucket.iter().all(|&n| n > 0), "every output bucket is exercised: {:?}", report.per_bucket);
    assert!(report.passed(), "{} of {} positions differ", report.positions - report.matched, report.positions);
}
