//! Game review, comparing players, and other Lichess players.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use super::{ApiError, AppState};
use crate::{
    jobs::Job,
    lichess::players::{self, PlayerImportRequest, PlayerProfile, TopPlayer},
    review::{
        self, Review, ReviewRequest, Who,
        compare::{self, Comparison},
    },
};

/// Queues a review: every position of each game searched by a build (default: the newest build of `main`) at a node
/// budget (default 200,000), MultiPV 2, on several one-thread engines, and each move judged. Give `matches`, or
/// `account` (`me`, `bot`) or `player` (an imported Lichess player) for their newest finished games not yet reviewed
/// with that build and budget. A game already reviewed so is skipped. Progress comes as `review.progress` events and
/// the job's log; the result is `{ reviewed, skipped, failed, positions_searched, positions_cached, build, nodes }`.
/// 409 while you play on Lichess (a running review pauses then), or with no build to review with.
#[utoipa::path(post, path = "/api/reviews", operation_id = "create_review", tag = "reviews", request_body = ReviewRequest, responses((status = 202, body = Job), (status = 400), (status = 404), (status = 409)))]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReviewRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    Ok((StatusCode::ACCEPTED, Json(review::submit(&state, req).await?)))
}

#[derive(Deserialize, IntoParams)]
pub struct ReviewQuery {
    /// The review made with this build; the newest review when absent.
    build: Option<i64>,
    /// The review made at this node budget.
    nodes: Option<u64>,
}

/// A game's review: per side accuracy, ACPL, classification counts, phases and time trouble, and every move with its
/// evaluations, best move, win percentages, accuracy and classification. 404 when the game has no review (yet).
#[utoipa::path(get, path = "/api/matches/{id}/review", operation_id = "get_match_review", tag = "reviews", params(ReviewQuery), responses((status = 200, body = Review), (status = 404)))]
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<Review>, ApiError> {
    review::get(&state, id, q.build, q.nodes).await?.map(Json).ok_or(ApiError::not_found("this game has no review"))
}

#[derive(Deserialize, IntoParams)]
pub struct CompareQuery {
    /// Comma-separated: `me` (your account), `bot`, or Lichess usernames whose games were imported.
    players: String,
    /// Comma-separated Lichess speeds; all when absent.
    speeds: Option<String>,
    /// Only rated (true) or casual (false) games.
    rated: Option<bool>,
    /// Only games played since then: an ISO date or time (`2026-01-01`), or milliseconds since 1970.
    since: Option<String>,
    /// The newest games per player, at most. Default 200, at most 5000.
    limit: Option<u32>,
}

fn list(s: &Option<String>) -> Vec<String> {
    s.as_deref().unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

/// Players side by side over their finished games: results (overall and by colour), average accuracy (overall, by
/// phase, by speed), ACPL, classifications per game, opponents' average and performance rating, top openings, time
/// per move by phase and time-trouble blunders, longest win streak, and notable moments with match id and ply. Engine
/// figures count reviewed games only; `pending_review` says how many are not reviewed.
#[utoipa::path(get, path = "/api/compare", operation_id = "compare_players", tag = "reviews", params(CompareQuery), responses((status = 200, body = Comparison), (status = 400)))]
pub async fn compare_players(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<Comparison>, ApiError> {
    let names = list(&Some(q.players.clone()));
    if names.is_empty() || names.len() > 8 {
        return Err(ApiError::bad_request("players: one to eight of me, bot or Lichess usernames"));
    }
    let mut who = Vec::new();
    for n in &names {
        let w = Who::parse(n);
        if let Who::Player(p) = &w {
            players::check_username(p)?;
        }
        who.push(w);
    }
    let speeds = list(&q.speeds);
    review::check_speeds(&speeds)?;
    let since = match q.since.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(s) if s.chars().all(|c| c.is_ascii_digit()) => {
            Some(crate::lichess::quota::iso(s.parse::<i64>().map_err(|_| ApiError::bad_request("since"))?))
        }
        Some(s) if s.len() >= 10 && s.as_bytes()[4] == b'-' => Some(s.to_owned()),
        Some(_) => return Err(ApiError::bad_request("since is an ISO date (2026-01-01) or milliseconds")),
    };
    let limit = q.limit.unwrap_or(200).clamp(1, 5000);
    let (rated, s2, since2) = (q.rated, speeds.clone(), since.clone());
    let players = state
        .store
        .call(move |c| {
            who.iter()
                .map(|w| {
                    let games = compare::games_of(c, w, &s2, rated, since2.as_deref(), limit)?;
                    Ok(compare::aggregate(&w.key(), &games))
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .await?;
    Ok(Json(Comparison { players, speeds, rated: q.rated, since, limit }))
}

#[derive(Deserialize, IntoParams)]
pub struct TopQuery {
    /// `ultraBullet`, `bullet`, `blitz`, `rapid` or `classical`. Default `blitz`.
    perf: Option<String>,
    /// How many, at most 200. Default 10.
    nb: Option<u32>,
}

/// Lichess's leaderboard at a speed, cached ten minutes.
#[utoipa::path(get, path = "/api/lichess/players/top", operation_id = "lichess_top_players", tag = "lichess-players", params(TopQuery), responses((status = 200, body = Vec<TopPlayer>), (status = 400), (status = 429), (status = 502)))]
pub async fn top(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TopQuery>,
) -> Result<Json<Vec<TopPlayer>>, ApiError> {
    Ok(Json(players::top(&state, q.perf.as_deref().unwrap_or("blitz"), q.nb.unwrap_or(10)).await?))
}

/// A Lichess player's profile (ratings, title, game counts; cached ten minutes) and how many of their games are
/// imported and reviewed here. 404 when Lichess has no such player.
#[utoipa::path(get, path = "/api/lichess/players/{username}", operation_id = "lichess_player", tag = "lichess-players", responses((status = 200, body = PlayerProfile), (status = 400), (status = 404), (status = 429), (status = 502)))]
pub async fn profile(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> Result<Json<PlayerProfile>, ApiError> {
    Ok(Json(players::profile(&state, &username).await?))
}

/// Imports a Lichess player's newest public games (default 50) as matches with `account: "player"` and `player`
/// (`GET /api/matches?player=<name>`), each Lichess game once. A job beside the queue; its result is
/// `{ player, imported, skipped, other }`. 409 while you play on Lichess.
#[utoipa::path(post, path = "/api/lichess/players/{username}/import", operation_id = "import_lichess_player_games", tag = "lichess-players", request_body = PlayerImportRequest, responses((status = 202, body = Job), (status = 400), (status = 409)))]
pub async fn import(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
    Json(req): Json<PlayerImportRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    Ok((StatusCode::ACCEPTED, Json(players::start_import(&state, &username, req).await?)))
}
