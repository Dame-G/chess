use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{ApiError, AppState};
use crate::{
    engine::build,
    jobs::{self, Job, JobSpec},
    tune::{self, ApplyTune, NewTune, Param, Tune, TuneIteration},
};

#[derive(Deserialize, IntoParams)]
pub struct ListQuery {
    /// How many, newest first. Default 100.
    limit: Option<i64>,
}

/// SPSA tunes, newest first.
#[utoipa::path(get, path = "/api/tunes", operation_id = "list_tunes", tag = "tunes", params(ListQuery), responses((status = 200, body = Vec<Tune>)))]
pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Tune>>, ApiError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    Ok(Json(state.store.call(move |c| tune::list(c, limit)).await?))
}

/// Queues an SPSA tune of a build's tunables (every declared one unless `params` names some). It is a job: it runs
/// when the queue reaches it, one at a time with tests.
#[utoipa::path(post, path = "/api/tunes", operation_id = "create_tune", tag = "tunes", request_body = NewTune, responses((status = 202, body = Tune), (status = 400)))]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(new): Json<NewTune>,
) -> Result<(StatusCode, Json<Tune>), ApiError> {
    if new.iterations == Some(0) || new.pairs_per_iteration == Some(0) {
        return Err(ApiError::bad_request("iterations and pairs_per_iteration must be at least 1"));
    }
    Ok((StatusCode::ACCEPTED, Json(tune::submit(&state, new).await?)))
}

#[derive(Serialize, ToSchema)]
pub struct TuneDetail {
    pub tune: Tune,
    /// Every finished iteration, in order: the parameters after it (in the order of `tune.params`) and its result.
    pub history: Vec<TuneIteration>,
}

/// One tune with its history.
#[utoipa::path(get, path = "/api/tunes/{id}", operation_id = "get_tune", tag = "tunes", responses((status = 200, body = TuneDetail), (status = 404)))]
pub async fn get(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<TuneDetail>, ApiError> {
    let detail = state
        .store
        .call(move |c| {
            let Some(tune) = tune::get(c, id)? else { return Ok(None) };
            Ok(Some(TuneDetail { tune, history: tune::history(c, id)? }))
        })
        .await?;
    detail.map(Json).ok_or(ApiError::not_found("no such tune"))
}

/// Cancels a queued tune or stops a running one. Its iterations so far are kept, and it can resume.
#[utoipa::path(delete, path = "/api/tunes/{id}", operation_id = "cancel_tune", tag = "tunes", responses((status = 200, body = Tune), (status = 404)))]
pub async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Tune>, ApiError> {
    if state.store.call(move |c| tune::get(c, id)).await?.is_none() {
        return Err(ApiError::not_found("no such tune"));
    }
    Ok(Json(tune::cancel(&state, id).await?))
}

/// Queues a stopped (failed or cancelled) tune again; it carries on from its last iteration.
#[utoipa::path(post, path = "/api/tunes/{id}/resume", operation_id = "resume_tune", tag = "tunes", responses((status = 202, body = Tune), (status = 404), (status = 409)))]
pub async fn resume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<Tune>), ApiError> {
    let Some(t) = state.store.call(move |c| tune::get(c, id)).await? else {
        return Err(ApiError::not_found("no such tune"));
    };
    if !matches!(t.status, tune::TuneStatus::Failed | tune::TuneStatus::Cancelled) {
        return Err(ApiError::conflict("only a failed or cancelled tune can resume"));
    }
    Ok((StatusCode::ACCEPTED, Json(tune::resume(&state, id).await?)))
}

/// Queues a job that writes the tune's current values into the engine as its new defaults, on a new branch of the
/// fork with the right `Bench:` line, and (unless `experiment` is false) queues a gainer experiment of that branch
/// against `base` at SPRT [0, 4]. The job's result is `{ tune, branch, commit, bench, experiment }`.
#[utoipa::path(post, path = "/api/tunes/{id}/apply", operation_id = "apply_tune", tag = "tunes", request_body = ApplyTune, responses((status = 202, body = Job), (status = 404), (status = 409)))]
pub async fn apply(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(options): Json<ApplyTune>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let Some(t) = state.store.call(move |c| tune::get(c, id)).await? else {
        return Err(ApiError::not_found("no such tune"));
    };
    if t.build.is_none() || t.params.is_empty() {
        return Err(ApiError::conflict("the tune has not started: there is nothing to apply yet"));
    }
    Ok((StatusCode::ACCEPTED, Json(jobs::submit(&state, JobSpec::ApplyTune { id, options }).await?)))
}

/// The tunables a build declares, as a tune would read them. The build must be an `spsa` build (made by a tune).
#[utoipa::path(get, path = "/api/builds/{id}/tunables", operation_id = "list_tunables", tag = "tunes", responses((status = 200, body = Vec<Param>), (status = 404), (status = 409)))]
pub async fn declared(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Vec<Param>>, ApiError> {
    let Some(b) = state.store.call(move |c| build::get(c, id)).await? else {
        return Err(ApiError::not_found("no such build"));
    };
    if b.deleted_at.is_some() {
        return Err(ApiError::conflict("the build was deleted: its binary is gone"));
    }
    if !build::is_spsa(&b) {
        return Err(ApiError::conflict("not an spsa build: its tunables are compiled in as constants"));
    }
    Ok(Json(tune::declared(&build::binary(&state.config.data, &b)).await?))
}
