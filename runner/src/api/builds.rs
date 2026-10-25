use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;

use super::{ApiError, AppState};
use crate::{
    engine::{
        build::{self, Build},
        git,
    },
    jobs::{self, Job, JobSpec},
};

/// Every build, newest first.
#[utoipa::path(get, path = "/api/builds", operation_id = "list_builds", tag = "builds", responses((status = 200, body = Vec<Build>)))]
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Build>>, ApiError> {
    Ok(Json(state.store.call(|c| build::list(c)).await?))
}

/// One build.
#[utoipa::path(get, path = "/api/builds/{id}", operation_id = "get_build", tag = "builds", responses((status = 200, body = Build), (status = 404)))]
pub async fn get(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Build>, ApiError> {
    state.store.call(move |c| build::get(c, id)).await?.map(Json).ok_or(ApiError::not_found("no such build"))
}

#[derive(Deserialize, ToSchema)]
pub struct BuildRequest {
    /// A branch, tag or commit of the Reckless checkout, such as `main` or `upstream/main`.
    #[serde(rename = "ref")]
    pub refname: String,
}

/// Queues a build. A commit that is already built finishes at once with the existing build.
#[utoipa::path(post, path = "/api/builds", operation_id = "create_build", tag = "builds", request_body = BuildRequest, responses((status = 202, body = Job)))]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BuildRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let job = jobs::submit(&state, JobSpec::Build { refname: req.refname }).await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// The Reckless checkout's branches and upstream's, most recently committed first: what can be built or tested.
#[utoipa::path(get, path = "/api/branches", operation_id = "list_branches", tag = "builds", responses((status = 200, body = Vec<git::Branch>)))]
pub async fn branches(State(state): State<Arc<AppState>>) -> Result<Json<Vec<git::Branch>>, ApiError> {
    Ok(Json(git::branches(&state.config.reckless).await?))
}
