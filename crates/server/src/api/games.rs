//! Games: lists, one game, starting a local game, your actions in a game, stopping one, PGN both ways, and a game's
//! review.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use dameless_core::{
    ConnectionKey, Error, GameId, PlayerKey, Side, TimeControl, Timestamp,
    chess::pgn,
    events::Payload,
    game::{Ending, Game, GameStatus, GameSummary, Opening, Seat, SeatAction, Source},
    review::Review,
    time::Speed,
};
use dameless_play::NewGameRequest;
use dameless_store::{GameFilter, NewGame, SourceKind};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiError, ApiResult, Json, Path, Query, list, parse};
use crate::App;

/// A plain string enum (`finished`, `blitz`) read as serde reads it.
pub(crate) fn word<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.trim().to_owned())).ok()
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GamesQuery {
    /// Games this player sat in: `you`, `engine:<id>`, `lichess:<name>`.
    pub player: Option<String>,
    /// Sources, comma-separated: `local`, `lichess`, `import`.
    pub source: Option<String>,
    /// States, comma-separated: `starting`, `live`, `finished`, `aborted`.
    pub status: Option<String>,
    /// Speeds, comma-separated: `bullet`, `blitz`, `rapid`, `classical`, `correspondence`.
    pub speed: Option<String>,
    pub rated: Option<bool>,
    /// Games started at or after this time (ISO-8601).
    pub since: Option<String>,
    /// The page after this game (the last id of the previous page).
    pub before: Option<i64>,
    /// At most this many (default 50).
    pub limit: Option<u32>,
    /// Calibration games between levels: left out unless `true` (only them) or `all`.
    pub calibration: Option<String>,
}

