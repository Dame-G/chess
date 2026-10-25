use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use super::{ApiError, AppState};

/// The machine the runner is on, read once at start.
#[derive(Clone, Serialize, ToSchema)]
pub struct Machine {
    pub name: String,
    pub cores: usize,
    pub threads: usize,
    /// Total memory in bytes.
    pub memory: u64,
}

impl Machine {
    pub fn detect() -> Self {
        use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        let threads = sys.cpus().len();
        Self {
            name: sys.cpus().first().map(|c| c.brand().trim().to_owned()).unwrap_or_default(),
            cores: System::physical_core_count().unwrap_or(threads),
            threads,
            memory: sys.total_memory(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct Status {
    /// The runner's version.
    pub version: String,
    pub cpu: Machine,
    /// The Reckless checkout the runner builds from.
    pub reckless_dir: String,
    /// Whether that checkout exists.
    pub reckless_found: bool,
    /// The database schema version (migrations applied).
    pub schema: i64,
}

/// The runner's version, machine and engine checkout.
#[utoipa::path(get, path = "/api/status", tag = "runner", responses((status = 200, body = Status)))]
pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<Status>, ApiError> {
    let reckless = &state.config.reckless;
    let schema = state.store.call(|c| Ok(c.pragma_query_value(None, "user_version", |r| r.get(0))?)).await?;
    Ok(Json(Status {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        cpu: state.machine.clone(),
        reckless_dir: reckless.display().to_string(),
        reckless_found: reckless.join("Cargo.toml").is_file(),
        schema,
    }))
}
