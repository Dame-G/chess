//! Health: what Settings shows about the machine, one item per thing that has to be right for the app to work or for
//! its results to be trusted. fastchess and the book, the engine checkout, clang (Syzygy builds), the CUDA Toolkit and
//! the GPU (for training only), free disk, the database, builds on disk, and the harness check.
//!
//! It is cheap enough to ask on every Settings open: the probes that start processes (clang, nvcc, nvidia-smi) are
//! kept for a minute. Probing goes through [`Probes`], so tests inject what they need and never depend on the machine
//! they run on.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use dameless_core::{Error, harness::Machine};
use dameless_engine::proc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{harness::Harness, versions::MAIN};

/// Free disk under this is a warning: builds, games and training data need room.
pub const DISK_WARN_BYTES: u64 = 20 * GB;
/// Free disk under this fails: builds and fastchess's PGNs may not fit.
pub const DISK_FAIL_BYTES: u64 = 5 * GB;
/// How long the probes that start processes are kept.
pub const SLOW_PROBES_TTL: Duration = Duration::from_secs(60);
/// How long one of those processes may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const GB: u64 = 1_000_000_000;

/// The CUDA Toolkit, as found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Cuda {
    /// `CUDA_PATH`, when set.
    pub path: Option<String>,
    /// nvcc's release line ("release 12.4, V12.4.131"), when it runs.
    pub nvcc: Option<String>,
}

/// The GPU, from nvidia-smi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Gpu {
    pub name: String,
    pub memory_mb: Option<u64>,
}

/// Space on the drive holding a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiskSpace {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// What the machine is and has. [`SystemProbes`] asks the real one.
#[async_trait]
pub trait Probes: Send + Sync {
    /// The CPU, its physical cores and the OS: what a harness check certifies.
    fn machine(&self) -> Machine;
    /// clang's version line, when it is on the PATH.
    async fn clang(&self) -> Option<String>;
    async fn cuda(&self) -> Cuda;
    async fn gpu(&self) -> Option<Gpu>;
    /// Space on the drive that holds `path`.
    fn disk(&self, path: &Path) -> Option<DiskSpace>;
}

/// The real machine: sysinfo, and the tools' own `--version`.
#[derive(Default)]
pub struct SystemProbes {
    machine: OnceLock<Machine>,
}

/// Runs a probe command, giving up after a few seconds; its stdout, when it worked.
async fn probe(program: impl Into<PathBuf>, args: &[&str]) -> Option<String> {
    let cmd = proc::Command::new(program).args(args.iter().copied());
    tokio::time::timeout(PROBE_TIMEOUT, proc::output(&cmd)).await.ok()?.ok()
}

#[async_trait]
impl Probes for SystemProbes {
    fn machine(&self) -> Machine {
        self.machine
            .get_or_init(|| {
                let mut sys = sysinfo::System::new();
                sys.refresh_cpu_all();
                let cpu = sys.cpus().first().map(|c| c.brand().trim().to_owned()).unwrap_or_default();
                let cores = sysinfo::System::physical_core_count()
                    .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
                    .unwrap_or(1) as u32;
                let os = sysinfo::System::long_os_version()
                    .or_else(sysinfo::System::name)
                    .unwrap_or_else(|| std::env::consts::OS.to_owned());
                Machine { cpu, cores, os }
            })
            .clone()
    }

    async fn clang(&self) -> Option<String> {
        probe("clang", &["--version"]).await.and_then(|o| o.lines().next().map(|l| l.trim().to_owned()))
    }

    async fn cuda(&self) -> Cuda {
        let path = std::env::var("CUDA_PATH").ok().filter(|p| !p.trim().is_empty());
        let release = |out: String| out.lines().find(|l| l.contains("release")).map(|l| l.trim().to_owned());
        let mut nvcc = probe("nvcc", &["--version"]).await.and_then(release);
        if nvcc.is_none()
            && let Some(p) = &path
        {
            let bin = Path::new(p).join("bin").join(dameless_engine::exe("nvcc"));
            nvcc = probe(bin, &["--version"]).await.and_then(release);
        }
        Cuda { path, nvcc }
    }

    async fn gpu(&self) -> Option<Gpu> {
        let out = probe("nvidia-smi", &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]).await?;
        parse_gpu(&out)
    }

    fn disk(&self, path: &Path) -> Option<DiskSpace> {
        let path = existing_ancestor(path)?;
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let text = path.to_string_lossy();
        // Windows' canonical form starts with \\?\, which mount points do not.
        let path = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text));
        let disks = sysinfo::Disks::new_with_refreshed_list();
        disks
            .list()
            .iter()
            .filter(|d| path.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| DiskSpace { free_bytes: d.available_space(), total_bytes: d.total_space() })
    }
}

