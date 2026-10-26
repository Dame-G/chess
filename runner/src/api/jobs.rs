use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use utoipa::IntoParams;

use super::{ApiError, AppState};
use crate::jobs::{self, Job};

#[derive(Deserialize, IntoParams)]
pub struct ListQuery {
    /// How many jobs, newest first. Default 50.
    limit: Option<i64>,
}

/// Recent jobs, newest first.
#[utoipa::path(get, path = "/api/jobs", operation_id = "list_jobs", tag = "jobs", params(ListQuery), responses((status = 200, body = Vec<Job>)))]
pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Job>>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 1000);
    Ok(Json(state.store.call(move |c| jobs::list(c, limit)).await?))
}

/// One job.
#[utoipa::path(get, path = "/api/jobs/{id}", operation_id = "get_job", tag = "jobs", responses((status = 200, body = Job), (status = 404)))]
pub async fn get(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Job>, ApiError> {
    state.store.call(move |c| jobs::get(c, id)).await?.map(Json).ok_or(ApiError::not_found("no such job"))
}

/// A job's output so far, as plain text.
#[utoipa::path(get, path = "/api/jobs/{id}/log", operation_id = "get_job_log", tag = "jobs", responses((status = 200, content_type = "text/plain", body = String), (status = 404)))]
pub async fn log(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<String, ApiError> {
    tokio::fs::read_to_string(jobs::log_path(&state, id)).await.map_err(|_| ApiError::not_found("no log for this job"))
}

/// Cancels a queued job or stops a running one.
#[utoipa::path(delete, path = "/api/jobs/{id}", operation_id = "cancel_job", tag = "jobs", responses((status = 200, body = Job), (status = 404)))]
pub async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Job>, ApiError> {
    jobs::cancel(&state, id).await?.map(Json).ok_or(ApiError::not_found("no such job"))
}
