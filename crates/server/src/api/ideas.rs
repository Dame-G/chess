//! Ideas: listing, creating (one or a batch), their trials and tunes, stopping, resuming and promoting.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use dameless_core::{
    Error, IdeaId, TrialId,
    ideas::{Idea, IdeaStatus, Trial},
    spsa::{TuneSpec, TuneStep},
    version::Version,
    work::WorkStatus,
};
use dameless_store::IdeaFilter;
use dameless_work::{BatchRequest, NewIdeaRequest, VersionRef};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{
    error::{ApiError, ApiResult, Json, Path, Query, list},
    games::word,
};
use crate::App;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct IdeasQuery {
    /// States, comma-separated: `queued`, `running`, `kept`, `dropped`, `stopped`, `failed`.
    pub status: Option<String>,
    /// Ideas with this tag (a batch).
    pub tag: Option<String>,
    /// At most this many, newest first; all when none.
    pub limit: Option<u32>,
}

/// Ideas, newest first.
#[utoipa::path(get, path = "/api/ideas", operation_id = "list_ideas", tag = "ideas", params(IdeasQuery),
    responses((status = 200, body = Vec<Idea>)))]
pub async fn list_ideas(State(app): State<Arc<App>>, Query(q): Query<IdeasQuery>) -> ApiResult<Json<Vec<Idea>>> {
    let filter = IdeaFilter {
        statuses: list(q.status.as_deref(), "an idea status", word::<IdeaStatus>)?,
        tag: q.tag.filter(|t| !t.trim().is_empty()),
        limit: q.limit.unwrap_or(0),
    };
    Ok(Json(app.ideas.list(filter).await?))
}

