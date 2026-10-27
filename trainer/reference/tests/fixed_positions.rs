//! The reference against evaluations Reckless printed for a handful of fixed positions, checked in.
//!
//! Needs the shipped net (63 MB, not in this repository): `RECKLESS_NET=<path>`, or Reckless's checkout beside this
//! one. Without it the test says so and passes.

mod common;

use reference::{
    Network,
    arch::MAIN_NET_SHA256,
    parity::{parse_fen, read_expected, reference_eval},
};

const FIXTURE: &str = include_str!("data/fixed-v60-7f587dfb.txt");

#[test]
fn fixed_positions_match_the_engine() {
    let Some(path) = common::main_net() else {
        eprintln!("skipped: the shipped net was not found (set RECKLESS_NET)");
        return;
    };
    let network = Network::load(&path).unwrap();
    assert_eq!(network.sha256, MAIN_NET_SHA256, "{} is not the net the fixture was made with", path.display());

    let expected = read_expected(FIXTURE).unwrap();
    assert_eq!(expected.len(), 16);

    for (fen, engine) in expected {
        let position = parse_fen(&fen).unwrap();
        let (reference, wrapped) = reference_eval(&network, &position);
        assert_eq!(reference, engine, "{fen}");
        assert!(!wrapped);
        assert_eq!(network.evaluate(&position), engine.eval, "{fen}");
    }
}