/// The first line of nvidia-smi's CSV: "NVIDIA GeForce RTX 3080, 10240".
fn parse_gpu(out: &str) -> Option<Gpu> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next().filter(|n| !n.is_empty())?.to_owned();
    Some(Gpu { name, memory_mb: parts.next().and_then(|m| m.parse().ok()) })
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.exists()).map(Path::to_path_buf)
}

/// How an item stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Ok,
    /// For your information: not needed for what the app does now (CUDA, the GPU).
    Info,
    /// Works, but needs attention soon (low disk, the harness unchecked, no clang).
    Warn,
    /// Broken: something will fail until it is fixed.
    Fail,
}

/// What fixes an item: a label for the button, and the CLI command that does the same.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthAction {
    pub label: String,
    pub command: String,
}

/// One thing Settings shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthItem {
    /// `fastchess`, `book`, `engine`, `clang`, `cuda`, `gpu`, `disk`, `database`, `builds`, `harness`.
    pub key: String,
    /// Not failing (a warning or a note is still ok).
    pub ok: bool,
    pub level: HealthLevel,
    pub title: String,
    /// One sentence for people.
    pub detail: String,
    pub action: Option<HealthAction>,
}

/// `GET /api/health`: every item, and whether none fails.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Health {
    pub ok: bool,
    pub items: Vec<HealthItem>,
}

fn item(key: &str, level: HealthLevel, title: &str, detail: String, action: Option<(&str, &str)>) -> HealthItem {
    HealthItem {
        key: key.to_owned(),
        ok: level != HealthLevel::Fail,
        level,
        title: title.to_owned(),
        detail,
        action: action.map(|(label, command)| HealthAction { label: label.to_owned(), command: command.to_owned() }),
    }
}

/// Bytes for people: "12.3 GB".
pub fn bytes_text(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[unit]) }
}

/// The probes that start processes, as last asked.
#[derive(Clone)]
struct Slow {
    at: Instant,
    clang: Option<String>,
    cuda: Cuda,
    gpu: Option<Gpu>,
}

/// Health of the machine and the app. Cheap to clone.
#[derive(Clone)]
pub struct Diagnostics {
    harness: Harness,
    probes: Arc<dyn Probes>,
    slow: Arc<Mutex<Option<Slow>>>,
}

impl Diagnostics {
    /// Over the harness check's probes.
    pub fn new(harness: Harness) -> Self {
        let probes = harness.probes().clone();
        Self { harness, probes, slow: Arc::default() }
    }

    async fn slow(&self) -> Slow {
        if let Some(s) = self.slow.lock().unwrap().clone().filter(|s| s.at.elapsed() < SLOW_PROBES_TTL) {
            return s;
        }
        let (clang, cuda, gpu) = tokio::join!(self.probes.clang(), self.probes.cuda(), self.probes.gpu());
        let s = Slow { at: Instant::now(), clang, cuda, gpu };
        *self.slow.lock().unwrap() = Some(s.clone());
        s
    }

