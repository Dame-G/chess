//! The player side: the levels and their calibration, your drills, your repertoire, and your progress.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use dameless_analysis::{
    AnswerRequest, DrillAnswerResult, DrillsToday, MakeDrillsRequest, RepertoireRequest, SessionRequest,
};
use dameless_core::{
    GameId,
    drills::{Drill, DrillHint, DrillId, HitRatePoint},
    levels::PlannedMatch,
    prep::{Prep, PrepRequest},
    progress::Progress,
    repertoire::Repertoire,
    themes::ThemeKind,
    work::Work,
};
use dameless_play::{CalibrateRequest, LevelsOverview};
use serde::Deserialize;
use utoipa::IntoParams;

use super::error::{ApiResult, Json, Path, Query};
use crate::App;

/// The ladder of levels with their calibrated ratings, your rating on it and the level suggested for you.
#[utoipa::path(get, path = "/api/levels", operation_id = "list_levels", tag = "levels",
    responses((status = 200, body = LevelsOverview)))]
pub async fn list_levels(State(app): State<Arc<App>>) -> ApiResult<Json<LevelsOverview>> {
    Ok(Json(app.games.levels().overview().await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlanQuery {
    /// How many matches (default 3).
    pub count: Option<usize>,
}

/// The calibration matches that would teach the most now: neighbouring levels, the widest and most even first.
#[utoipa::path(get, path = "/api/levels/plan", operation_id = "plan_levels", tag = "levels", params(PlanQuery),
    responses((status = 200, body = Vec<PlannedMatch>)))]
pub async fn plan_levels(
    State(app): State<Arc<App>>,
    Query(q): Query<PlanQuery>,
) -> ApiResult<Json<Vec<PlannedMatch>>> {
    Ok(Json(app.games.levels().plan(q.count.unwrap_or(3)).await?))
}

/// Queues calibration (background work at the lowest priority): levels play each other at fixed nodes, and the
/// ladder is refitted after each match.
#[utoipa::path(post, path = "/api/levels/calibrate", operation_id = "calibrate_levels", tag = "levels",
    request_body(content = Option<CalibrateRequest>),
    responses((status = 202, body = Work), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn calibrate_levels(
    State(app): State<Arc<App>>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<Work>)> {
    let req: CalibrateRequest = super::optional_body(&body)?;
    let new = app.games.levels().prepare_calibration(&app.engines, req).await?;
    Ok((StatusCode::ACCEPTED, Json(app.scheduler.submit(new).await?)))
}

/// Every drill, soonest due first.
#[utoipa::path(get, path = "/api/drills", operation_id = "list_drills", tag = "drills",
    responses((status = 200, body = Vec<Drill>)))]
pub async fn list_drills(State(app): State<Arc<App>>) -> ApiResult<Json<Vec<Drill>>> {
    Ok(Json(app.drills.list().await?))
}

/// Makes drills from your profile's weak spots (new positions only); answers the drills made.
#[utoipa::path(post, path = "/api/drills", operation_id = "make_drills", tag = "drills",
    request_body(content = Option<MakeDrillsRequest>), responses((status = 201, body = Vec<Drill>)))]
