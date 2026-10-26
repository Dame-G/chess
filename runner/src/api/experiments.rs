use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{ApiError, AppState};
use crate::experiments::{
    self, Experiment, NewExperiment,
    tests::{self, Point, Test},
};

#[derive(Deserialize, IntoParams)]
pub struct ListQuery {
    /// Only the experiments of this batch.
    batch: Option<String>,
    /// How many, newest first. Default 100.
    limit: Option<i64>,
}

/// Experiments, newest first.
#[utoipa::path(get, path = "/api/experiments", operation_id = "list_experiments", tag = "experiments", params(ListQuery), responses((status = 200, body = Vec<Experiment>)))]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Experiment>>, ApiError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    Ok(Json(state.store.call(move |c| experiments::list(c, q.batch.as_deref(), limit)).await?))
}

/// Queues an experiment: a dev ref against a base ref, through the default funnel for its kind, the given stages,
/// or a single match (`preset`).
#[utoipa::path(post, path = "/api/experiments", operation_id = "create_experiment", tag = "experiments", request_body = NewExperiment, responses((status = 202, body = Experiment), (status = 400)))]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(new): Json<NewExperiment>,
) -> Result<(StatusCode, Json<Experiment>), ApiError> {
    let new = vec![new];
    check(&new)?;
    let mut created = experiments::submit(&state, new, None).await?;
    Ok((StatusCode::ACCEPTED, Json(created.remove(0))))
}

#[derive(Deserialize, ToSchema)]
pub struct NewBatch {
    /// Names the batch; list it with `?batch=`.
    pub name: String,
    pub experiments: Vec<NewExperiment>,
}

/// Queues several experiments under one name. They run in order.
#[utoipa::path(post, path = "/api/batches", operation_id = "create_batch", tag = "experiments", request_body = NewBatch, responses((status = 202, body = Vec<Experiment>), (status = 400)))]
pub async fn batch(
    State(state): State<Arc<AppState>>,
    Json(b): Json<NewBatch>,
) -> Result<(StatusCode, Json<Vec<Experiment>>), ApiError> {
    check(&b.experiments)?;
    Ok((StatusCode::ACCEPTED, Json(experiments::submit(&state, b.experiments, Some(b.name)).await?)))
}

fn check(new: &[NewExperiment]) -> Result<(), ApiError> {
    match new.iter().any(|n| n.funnel().is_empty()) {
        true => Err(ApiError::bad_request(experiments::NO_STAGES)),
        false => Ok(()),
    }
}

#[derive(Serialize, ToSchema)]
pub struct ExperimentDetail {
    pub experiment: Experiment,
    /// Its matches, in the order they ran.
    pub tests: Vec<Test>,
}

/// One experiment with its matches.
#[utoipa::path(get, path = "/api/experiments/{id}", operation_id = "get_experiment", tag = "experiments", responses((status = 200, body = ExperimentDetail), (status = 404)))]
pub async fn get(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<ExperimentDetail>, ApiError> {
    let detail = state
        .store
        .call(move |c| {
            let Some(experiment) = experiments::get(c, id)? else { return Ok(None) };
            Ok(Some(ExperimentDetail { experiment, tests: tests::for_experiment(c, id)? }))
        })
        .await?;
    detail.map(Json).ok_or(ApiError::not_found("no such experiment"))
}

/// Cancels a queued experiment or stops a running one.
#[utoipa::path(delete, path = "/api/experiments/{id}", operation_id = "cancel_experiment", tag = "experiments", responses((status = 200, body = Experiment), (status = 404)))]
pub async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Experiment>, ApiError> {
    experiments::cancel(&state, id).await?.map(Json).ok_or(ApiError::not_found("no such experiment"))
}

#[derive(Serialize, ToSchema)]
pub struct TestDetail {
    pub test: Test,
    /// Progress over the match: LLR and nElo by games played.
    pub points: Vec<Point>,
}

/// One match with its progress.
#[utoipa::path(get, path = "/api/tests/{id}", operation_id = "get_test", tag = "experiments", responses((status = 200, body = TestDetail), (status = 404)))]
pub async fn test(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<TestDetail>, ApiError> {
    let detail = state
        .store
        .call(move |c| {
            let Some(test) = tests::get(c, id)? else { return Ok(None) };
            Ok(Some(TestDetail { test, points: tests::points(c, id)? }))
        })
        .await?;
    detail.map(Json).ok_or(ApiError::not_found("no such test"))
}

#[derive(Serialize, ToSchema)]
pub struct GameSummary {
    /// Position in the match's PGN, from 0.
    pub index: usize,
    pub white: String,
    pub black: String,
    pub result: String,
    pub termination: String,
    pub plies: u32,
}

/// A match's games so far.
#[utoipa::path(get, path = "/api/tests/{id}/games", operation_id = "list_games", tag = "experiments", responses((status = 200, body = Vec<GameSummary>)))]
pub async fn games(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<GameSummary>>, ApiError> {
    let pgn = tokio::fs::read_to_string(tests::dir(&state, id).join("games.pgn")).await.unwrap_or_default();
    Ok(Json(split(&pgn).into_iter().enumerate().map(|(index, game)| summary(index, game)).collect()))
}

/// One game as PGN, with each move's evaluation, depth, nodes and time in its comment.
#[utoipa::path(get, path = "/api/tests/{id}/games/{index}", operation_id = "get_game", tag = "experiments", responses((status = 200, content_type = "text/plain", body = String), (status = 404)))]
pub async fn game(
    State(state): State<Arc<AppState>>,
    Path((id, index)): Path<(i64, usize)>,
) -> Result<String, ApiError> {
    let pgn = tokio::fs::read_to_string(tests::dir(&state, id).join("games.pgn")).await.unwrap_or_default();
    split(&pgn).get(index).map(|g| (*g).to_owned()).ok_or(ApiError::not_found("no such game"))
}

/// The games of a PGN file: each starts at an `[Event` tag at the start of a line.
fn split(pgn: &str) -> Vec<&str> {
    let mut starts: Vec<usize> =
        pgn.match_indices("[Event ").map(|(i, _)| i).filter(|&i| i == 0 || pgn.as_bytes()[i - 1] == b'\n').collect();
    starts.push(pgn.len());
    starts.windows(2).map(|w| pgn[w[0]..w[1]].trim_end()).collect()
}

fn summary(index: usize, game: &str) -> GameSummary {
    let tag = |name: &str| {
        let prefix = format!("[{name} \"");
        game.lines().find_map(|l| l.strip_prefix(&prefix)?.strip_suffix("\"]")).unwrap_or_default().to_owned()
    };
    GameSummary {
        index,
        white: tag("White"),
        black: tag("Black"),
        result: tag("Result"),
        termination: tag("Termination"),
        plies: tag("PlyCount").parse().unwrap_or(0),
    }
}

#[cfg(test)]
mod pgn_tests {
    use super::*;

    #[test]
    fn splits_a_pgn_into_games() {
        let pgn = "[Event \"a\"]\n[White \"dev\"]\n[Result \"1-0\"]\n\n1. e4 1-0\n\n[Event \"b\"]\n[White \"base\"]\n[PlyCount \"3\"]\n\n1. d4 *\n";
        let games = split(pgn);
        assert_eq!(games.len(), 2);
        assert!(games[1].starts_with("[Event \"b\"]"));
        let s = summary(1, games[1]);
        assert_eq!((s.white.as_str(), s.plies), ("base", 3));
        assert!(split("").is_empty());
    }
}
