//! Setup: downloading the built-in engines into the data directory's `tools/` and adding them to the engines list.
//!
//! - **Stockfish 19**, the release build for this platform (`tools/stockfish/`), picked from the release's asset list.
//! - **Maia** 1100 to 1900: lc0 v0.32.1 (`tools/lc0/`; lc0 publishes Windows builds only, so elsewhere an `lc0` on the
//!   PATH is used) with Maia's nine nets (`tools/maia/maia-<n>.pb.gz`), each played at one node a move.
//!
//! Nothing downloads until asked: `GET /api/setup` says what would be fetched and how big it is (read from GitHub),
//! `POST /api/setup` queues the `setup` work that fetches it.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use dameless_core::{
    Error,
    engines::{Builtin, MAIA_RATINGS, Package, builtin},
    work::{Priority, Progress, Subject, WorkKind},
};
use dameless_engine::{
    proc,
    tools::{self, ReleaseAsset},
};
use dameless_hub::{Executor, WorkContext};
use dameless_store::{EngineRecord, NewWork};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::engines::Engines;

/// Stockfish's repository and the release the app installs.
pub const STOCKFISH_REPO: &str = "official-stockfish/Stockfish";
pub const STOCKFISH_TAG: &str = "sf_19";
/// The Windows build, when the release's asset list cannot be read.
pub const STOCKFISH_WINDOWS: &str =
    "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-windows-x86-64-universal.zip";
/// lc0's repository, the release, and its CPU build for Windows (the only platform it publishes binaries for).
pub const LC0_REPO: &str = "LeelaChessZero/lc0";
pub const LC0_TAG: &str = "v0.32.1";
pub const LC0_WINDOWS_ASSET: &str = "lc0-v0.32.1-windows-cpu-dnnl.zip";
/// Maia's nets, by rating.
pub fn maia_url(rating: u32) -> String {
    format!("https://github.com/CSSLab/maia-chess/raw/master/maia_weights/maia-{rating}.pb.gz")
}

/// What setup installs: `stockfish`, `maia`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetupPackage {
    Stockfish,
    Maia,
}

/// One download of the plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SetupDownload {
    pub package: SetupPackage,
    /// For people: `Stockfish 19`, `lc0 v0.32.1`, `Maia 1500's net`.
    pub name: String,
    pub url: String,
    /// Its size, when GitHub said.
    pub bytes: Option<u64>,
}

/// `GET /api/setup`: what `POST /api/setup` would download, and what is installed already.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SetupPlan {
    pub downloads: Vec<SetupDownload>,
    /// Everything together, of the sizes known.
    pub bytes: u64,
    /// The packages installed already (they are skipped).
    pub installed: Vec<SetupPackage>,
    /// What cannot be installed here, for people: "lc0 publishes no build for this platform: put lc0 on the PATH".
    pub notes: Vec<String>,
}

/// `POST /api/setup`: which packages to install; all when empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SetupRequest {
    #[serde(default)]
    pub packages: Vec<SetupPackage>,
}

impl SetupRequest {
    fn wants(&self, p: SetupPackage) -> bool {
        self.packages.is_empty() || self.packages.contains(&p)
    }
}

/// The Stockfish build for a platform (`std::env::consts::OS` and `ARCH`) among a release's assets: the universal
/// build when there is one, else AVX2, else the plain x86-64 build.
pub fn pick_stockfish<'a>(assets: &'a [ReleaseAsset], os: &str, arch: &str) -> Option<&'a ReleaseAsset> {
    let os_word = match os {
        "windows" => "windows",
        "linux" => "ubuntu",
        "macos" => "macos",
        _ => return None,
    };
    let candidates: Vec<&ReleaseAsset> = assets
        .iter()
        .filter(|a| a.name.contains(os_word) && (a.name.ends_with(".zip") || a.name.ends_with(".tar")))
        .filter(|a| match arch {
            "x86_64" => a.name.contains("x86-64"),
            "aarch64" => a.name.contains("apple-silicon") || a.name.contains("armv8") || a.name.contains("m1"),
            _ => false,
        })
        .collect();
    let stem = |a: &ReleaseAsset| a.name.trim_end_matches(".zip").trim_end_matches(".tar").to_owned();
    ["-universal", "-avx2", "x86-64", ""]
        .iter()
        .find_map(|suffix| candidates.iter().copied().find(|a| stem(a).ends_with(suffix)))
}

/// Where setup puts things, under the tools directory.
fn dirs(tools: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    (tools.join("stockfish"), tools.join("lc0"), tools.join("maia"), tools.join("downloads"))
}

/// The Stockfish binary in its unpacked release.
fn stockfish_binary(dir: &Path) -> Option<PathBuf> {
    tools::find_binary(dir, |n| n.starts_with("stockfish") && (!cfg!(windows) || n.ends_with(".exe")))
}

/// lc0: the one setup unpacked, else one on the PATH.
fn lc0_binary(dir: &Path) -> Option<PathBuf> {
    tools::find_binary(dir, |n| n == dameless_engine::exe("lc0")).or_else(|| proc::which("lc0"))
}