/// Games, live ones first in the list's order of newest first.
#[utoipa::path(get, path = "/api/games", operation_id = "list_games", tag = "games", params(GamesQuery),
    responses((status = 200, body = Vec<GameSummary>), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn list_games(State(app): State<Arc<App>>, Query(q): Query<GamesQuery>) -> ApiResult<Json<Vec<GameSummary>>> {
    let filter = GameFilter {
        player: q.player.as_deref().map(|p| parse::<PlayerKey>("a player", p)).transpose()?,
        sources: list(q.source.as_deref(), "a source", SourceKind::parse)?,
        statuses: list(q.status.as_deref(), "a game status", word::<GameStatus>)?,
        speeds: list(q.speed.as_deref(), "a speed", Speed::parse)?,
        rated: q.rated,
        since: q
            .since
            .as_deref()
            .map(|s| Timestamp::parse(s).ok_or_else(|| bad(format!("{s:?} is not a time"))))
            .transpose()?,
        before: q.before.map(GameId),
        limit: q.limit.unwrap_or(50),
        live_first: true,
        calibration: match q.calibration.as_deref().map(str::trim) {
            None | Some("" | "false") => Some(false),
            Some("true") => Some(true),
            Some("all") => None,
            Some(other) => return Err(bad(format!("calibration is true, false or all, not {other:?}"))),
        },
    };
    Ok(Json(app.hub.store.games(filter).await?))
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError(Error::bad_request(message))
}

/// Starts a local game: you against an engine, or two engines. Engines are started before the game is stored.
#[utoipa::path(post, path = "/api/games", operation_id = "start_game", tag = "games", request_body = NewGameRequest,
    responses((status = 201, body = Game), (status = 400, body = dameless_core::error::ErrorBody), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn start_game(
    State(app): State<Arc<App>>,
    Json(req): Json<NewGameRequest>,
) -> ApiResult<(StatusCode, Json<Game>)> {
    Ok((StatusCode::CREATED, Json(app.games.start(req).await?)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct GamePath {
    /// The game.
    pub id: i64,
}

/// A game as it is now: a live game's clocks, offers and your actions laid over the stored record.
#[utoipa::path(get, path = "/api/games/{id}", operation_id = "get_game", tag = "games", params(GamePath),
    responses((status = 200, body = Game), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_game(State(app): State<Arc<App>>, Path(p): Path<GamePath>) -> ApiResult<Json<Game>> {
    Ok(Json(app.games.game(GameId(p.id)).await?))
}

/// A game as PGN, with a comment on every move (eval/depth, think time, nodes, clock).
#[utoipa::path(get, path = "/api/games/{id}/pgn", operation_id = "get_game_pgn", tag = "games", params(GamePath),
    responses((status = 200, content_type = "application/x-chess-pgn", body = String), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_game_pgn(State(app): State<Arc<App>>, Path(p): Path<GamePath>) -> ApiResult<impl IntoResponse> {
    let game = app.games.game(GameId(p.id)).await?;
    Ok(([(header::CONTENT_TYPE, "application/x-chess-pgn; charset=utf-8")], pgn::write(&game)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ActionPath {
    pub id: i64,
    /// `move`, `offer_draw`, `accept_draw`, `decline_draw`, `takeback`, `accept_takeback`, `decline_takeback`,
    /// `resign`, `abort`, `claim_victory`, `claim_draw`.
    pub action: String,
}

/// The body of an action; only `move` needs one.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct ActionBody {
    /// The move, UCI (`e2e4`, `e7e8q`) or SAN (`e4`, `O-O`).
    #[serde(default)]
    pub uci: Option<String>,
}

/// Does an action as you in a game you play: 409 when the game is over or the action is not one of `Game.actions`
/// now, 400 for a move that is not legal. A move answers once it is on the board.
#[utoipa::path(post, path = "/api/games/{id}/actions/{action}", operation_id = "act_in_game", tag = "games",
    params(ActionPath), request_body(content = Option<ActionBody>),
    responses((status = 200, body = Game), (status = 400, body = dameless_core::error::ErrorBody),
        (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn act_in_game(State(app): State<Arc<App>>, Path(p): Path<ActionPath>, body: Bytes) -> ApiResult<Json<Game>> {
    let action = SeatAction::parse(&p.action)
        .ok_or_else(|| bad(format!("{:?} is not an action: {}", p.action, actions_text())))?;
    let body: ActionBody = super::optional_body(&body)?;
    if action == SeatAction::Move && body.uci.as_deref().is_none_or(|m| m.trim().is_empty()) {
        return Err(bad("a move needs { \"uci\": \"e2e4\" }"));
    }
    Ok(Json(app.games.act(GameId(p.id), action, body.uci).await?))
}

fn actions_text() -> String {
    SeatAction::ALL.iter().map(|a| a.as_str()).collect::<Vec<_>>().join(", ")
}

/// Stops a live game: a game you sit in is resigned (aborted before each side has moved), an engine game aborted,
/// the bot's resigned on Lichess. Answers the game once it has ended, or as it stands after a few seconds.
#[utoipa::path(post, path = "/api/games/{id}/stop", operation_id = "stop_game", tag = "games", params(GamePath),
    responses((status = 200, body = Game), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn stop_game(State(app): State<Arc<App>>, Path(p): Path<GamePath>) -> ApiResult<Json<Game>> {
    Ok(Json(app.games.stop(GameId(p.id)).await?))
}

/// `POST /api/games/import`: games you paste.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ImportPgnRequest {
    /// One or more games.
    pub pgn: String,
    /// The side you played, when not found by your Lichess username in the tags.
    #[serde(default)]
    pub you: Option<Side>,
}

/// Stores pasted PGN as finished local games (one per game in the text). A player is you when `you` says so or the
/// tag names your Lichess account; anyone else is kept as a Lichess player of that name.
#[utoipa::path(post, path = "/api/games/import", operation_id = "import_pgn", tag = "games", request_body = ImportPgnRequest,
    responses((status = 201, body = Vec<GameSummary>), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn import_pgn(
    State(app): State<Arc<App>>,
    Json(req): Json<ImportPgnRequest>,
) -> ApiResult<(StatusCode, Json<Vec<GameSummary>>)> {
    let read = pgn::read(&req.pgn).map_err(|e| bad(format!("the PGN does not read: {e}")))?;
    if read.is_empty() {
        return Err(bad("no game in the PGN"));
    }
    let username =
        app.connections.status(ConnectionKey::You).account.map(|a| a.username.to_lowercase()).filter(|u| !u.is_empty());
    let now = dameless_store::now();
    let mut stored = Vec::with_capacity(read.len());
    for g in read {
        let seat = |side: Side| {
            let tag = if side == Side::White { "White" } else { "Black" };
            let name = g.header(tag).unwrap_or(tag).to_owned();
            let is_you = req.you == Some(side) || username.as_deref() == Some(&name.to_lowercase());
            let player =
                if is_you { PlayerKey::You } else { PlayerKey::lichess(&name).unwrap_or_else(|_| sanitized(&name)) };
            let mut s = Seat::new(player, name);
            s.rating = g.header(&format!("{tag}Elo")).and_then(|r| r.parse().ok());
            s.title = g.header(&format!("{tag}Title")).map(str::to_owned);
            s
        };
        let end = g.end();
        let started_at = g
            .header("UTCDate")
            .or_else(|| g.header("Date"))
            .and_then(|d| {
                Timestamp::parse(&format!("{} {}", d.replace('.', "-"), g.header("UTCTime").unwrap_or("00:00:00")))
            })
            .unwrap_or_else(|| now.clone());
        let opening = g.header("Opening").map(|name| Opening {
            eco: g.header("ECO").map(str::to_owned),
            name: name.to_owned(),
            ply: None,
        });
        let new = NewGame {
            white: seat(Side::White),
            black: seat(Side::Black),
            source: Source::Local,
            time: g.time().unwrap_or(TimeControl::Correspondence { days: 0 }),
            status: GameStatus::Finished,
            outcome: end.and_then(|e| e.outcome),
            ending: Some(end.map_or(Ending::Other, |e| e.ending)),
            opening,
            start_fen: g.start_fen.clone(),
            moves: g.moves.clone(),
            started_at: started_at.clone(),
            ended_at: Some(started_at),
            url: g.header("Site").filter(|s| s.starts_with("http")).map(str::to_owned),
        };
        let id = app.hub.store.insert_game(new).await?;
        let summary =
            app.hub.store.game_summary(id).await?.ok_or_else(|| Error::internal("the game was not stored"))?;
        app.hub.bus.emit(Payload::GameUpserted(Box::new(summary.clone())));
        stored.push(summary);
    }
    Ok((StatusCode::CREATED, Json(stored)))
}

/// A name that is not a Lichess username, made into a key: letters, digits, `_` and `-`, at most 30.
fn sanitized(name: &str) -> PlayerKey {
    let mut id: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .take(30)
        .collect();
    if id.trim_matches('_').is_empty() {
        id = "anonymous".into();
    }
    PlayerKey::Lichess(id)
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReviewQuery {
    /// The engine that reviewed it, by id; the newest review when none.
    pub engine: Option<String>,
    /// The node budget per position; any when none.
    pub nodes: Option<u64>,
}

/// A game's newest review (or the one by this engine and budget). 404 without one.
#[utoipa::path(get, path = "/api/games/{id}/review", operation_id = "get_review", tag = "games", params(GamePath, ReviewQuery),
    responses((status = 200, body = Review), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn get_review(
    State(app): State<Arc<App>>,
    Path(p): Path<GamePath>,
    Query(q): Query<ReviewQuery>,
) -> ApiResult<Json<Review>> {
    app.reviews
        .get(GameId(p.id), q.engine.as_deref(), q.nodes)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError(Error::not_found(format!("Game {} has no review yet", p.id))))
}
