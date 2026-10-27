//! The Lichess connections: you (your account) and bot (Dameless's BOT account). Tokens go in, never out.

use std::sync::Arc;

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use dameless_core::{
    ConnectionKey, Error,
    lichess::{BotSettings, ConnectionStatus, SeekMode, seek::OnlineBot},
};
use dameless_play::{
    LoginStarted,
    you::{ChallengeBody, ChallengeSent, SeekRequestBody},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{
    error::{ApiError, ApiResult, Json, Path, Query},
    games::word,
};
use crate::App;

/// Both connections.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Connections {
    pub you: ConnectionStatus,
    pub bot: ConnectionStatus,
}

/// Both Lichess connections as the app shows them.
#[utoipa::path(get, path = "/api/connections", operation_id = "list_connections", tag = "connections",
    responses((status = 200, body = Connections)))]
pub async fn list_connections(State(app): State<Arc<App>>) -> Json<Connections> {
    Json(Connections {
        you: app.connections.status(ConnectionKey::You),
        bot: app.connections.status(ConnectionKey::Bot),
    })
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct KeyPath {
    /// `you` or `bot`.
    pub key: String,
}

fn key_of(text: &str) -> ApiResult<ConnectionKey> {
    word::<ConnectionKey>(text).ok_or_else(|| ApiError(Error::bad_request(format!("{text:?} is not you or bot"))))
}

/// `POST /api/connections/you/login`.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct LoginBody {
    /// The page to come back to: the app on a loopback address or the hosted UI (default the app's `/you`).
    #[serde(default)]
    pub return_to: Option<String>,
}

/// Starts logging in to your Lichess account (OAuth with PKCE): open the link. Lichess sends the browser back to the
/// app's callback, which stores the token and sends the browser on to `return_to` with `?lichess=connected` (or
/// `?lichess=error&message=…`). The link works once, for ten minutes.
#[utoipa::path(post, path = "/api/connections/you/login", operation_id = "login", tag = "connections",
    request_body(content = Option<LoginBody>),
    responses((status = 200, body = LoginStarted), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn login(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<LoginStarted>> {
    let b: LoginBody = super::optional_body(&body)?;
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| app.hub.config.listen.to_string());
    let redirect_uri = format!("http://{host}{CALLBACK}");
    Ok(Json(app.connections.start_login(b.return_to.as_deref(), &redirect_uri)?))
}

/// Where Lichess sends the browser back to.
pub const CALLBACK: &str = "/api/connections/you/callback";

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// The OAuth callback, outside the guard and the OpenAPI document: a browser arriving from lichess.org has no token,
/// and the login's single-use `state` is what makes it safe.
pub fn callback_router() -> Router<Arc<App>> {
    Router::new().route(CALLBACK, get(callback))
}

async fn callback(State(app): State<Arc<App>>, Query(q): Query<CallbackQuery>) -> Response {
    let Some(state) = q.state.as_deref() else {
        return ApiError(Error::bad_request("Lichess sent no state: log in again")).into_response();
    };
    match app.connections.finish_login(q.code.as_deref(), state, q.error.as_deref()).await {
        Ok(done) => Redirect::to(&done.redirect()).into_response(),
        Err(e) => ApiError(e).into_response(),
    }
}

/// `PUT /api/connections/{key}/token`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct TokenBody {
    /// A personal Lichess token: yours with `board:play` (and `challenge:read`, `challenge:write`), the bot's with
    /// `bot:play`.
    pub token: String,
}

/// Stores a Lichess token after Lichess checks it. Yours must be a person's account allowed to play; the bot's a
/// BOT account. Neither may be the other's.
#[utoipa::path(put, path = "/api/connections/{key}/token", operation_id = "set_connection_token", tag = "connections",
    params(KeyPath), request_body = TokenBody,
    responses((status = 200, body = ConnectionStatus), (status = 400, body = dameless_core::error::ErrorBody), (status = 502, body = dameless_core::error::ErrorBody)))]
pub async fn set_token(
    State(app): State<Arc<App>>,
    Path(p): Path<KeyPath>,
    Json(b): Json<TokenBody>,
) -> ApiResult<Json<ConnectionStatus>> {
    let key = key_of(&p.key)?;
    Ok(Json(app.connections.set_token(key, b.token.trim()).await?))
}

/// Logs a connection out: forgets its token (revoking yours at Lichess). Games in progress go on at Lichess.
#[utoipa::path(delete, path = "/api/connections/{key}", operation_id = "logout", tag = "connections", params(KeyPath),
    responses((status = 200, body = ConnectionStatus)))]
pub async fn logout(State(app): State<Arc<App>>, Path(p): Path<KeyPath>) -> ApiResult<Json<ConnectionStatus>> {
    let key = key_of(&p.key)?;
    Ok(Json(app.connections.logout(key).await?))
}

/// Opens a seek in the lobby for you (rapid or slower, the Board API's rule). The game starts when someone takes it.
#[utoipa::path(post, path = "/api/connections/you/seek", operation_id = "seek", tag = "connections",
    request_body = SeekRequestBody,
    responses((status = 200, body = ConnectionStatus), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn seek(State(app): State<Arc<App>>, Json(b): Json<SeekRequestBody>) -> ApiResult<Json<ConnectionStatus>> {
    Ok(Json(app.connections.you().seek(b).await?))
}

/// Cancels your open seek.
#[utoipa::path(delete, path = "/api/connections/you/seek", operation_id = "cancel_seek", tag = "connections",
    responses((status = 200, body = ConnectionStatus)))]
pub async fn cancel_seek(State(app): State<Arc<App>>) -> Json<ConnectionStatus> {
    Json(app.connections.you().cancel_seek().await)
}

