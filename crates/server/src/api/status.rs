//! The app itself: the health check, its status, the token, and setup.

use std::{path::Path, sync::Arc, sync::OnceLock};

use axum::extract::State;
use dameless_core::{Error, events::Policy};
use dameless_engine::git::Repo;
use dameless_hub::LedgerSnapshot;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::error::{ApiResult, Json};
use crate::App;

/// The health check's answer.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Liveness {
    pub ok: bool,
    /// The app's version.
    pub version: String,
}

/// Whether the app is up. The one route that needs no token.
#[utoipa::path(get, path = "/api/health", operation_id = "health", tag = "app", security(()),
    responses((status = 200, body = Liveness)))]
pub async fn health() -> Json<Liveness> {
    Json(Liveness { ok: true, version: env!("CARGO_PKG_VERSION").into() })
}

/// The app and the machine it runs on.
#[derive(Serialize, ToSchema)]
pub struct Status {
    /// The app's version.
    pub version: String,
    pub machine: Machine,
    /// Cores the scheduler hands out (`DAMELESS_CORES`).
    pub cores: u32,
    /// Who holds them now.
    pub ledger: LedgerSnapshot,
    /// Fair play: whether analysis is allowed now.
    pub policy: Policy,
    /// The Lichess the connections talk to.
    pub lichess_url: String,
    /// The id the next event will get.
    pub next_event: u64,
}

/// What the app runs on.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[schema(as = MachineInfo)]
pub struct Machine {
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub physical_cores: Option<u32>,
    pub logical_cores: Option<u32>,
    pub memory_bytes: Option<u64>,
    /// The data directory.
    pub data_dir: String,
    pub engine: EngineCheckout,
    /// Built binaries on disk.
    pub builds: BuildsOnDisk,
}

/// The engine checkout (`RECKLESS_DIR`).
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct EngineCheckout {
    pub dir: String,
    /// Whether it is a git checkout.
    pub found: bool,
    /// The branch checked out, if any.
    pub branch: Option<String>,
    /// The commit checked out.
    pub commit: Option<String>,
    /// Uncommitted changes (they block promoting into a checked-out `main`).
    pub dirty: bool,
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildsOnDisk {
    /// Builds (folders holding an engine binary).
    pub count: u32,
    /// Everything under the builds directory: the builds, the nets they embed and cargo's cache.
    pub bytes: u64,
}

/// The status line's facts, and Settings › Machine.
#[utoipa::path(get, path = "/api/status", operation_id = "status", tag = "app", responses((status = 200, body = Status)))]
pub async fn status(State(app): State<Arc<App>>) -> ApiResult<Json<Status>> {
    let config = &app.hub.config;
    let engine = engine_checkout(&config.reckless_dir).await;
    let builds_dir = config.builds_dir();
    let builds = tokio::task::spawn_blocking(move || disk_usage(&builds_dir))
        .await
        .map_err(|e| Error::internal(e.to_string()))?;
    let fixed = fixed_machine();
    Ok(Json(Status {
        version: env!("CARGO_PKG_VERSION").into(),
        machine: Machine {
            os: fixed.os.clone(),
            cpu: fixed.cpu.clone(),
            physical_cores: fixed.physical_cores,
            logical_cores: fixed.logical_cores,
            memory_bytes: fixed.memory_bytes,
            data_dir: config.data_dir.display().to_string(),
            engine,
            builds,
        },
        cores: app.hub.ledger.total(),
        ledger: app.hub.ledger.snapshot(),
        policy: app.hub.policy.current(),
        lichess_url: app.connections.lichess_url().to_owned(),
        next_event: app.hub.bus.next_id(),
    }))
}

struct Fixed {
    os: Option<String>,
    cpu: Option<String>,
    physical_cores: Option<u32>,
    logical_cores: Option<u32>,
    memory_bytes: Option<u64>,
}

/// What does not change while the app runs, read once.
fn fixed_machine() -> &'static Fixed {
    static FIXED: OnceLock<Fixed> = OnceLock::new();
    FIXED.get_or_init(|| {
        use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        Fixed {
            os: System::long_os_version(),
            cpu: sys.cpus().first().map(|c| c.brand().trim().to_owned()).filter(|b| !b.is_empty()),
            physical_cores: System::physical_core_count().map(|n| n as u32),
            logical_cores: std::thread::available_parallelism().ok().map(|n| n.get() as u32),
            memory_bytes: Some(sys.total_memory()).filter(|m| *m > 0),
        }
    })
}

async fn engine_checkout(dir: &Path) -> EngineCheckout {
    let repo = Repo::new(dir);
    let commit = repo.git(&["rev-parse", "HEAD"]).await.ok().map(|s| s.trim().to_owned());
    let dirty = match commit {
        Some(_) => repo.git(&["status", "--porcelain"]).await.is_ok_and(|s| !s.trim().is_empty()),
        None => false,
    };
    EngineCheckout {
        dir: dir.display().to_string(),
        found: commit.is_some(),
        branch: if commit.is_some() { repo.checked_out().await } else { None },
        commit,
        dirty,
    }
}

/// The builds directly under `dir` and the bytes of everything in it.
fn disk_usage(dir: &Path) -> BuildsOnDisk {
    fn bytes(path: &Path) -> u64 {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.is_dir() => {
                std::fs::read_dir(path).map(|rd| rd.flatten().map(|e| bytes(&e.path())).sum()).unwrap_or(0)
            }
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    // A build is a folder holding the engine; `networks` and cargo's `target` cache are not builds, though their
    // bytes count as the disk the builds take.
    let count = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter(|e| e.path().join(dameless_engine::exe("reckless")).is_file()).count())
        .unwrap_or(0);
    BuildsOnDisk { count: count as u32, bytes: bytes(dir) }
}

/// The API token.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Token {
    pub token: String,
}

/// The API token, for pairing the hosted UI with this app (Settings shows it to copy).
#[utoipa::path(get, path = "/api/token", operation_id = "token", tag = "app", responses((status = 200, body = Token)))]
pub async fn token(State(app): State<Arc<App>>) -> Json<Token> {
    Json(Token { token: app.hub.token.clone() })
}

/// What setup did.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct SetupReport {
    /// What it did, line by line.
    pub log: Vec<String>,
}

/// Downloads fastchess and the opening book the trials use into the tools folder. Only on this explicit request.
#[utoipa::path(post, path = "/api/setup", operation_id = "setup", tag = "app",
    responses((status = 200, body = SetupReport), (status = 502, body = dameless_core::error::ErrorBody)))]
pub async fn setup(State(app): State<Arc<App>>) -> ApiResult<Json<SetupReport>> {
    let mut log = Vec::new();
    dameless_work::setup(&app.hub, &mut |line: &str| log.push(line.to_owned())).await?;
    Ok(Json(SetupReport { log }))
}