/// Setup: the plan and the `setup` work. Cheap to clone.
#[derive(Clone)]
pub struct Setup {
    engines: Engines,
}

impl Setup {
    pub fn new(engines: Engines) -> Self {
        Self { engines }
    }

    /// The executor to register with the scheduler.
    pub fn executor(&self) -> Arc<dyn Executor> {
        Arc::new(self.clone())
    }

    fn tools(&self) -> PathBuf {
        self.engines.hub().config.tools_dir()
    }

    async fn installed(&self, id: &str) -> bool {
        self.engines.get(id).await.is_ok_and(|e| e.installed)
    }

    /// What would be downloaded, with sizes from GitHub (missing when it does not answer).
    pub async fn plan(&self) -> Result<SetupPlan, Error> {
        let api = self.engines.hub().config.github_api.clone();
        let mut plan = SetupPlan { downloads: Vec::new(), bytes: 0, installed: Vec::new(), notes: Vec::new() };
        if self.installed("stockfish").await {
            plan.installed.push(SetupPackage::Stockfish);
        } else {
            let assets = tools::release_assets(&api, STOCKFISH_REPO, STOCKFISH_TAG).await.unwrap_or_default();
            match pick_stockfish(&assets, std::env::consts::OS, std::env::consts::ARCH) {
                Some(a) => plan.downloads.push(SetupDownload {
                    package: SetupPackage::Stockfish,
                    name: "Stockfish 19".into(),
                    url: a.url.clone(),
                    bytes: Some(a.size),
                }),
                None if cfg!(windows) => plan.downloads.push(SetupDownload {
                    package: SetupPackage::Stockfish,
                    name: "Stockfish 19".into(),
                    url: STOCKFISH_WINDOWS.into(),
                    bytes: None,
                }),
                None => plan.notes.push("Stockfish 19 has no release build for this platform found".into()),
            }
        }
        let maias: Vec<&Builtin> = MAIA_RATINGS.iter().filter_map(|r| builtin(&format!("maia-{r}"))).collect();
        let mut missing = Vec::new();
        for m in &maias {
            if !self.installed(m.id).await {
                missing.push(*m);
            }
        }
        if missing.is_empty() {
            plan.installed.push(SetupPackage::Maia);
        } else {
            let (_, lc0_dir, _, _) = dirs(&self.tools());
            if lc0_binary(&lc0_dir).is_none() {
                if cfg!(windows) {
                    let assets = tools::release_assets(&api, LC0_REPO, LC0_TAG).await.unwrap_or_default();
                    let asset = assets.iter().find(|a| a.name == LC0_WINDOWS_ASSET);
                    plan.downloads.push(SetupDownload {
                        package: SetupPackage::Maia,
                        name: format!("lc0 {LC0_TAG}"),
                        url: asset.map(|a| a.url.clone()).unwrap_or_else(|| {
                            format!("https://github.com/{LC0_REPO}/releases/download/{LC0_TAG}/{LC0_WINDOWS_ASSET}")
                        }),
                        bytes: asset.map(|a| a.size),
                    });
                } else {
                    plan.notes.push("lc0 publishes no build for this platform: put lc0 on the PATH for Maia".into());
                }
            }
            let sizes = join_all(missing.iter().map(|m| {
                let url = maia_url(rating(m));
                async move { tools::content_length(&url).await }
            }))
            .await;
            for (m, bytes) in missing.iter().zip(sizes) {
                plan.downloads.push(SetupDownload {
                    package: SetupPackage::Maia,
                    name: format!("{}'s net", m.name),
                    url: maia_url(rating(m)),
                    bytes,
                });
            }
        }
        plan.bytes = plan.downloads.iter().filter_map(|d| d.bytes).sum();
        Ok(plan)
    }

    /// Queues the setup work.
    pub fn work(&self, req: &SetupRequest) -> NewWork {
        NewWork {
            kind: WorkKind::Setup,
            priority: Priority::Review,
            title: "Installing engines".into(),
            subject: Subject::Engines,
            spec: serde_json::to_value(req).unwrap_or_default(),
        }
    }

    async fn download(
        &self,
        cx: &WorkContext,
        d: &SetupDownload,
        to: &Path,
        total: u64,
        done: &mut u64,
    ) -> Result<(), Error> {
        cx.log(format!("Downloading {} from {}", d.name, d.url));
        let start = *done;
        let mut report = |n: u64| {
            *done = start + n;
        };
        tools::download(&d.url, to, &mut report).await.map_err(|e| Error::upstream(format!("{e:#}")))?;
        let _ = cx
            .progress_now(Progress {
                done: *done,
                total: (total > 0).then_some(total),
                unit: "bytes".into(),
                eta_seconds: None,
                text: Some(format!("{} downloaded", d.name)),
            })
            .await;
        Ok(())
    }
}

