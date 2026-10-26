use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use utoipa::ToSchema;

use super::{ApiError, AppState};
use crate::lichess::{
    self, ChallengeRequest, ChallengeSent, LichessStatus, OnlineBot, rules::Settings, seek::SeekMode,
};

/// The bot: whether a token is stored and whose it is, whether it is online and how its connection stands, the
/// matches of the games it is playing, and its settings. The token itself is never returned.
#[utoipa::path(get, path = "/api/lichess", operation_id = "get_lichess", tag = "lichess", responses((status = 200, body = LichessStatus)))]
pub async fn status(State(state): State<Arc<AppState>>) -> Json<LichessStatus> {
    Json(state.lichess.status())
}

#[derive(Deserialize, ToSchema)]
pub struct TokenRequest {
    /// A Lichess personal API token with the `bot:play` scope, made at https://lichess.org/account/oauth/token.
    pub token: String,
}

/// Stores the bot's Lichess token, once Lichess has accepted it, and answers with the account it belongs to (whose
/// `bot` says whether it can play yet). The token stays in the runner's data directory and is never returned. 400 if
/// Lichess rejects it.
#[utoipa::path(put, path = "/api/lichess/token", operation_id = "set_lichess_token", tag = "lichess", request_body = TokenRequest, responses((status = 200, body = LichessStatus), (status = 400), (status = 502)))]
pub async fn set_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TokenRequest>,
) -> Result<Json<LichessStatus>, ApiError> {
    Ok(Json(lichess::set_token(&state, &req.token).await?))
}

/// Forgets the token and takes the bot offline. Games in progress are played out.
#[utoipa::path(delete, path = "/api/lichess/token", operation_id = "clear_lichess_token", tag = "lichess", responses((status = 200, body = LichessStatus)))]
pub async fn clear_token(State(state): State<Arc<AppState>>) -> Result<Json<LichessStatus>, ApiError> {
    Ok(Json(lichess::clear_token(&state).await?))
}

/// The bot's settings: the build that plays, its hash, threads and move overhead, and which challenges it accepts.
#[utoipa::path(get, path = "/api/lichess/settings", operation_id = "get_lichess_settings", tag = "lichess", responses((status = 200, body = Settings)))]
pub async fn settings(State(state): State<Arc<AppState>>) -> Json<Settings> {
    Json(state.lichess.status().settings)
}

/// Replaces the bot's settings (fields left out take their defaults) and answers with them as stored, numbers brought
/// into range. 400 if the build does not exist or a range is upside down.
#[utoipa::path(put, path = "/api/lichess/settings", operation_id = "set_lichess_settings", tag = "lichess", request_body = Settings, responses((status = 200, body = Settings), (status = 400)))]
pub async fn put_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    Ok(Json(lichess::put_settings(&state, req).await?))
}

/// Takes the bot online: it follows the account's event stream, answers challenges by its settings and plays every
/// game that starts, and stays online across restarts until stopped. 409 without a token, a BOT account or a build.
#[utoipa::path(post, path = "/api/lichess/start", operation_id = "start_lichess", tag = "lichess", responses((status = 200, body = LichessStatus), (status = 409)))]
pub async fn start(State(state): State<Arc<AppState>>) -> Result<Json<LichessStatus>, ApiError> {
    Ok(Json(lichess::start(&state).await?))
}

/// Takes the bot offline. Games in progress are played out.
#[utoipa::path(post, path = "/api/lichess/stop", operation_id = "stop_lichess", tag = "lichess", responses((status = 200, body = LichessStatus)))]
pub async fn stop(State(state): State<Arc<AppState>>) -> Result<Json<LichessStatus>, ApiError> {
    Ok(Json(lichess::stop(&state).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct UpgradeRequest {
    /// Must be `true`: the upgrade cannot be undone.
    #[serde(default)]
    pub confirm: bool,
}

/// Turns the token's account into a Lichess BOT account. IRREVERSIBLE: a BOT account can never play as a person
/// again, and Lichess only upgrades an account that has never played a game. Requires `{ "confirm": true }`.
#[utoipa::path(post, path = "/api/lichess/upgrade", operation_id = "upgrade_lichess_account", tag = "lichess", request_body = UpgradeRequest, responses((status = 200, body = LichessStatus), (status = 400), (status = 409)))]
pub async fn upgrade(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpgradeRequest>,
) -> Result<Json<LichessStatus>, ApiError> {
    if !req.confirm {
        return Err(ApiError::bad_request(
            "upgrading to a BOT account cannot be undone: send { \"confirm\": true } to go ahead",
        ));
    }
    Ok(Json(lichess::upgrade(&state).await?))
}

/// Challenges a Lichess player (usually another bot) to a standard game with our build. If accepted, the game starts
/// like any other and is recorded as a match. 409 while the bot is offline.
#[utoipa::path(post, path = "/api/lichess/challenge", operation_id = "create_lichess_challenge", tag = "lichess", request_body = ChallengeRequest, responses((status = 201, body = ChallengeSent), (status = 400), (status = 409)))]
pub async fn challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChallengeRequest>,
) -> Result<(StatusCode, Json<ChallengeSent>), ApiError> {
    Ok((StatusCode::CREATED, Json(lichess::challenge(&state, req).await?)))
}

/// The bots online on Lichess now (up to fifty), with their ratings: who there is to challenge.
#[utoipa::path(get, path = "/api/lichess/bots", operation_id = "list_lichess_bots", tag = "lichess", responses((status = 200, body = Vec<OnlineBot>), (status = 409)))]
pub async fn bots(State(state): State<Arc<AppState>>) -> Result<Json<Vec<OnlineBot>>, ApiError> {
    Ok(Json(lichess::online_bots(&state).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct SeekRequest {
    /// `once` looks until one game starts; `always` looks again after every game, and across restarts, until stopped.
    /// `off` stops, as `DELETE` does.
    pub mode: SeekMode,
}

/// Looks for a game: the bot challenges a random online bot near its rating, at a clock from `seek_clocks`, and tries
/// another when one declines or does not answer within 25 seconds. Answers the status, whose `seek` follows the
/// search. 409 while the bot is offline or has no token.
#[utoipa::path(post, path = "/api/lichess/seek", operation_id = "start_lichess_seek", tag = "lichess", request_body = SeekRequest, responses((status = 200, body = LichessStatus), (status = 409)))]
pub async fn seek(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SeekRequest>,
) -> Result<Json<LichessStatus>, ApiError> {
    Ok(Json(lichess::seek::start(&state, req.mode).await?))
}

/// Stops looking for games and withdraws the challenge waiting for an answer. Games in progress are played out.
#[utoipa::path(delete, path = "/api/lichess/seek", operation_id = "stop_lichess_seek", tag = "lichess", responses((status = 200, body = LichessStatus)))]
pub async fn stop_seek(State(state): State<Arc<AppState>>) -> Result<Json<LichessStatus>, ApiError> {
    Ok(Json(lichess::seek::stop(&state).await?))
}
