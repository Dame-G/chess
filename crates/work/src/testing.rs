//! What the crate's tests share: a hub on a fresh data directory, and waiting for a condition.

use std::{future::Future, sync::Arc, time::Duration};

use dameless_hub::{Config, Hub};

/// A hub with `cores` cores on a fresh data directory, removed when the returned directory is dropped.
pub fn bare_hub(cores: u32) -> (Arc<Hub>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), cores);
    (Hub::open(config).unwrap(), dir)
}

/// Polls `check` until it holds, failing after ten seconds.
pub async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..1000 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

