use std::path::PathBuf;

use reference::arch::MAIN_NET_NAME;

/// The net `main` ships: `RECKLESS_NET`, else Reckless's `networks/` beside this repository (`C:\dev\reckless` next to
/// `C:\dev\chess-next`). `None` when neither exists.
pub fn main_net() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("RECKLESS_NET") {
        return Some(PathBuf::from(path));
    }
    let beside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../reckless/networks").join(MAIN_NET_NAME);
    beside.exists().then_some(beside)
}