fn rating(b: &Builtin) -> u32 {
    match b.package {
        Package::Maia(r) => r,
        Package::Stockfish => 0,
    }
}

#[async_trait]
impl Executor for Setup {
    fn kind(&self) -> WorkKind {
        WorkKind::Setup
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let req: SetupRequest = cx.spec()?;
        let plan = self.plan().await?;
        for note in &plan.notes {
            cx.log(note);
        }
        let (sf_dir, lc0_dir, maia_dir, downloads) = dirs(&self.tools());
        let mut done = 0u64;
        let total = plan.bytes;
        let mut installed = Vec::new();
        for d in plan.downloads.iter().filter(|d| req.wants(d.package)) {
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            let file_name = d.url.rsplit('/').next().unwrap_or("download").to_owned();
            match d.package {
                SetupPackage::Stockfish => {
                    let archive = downloads.join(&file_name);
                    self.download(&cx, d, &archive, total, &mut done).await?;
                    tools::unpack(&archive, &sf_dir).await.map_err(|e| Error::internal(format!("{e:#}")))?;
                    let bin = stockfish_binary(&sf_dir)
                        .ok_or_else(|| Error::internal("the Stockfish release holds no stockfish binary"))?;
                    let engine = self
                        .engines
                        .install(EngineRecord::new(
                            "stockfish",
                            "Stockfish",
                            dameless_core::engines::EngineKind::Builtin,
                            bin.display().to_string(),
                        ))
                        .await?;
                    cx.log(format!("Installed {}", engine.label()));
                    installed.push(engine.id);
                }
                SetupPackage::Maia if file_name.starts_with("lc0") => {
                    let archive = downloads.join(&file_name);
                    self.download(&cx, d, &archive, total, &mut done).await?;
                    tools::unpack(&archive, &lc0_dir).await.map_err(|e| Error::internal(format!("{e:#}")))?;
                    cx.log(format!("Unpacked lc0 into {}", lc0_dir.display()));
                }
                SetupPackage::Maia => {
                    self.download(&cx, d, &maia_dir.join(&file_name), total, &mut done).await?;
                }
            }
        }
        if req.wants(SetupPackage::Maia) {
            match lc0_binary(&lc0_dir) {
                Some(lc0) => {
                    for r in MAIA_RATINGS {
                        let net = maia_dir.join(format!("maia-{r}.pb.gz"));
                        let id = format!("maia-{r}");
                        if !net.is_file() || self.installed(&id).await {
                            continue;
                        }
                        let b = builtin(&id).expect("every Maia rating is a built-in");
                        let mut record = EngineRecord::new(
                            &id,
                            b.name,
                            dameless_core::engines::EngineKind::Builtin,
                            lc0.display().to_string(),
                        );
                        record.options.insert("WeightsFile".into(), net.display().to_string());
                        record.nodes = b.nodes();
                        record.rating = b.rating();
                        let engine = self.engines.install(record).await?;
                        cx.log(format!("Installed {} ({})", engine.name, engine.label()));
                        installed.push(engine.id);
                    }
                }
                None => cx.log("No lc0: the Maia engines are not installed"),
            }
        }
        let _ = std::fs::remove_dir(&downloads);
        Ok(serde_json::json!({ "installed": installed }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset { name: name.into(), url: format!("https://x/{name}"), size: 1 }
    }

    #[test]
    fn picks_the_stockfish_build_for_the_platform() {
        let assets: Vec<ReleaseAsset> = [
            "stockfish-windows-x86-64.zip",
            "stockfish-windows-x86-64-avx2.zip",
            "stockfish-windows-x86-64-universal.zip",
            "stockfish-ubuntu-x86-64-avx2.tar",
            "stockfish-ubuntu-x86-64.tar",
            "stockfish-macos-m1-apple-silicon.tar",
            "stockfish-android-armv8.tar",
        ]
        .into_iter()
        .map(asset)
        .collect();
        let pick = |os, arch| pick_stockfish(&assets, os, arch).map(|a| a.name.as_str());
        assert_eq!(pick("windows", "x86_64"), Some("stockfish-windows-x86-64-universal.zip"));
        assert_eq!(pick("linux", "x86_64"), Some("stockfish-ubuntu-x86-64-avx2.tar"));
        assert_eq!(pick("macos", "aarch64"), Some("stockfish-macos-m1-apple-silicon.tar"));
        assert_eq!(pick("freebsd", "x86_64"), None);
        assert_eq!(pick_stockfish(&[asset("stockfish-ubuntu-x86-64.tar")], "linux", "x86_64").unwrap().size, 1);
    }

    #[test]
    fn setup_takes_everything_unless_told() {
        assert!(SetupRequest::default().wants(SetupPackage::Maia));
        let only = SetupRequest { packages: vec![SetupPackage::Stockfish] };
        assert!(only.wants(SetupPackage::Stockfish) && !only.wants(SetupPackage::Maia));
        assert!(maia_url(1500).ends_with("/maia_weights/maia-1500.pb.gz"));
    }
}
