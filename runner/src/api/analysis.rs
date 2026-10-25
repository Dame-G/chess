use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};

use super::{ApiError, AppState};
use crate::analysis::{self, AnalysisRequest, AnalysisResult, LiveSession};

/// Searches a position to a limit (depth 20 when none is given) and answers with the best move and lines.
#[utoipa::path(post, path = "/api/analyse", operation_id = "analyse", tag = "analysis", request_body = AnalysisRequest, responses((status = 200, body = AnalysisResult)))]
pub async fn analyse(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<AnalysisResult>, ApiError> {
    Ok(Json(analysis::analyse(&state, req).await?))
}

/// Starts a live search, replacing any other. Reports arrive as `analysis.info` events until it is stopped.
#[utoipa::path(post, path = "/api/analysis", operation_id = "start_analysis", tag = "analysis", request_body = AnalysisRequest, responses((status = 200, body = LiveSession)))]
pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<LiveSession>, ApiError> {
    Ok(Json(analysis::start_live(&state, req).await?))
}

/// Stops the live search.
#[utoipa::path(delete, path = "/api/analysis", operation_id = "stop_analysis", tag = "analysis", responses((status = 204)))]
pub async fn stop(State(state): State<Arc<AppState>>) -> StatusCode {
    analysis::stop_live(&state);
    StatusCode::NO_CONTENT
}
