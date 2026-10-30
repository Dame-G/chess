//! Setup: downloading the built-in engines into the data directory's `tools/` and adding them to the engines list.
//!
//! - **Stockfish 19**, the release build for this platform (`tools/stockfish/`), picked from the release's asset list.
//! - **Reckless 0.9.0**, the release build for this platform (`tools/reckless/`): its assets are bare executables, one
//!   per CPU family, and setup takes the AVX2 build when the CPU has AVX2, else the generic one.
//!
//! Each is its own package: setup installs both unless asked for one. Nothing downloads until asked: `GET /api/setup`
//! says what would be fetched and how big it is (read from GitHub), `POST /api/setup` queues the `setup` work that
//! fetches it.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use chess_core::{
    Error,
    engines::EngineKind,
    work::{Priority, Progress, Subject, WorkKind},
};
use chess_engine::tools::{self, ReleaseAsset};
use chess_hub::{Executor, WorkContext};
use chess_store::{EngineRecord, NewWork};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::engines::Engines;

/// Stockfish's repository and the release the app installs.
pub const STOCKFISH_REPO: &str = "official-stockfish/Stockfish";
pub const STOCKFISH_TAG: &str = "sf_19";
/// The Windows build, when the release's asset list cannot be read.
pub const STOCKFISH_WINDOWS: &str =
    "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-windows-x86-64-universal.zip";
/// Reckless's repository and the release the app installs.
pub const RECKLESS_REPO: &str = "codedeliveryservice/Reckless";
pub const RECKLESS_TAG: &str = "v0.9.0";

/// What setup installs: `stockfish`, `reckless`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetupPackage {
    Stockfish,
    Reckless,
}

/// One download of the plan: a package's file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SetupDownload {
    pub package: SetupPackage,
    /// For people: `Stockfish 19`, `Reckless 0.9.0`.
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
    /// What cannot be installed here, for people: "Reckless 0.9.0 has no release build for this platform".
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

/// The names of Reckless's builds for a platform, best first (its assets are bare executables:
/// `reckless-windows-avx2.exe`, `reckless-linux-generic`, `reckless-macos`): on x86-64 the AVX2 build when the CPU
/// has AVX2, then the generic one; on macOS its one build. Empty where Reckless publishes none.
fn reckless_names(os: &str, arch: &str, avx2: bool) -> Vec<String> {
    match (os, arch) {
        ("macos", _) => vec!["reckless-macos".into()],
        ("windows" | "linux", "x86_64") => {
            let ext = if os == "windows" { ".exe" } else { "" };
            let mut names = Vec::new();
            if avx2 {
                names.push(format!("reckless-{os}-avx2{ext}"));
            }
            names.push(format!("reckless-{os}-generic{ext}"));
            names
        }
        _ => Vec::new(),
    }
}

/// The Reckless build for a platform among a release's assets, by [`reckless_names`].
pub fn pick_reckless<'a>(assets: &'a [ReleaseAsset], os: &str, arch: &str, avx2: bool) -> Option<&'a ReleaseAsset> {
    reckless_names(os, arch, avx2).iter().find_map(|n| assets.iter().find(|a| &a.name == n))
}

/// Whether this CPU has AVX2, which Reckless's fast x86-64 build needs.
fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Where setup puts things, under the tools directory.
fn dirs(tools: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (tools.join("stockfish"), tools.join("reckless"), tools.join("downloads"))
}