/// Challenges from a connection: yours to a player (blitz or slower), Lichess's AI (`ai_level`) or your bot
/// (`my_bot`); the bot's to a player (the bot must be online).
#[utoipa::path(post, path = "/api/connections/{key}/challenge", operation_id = "challenge", tag = "connections",
    params(KeyPath), request_body = ChallengeBody,
    responses((status = 201, body = ChallengeSent), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn challenge(
    State(app): State<Arc<App>>,
    Path(p): Path<KeyPath>,
    Json(b): Json<ChallengeBody>,
) -> ApiResult<(StatusCode, Json<ChallengeSent>)> {
    let key = key_of(&p.key)?;
    Ok((StatusCode::CREATED, Json(app.connections.challenge(key, b).await?)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ChallengePath {
    /// `you` (the bot answers challenges by its settings).
    pub key: String,
    /// Lichess's challenge id.
    pub id: String,
}

fn yours(key: &str) -> ApiResult<()> {
    match key_of(key)? {
        ConnectionKey::You => Ok(()),
        ConnectionKey::Bot => Err(ApiError(Error::bad_request("the bot answers challenges by its settings"))),
    }
}

/// Accepts a challenge to you; the game starts on the event stream. 400 for one the Board API cannot play (bullet,
/// variants): accept that on lichess.org.
#[utoipa::path(post, path = "/api/connections/{key}/challenges/{id}/accept", operation_id = "accept_challenge",
    tag = "connections", params(ChallengePath),
    responses((status = 200, body = ConnectionStatus), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn accept_challenge(
    State(app): State<Arc<App>>,
    Path(p): Path<ChallengePath>,
) -> ApiResult<Json<ConnectionStatus>> {
    yours(&p.key)?;
    Ok(Json(app.connections.you().accept(&p.id).await?))
}

/// `POST …/decline`.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct DeclineBody {
    /// Lichess's reason key: `generic` (default), `later`, `tooFast`, `tooSlow`, `timeControl`, `rated`, `casual`,
    /// `standard`, `variant`, `noBot`, `onlyBot`.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Declines a challenge to you.
#[utoipa::path(post, path = "/api/connections/{key}/challenges/{id}/decline", operation_id = "decline_challenge",
    tag = "connections", params(ChallengePath), request_body(content = Option<DeclineBody>),
    responses((status = 200, body = ConnectionStatus), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn decline_challenge(
    State(app): State<Arc<App>>,
    Path(p): Path<ChallengePath>,
    body: axum::body::Bytes,
) -> ApiResult<Json<ConnectionStatus>> {
    yours(&p.key)?;
    let b: DeclineBody = super::optional_body(&body)?;
    Ok(Json(app.connections.you().decline(&p.id, b.reason.as_deref()).await?))
}

/// Withdraws a challenge of yours.
#[utoipa::path(delete, path = "/api/connections/{key}/challenges/{id}", operation_id = "cancel_challenge",
    tag = "connections", params(ChallengePath),
    responses((status = 200, body = ConnectionStatus), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn cancel_challenge(
    State(app): State<Arc<App>>,
    Path(p): Path<ChallengePath>,
) -> ApiResult<Json<ConnectionStatus>> {
    yours(&p.key)?;
    Ok(Json(app.connections.you().cancel(&p.id).await?))
}

/// Replaces the bot's settings: the version it plays, hash, threads, move overhead, which challenges it takes, and
/// how it seeks. Answers the settings as stored.
#[utoipa::path(put, path = "/api/connections/bot/settings", operation_id = "set_bot_settings", tag = "connections",
    request_body = BotSettings,
    responses((status = 200, body = BotSettings), (status = 400, body = dameless_core::error::ErrorBody), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn set_bot_settings(State(app): State<Arc<App>>, Json(b): Json<BotSettings>) -> ApiResult<Json<BotSettings>> {
    Ok(Json(app.connections.bot().set_settings(b).await?))
}

/// `POST /api/connections/bot/online`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct OnlineBody {
    pub online: bool,
}

/// Takes the bot online (it stays online across restarts until taken offline) or offline (games in progress are
/// played out).
#[utoipa::path(post, path = "/api/connections/bot/online", operation_id = "set_bot_online", tag = "connections",
    request_body = OnlineBody,
    responses((status = 200, body = ConnectionStatus), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn set_bot_online(
    State(app): State<Arc<App>>,
    Json(b): Json<OnlineBody>,
) -> ApiResult<Json<ConnectionStatus>> {
    Ok(Json(app.connections.bot().set_online(b.online).await?))
}

/// `POST /api/connections/bot/seek`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct BotSeekBody {
    pub mode: SeekMode,
}

/// The bot looks for games against other bots: `once` until one starts, `always` after every game too, `off` stops
/// (and withdraws the challenge waiting for an answer).
#[utoipa::path(post, path = "/api/connections/bot/seek", operation_id = "set_bot_seek", tag = "connections",
    request_body = BotSeekBody,
    responses((status = 200, body = ConnectionStatus), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn set_bot_seek(
    State(app): State<Arc<App>>,
    Json(b): Json<BotSeekBody>,
) -> ApiResult<Json<ConnectionStatus>> {
    Ok(Json(app.connections.bot().set_seek(b.mode).await?))
}

/// The bots online on Lichess now, with their ratings.
#[utoipa::path(get, path = "/api/connections/bot/bots", operation_id = "online_bots", tag = "connections",
    responses((status = 200, body = Vec<OnlineBot>), (status = 409, body = dameless_core::error::ErrorBody), (status = 502, body = dameless_core::error::ErrorBody)))]
pub async fn online_bots(State(app): State<Arc<App>>) -> ApiResult<Json<Vec<OnlineBot>>> {
    Ok(Json(app.connections.bot().online_bots().await?))
}
