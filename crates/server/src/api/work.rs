//! Work: what runs in the background, stopping, pausing and resuming it, and its log.

use std::sync::Arc;

use axum::{extract::State, http::header, response::IntoResponse};
use chess_core::{Error, WorkId, work::Work};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiError, ApiResult, Json, Path, Query};
use crate::App;

/// The queue as people see it.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct WorkLists {
    /// Running and paused, oldest first.
    pub running: Vec<Work>,
    /// In the order it will run.
    pub queued: Vec<Work>,
    /// Finished, newest first.
    pub recent: Vec<Work>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WorkQuery {
    /// How many finished items (default 20).
    pub recent: Option<u32>,
}

/// Work running, queued and recently finished.
#[utoipa::path(get, path = "/api/work", operation_id = "list_work", tag = "work", params(WorkQuery),
    responses((status = 200, body = WorkLists)))]
pub async fn list_work(State(app): State<Arc<App>>, Query(q): Query<WorkQuery>) -> ApiResult<Json<WorkLists>> {
    let l = app.scheduler.list(q.recent.unwrap_or(20)).await?;
    Ok(Json(WorkLists { running: l.running, queued: l.queued, recent: l.recent }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct WorkPath {
    pub id: i64,
}

/// One work item.
#[utoipa::path(get, path = "/api/work/{id}", operation_id = "get_work", tag = "work", params(WorkPath),
    responses((status = 200, body = Work), (status = 404, body = chess_core::error::ErrorBody)))]
pub async fn get_work(State(app): State<Arc<App>>, Path(p): Path<WorkPath>) -> ApiResult<Json<Work>> {
    Ok(Json(app.scheduler.get(WorkId(p.id)).await?))
}

/// Stops an item: a queued one at once, a running one at its next safe point.
#[utoipa::path(post, path = "/api/work/{id}/stop", operation_id = "stop_work", tag = "work", params(WorkPath),
    responses((status = 200, body = Work), (status = 404, body = chess_core::error::ErrorBody)))]
pub async fn stop_work(State(app): State<Arc<App>>, Path(p): Path<WorkPath>) -> ApiResult<Json<Work>> {
    Ok(Json(app.scheduler.stop(WorkId(p.id)).await?))
}

/// Pauses an item until it is resumed: a running one at its next safe point, a queued one is held.
#[utoipa::path(post, path = "/api/work/{id}/pause", operation_id = "pause_work", tag = "work", params(WorkPath),
    responses((status = 200, body = Work), (status = 404, body = chess_core::error::ErrorBody), (status = 409, body = chess_core::error::ErrorBody)))]
pub async fn pause_work(State(app): State<Arc<App>>, Path(p): Path<WorkPath>) -> ApiResult<Json<Work>> {
    Ok(Json(app.scheduler.pause(WorkId(p.id), chess_work::scheduler::PAUSED_BY_YOU).await?))
}

/// Carries on paused work.
#[utoipa::path(post, path = "/api/work/{id}/resume", operation_id = "resume_work", tag = "work", params(WorkPath),
    responses((status = 200, body = Work), (status = 404, body = chess_core::error::ErrorBody), (status = 409, body = chess_core::error::ErrorBody)))]
pub async fn resume_work(State(app): State<Arc<App>>, Path(p): Path<WorkPath>) -> ApiResult<Json<Work>> {
    Ok(Json(app.scheduler.resume(WorkId(p.id)).await?))
}

/// A work item's log, as plain text. Lines also come live as `work.log` events.
#[utoipa::path(get, path = "/api/work/{id}/log", operation_id = "work_log", tag = "work", params(WorkPath),
    responses((status = 200, content_type = "text/plain", body = String), (status = 404, body = chess_core::error::ErrorBody)))]
pub async fn work_log(State(app): State<Arc<App>>, Path(p): Path<WorkPath>) -> ApiResult<impl IntoResponse> {
    let id = WorkId(p.id);
    app.scheduler.get(id).await?;
    let text = match tokio::fs::read_to_string(app.hub.config.work_log_path(id)).await {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(ApiError(Error::internal(format!("read the log: {e}")))),
    };
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text))
}
