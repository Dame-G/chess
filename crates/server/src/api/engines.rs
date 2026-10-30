//! The engines list: the built-ins and the engines you added, which one analyses, setup (downloading the built-ins),
//! and the machine's diagnostics.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use chess_core::{
    engines::{Engine, EnginesOverview},
    work::Work,
};
use chess_work::{Health, NewEngineRequest, SetupPlan, SetupRequest};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiResult, Json};
use crate::App;

/// Every engine: the built-ins first (installed or not), then the engines you added; and the analysis engine.
#[utoipa::path(get, path = "/api/engines", operation_id = "list_engines", tag = "engines",
    responses((status = 200, body = EnginesOverview)))]
pub async fn list_engines(State(app): State<Arc<App>>) -> ApiResult<Json<EnginesOverview>> {
    Ok(Json(app.engines.overview().await?))
}

/// Adds a UCI engine by path. It must answer UCI's handshake; what it calls itself (`id name`) is kept. 409 when the
/// id is taken (a built-in's or another engine's), 400 when the file is not a UCI engine.
#[utoipa::path(post, path = "/api/engines", operation_id = "add_engine", tag = "engines", request_body = NewEngineRequest,
    responses((status = 201, body = Engine), (status = 400, body = chess_core::error::ErrorBody),
        (status = 409, body = chess_core::error::ErrorBody)))]
pub async fn add_engine(
    State(app): State<Arc<App>>,
    Json(b): Json<NewEngineRequest>,
) -> ApiResult<(StatusCode, Json<Engine>)> {
    Ok((StatusCode::CREATED, Json(app.engines.add(b).await?)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct EnginePath {
    /// The engine's id: `stockfish`, `maia-1500`, `my-engine`.
    pub id: String,
}

/// Removes an engine from the list. A built-in's files stay (setup adds it again); when it was the analysis engine,
/// Stockfish analyses again.
#[utoipa::path(delete, path = "/api/engines/{id}", operation_id = "remove_engine", tag = "engines", params(EnginePath),
    responses((status = 200, body = EnginesOverview), (status = 404, body = chess_core::error::ErrorBody)))]
pub async fn remove_engine(State(app): State<Arc<App>>, Path(p): Path<EnginePath>) -> ApiResult<Json<EnginesOverview>> {
    Ok(Json(app.engines.remove(&p.id).await?))
}

/// `PUT /api/engines/analysis`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct AnalysisEngine {
    /// The engine's id.
    pub engine: String,
}

/// Chooses the analysis engine: reviews, live evaluation, drills and the levels run on it. It must be installed and
/// play on the clock (Maia's fixed node cannot analyse: 400).
#[utoipa::path(put, path = "/api/engines/analysis", operation_id = "set_analysis_engine", tag = "engines",
    request_body = AnalysisEngine,
    responses((status = 200, body = EnginesOverview), (status = 400, body = chess_core::error::ErrorBody),
        (status = 404, body = chess_core::error::ErrorBody), (status = 409, body = chess_core::error::ErrorBody)))]
pub async fn set_analysis_engine(
    State(app): State<Arc<App>>,
    Json(b): Json<AnalysisEngine>,
) -> ApiResult<Json<EnginesOverview>> {
    Ok(Json(app.engines.set_analysis(&b.engine).await?))
}

/// What setup would download (Stockfish 19, lc0 and Maia's nets) with the sizes GitHub gives, and what is installed
/// already. Downloads nothing.
#[utoipa::path(get, path = "/api/setup", operation_id = "setup_plan", tag = "engines",
    responses((status = 200, body = SetupPlan)))]
pub async fn setup_plan(State(app): State<Arc<App>>) -> ApiResult<Json<SetupPlan>> {
    Ok(Json(app.setup.plan().await?))
}

/// Downloads the built-in engines into the data directory's `tools/` and adds them to the list, as work. Only on this
/// explicit request (the CLI shows the sizes and asks first).
#[utoipa::path(post, path = "/api/setup", operation_id = "setup", tag = "engines", request_body = SetupRequest,
    responses((status = 201, body = Work)))]
pub async fn setup(State(app): State<Arc<App>>, body: axum::body::Bytes) -> ApiResult<(StatusCode, Json<Work>)> {
    let req: SetupRequest = super::optional_body(&body)?;
    let work = app.scheduler.submit(app.setup.work(&req)).await?;
    Ok((StatusCode::CREATED, Json(work)))
}

/// The machine's health for Settings: the analysis engine, the Maia engines, the disk and the database, each with
/// what to do about it.
#[utoipa::path(get, path = "/api/diagnostics", operation_id = "diagnostics", tag = "engines",
    responses((status = 200, body = Health)))]
pub async fn diagnostics(State(app): State<Arc<App>>) -> ApiResult<Json<Health>> {
    Ok(Json(app.diagnostics.health().await?))
}
