//! How analysis turns a version into a binary.
//!
//! Versions and builds belong to `work` ([`Versions`]). Analysis needs only two of its calls, so it asks for them
//! through [`Binaries`]: the server hands it `work`'s `Versions`, and the tests a fake that points at the fake UCI
//! engine. Requests name a version with `work`'s [`VersionRef`].

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use dameless_core::{
    Error,
    version::{Build, Version},
};
pub use dameless_work::VersionRef;
use dameless_work::Versions;

/// What analysis needs of `work::versions::Versions`.
#[async_trait]
pub trait Binaries: Send + Sync + 'static {
    /// The version a reference names (making a version of a branch, tag or commit on first use).
    async fn resolve(&self, version: &VersionRef) -> Result<Version, Error>;

    /// A ready binary of the version with these cargo features, built first (as `Build` work, awaited) when there is
    /// none. Never call it while holding cores: the build needs them.
    async fn binary(&self, version: &VersionRef, features: &[&str]) -> Result<(Build, PathBuf), Error>;
}

#[async_trait]
impl Binaries for Versions {
    async fn resolve(&self, version: &VersionRef) -> Result<Version, Error> {
        Versions::resolve(self, version).await
    }

    async fn binary(&self, version: &VersionRef, features: &[&str]) -> Result<(Build, PathBuf), Error> {
        Versions::binary(self, version, features).await
    }
}

#[async_trait]
impl<T: Binaries + ?Sized> Binaries for Arc<T> {
    async fn resolve(&self, version: &VersionRef) -> Result<Version, Error> {
        (**self).resolve(version).await
    }

    async fn binary(&self, version: &VersionRef, features: &[&str]) -> Result<(Build, PathBuf), Error> {
        (**self).binary(version, features).await
    }
}
