//! Analysis: live evaluation sessions, the explorer, the tablebase and queueing reviews. Every one of these is
//! refused (409, `fair_play`) while you play on Lichess; the services check the policy.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use chess_analysis::{Budget, EvalRequest, EvalSession, ReviewRequest};
use chess_core::{Error, GameId, PlayerKey, time::Speed, work::Work};
use chess_lichess::explorer::{ExplorerDb, ExplorerQuery};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiError, ApiResult, Json, Path, Query};
use crate::App;

/// `POST /api/eval`: a live search of a position.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct EvalBody {
    /// The start position as FEN; none for the standard start.
    #[serde(default)]
    pub fen: Option<String>,
    /// Moves from it, UCI or SAN.
    #[serde(default)]
    pub moves: Vec<String>,
    /// The engine to search with, by id; the analysis engine when none.
    #[serde(default)]
    pub engine: Option<String>,
    /// Lines to report, 1 to 10.
    #[serde(default = "one")]
    pub multipv: u32,
    #[serde(default)]
    pub budget: Budget,
    /// Search threads (cores reserved); 1 when none.
    #[serde(default)]
    pub threads: Option<u32>,
    /// Who asks (a browser tab, a CLI run): one session per client, a new one replaces the old.
    pub client: String,
}

fn one() -> u32 {
    1
}

/// Starts a live search. Its progress comes as `eval.info` events on topic `eval:<session>`; it ends at its budget,
/// when stopped, or when the policy turns analysis off.
#[utoipa::path(post, path = "/api/eval", operation_id = "start_eval", tag = "analysis", request_body = EvalBody,
    responses((status = 200, body = EvalSession), (status = 400, body = chess_core::error::ErrorBody), (status = 409, body = chess_core::error::ErrorBody)))]
pub async fn start_eval(State(app): State<Arc<App>>, Json(b): Json<EvalBody>) -> ApiResult<Json<EvalSession>> {
    let req = EvalRequest {
        fen: b.fen,
        moves: b.moves,
        engine: b.engine,
        multipv: b.multipv,
        budget: b.budget,
        threads: b.threads,
        client: b.client,
    };
    Ok(Json(app.eval.start_session(req).await?))
}

/// A running evaluation session.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct EvalSessionInfo {
    pub session: String,
    pub client: String,
}

/// The evaluation sessions running now. A session that reached its budget is no longer listed.
#[utoipa::path(get, path = "/api/eval", operation_id = "list_evals", tag = "analysis",
    responses((status = 200, body = Vec<EvalSessionInfo>)))]
pub async fn list_evals(State(app): State<Arc<App>>) -> Json<Vec<EvalSessionInfo>> {
    Json(app.eval.sessions().into_iter().map(|s| EvalSessionInfo { session: s.session, client: s.client }).collect())
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct SessionPath {
    pub session: String,
}

/// Whether a session was stopped.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Stopped {
    /// False when it had already ended.
    pub stopped: bool,
}

/// Stops a session and waits until its engine has stopped.
#[utoipa::path(delete, path = "/api/eval/{session}", operation_id = "stop_eval", tag = "analysis", params(SessionPath),
    responses((status = 200, body = Stopped)))]
pub async fn stop_eval(State(app): State<Arc<App>>, Path(p): Path<SessionPath>) -> Json<Stopped> {
    Json(Stopped { stopped: app.eval.stop_session(&p.session).await })
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExplorerParams {
    /// `lichess` (default), `masters` or `player`.
    pub db: Option<String>,
    /// The position as FEN; the start position when none.
    pub fen: Option<String>,
    /// Moves from `fen`, UCI, comma-separated.
    pub play: Option<String>,
    /// `lichess` and `player`: speeds, comma-separated (`blitz,rapid`).
    pub speeds: Option<String>,
    /// `lichess`: rating groups, comma-separated (`1600,1800`).
    pub ratings: Option<String>,
    /// `player`: `rated` and/or `casual`, comma-separated.
    pub modes: Option<String>,
    /// From this month (`2020-01`), or year for `masters`.
    pub since: Option<String>,
    pub until: Option<String>,
    pub moves: Option<u32>,
    pub top_games: Option<u32>,
    pub recent_games: Option<u32>,
    /// `player`: whose games.
    pub player: Option<String>,
    /// `player`: `white` or `black`.
    pub color: Option<String>,
}

fn words(text: &Option<String>) -> Vec<String> {
    text.as_deref().unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

/// Lichess's opening explorer, as Lichess answers it. Needs your Lichess login (409 without it); refused while you
/// play on Lichess unless every game of yours is correspondence.
#[utoipa::path(get, path = "/api/explorer", operation_id = "explorer", tag = "analysis", params(ExplorerParams),
    responses((status = 200, body = serde_json::Value), (status = 409, body = chess_core::error::ErrorBody), (status = 502, body = chess_core::error::ErrorBody)))]
