use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use super::{ApiError, AppState};
use crate::matches::{self, Match, MatchRequest, MatchSummary, Source};

#[derive(Deserialize, IntoParams)]
pub struct ListQuery {
    /// How many matches, newest first. Default 50.
    limit: Option<u32>,
    /// Only local matches (`local`) or only the bot's Lichess games (`lichess`). Both when absent.
    #[param(inline)]
    source: Option<Source>,
}

/// Recent matches, newest first, without their moves: local ones and the bot's Lichess games alike, unless `source`
/// picks one kind.
#[utoipa::path(get, path = "/api/matches", operation_id = "list_matches", tag = "matches", params(ListQuery), responses((status = 200, body = Vec<MatchSummary>)))]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<MatchSummary>>, ApiError> {
    Ok(Json(matches::list(&state, q.limit.unwrap_or(50).clamp(1, 1000), q.source).await?))
}

/// Starts a match between two builds and returns it. The moves arrive as `match.move` events, the thinking engine's
/// progress as `match.info`, and the end as `match.ended`. One local match runs at a time: 409 while another is
/// running. The bot's Lichess games run beside it and do not count.
#[utoipa::path(post, path = "/api/matches", operation_id = "create_match", tag = "matches", request_body = MatchRequest, responses((status = 201, body = Match), (status = 400), (status = 409)))]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MatchRequest>,
) -> Result<(StatusCode, Json<Match>), ApiError> {
    Ok((StatusCode::CREATED, Json(matches::start(&state, req).await?)))
}

/// A match with its headers, every move (evaluation, depth, nodes, time and the clocks after it), the clocks now, the
/// side to move, its status and, once over, the result and reason.
#[utoipa::path(get, path = "/api/matches/{id}", operation_id = "get_match", tag = "matches", responses((status = 200, body = Match), (status = 404)))]
pub async fn get(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    matches::get(&state, id).await?.map(Json).ok_or(ApiError::not_found("no such match"))
}

/// Stops the running match, which is recorded as aborted, and returns it. 409 if it is not running. A Lichess game is
/// resigned instead (aborted, if it has not got going), and the answer is the game as it stands: it ends, and
/// `match.ended` follows, once Lichess confirms.
#[utoipa::path(delete, path = "/api/matches/{id}", operation_id = "stop_match", tag = "matches", responses((status = 200, body = Match), (status = 404), (status = 409)))]
pub async fn stop(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    Ok(Json(matches::stop(&state, id).await?))
}

/// The match as PGN, with a comment on every move (score/depth, time, nodes, clock) like the test PGNs.
#[utoipa::path(get, path = "/api/matches/{id}/pgn", operation_id = "get_match_pgn", tag = "matches", responses((status = 200, content_type = "application/x-chess-pgn", body = String), (status = 404)))]
pub async fn pgn(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<([(axum::http::header::HeaderName, &'static str); 1], String), ApiError> {
    let m = matches::get(&state, id).await?.ok_or(ApiError::not_found("no such match"))?;
    Ok(([(axum::http::header::CONTENT_TYPE, "application/x-chess-pgn; charset=utf-8")], matches::to_pgn(&m)))
}