/// The Stockfish binary in its unpacked release.
fn stockfish_binary(dir: &Path) -> Option<PathBuf> {
    tools::find_binary(dir, |n| n.starts_with("stockfish") && (!cfg!(windows) || n.ends_with(".exe")))
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
        if self.installed("reckless").await {
            plan.installed.push(SetupPackage::Reckless);
        } else {
            let assets = tools::release_assets(&api, RECKLESS_REPO, RECKLESS_TAG).await.unwrap_or_default();
            let name = format!("Reckless {}", RECKLESS_TAG.trim_start_matches('v'));
            let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
            match pick_reckless(&assets, os, arch, has_avx2()) {
                Some(a) => plan.downloads.push(SetupDownload {
                    package: SetupPackage::Reckless,
                    name,
                    url: a.url.clone(),
                    bytes: Some(a.size),
                }),
                // GitHub did not answer: the build's name is known, its size is not.
                None => match reckless_names(os, arch, has_avx2()).first() {
                    Some(file) if assets.is_empty() => plan.downloads.push(SetupDownload {
                        package: SetupPackage::Reckless,
                        name,
                        url: format!("https://github.com/{RECKLESS_REPO}/releases/download/{RECKLESS_TAG}/{file}"),
                        bytes: None,
                    }),
                    _ => plan.notes.push(format!("{name} has no release build for this platform found")),
                },
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
        let (sf_dir, reckless_dir, downloads) = dirs(&self.tools());
        let mut done = 0u64;
        let total = plan.downloads.iter().filter(|d| req.wants(d.package)).filter_map(|d| d.bytes).sum();
        let mut installed = Vec::new();
        for d in plan.downloads.iter().filter(|d| req.wants(d.package)) {
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            let file_name = d.url.rsplit('/').next().unwrap_or("download").to_owned();
            let record = match d.package {
                SetupPackage::Stockfish => {
                    let archive = downloads.join(&file_name);
                    self.download(&cx, d, &archive, total, &mut done).await?;
                    tools::unpack(&archive, &sf_dir).await.map_err(|e| Error::internal(format!("{e:#}")))?;
                    let bin = stockfish_binary(&sf_dir)
                        .ok_or_else(|| Error::internal("the Stockfish release holds no stockfish binary"))?;
                    EngineRecord::new("stockfish", "Stockfish", EngineKind::Builtin, bin.display().to_string())
                }
                SetupPackage::Reckless => {
                    // The release's assets are the executables themselves: nothing to unpack.
                    let bin = reckless_dir.join(&file_name);
                    self.download(&cx, d, &bin, total, &mut done).await?;
                    tools::make_executable(&bin).map_err(|e| Error::internal(format!("{e:#}")))?;
                    EngineRecord::new("reckless", "Reckless", EngineKind::Builtin, bin.display().to_string())
                }
            };
            let engine = self.engines.install(record).await?;
            cx.log(format!("Installed {}", engine.label()));
            installed.push(engine.id);
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
    fn picks_the_reckless_build_for_the_cpu() {
        let assets: Vec<ReleaseAsset> = [
            "reckless-windows-avx2.exe",
            "reckless-windows-avx512.exe",
            "reckless-windows-generic.exe",
            "reckless-linux-avx2",
            "reckless-linux-avx512",
            "reckless-linux-generic",
            "reckless-macos",
        ]
        .into_iter()
        .map(asset)
        .collect();
        let pick = |os, arch, avx2| pick_reckless(&assets, os, arch, avx2).map(|a| a.name.as_str());
        assert_eq!(pick("windows", "x86_64", true), Some("reckless-windows-avx2.exe"));
        assert_eq!(pick("windows", "x86_64", false), Some("reckless-windows-generic.exe"));
        assert_eq!(pick("linux", "x86_64", true), Some("reckless-linux-avx2"));
        assert_eq!(pick("linux", "x86_64", false), Some("reckless-linux-generic"));
        assert_eq!(pick("macos", "aarch64", false), Some("reckless-macos"));
        assert_eq!(pick("linux", "aarch64", true), None);
        assert_eq!(pick("freebsd", "x86_64", true), None);
        // Without an AVX2 build in the release, the generic one.
        let generic = [asset("reckless-windows-generic.exe")];
        assert_eq!(pick_reckless(&generic, "windows", "x86_64", true).unwrap().name, "reckless-windows-generic.exe");
    }

    #[test]
    fn setup_takes_everything_unless_told() {
        assert!(SetupRequest::default().wants(SetupPackage::Reckless));
        let only = SetupRequest { packages: vec![SetupPackage::Stockfish] };
        assert!(only.wants(SetupPackage::Stockfish) && !only.wants(SetupPackage::Reckless));
    }
}
