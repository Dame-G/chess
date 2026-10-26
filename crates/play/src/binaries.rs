//! Turning a version of Dameless into a binary an engine seat can start. The runtime asks through [`Binaries`], which
//! `work::Versions` implements here (resolving `champion`, a name or an id, building the version first when it has no
//! binary); tests use [`FixedBinaries`], which points at a fake engine, so they run without the scheduler.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use dameless_core::{
    BuildId, Error, Timestamp, VersionId,
    version::{Line, Version},
};
use dameless_work::{VersionRef, Versions};

/// A ready binary of a version.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineBinary {
    pub version: Version,
    pub build: BuildId,
    pub path: PathBuf,
    /// Arguments the engine is started with. Empty for Dameless itself; tests use them to script a fake engine.
    pub args: Vec<String>,
}

/// Versions to binaries.
#[async_trait]
pub trait Binaries: Send + Sync + 'static {
    /// The version `version` names (`champion`, `v14`, a branch, a commit or an id), without building it: whether a
    /// game could be played with it.
    async fn resolve(&self, version: &str) -> Result<Version, Error>;

    /// A ready binary of the version, built first when there is none (which can take minutes).
    async fn binary(&self, version: &str) -> Result<EngineBinary, Error>;
}

/// The app's versions: `champion` (or `main`), `#12` for an id, else a version's name or a branch, tag or commit
/// (`work::VersionRef::parse`). Games play builds without cargo features.
#[async_trait]
impl Binaries for Versions {
    async fn resolve(&self, version: &str) -> Result<Version, Error> {
        Versions::resolve(self, &VersionRef::parse(version)).await
    }

    async fn binary(&self, version: &str) -> Result<EngineBinary, Error> {
        let version = Versions::resolve(self, &VersionRef::parse(version)).await?;
        let (build, path) = self.binary_of(&version, &[]).await?;
        Ok(EngineBinary { version, build: build.id, path, args: Vec::new() })
    }
}

/// The same binary for every version name it knows, for tests and for running an engine that is not built here.
pub struct FixedBinaries {
    versions: Vec<(String, EngineBinary)>,
}

impl FixedBinaries {
    /// `path` (with `args`) as the champion, `v1`.
    pub fn new(path: impl Into<PathBuf>, args: &[&str]) -> Arc<Self> {
        Arc::new(Self::empty().with("champion", path, args))
    }

    /// No versions yet: add them with [`FixedBinaries::with`].
    pub fn empty() -> Self {
        Self { versions: Vec::new() }
    }

    /// Adds a version by name: another binary, or the same with other arguments.
    pub fn with(mut self, name: &str, path: impl Into<PathBuf>, args: &[&str]) -> Self {
        let id = VersionId(self.versions.len() as i64 + 1);
        let shown = if name == "champion" { "v1".to_owned() } else { name.to_owned() };
        let version = Version {
            id,
            name: shown,
            commit: format!("{:040}", id.0),
            branch: None,
            line: if name == "champion" { Line::Champion { number: 1 } } else { Line::Other },
            parent: None,
            bench: None,
            elo: None,
            created_at: Timestamp::from_unix_ms(0),
        };
        let binary = EngineBinary {
            version,
            build: BuildId(id.0),
            path: path.into(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        };
        self.versions.push((name.to_owned(), binary));
        self
    }

    /// The versions as a shared [`Binaries`].
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    fn find(&self, version: &str) -> Result<&EngineBinary, Error> {
        let version = version.trim();
        self.versions
            .iter()
            .find(|(name, b)| name == version || b.version.name == version || b.version.id.to_string() == version)
            .map(|(_, b)| b)
            .ok_or_else(|| Error::not_found(format!("No version {version}")))
    }
}

#[async_trait]
impl Binaries for FixedBinaries {
    async fn resolve(&self, version: &str) -> Result<Version, Error> {
        self.find(version).map(|b| b.version.clone())
    }

    async fn binary(&self, version: &str) -> Result<EngineBinary, Error> {
        self.find(version).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn names_ids_and_the_champion_resolve() {
        let b = FixedBinaries { versions: Vec::new() }.with("champion", "a", &[]).with("slow", "a", &["--delay", "5"]);
        assert_eq!(b.resolve("champion").await.unwrap().name, "v1");
        assert_eq!(b.resolve("v1").await.unwrap().id, VersionId(1));
        assert_eq!(b.binary("2").await.unwrap().args, ["--delay", "5"]);
        assert_eq!(b.resolve("nope").await.unwrap_err().status(), 404);
    }
}
