//! Your own Lichess account: logging in, finding games, your history, and the explorer and tablebase. Games you play
//! are matches, played through the match routes (`/api/matches/{id}/moves`, `/draw`, `/takeback`, `DELETE`), plus the
//! few actions only a Lichess game has, here.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
};
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::{ApiError, AppState};
use crate::{
    jobs::{self, Job, JobSpec},
    lichess::{
        ChallengeSent,
        me::{
            self, MeChallengeRequest, MeSeekRequest, MeStatus,
            explorer::{ExplorerQuery, TablebaseQuery},
            import::ImportRequest,
            oauth::{LoginRequest, LoginStarted},
        },
    },
    matches::{self, Action, Match},
};

/// Your account: whether you are logged in and as whom (with your ratings), the token's scopes, the connection, the
/// games you are playing (matches here, and games that can only be played on lichess.org), open challenges both ways,
/// your seek, and `fair_play_lock`, true while a game runs (analysis, explorer and tablebase are then refused).
#[utoipa::path(get, path = "/api/lichess/me", operation_id = "get_lichess_me", tag = "lichess-me", responses((status = 200, body = MeStatus)))]
pub async fn status(State(state): State<Arc<AppState>>) -> Json<MeStatus> {
    Json(state.me.status())
}

/// Starts logging in: the answer's `url` is the Lichess page to open, where you allow access (scopes `board:play`,
/// `challenge:read`, `challenge:write`). Lichess then sends the browser to `/api/lichess/me/callback`, which stores the
/// token and sends the browser on to `return_to` with `?lichess=connected` (or `?lichess=error&message=...`). The link
/// works for ten minutes, once.
#[utoipa::path(post, path = "/api/lichess/me/login", operation_id = "start_lichess_me_login", tag = "lichess-me", request_body = LoginRequest, responses((status = 200, body = LoginStarted), (status = 400)))]
pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginStarted>, ApiError> {
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    Ok(Json(me::oauth::start(&state, host, req)?))
}

#[derive(Deserialize, ToSchema)]
pub struct MeTokenRequest {
    /// A personal Lichess token of your own (not a BOT) account, with `board:play`, `challenge:read` and
    /// `challenge:write`, made at https://lichess.org/account/oauth/token.
    pub token: String,
}

/// Logs in with a personal token instead of the login page. Lichess checks it: 400 if it refuses it, if it cannot
/// play (`board:play`), or if it is a BOT account or the bot's own.
#[utoipa::path(put, path = "/api/lichess/me/token", operation_id = "set_lichess_me_token", tag = "lichess-me", request_body = MeTokenRequest, responses((status = 200, body = MeStatus), (status = 400), (status = 502)))]
pub async fn set_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MeTokenRequest>,
) -> Result<Json<MeStatus>, ApiError> {
    Ok(Json(me::set_token(&state, &req.token).await?))
}

/// Logs out: revokes the token at Lichess and forgets it. Games in progress go on at Lichess, unfollowed.
#[utoipa::path(delete, path = "/api/lichess/me", operation_id = "logout_lichess_me", tag = "lichess-me", responses((status = 200, body = MeStatus)))]
pub async fn logout(State(state): State<Arc<AppState>>) -> Result<Json<MeStatus>, ApiError> {
    Ok(Json(me::logout(&state).await?))
}

/// Opens a seek in the lobby, open until someone takes it (the game then starts, as a match in `playing`) or it is
/// cancelled. The Board API seeks only rapid and slower: initial + 40 × increment at least 8 minutes, else 400 (for
/// blitz, challenge someone). 409 while not connected or while a seek is open.
#[utoipa::path(post, path = "/api/lichess/me/seek", operation_id = "start_lichess_me_seek", tag = "lichess-me", request_body = MeSeekRequest, responses((status = 200, body = MeStatus), (status = 400), (status = 409)))]
pub async fn seek(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MeSeekRequest>,
) -> Result<Json<MeStatus>, ApiError> {
    Ok(Json(me::seek(&state, req).await?))
}

/// Cancels your seek.
#[utoipa::path(delete, path = "/api/lichess/me/seek", operation_id = "stop_lichess_me_seek", tag = "lichess-me", responses((status = 200, body = MeStatus)))]
pub async fn stop_seek(State(state): State<Arc<AppState>>) -> Json<MeStatus> {
    Json(me::stop_seek(&state).await)
}

/// Challenges a player (`username`), your own bot (`my_bot`) or Lichess's AI (`ai_level` 1-8, casual, starts at
/// once). Blitz or slower (initial + 40 × increment at least 3 minutes), as the Board API plays; 400 otherwise.
#[utoipa::path(post, path = "/api/lichess/me/challenge", operation_id = "create_lichess_me_challenge", tag = "lichess-me", request_body = MeChallengeRequest, responses((status = 201, body = ChallengeSent), (status = 400), (status = 409)))]
pub async fn challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MeChallengeRequest>,
) -> Result<(StatusCode, Json<ChallengeSent>), ApiError> {
    Ok((StatusCode::CREATED, Json(me::challenge(&state, req).await?)))
}

