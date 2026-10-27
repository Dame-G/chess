//! Trusting the harness: the false-accept check, the `Bench:` line checker and fixer, and the machine's diagnostics.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use dameless_core::{harness::HarnessStatus, ideas::Idea};
use dameless_work::{BranchCheck, FixBenchRequest, HarnessCheckRequest, Health};
use serde::Deserialize;
use utoipa::IntoParams;

use super::error::{ApiResult, Json, Query};
use crate::App;

/// Starts a harness check: `main` against a branch identical to it at VSTC [0, 3] (which must reject it), and with
/// `pairs`, a symmetry run of that many pairs. Answers its idea. 409 while one runs or without fastchess and the book.
#[utoipa::path(post, path = "/api/harness/check", operation_id = "check_harness", tag = "harness",
    request_body(content = Option<HarnessCheckRequest>),
    responses((status = 201, body = Idea), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn check_harness(
    State(app): State<Arc<App>>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<Idea>)> {
    let req: HarnessCheckRequest = super::optional_body(&body)?;
    Ok((StatusCode::CREATED, Json(app.harness.start_check(req).await?)))
}

/// Whether this machine's harness is trusted: the last check, whether it still applies, and one running.
#[utoipa::path(get, path = "/api/harness", operation_id = "harness_status", tag = "harness",
    responses((status = 200, body = HarnessStatus)))]
pub async fn harness_status(State(app): State<Arc<App>>) -> ApiResult<Json<HarnessStatus>> {
    Ok(Json(app.harness.status().await?))
}

/// The machine's health for Settings: fastchess, the book, the engine checkout, clang, CUDA, the GPU, the disk, the
/// database, builds and the harness check, each with what to do about it.
#[utoipa::path(get, path = "/api/diagnostics", operation_id = "diagnostics", tag = "harness",
    responses((status = 200, body = Health)))]
pub async fn diagnostics(State(app): State<Arc<App>>) -> ApiResult<Json<Health>> {
    Ok(Json(app.diagnostics.health().await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BranchQuery {
    pub branch: String,
}

/// Whether a branch's tip has a right `Bench:` line (ok, missing, stale, or not built yet to tell). Never builds.
#[utoipa::path(get, path = "/api/ideas/check-branch", operation_id = "check_branch", tag = "ideas", params(BranchQuery),
    responses((status = 200, body = BranchCheck), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn check_branch(State(app): State<Arc<App>>, Query(q): Query<BranchQuery>) -> ApiResult<Json<BranchCheck>> {
    Ok(Json(app.ideas.check_branch(&q.branch).await?))
}

/// Builds a branch's tip if needed and rewords it with the measured `Bench:` line (locally, never pushed). 409 when
/// the branch moved since `commit`; 400 for `main` or upstream.
#[utoipa::path(post, path = "/api/ideas/fix-bench", operation_id = "fix_bench", tag = "ideas", request_body = FixBenchRequest,
    responses((status = 200, body = BranchCheck), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn fix_bench(State(app): State<Arc<App>>, Json(b): Json<FixBenchRequest>) -> ApiResult<Json<BranchCheck>> {
    Ok(Json(app.ideas.fix_bench_line(&b.branch, b.commit.as_deref()).await?))
}
