use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{ApiError, AppState};
use crate::matches::{self, Action, Match, MatchRequest, MatchSummary, Reply, Source};

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

/// Stops the running match, which is recorded as aborted, and returns it. 409 if it is not running. In a game you play
/// it is a resignation once each side has moved. A Lichess game is
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

#[derive(Deserialize, ToSchema)]
pub struct MoveRequest {
    /// The move in UCI notation, such as `e2e4` or `e7e8q`.
    uci: String,
}

#[derive(Serialize, ToSchema)]
pub struct DrawAnswer {
    /// Whether the engine took the draw. If it did, the game is over.
    accepted: bool,
}

/// Plays your move in a match where you hold a seat (`"human"`). Your clock is charged like an engine's, from the start
/// of your turn to the arrival of the move. The answer is the match with the move on the board. 400 when the move is
/// not legal or the match has no human player, 409 when it is not your turn or the match is over.
#[utoipa::path(post, path = "/api/matches/{id}/moves", operation_id = "play_move", tag = "matches", request_body = MoveRequest, responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn play_move(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<Match>, ApiError> {
    matches::act(&state, id, Action::Move(req.uci)).await?;
    matches::get(&state, id).await?.map(Json).ok_or(ApiError::not_found("no such match"))
}

/// Takes back your last move and the engine's reply to it, on your turn, in a game without a clock. A `match.takeback`
/// event carries the number of plies left. 409 in a timed game, when there is nothing to take back or it is not your
/// turn.
#[utoipa::path(post, path = "/api/matches/{id}/takeback", operation_id = "takeback_move", tag = "matches", responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn takeback(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    matches::act(&state, id, Action::Takeback).await?;
    matches::get(&state, id).await?.map(Json).ok_or(ApiError::not_found("no such match"))
}

/// Offers the engine a draw, on your turn. It accepts once twenty plies are played and its own last score is no more
/// than +0.10; the game then ends as a draw by agreement. Otherwise it declines and the game goes on.
#[utoipa::path(post, path = "/api/matches/{id}/draw", operation_id = "offer_draw", tag = "matches", responses((status = 200, body = DrawAnswer), (status = 400), (status = 404), (status = 409)))]
pub async fn offer_draw(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<DrawAnswer>, ApiError> {
    match matches::act(&state, id, Action::Draw).await? {
        Reply::Draw { accepted } => Ok(Json(DrawAnswer { accepted })),
        _ => Err(ApiError::conflict("the game moved on")),
    }
}
