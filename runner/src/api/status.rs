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

impl Machine {
    /// How many games run at once by default, each on `threads_per_game` threads: one per physical core but one, as
    /// Fishtest advises (OpenBench's client takes every physical core). Games on both threads of a core each run slower
    /// and noisier, and the spare core keeps the Lichess bot and the UI off the games' cores.
    pub fn default_concurrency(&self, threads_per_game: u32) -> u32 {
        (self.cores.saturating_sub(1) as u32 / threads_per_game.max(1)).max(1)
    }
}

#[derive(Serialize, ToSchema)]
pub struct RunnerStatus {
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
#[utoipa::path(get, path = "/api/status", operation_id = "get_status", tag = "runner", responses((status = 200, body = RunnerStatus)))]
pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<RunnerStatus>, ApiError> {
    let reckless = &state.config.reckless;
    let schema = state.store.call(|c| Ok(c.pragma_query_value(None, "user_version", |r| r.get(0))?)).await?;
    Ok(Json(RunnerStatus {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        cpu: state.machine.clone(),
        reckless_dir: reckless.display().to_string(),
        reckless_found: reckless.join("Cargo.toml").is_file(),
        schema,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_game_per_physical_core_but_one() {
        let machine = Machine { name: String::new(), cores: 8, threads: 16, memory: 0 };
        assert_eq!(machine.default_concurrency(1), 7);
        assert_eq!(machine.default_concurrency(6), 1);
        assert_eq!(Machine { cores: 1, ..machine }.default_concurrency(1), 1);
    }
}
