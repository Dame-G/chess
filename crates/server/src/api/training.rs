//! Training: nets, datasets (self-play data, downloads, trial games) and training runs on the GPU.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use dameless_core::{
    IdeaId, WorkId,
    training::{Dataset, DatasetId, Net, NetId, TrainSpec, TrainStep},
    work::Work,
};
use dameless_work::{DataStatus, DatagenRequest, ImportRequest, TrainView};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiResult, Json, Path};
use crate::App;

/// Every net the app knows, newest first.
#[utoipa::path(get, path = "/api/nets", operation_id = "list_nets", tag = "training",
    responses((status = 200, body = Vec<Net>)))]
pub async fn list_nets(State(app): State<Arc<App>>) -> ApiResult<Json<Vec<Net>>> {
    Ok(Json(app.nets.list().await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct IdPath {
    pub id: i64,
}

/// One net.
#[utoipa::path(get, path = "/api/nets/{id}", operation_id = "get_net", tag = "training", params(IdPath),
    responses((status = 200, body = Net), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_net(State(app): State<Arc<App>>, Path(p): Path<IdPath>) -> ApiResult<Json<Net>> {
    Ok(Json(app.nets.get(NetId(p.id)).await?))
}

/// Every dataset, the cap on them together and the room left.
#[utoipa::path(get, path = "/api/datasets", operation_id = "list_datasets", tag = "training",
    responses((status = 200, body = DataStatus)))]
pub async fn list_datasets(State(app): State<Arc<App>>) -> ApiResult<Json<DataStatus>> {
    Ok(Json(app.datasets.status().await?))
}

/// Self-play data with Reckless's datagen branch (work). The same name again grows that dataset. 409 when the cap or
/// the disk leaves no room.
#[utoipa::path(post, path = "/api/datasets/datagen", operation_id = "datagen", tag = "training", request_body = DatagenRequest,
    responses((status = 201, body = Work), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn datagen(
    State(app): State<Arc<App>>,
    Json(b): Json<DatagenRequest>,
) -> ApiResult<(StatusCode, Json<Work>)> {
    Ok((StatusCode::CREATED, Json(app.datasets.datagen(b).await?)))
}

/// Downloads one shard into a dataset (work). Refused (400) without `confirm: true` for the size shown; 409 when the
/// download and its conversion would pass the cap or fill the disk.
#[utoipa::path(post, path = "/api/datasets/import", operation_id = "import_dataset", tag = "training", request_body = ImportRequest,
    responses((status = 201, body = Work), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn import_dataset(
    State(app): State<Arc<App>>,
    Json(b): Json<ImportRequest>,
) -> ApiResult<(StatusCode, Json<Work>)> {
    Ok((StatusCode::CREATED, Json(app.datasets.import(b).await?)))
}

/// `DELETE /api/datasets/{id}`.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct ConfirmBody {
    /// Your explicit go-ahead: deleting a dataset cannot be undone.
    #[serde(default)]
    pub confirm: bool,
}

/// Deletes a dataset's files. 400 without `confirm: true`; 409 while something uses it.
#[utoipa::path(delete, path = "/api/datasets/{id}", operation_id = "delete_dataset", tag = "training", params(IdPath),
    request_body = ConfirmBody,
    responses((status = 200, body = Dataset), (status = 400, body = dameless_core::error::ErrorBody), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn delete_dataset(
    State(app): State<Arc<App>>,
    Path(p): Path<IdPath>,
    body: axum::body::Bytes,
) -> ApiResult<Json<Dataset>> {
    let b: ConfirmBody = super::optional_body(&body)?;
    Ok(Json(app.datasets.delete(DatasetId(p.id), b.confirm).await?))
}

/// `POST /api/ideas/{id}/dataset`.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct FromTrialsBody {
    /// `trials-<id>` when left out.
    #[serde(default)]
    pub name: Option<String>,
}

/// A dataset made from an idea's trial games. 404 without trial games.
#[utoipa::path(post, path = "/api/ideas/{id}/dataset", operation_id = "dataset_from_trials", tag = "training",
    params(IdPath), request_body(content = Option<FromTrialsBody>),
    responses((status = 201, body = Dataset), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn dataset_from_trials(
    State(app): State<Arc<App>>,
    Path(p): Path<IdPath>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<Dataset>)> {
    let b: FromTrialsBody = super::optional_body(&body)?;
    Ok((StatusCode::CREATED, Json(app.datasets.from_trials(IdeaId(p.id), b.name).await?)))
}

/// A net idea's training: its settings, every reading (resumes included) and the net it made. 404 for another kind.
#[utoipa::path(get, path = "/api/ideas/{id}/train", operation_id = "get_idea_train", tag = "training", params(IdPath),
    responses((status = 200, body = TrainView), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_idea_train(State(app): State<Arc<App>>, Path(p): Path<IdPath>) -> ApiResult<Json<TrainView>> {
    Ok(Json(app.training.idea(IdeaId(p.id)).await?))
}

/// A training run outside any idea (work on the GPU). 409 without a GPU; 404 for an unknown parent net or dataset.
#[utoipa::path(post, path = "/api/train", operation_id = "train", tag = "training", request_body = TrainSpec,
    responses((status = 201, body = Work), (status = 400, body = dameless_core::error::ErrorBody), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn train(State(app): State<Arc<App>>, Json(b): Json<TrainSpec>) -> ApiResult<(StatusCode, Json<Work>)> {
    Ok((StatusCode::CREATED, Json(app.training.submit(b).await?)))
}

/// One run's readings.
#[utoipa::path(get, path = "/api/work/{id}/train", operation_id = "work_train", tag = "training", params(IdPath),
    responses((status = 200, body = Vec<TrainStep>)))]
pub async fn work_train(State(app): State<Arc<App>>, Path(p): Path<IdPath>) -> ApiResult<Json<Vec<TrainStep>>> {
    Ok(Json(app.training.steps(WorkId(p.id)).await?))
}