    /// Every item, in the order Settings shows them.
    pub async fn health(&self) -> Result<Health, Error> {
        let versions = self.harness.ideas().versions();
        let hub = versions.hub();
        let config = &hub.config;
        let mut items = Vec::new();

        let tools = versions.toolchain().tools(&config.tools_dir());
        let setup = Some(("Install", "dameless setup"));
        items.push(match &tools.fastchess {
            Some(p) => item(
                "fastchess",
                HealthLevel::Ok,
                "fastchess",
                format!("{} installed: it plays every trial's games", crate::harness::tool_name(p)),
                None,
            ),
            None => item(
                "fastchess",
                HealthLevel::Fail,
                "fastchess",
                "Not installed: trials cannot play games".into(),
                setup,
            ),
        });
        items.push(match &tools.book {
            Some(p) => item(
                "book",
                HealthLevel::Ok,
                "Opening book",
                format!("{} installed", crate::harness::tool_name(p)),
                None,
            ),
            None => item("book", HealthLevel::Fail, "Opening book", "Not installed: trials need it".into(), setup),
        });

        let dir = config.reckless_dir.display().to_string();
        items.push(match versions.repo().resolve(MAIN).await {
            Ok(c) => item(
                "engine",
                HealthLevel::Ok,
                "Engine checkout",
                format!(
                    "{dir}: main at {}{}",
                    c.sha.get(..8).unwrap_or(&c.sha),
                    c.bench.map(|b| format!(", Bench: {b}")).unwrap_or_else(|| ", with no Bench: line".into())
                ),
                None,
            ),
            Err(_) if !config.reckless_dir.is_dir() => item(
                "engine",
                HealthLevel::Fail,
                "Engine checkout",
                format!("No checkout at {dir}: set RECKLESS_DIR to the fork's clone"),
                None,
            ),
            Err(_) => item(
                "engine",
                HealthLevel::Fail,
                "Engine checkout",
                format!("{dir} has no branch main to build the champion from"),
                None,
            ),
        });

        let slow = self.slow().await;
        items.push(match slow.clang {
            Some(v) => item("clang", HealthLevel::Ok, "clang", format!("{v}: builds include Syzygy tablebases"), None),
            None => item(
                "clang",
                HealthLevel::Warn,
                "clang",
                "Not found: engines are built without Syzygy tablebases".into(),
                None,
            ),
        });
        items.push(match (&slow.cuda.path, &slow.cuda.nvcc) {
            (Some(_), Some(v)) => {
                item("cuda", HealthLevel::Ok, "CUDA Toolkit", format!("nvcc {v}: ready for training"), None)
            }
            (Some(p), None) => item(
                "cuda",
                HealthLevel::Info,
                "CUDA Toolkit",
                format!("CUDA_PATH is {p}, but nvcc does not run; needed only for training"),
                None,
            ),
            (None, _) => item(
                "cuda",
                HealthLevel::Info,
                "CUDA Toolkit",
                "Not installed (CUDA_PATH is not set); needed only for training".into(),
                None,
            ),
        });
        items.push(match &slow.gpu {
            Some(g) => item(
                "gpu",
                HealthLevel::Ok,
                "GPU",
                match g.memory_mb {
                    Some(mb) => format!("{}, {} of memory", g.name, bytes_text(mb * 1_048_576)),
                    None => g.name.clone(),
                },
                None,
            ),
            None => item(
                "gpu",
                HealthLevel::Info,
                "GPU",
                "No NVIDIA GPU found (nvidia-smi); needed only for training".into(),
                None,
            ),
        });

        items.push(match self.probes.disk(&config.data_dir) {
            Some(d) => {
                let level = if d.free_bytes < DISK_FAIL_BYTES {
                    HealthLevel::Fail
                } else if d.free_bytes < DISK_WARN_BYTES {
                    HealthLevel::Warn
                } else {
                    HealthLevel::Ok
                };
                let detail = format!(
                    "{} free of {} on the data directory's drive{}",
                    bytes_text(d.free_bytes),
                    bytes_text(d.total_bytes),
                    match level {
                        HealthLevel::Fail => ": builds and games may not fit",
                        HealthLevel::Warn => ": builds, games and data need room",
                        _ => "",
                    }
                );
                let action = (level != HealthLevel::Ok).then_some(("Prune builds", "dameless builds prune"));
                item("disk", level, "Disk", detail, action)
            }
            None => item("disk", HealthLevel::Warn, "Disk", "Free space could not be read".into(), None),
        });

        let db = hub.store.path().to_path_buf();
        let db_bytes = [db.clone(), PathBuf::from(format!("{}-wal", db.display()))]
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum::<u64>();
        items.push(item(
            "database",
            HealthLevel::Ok,
            "Database",
            format!("{} at {}", bytes_text(db_bytes), db.display()),
            None,
        ));

        let (count, bytes) = builds_on_disk(&config.builds_dir());
        items.push(item(
            "builds",
            HealthLevel::Ok,
            "Builds",
            format!("{count} build{} on disk, {}", if count == 1 { "" } else { "s" }, bytes_text(bytes)),
            (count > 0).then_some(("Prune builds", "dameless builds prune")),
        ));

        let h = self.harness.status().await?;
        let check = Some(("Check the harness", "dameless harness check"));
        items.push(if h.trusted {
            item("harness", HealthLevel::Ok, "Harness", h.summary, None)
        } else if h.running.is_some() {
            item("harness", HealthLevel::Warn, "Harness", h.summary, None)
        } else if h.current && h.passed == Some(false) {
            item("harness", HealthLevel::Fail, "Harness", h.summary, check)
        } else {
            item("harness", HealthLevel::Warn, "Harness", h.summary, check)
        });

        Ok(Health { ok: items.iter().all(|i| i.ok), items })
    }
}

/// Built binaries under the builds directory (each in its own folder; cargo's `target` and the networks are not
/// builds): how many, and how much space their folders take.
fn builds_on_disk(dir: &Path) -> (u32, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return (0, 0) };
    let mut count = 0;
    let mut bytes = 0;
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name();
        if !path.is_dir() || name == "target" || name == "networks" {
            continue;
        }
        if path.join(dameless_engine::exe("reckless")).is_file() {
            count += 1;
            bytes += dir_size(&path);
        }
    }
    (count, bytes)
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests;
