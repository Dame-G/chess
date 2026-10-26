//! The shared runtime: config, data directory, secrets, the event bus, the fair-play policy and the core ledger.
//!
//! Every service (`play`, `analysis`, `work`, the server) gets one [`Hub`]: the config, the [`Store`], the [`Bus`],
//! the [`PolicyState`], the [`Ledger`] and the [`Secrets`]. `hub` depends on `store` (and `core`); `store` knows nothing
//! of the hub.

use std::sync::Arc;

use anyhow::{Context, Result};
pub use dameless_store::Store;

pub mod bus;
pub mod config;
pub mod ledger;
pub mod policy;
pub mod secrets;

pub use bus::{Bus, Next, Subscription, TopicFilter};
pub use config::{Config, LichessUrls};
pub use ledger::{Grant, Ledger, LedgerSnapshot, Reservation};
pub use policy::PolicyState;
pub use secrets::Secrets;

/// What every service shares.
pub struct Hub {
    pub config: Config,
    pub store: Store,
    pub bus: Arc<Bus>,
    pub policy: PolicyState,
    pub ledger: Arc<Ledger>,
    pub secrets: Secrets,
    /// The API token every request but the health check carries.
    pub token: String,
}

impl Hub {
    /// Opens the app's runtime: creates the data directory's folders, opens (and migrates) the database, and loads or
    /// creates the API token.
    pub fn open(config: Config) -> Result<Arc<Self>> {
        config.create_dirs()?;
        let store = Store::open(config.db_path()).with_context(|| format!("open {}", config.db_path().display()))?;
        let secrets = Secrets::new(&config);
        let token = secrets.api_token().with_context(|| format!("read or create {}", config.token_path().display()))?;
        let bus = Arc::new(Bus::new());
        Ok(Arc::new(Self {
            policy: PolicyState::new(bus.clone()),
            ledger: Ledger::new(config.cores),
            bus,
            store,
            secrets,
            token,
            config,
        }))
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::events::AssistKind;

    use super::*;

    #[tokio::test]
    async fn opens_a_fresh_data_directory_and_again() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path().join("data"), config::DEFAULT_LISTEN.parse().unwrap(), "reckless", 3);
        let hub = Hub::open(config.clone()).unwrap();
        assert_eq!(hub.ledger.total(), 3);
        assert!(hub.policy.allows(AssistKind::Eval));
        hub.store.put_meta("hello", &1).await.unwrap();
        let token = hub.token.clone();
        drop(hub);

        let again = Hub::open(config).unwrap();
        assert_eq!(again.token, token);
        assert_eq!(again.store.meta::<i32>("hello").await.unwrap(), Some(1));
    }
}
