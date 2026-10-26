use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use utoipa::IntoParams;

use super::{ApiError, AppState};
use crate::{
    engine::build::Build,
    versions::{self, DeleteVersion, DeletedVersion, Hidden, PruneRequest, PruneResult, Version},
};

/// One entry per branch (the fork's and upstream's, as `/api/branches`), newest commit first: its builds, the
/// verdict of its latest experiment, its latest tune and whether the Lichess bot plays it. Hidden ones are included
/// with `hidden: true`. A branch name with a `/` goes in a path as `%2F`.
#[utoipa::path(get, path = "/api/versions", operation_id = "list_versions", tag = "versions", responses((status = 200, body = Vec<Version>)))]
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Version>>, ApiError> {
    Ok(Json(versions::list(&state).await?))
}

/// Hides a version from the Versions view, or shows it again. It can still be built and tested.
#[utoipa::path(put, path = "/api/versions/{name}/hidden", operation_id = "set_version_hidden", tag = "versions", request_body = Hidden, responses((status = 200, body = Hidden), (status = 404)))]
pub async fn hide(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<Hidden>,
) -> Result<Json<Hidden>, ApiError> {
    Ok(Json(versions::set_hidden(&state, name, req.hidden).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct DeleteQuery {
    /// Delete the branch on `origin` too.
    #[serde(default)]
    remote: bool,
    /// Delete its builds' binaries too (those no other branch shares); their records stay.
    #[serde(default)]
    builds: bool,
}

/// Deletes a fork branch, locally and with `remote` on `origin`. The body must repeat its name. 400 for `main`, an
/// upstream branch, a wrong `confirm`, or a branch a queued or running experiment or tune uses; 409 when the Lichess
/// bot plays it, it is checked out, or (with `builds`) one of its builds is in use. Experiments and tunes that used it
/// keep their records.
#[utoipa::path(delete, path = "/api/versions/{name}", operation_id = "delete_version", tag = "versions", params(DeleteQuery), request_body = DeleteVersion, responses((status = 200, body = DeletedVersion), (status = 400), (status = 404), (status = 409)))]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DeleteQuery>,
    Json(req): Json<DeleteVersion>,
) -> Result<Json<DeletedVersion>, ApiError> {
    Ok(Json(versions::delete_version(&state, &name, &req, q.remote, q.builds).await?))
}

/// Deletes a build's binary; the build stays on record (with `deleted_at`) so everything that used it still reads,
/// and a later build of its commit builds afresh. 409 while a running experiment, test, tune or match uses it, or
/// it is the Lichess bot's build.
#[utoipa::path(delete, path = "/api/builds/{id}", operation_id = "delete_build", tag = "builds", responses((status = 200, body = Build), (status = 404), (status = 409)))]
pub async fn delete_build(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Build>, ApiError> {
    Ok(Json(versions::delete_build(&state, id).await?))
}

/// Deletes the binaries of builds nothing uses: those older than `older_than_days`, keeping each branch's newest
/// plain and `spsa` build unless told otherwise. `dry_run` only says what would go.
#[utoipa::path(post, path = "/api/builds/prune", operation_id = "prune_builds", tag = "builds", request_body = PruneRequest, responses((status = 200, body = PruneResult)))]
pub async fn prune(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PruneRequest>,
) -> Result<Json<PruneResult>, ApiError> {
    Ok(Json(versions::prune(&state, req).await?))
}