pub async fn make_drills(
    State(app): State<Arc<App>>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<Vec<Drill>>)> {
    let req: MakeDrillsRequest = super::optional_body(&body)?;
    Ok((StatusCode::CREATED, Json(app.drills.make_from_weak_spots(req.limit).await?)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TodayQuery {
    /// New drills a day (default 10).
    pub new_per_day: Option<u32>,
    /// At most this many in all (default 30; 10 for a theme or a line).
    pub limit: Option<usize>,
    /// Only this theme: a session on it, with the day's cap on new drills set aside.
    pub theme: Option<ThemeKind>,
    /// Only drills from these games (comma-separated ids, at most 200): the games of an opening line.
    pub games: Option<String>,
    /// Only mistakes made by this ply (24 is the first twelve moves).
    pub max_ply: Option<u32>,
}

/// Most game ids `games` takes.
const MAX_GAMES: usize = 200;

/// Today's drills: due ones first, then new ones up to the day's cap; or, with `theme` or `games`, a session on that
/// theme or opening line. They come as questions: the `fen`, `side` and `theme`, without the answer, which
/// `answer_drill` returns.
#[utoipa::path(get, path = "/api/drills/today", operation_id = "drills_today", tag = "drills", params(TodayQuery),
    responses((status = 200, body = DrillsToday), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn drills_today(State(app): State<Arc<App>>, Query(q): Query<TodayQuery>) -> ApiResult<Json<DrillsToday>> {
    let games = q
        .games
        .as_deref()
        .map(|s| {
            s.split(',')
                .filter(|p| !p.trim().is_empty())
                .map(|p| p.trim().parse::<i64>().map(GameId))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|_| dameless_core::Error::bad_request("games are comma-separated game ids"))?
        .unwrap_or_default();
    if games.len() > MAX_GAMES {
        return Err(dameless_core::Error::bad_request(format!("at most {MAX_GAMES} games")).into());
    }
    let req = SessionRequest { new_per_day: q.new_per_day, limit: q.limit, theme: q.theme, games, max_ply: q.max_ply };
    Ok(Json(app.drills.today(&dameless_store::now(), &req).await?))
}

/// The next hint at a drill, without the move: first what kind of move it is, then which piece moves. A hint is
/// remembered until the drill is answered, and a right answer after one counts half and comes back tomorrow.
#[utoipa::path(post, path = "/api/drills/{id}/hint", operation_id = "hint_drill", tag = "drills", params(DrillPath),
    responses((status = 200, body = DrillHint), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn hint_drill(State(app): State<Arc<App>>, Path(p): Path<DrillPath>) -> ApiResult<Json<DrillHint>> {
    Ok(Json(app.drills.hint(DrillId(p.id)).await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct DrillPath {
    pub id: i64,
}

/// Answers a drill (UCI or SAN): the outcome, the drill rescheduled, and the answer with its line.
#[utoipa::path(post, path = "/api/drills/{id}/answer", operation_id = "answer_drill", tag = "drills", params(DrillPath),
    request_body = AnswerRequest,
    responses((status = 200, body = DrillAnswerResult), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn answer_drill(
    State(app): State<Arc<App>>,
    Path(p): Path<DrillPath>,
    Json(b): Json<AnswerRequest>,
) -> ApiResult<Json<DrillAnswerResult>> {
    Ok(Json(app.drills.answer_now(DrillId(p.id), &b.mv).await?))
}

/// Deletes a drill.
#[utoipa::path(delete, path = "/api/drills/{id}", operation_id = "delete_drill", tag = "drills", params(DrillPath),
    responses((status = 204), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn delete_drill(State(app): State<Arc<App>>, Path(p): Path<DrillPath>) -> ApiResult<StatusCode> {
    app.drills.delete(DrillId(p.id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WindowQuery {
    /// The rolling window.
    pub window: Option<usize>,
}

/// Your drill hit rate over time, a point per day with a rolling mean (window 50 answers).
#[utoipa::path(get, path = "/api/drills/trend", operation_id = "drills_trend", tag = "drills", params(WindowQuery),
    responses((status = 200, body = Vec<HitRatePoint>)))]
pub async fn drills_trend(
    State(app): State<Arc<App>>,
    Query(q): Query<WindowQuery>,
) -> ApiResult<Json<Vec<HitRatePoint>>> {
    Ok(Json(app.drills.trend(q.window).await?))
}

/// Your repertoire with one colour, from your games: the lines you play, where you leave theory, how you score.
/// Refused (409) while you play on Lichess, unless every game of yours is correspondence.
#[utoipa::path(get, path = "/api/players/you/repertoire", operation_id = "get_repertoire", tag = "players",
    params(RepertoireRequest),
    responses((status = 200, body = Repertoire), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn get_repertoire(
    State(app): State<Arc<App>>,
    Query(q): Query<RepertoireRequest>,
) -> ApiResult<Json<Repertoire>> {
    Ok(Json(app.profiles.repertoire(q).await?))
}

/// Your opening lines with one colour: the named openings you play, your score in each, and how often you go wrong
/// in the first twelve moves, with the games to drill. Refused (409) while you play on Lichess, unless every game
/// of yours is correspondence.
#[utoipa::path(get, path = "/api/players/you/prep", operation_id = "get_prep", tag = "players",
    params(PrepRequest),
    responses((status = 200, body = Prep), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn get_prep(State(app): State<Arc<App>>, Query(q): Query<PrepRequest>) -> ApiResult<Json<Prep>> {
    Ok(Json(app.profiles.prep(q).await?))
}

/// Your lines: your rating, accuracy and drill hit rate over time.
#[utoipa::path(get, path = "/api/progress", operation_id = "get_progress", tag = "players", params(WindowQuery),
    responses((status = 200, body = Progress)))]
pub async fn get_progress(State(app): State<Arc<App>>, Query(q): Query<WindowQuery>) -> ApiResult<Json<Progress>> {
    Ok(Json(app.profiles.progress(q.window).await?))
}