pub async fn explorer(
    State(app): State<Arc<App>>,
    Query(q): Query<ExplorerParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = match q.db.as_deref().map(str::trim) {
        None | Some("") | Some("lichess") => ExplorerDb::Lichess,
        Some("masters") => ExplorerDb::Masters,
        Some("player") => ExplorerDb::Player,
        Some(other) => {
            return Err(ApiError(Error::bad_request(format!("{other:?} is not lichess, masters or player"))));
        }
    };
    let ratings = words(&q.ratings)
        .iter()
        .map(|r| r.parse().map_err(|_| ApiError(Error::bad_request(format!("{r:?} is not a rating group")))))
        .collect::<ApiResult<Vec<u32>>>()?;
    let query = ExplorerQuery {
        db,
        fen: q.fen.clone(),
        play: words(&q.play),
        speeds: words(&q.speeds),
        ratings,
        modes: words(&q.modes),
        since: q.since.clone(),
        until: q.until.clone(),
        moves: q.moves,
        top_games: q.top_games,
        recent_games: q.recent_games,
        player: q.player.clone(),
        color: q.color.clone(),
    };
    Ok(Json(app.connections.explorer(&query).await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TablebaseParams {
    /// A position with at most seven pieces.
    pub fen: String,
}

/// The tablebase's answer for a position of seven pieces or fewer, as Lichess gives it. Refused while you play.
#[utoipa::path(get, path = "/api/tablebase", operation_id = "tablebase", tag = "analysis", params(TablebaseParams),
    responses((status = 200, body = serde_json::Value), (status = 409, body = chess_core::error::ErrorBody), (status = 502, body = chess_core::error::ErrorBody)))]
pub async fn tablebase(
    State(app): State<Arc<App>>,
    Query(q): Query<TablebaseParams>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(app.connections.tablebase(&q.fen).await?))
}

/// `POST /api/reviews`: some games, or a player's newest games no engine has reviewed yet.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ReviewBody {
    /// These games. Or, instead, `player`.
    #[serde(default)]
    pub games: Vec<GameId>,
    /// This player's newest finished games no engine has reviewed: `you`, `engine:<id>`, `lichess:<name>`.
    #[serde(default)]
    pub player: Option<PlayerKey>,
    /// How many of the player's games; 20 when none, at most 1000. In the background: every one when none.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Review in the background: at the lowest priority (`background`, beside calibration), and in a lane of its own, so a review asked for meanwhile runs at once. With `player` and no `limit`: every game of
    /// theirs not reviewed yet, newest first.
    #[serde(default)]
    pub background: bool,
    #[serde(default)]
    pub speeds: Vec<Speed>,
    #[serde(default)]
    pub rated: Option<bool>,
    /// The engine to review with, by id; the analysis engine when none.
    #[serde(default)]
    pub engine: Option<String>,
    /// Nodes per position; 200,000 when none.
    #[serde(default)]
    pub nodes: Option<u64>,
    /// Engines at once. When none: half the machine's cores; in the background, all of them (it gives them back
    /// within one search when live play or other work needs them).
    #[serde(default)]
    pub concurrency: Option<u32>,
}

/// Queues a review (work). Refused while you play on Lichess; a running review pauses while you do.
#[utoipa::path(post, path = "/api/reviews", operation_id = "review_games", tag = "analysis", request_body = ReviewBody,
    responses((status = 202, body = Work), (status = 404, body = chess_core::error::ErrorBody), (status = 409, body = chess_core::error::ErrorBody)))]
pub async fn review_games(
    State(app): State<Arc<App>>,
    Json(b): Json<ReviewBody>,
) -> ApiResult<(StatusCode, Json<Work>)> {
    let req = ReviewRequest {
        games: b.games,
        player: b.player,
        limit: b.limit,
        background: b.background,
        speeds: b.speeds,
        rated: b.rated,
        engine: b.engine,
        nodes: b.nodes,
        concurrency: b.concurrency,
    };
    Ok((StatusCode::ACCEPTED, Json(app.reviews.submit(&app.scheduler, req).await?)))
}
