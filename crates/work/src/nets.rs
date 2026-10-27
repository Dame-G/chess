//! The net registry: every network file the app knows, kept in the data directory's `nets/` by SHA-256.
//!
//! Reckless's nets do not live in its repository: its build script names one (`NETWORK_NAME` in `build/build.rs`),
//! `networks/` is git-ignored, and the file comes from a separate releases repository. So the champion's **shipped**
//! net is found on disk (the engine checkout's `networks/<name>`, or the copy the builds keep in `builds/networks/`),
//! copied here and registered on first use ([`Nets::ensure_shipped`], which the server calls at startup). Every net a
//! training run makes is registered when the run ends ([`Nets::register_file`]).
//!
//! A champion can be code plus a trained net (a promoted `net` idea): [`Nets::champion_net`] is then that net, else the
//! shipped net of the champion's commit.

use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use dameless_core::{
    Error,
    events::Payload,
    training::{self, DatasetId, Net, NetId, NetRun, RECKLESS_ARCH, RECKLESS_NET_BYTES},
    version::Version,
};
use dameless_hub::Hub;
use dameless_store::NewNet;
use sha2::{Digest, Sha256};

use crate::versions::Versions;

/// The net registry. Cheap to clone.
#[derive(Clone)]
pub struct Nets {
    hub: Arc<Hub>,
}

/// The SHA-256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The network file a build script names: `const NETWORK_NAME: &str = "v60-7f587dfb.nnue";`.
pub fn network_name(build_script: &str) -> Option<String> {
    let line = build_script.lines().find(|l| l.contains("NETWORK_NAME") && l.contains('='))?;
    Some(line.split('"').nth(1)?.to_owned())
}

/// What registering a file needs besides the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetSource {
    pub name: String,
    pub parent: Option<NetId>,
    pub dataset: Option<DatasetId>,
    pub run: Option<NetRun>,
}

impl Nets {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }

    pub async fn list(&self) -> Result<Vec<Net>, Error> {
        self.hub.store.nets().await
    }

    pub async fn get(&self, id: NetId) -> Result<Net, Error> {
        self.hub.store.net(id).await?.ok_or_else(|| Error::not_found(format!("No net {id}")))
    }

    /// Copies a net file into `nets/<sha256>.nnue` (once) and records it; a file already known is that net. A name
    /// taken by another net gets a suffix (`net-3-2`).
    pub async fn register_file(&self, file: &Path, source: NetSource) -> Result<Net, Error> {
        let io = |e: std::io::Error| Error::internal(format!("{}: {e}", file.display()));
        let sha = {
            let file = file.to_owned();
            tokio::task::spawn_blocking(move || sha256_file(&file))
                .await
                .map_err(|e| Error::internal(e.to_string()))?
                .map_err(io)?
        };
        let store = &self.hub.store;
        if let Some(known) = store.net_by_sha(&sha).await? {
            return Ok(known);
        }
        let size = std::fs::metadata(file).map_err(io)?.len();
        let dir = self.hub.config.nets_dir();
        std::fs::create_dir_all(&dir).map_err(io)?;
        let path = dir.join(format!("{sha}.nnue"));
        if !path.is_file() {
            let partial = dir.join(format!("{sha}.nnue.partial"));
            std::fs::copy(file, &partial).map_err(io)?;
            std::fs::rename(&partial, &path).map_err(io)?;
        }
        let mut name = source.name.clone();
        let mut n = 2;
        while store.net_by_name(&name).await?.is_some() {
            name = format!("{}-{n}", source.name);
            n += 1;
        }
        let net = store
            .insert_net(NewNet {
                name,
                sha256: sha,
                path: path.display().to_string(),
                arch: if size == RECKLESS_NET_BYTES { RECKLESS_ARCH.to_owned() } else { format!("unknown-{size}") },
                size_bytes: size,
                parent: source.parent,
                dataset: source.dataset,
                run: source.run,
            })
            .await?;
        self.hub.bus.emit(Payload::NetUpserted(Box::new(net.clone())));
        Ok(net)
    }

    /// Where the shipped net of `commit` is on disk, by the name its build script gives.
    async fn shipped_file(&self, versions: &Versions, commit: &str) -> Result<(String, PathBuf), Error> {
        let script = versions
            .repo()
            .read_file(commit, "build/build.rs")
            .await
            .map_err(|e| Error::internal(format!("read build/build.rs of {commit}: {e:#}")))?;
        let file = network_name(&script)
            .ok_or_else(|| Error::internal(format!("{commit}'s build script names no network (NETWORK_NAME)")))?;
        let config = &self.hub.config;
        let candidates =
            [config.reckless_dir.join("networks").join(&file), config.builds_dir().join("networks").join(&file)];
        let found = candidates.iter().find(|p| p.is_file()).cloned().ok_or_else(|| {
            Error::conflict(format!(
                "The net {file} is not on disk: build the champion once (its build fetches it) or put it in {}",
                candidates[0].display()
            ))
        })?;
        Ok((file, found))
    }

    /// The shipped net of a version's commit, registered (copied into `nets/`) on first use.
    pub async fn shipped(&self, versions: &Versions, version: &Version) -> Result<Net, Error> {
        let (file, path) = self.shipped_file(versions, &version.commit).await?;
        let name = training::net_name_from_file(&file);
        if let Some(n) = self.hub.store.net_by_name(&name).await? {
            return Ok(n);
        }
        self.register_file(&path, NetSource { name, ..Default::default() }).await
    }

    /// Registers the champion's shipped net, when it is on disk. The server calls it at startup; a missing file is not
    /// an error there (the first build of the champion fetches it).
    pub async fn ensure_shipped(&self, versions: &Versions) -> Result<Option<Net>, Error> {
        let champion = versions.champion().await?;
        match self.shipped(versions, &champion).await {
            Ok(n) => Ok(Some(n)),
            Err(Error::Conflict(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The net a version plays with: its own trained net when it is code plus a net, else its commit's shipped net.
    pub async fn version_net(&self, versions: &Versions, version: &Version) -> Result<Net, Error> {
        match version.net {
            Some(id) => self.get(id).await,
            None => self.shipped(versions, version).await,
        }
    }

    /// The net the champion plays with.
    pub async fn champion_net(&self, versions: &Versions) -> Result<Net, Error> {
        let champion = versions.champion().await?;
        self.version_net(versions, &champion).await
    }
}