/// Accepts a challenge to you; the game starts as a match in `playing`. 400 for one the Board API cannot play
/// (bullet, variants): accept that on lichess.org.
#[utoipa::path(post, path = "/api/lichess/me/challenges/{id}/accept", operation_id = "accept_lichess_me_challenge", tag = "lichess-me", responses((status = 200, body = MeStatus), (status = 400), (status = 409)))]
pub async fn accept(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<MeStatus>, ApiError> {
    Ok(Json(me::accept(&state, &id).await?))
}

#[derive(Default, Deserialize, ToSchema)]
pub struct DeclineRequest {
    /// One of Lichess's reasons: `generic` (default), `later`, `tooFast`, `tooSlow`, `timeControl`, `rated`, `casual`,
    /// `standard`, `variant`, `noBot`, `onlyBot`.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Declines a challenge to you.
#[utoipa::path(post, path = "/api/lichess/me/challenges/{id}/decline", operation_id = "decline_lichess_me_challenge", tag = "lichess-me", request_body = DeclineRequest, responses((status = 200, body = MeStatus), (status = 400), (status = 409)))]
pub async fn decline(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DeclineRequest>,
) -> Result<Json<MeStatus>, ApiError> {
    Ok(Json(me::decline(&state, &id, req.reason).await?))
}

/// Withdraws a challenge of yours.
#[utoipa::path(delete, path = "/api/lichess/me/challenges/{id}", operation_id = "cancel_lichess_me_challenge", tag = "lichess-me", responses((status = 200, body = MeStatus), (status = 400), (status = 409)))]
pub async fn cancel_challenge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<MeStatus>, ApiError> {
    Ok(Json(me::cancel_challenge(&state, &id).await?))
}

/// Imports your finished Lichess games as matches (`GET /api/matches?account=me`), with clocks, the opening and
/// Lichess's evaluations where a game was analysed. A job that starts at once, beside the queue; its result is
/// `{ imported, skipped, other, newest }`. Each game is stored once; without `since`, it carries on after the newest
/// game imported before. 409 when not logged in.
#[utoipa::path(post, path = "/api/lichess/me/import", operation_id = "import_lichess_me_games", tag = "lichess-me", request_body = ImportRequest, responses((status = 202, body = Job), (status = 409)))]
pub async fn import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    if !state.me.status().configured {
        return Err(ApiError::conflict("your Lichess account is not connected: log in first"));
    }
    Ok((StatusCode::ACCEPTED, Json(jobs::run_now(&state, JobSpec::LichessImport { options: req }).await?)))
}

/// The opening explorer at a position: Lichess games (`db=lichess`, by speed and rating), masters' games
/// (`db=masters`) or one player's (`db=player`, with `player` and `color`). Lichess's own answer, cached for ten
/// minutes. It needs your login (409 without one), and is refused (409) while you play a game other than
/// correspondence.
#[utoipa::path(get, path = "/api/lichess/explorer", operation_id = "lichess_explorer", tag = "lichess-me", params(ExplorerQuery), responses((status = 200, body = Object), (status = 400), (status = 409), (status = 429), (status = 502)))]
pub async fn explorer(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ExplorerQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(me::explorer::explorer(&state, q).await?))
}

/// The tablebase at a position of at most seven pieces: Lichess's own answer (`category`, `dtz`, `dtm`, the moves
/// ranked). Refused (409) while you play.
#[utoipa::path(get, path = "/api/lichess/tablebase", operation_id = "lichess_tablebase", tag = "lichess-me", params(TablebaseQuery), responses((status = 200, body = Object), (status = 400), (status = 409), (status = 429), (status = 502)))]
pub async fn tablebase(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TablebaseQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(me::explorer::tablebase(&state, q).await?))
}

async fn act(state: &AppState, id: i64, action: Action) -> Result<Json<Match>, ApiError> {
    matches::act(state, id, action).await?;
    matches::get(state, id).await?.map(Json).ok_or(ApiError::not_found("no such match"))
}

/// Your Lichess game: aborts it, which Lichess allows before both sides have moved.
#[utoipa::path(post, path = "/api/matches/{id}/abort", operation_id = "abort_match", tag = "matches", responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn abort(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    act(&state, id, Action::Abort).await
}

/// Your Lichess game: claims the win once the opponent has been gone long enough (`match.opponent_gone` says when).
#[utoipa::path(post, path = "/api/matches/{id}/claim-victory", operation_id = "claim_match_victory", tag = "matches", responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn claim_victory(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    act(&state, id, Action::ClaimVictory).await
}

/// Your Lichess game: claims a draw once the opponent has been gone long enough.
#[utoipa::path(post, path = "/api/matches/{id}/claim-draw", operation_id = "claim_match_draw", tag = "matches", responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn claim_draw(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    act(&state, id, Action::ClaimDraw).await
}

/// Your Lichess game: declines the opponent's draw offer. 409 when there is none.
#[utoipa::path(post, path = "/api/matches/{id}/decline-draw", operation_id = "decline_match_draw", tag = "matches", responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn decline_draw(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Match>, ApiError> {
    act(&state, id, Action::DeclineDraw).await
}

/// Your Lichess game: declines the opponent's takeback proposal. 409 when there is none.
#[utoipa::path(post, path = "/api/matches/{id}/decline-takeback", operation_id = "decline_match_takeback", tag = "matches", responses((status = 200, body = Match), (status = 400), (status = 404), (status = 409)))]
pub async fn decline_takeback(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Match>, ApiError> {
    act(&state, id, Action::DeclineTakeback).await
}

/// The OAuth callback, outside the API's guard and the OpenAPI document: a browser arriving from lichess.org is
/// cross-site and has no token, and the single-use `state` of the login is what makes it safe.
pub fn callback_router() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route("/api/lichess/me/callback", axum::routing::get(me::oauth::callback))
}
