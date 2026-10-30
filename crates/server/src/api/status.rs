//! The app itself: the health check, its status and the token.

use std::{sync::Arc, sync::OnceLock};

use axum::extract::State;
use chess_core::events::Policy;
use chess_hub::LedgerSnapshot;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::error::Json;
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
    /// Cores the scheduler hands out (`CHESS_CORES`).
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
}

/// The status line's facts, and Settings › Machine.
#[utoipa::path(get, path = "/api/status", operation_id = "status", tag = "app", responses((status = 200, body = Status)))]
pub async fn status(State(app): State<Arc<App>>) -> Json<Status> {
    let config = &app.hub.config;
    let fixed = fixed_machine();
    Json(Status {
        version: env!("CARGO_PKG_VERSION").into(),
        machine: Machine {
            os: fixed.os.clone(),
            cpu: fixed.cpu.clone(),
            physical_cores: fixed.physical_cores,
            logical_cores: fixed.logical_cores,
            memory_bytes: fixed.memory_bytes,
            data_dir: config.data_dir.display().to_string(),
        },
        cores: app.hub.ledger.total(),
        ledger: app.hub.ledger.snapshot(),
        policy: app.hub.policy.current(),
        lichess_url: app.connections.lichess_url().to_owned(),
        next_event: app.hub.bus.next_id(),
    })
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
