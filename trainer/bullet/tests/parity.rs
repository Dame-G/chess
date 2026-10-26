//! The shipped net loaded into the bullet model on the GPU: every layer against the CPU reference, and the file saved
//! back. Needs the net (`RECKLESS_NET`, or Reckless's checkout beside this repository) and a CUDA device; passes with a
//! note without the net.

use std::path::PathBuf;

use bullet::parity;
use reference::arch::MAIN_NET_NAME;
use reference::{Network, positions};

fn main_net() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("RECKLESS_NET") {
        return Some(PathBuf::from(path));
    }
    let beside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../reckless/networks").join(MAIN_NET_NAME);
    beside.exists().then_some(beside)
}

#[test]
fn the_shipped_net_matches_the_reference_on_the_gpu_and_saves_back_unchanged() {
    let Some(path) = main_net() else {
        eprintln!("no net (set RECKLESS_NET): skipped");
        return;
    };
    let network = Network::load(&path).unwrap();
    let fens = positions::random_fens(7, 300);

    let report = parity::compare_layers(&network, &fens, true).unwrap();
    assert_eq!(report.evals_equal, fens.len(), "{report:?}");
    assert_eq!(report.acc_mismatches, 0, "{report:?}");
    assert_eq!(report.ft_mismatches, 0, "{report:?}");
    assert!(report.output_max_diff < 1e-4, "{report:?}");

    let bytes = parity::resave(&network).unwrap();
    assert!(bytes == std::fs::read(&path).unwrap(), "the re-saved net differs from the original");
}