/// Creates an idea and queues its first trial. A `custom` trial instead of the funnel is a single test (any preset,
/// bounds or a fixed number of pairs). A branch whose `Bench:` line is missing or stale is refused (409) unless
/// `fix_bench` asks for it to be fixed.
#[utoipa::path(post, path = "/api/ideas", operation_id = "create_idea", tag = "ideas", request_body = NewIdeaRequest,
    responses((status = 201, body = Idea), (status = 400, body = dameless_core::error::ErrorBody), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn create_idea(
    State(app): State<Arc<App>>,
    Json(req): Json<NewIdeaRequest>,
) -> ApiResult<(StatusCode, Json<Idea>)> {
    Ok((StatusCode::CREATED, Json(app.ideas.create(req).await?)))
}

/// Several ideas under one tag, all checked before any is queued; they run one after another.
#[utoipa::path(post, path = "/api/ideas/batch", operation_id = "create_batch", tag = "ideas", request_body = BatchRequest,
    responses((status = 201, body = Vec<Idea>), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn create_batch(
    State(app): State<Arc<App>>,
    Json(req): Json<BatchRequest>,
) -> ApiResult<(StatusCode, Json<Vec<Idea>>)> {
    Ok((StatusCode::CREATED, Json(app.ideas.batch(req).await?)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct IdeaPath {
    pub id: i64,
}

/// An idea with its trials.
#[utoipa::path(get, path = "/api/ideas/{id}", operation_id = "get_idea", tag = "ideas", params(IdeaPath),
    responses((status = 200, body = Idea), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_idea(State(app): State<Arc<App>>, Path(p): Path<IdeaPath>) -> ApiResult<Json<Idea>> {
    Ok(Json(app.ideas.get(IdeaId(p.id)).await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct TrialPath {
    pub id: i64,
    pub trial: i64,
}

/// One trial of an idea, with its evidence.
#[utoipa::path(get, path = "/api/ideas/{id}/trials/{trial}", operation_id = "get_trial", tag = "ideas", params(TrialPath),
    responses((status = 200, body = Trial), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_trial(State(app): State<Arc<App>>, Path(p): Path<TrialPath>) -> ApiResult<Json<Trial>> {
    Ok(Json(app.ideas.trial(IdeaId(p.id), TrialId(p.trial)).await?))
}

/// A tuned parameter: its name, and its range when the tuned branch's `spsa` build is known.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct TuneParamInfo {
    pub name: String,
    pub start: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// `GET /api/ideas/{id}/tune`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct TuneDetail {
    pub spec: TuneSpec,
    /// The parameters, in the order of each step's values.
    pub params: Vec<TuneParamInfo>,
    pub steps: Vec<TuneStep>,
}

/// A tune idea's settings, its parameters and its SPSA history (one step per update). 404 for an idea that is not a
/// tune.
#[utoipa::path(get, path = "/api/ideas/{id}/tune", operation_id = "get_tune", tag = "ideas", params(IdeaPath),
    responses((status = 200, body = TuneDetail), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_tune(State(app): State<Arc<App>>, Path(p): Path<IdeaPath>) -> ApiResult<Json<TuneDetail>> {
    let id = IdeaId(p.id);
    let view = app.ideas.tune(id).await?;
    let idea = app.ideas.get(id).await?;
    // The ranges come from the tuned branch's spsa build when it is there; nothing is built for this.
    let declared = match app.versions.resolve(&VersionRef::Name(idea.branch.clone())).await {
        Ok(v) => super::dameless::tunables_of(&app, &v, false).await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let selected = dameless_core::spsa::select(&declared, &view.spec.params).unwrap_or_default();
    let names: Vec<String> = if view.params.is_empty() {
        if view.spec.params.is_empty() {
            selected.iter().map(|t| t.name.clone()).collect()
        } else {
            view.spec.params.iter().map(|p| p.name.clone()).collect()
        }
    } else {
        view.params.clone()
    };
    let params = names
        .into_iter()
        .map(|name| {
            let t = selected.iter().find(|t| t.name == name);
            TuneParamInfo { start: t.map(|t| t.start), min: t.map(|t| t.min), max: t.map(|t| t.max), name }
        })
        .collect();
    Ok(Json(TuneDetail { spec: view.spec, params, steps: view.steps }))
}

/// Stops an idea: its running trial at the next safe point (keeping its rounds), or a queued one at once.
#[utoipa::path(post, path = "/api/ideas/{id}/stop", operation_id = "stop_idea", tag = "ideas", params(IdeaPath),
    responses((status = 200, body = Idea), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn stop_idea(State(app): State<Arc<App>>, Path(p): Path<IdeaPath>) -> ApiResult<Json<Idea>> {
    Ok(Json(app.ideas.stop(IdeaId(p.id)).await?))
}

/// Carries on a stopped or failed idea from the trial that did not finish.
#[utoipa::path(post, path = "/api/ideas/{id}/resume", operation_id = "resume_idea", tag = "ideas", params(IdeaPath),
    responses((status = 200, body = Idea), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn resume_idea(State(app): State<Arc<App>>, Path(p): Path<IdeaPath>) -> ApiResult<Json<Idea>> {
    Ok(Json(app.ideas.resume(IdeaId(p.id)).await?))
}

/// Promotes a kept idea: its branch is merged into the fork's `main` locally, making the next champion, which is the
/// answer. Refused (409, nothing changed) when the checkout has uncommitted changes. Pushing is separate.
#[utoipa::path(post, path = "/api/ideas/{id}/promote", operation_id = "promote_idea", tag = "ideas", params(IdeaPath),
    responses((status = 200, body = Version), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn promote_idea(State(app): State<Arc<App>>, Path(p): Path<IdeaPath>) -> ApiResult<Json<Version>> {
    let work = app.ideas.promote(IdeaId(p.id)).await?;
    let done = app.scheduler.wait(work.id).await?;
    match done.status {
        WorkStatus::Done => Ok(Json(app.versions.champion().await?)),
        WorkStatus::Stopped => Err(ApiError(Error::conflict("The promotion was stopped"))),
        _ => Err(ApiError(Error::conflict(done.error.unwrap_or_else(|| "The promotion failed; see its log".into())))),
    }
}
