//! Players: who they are, their profiles, comparisons, Lichess's leaderboard, and importing their history.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use dameless_core::{
    Error, PlayerKey, Timestamp,
    events::AssistKind,
    profile::{Comparison, Player, Profile, ProfileFilter},
    time::Speed,
    work::{Priority, Subject, Work, WorkKind},
};
use dameless_play::ImportSpec;
use dameless_store::NewWork;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiError, ApiResult, Json, Path, Query, list, parse};
use crate::App;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct PlayerPath {
    /// `you`, `dameless`, `lichess:<name>` or `stockfish:<1-8>`.
    pub key: String,
}

/// What is known of a player.
#[utoipa::path(get, path = "/api/players/{key}", operation_id = "get_player", tag = "players", params(PlayerPath),
    responses((status = 200, body = Player), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn get_player(State(app): State<Arc<App>>, Path(p): Path<PlayerPath>) -> ApiResult<Json<Player>> {
    let key = parse::<PlayerKey>("a player", &p.key)?;
    Ok(Json(app.profiles.player(&key).await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FilterQuery {
    /// Speeds, comma-separated (`blitz,rapid`); all when none.
    pub speeds: Option<String>,
    /// Only rated (true) or casual (false) games.
    pub rated: Option<bool>,
    /// Only games from this time on (ISO-8601, or a date).
    pub since: Option<String>,
    /// The newest games, at most (default 200); a number above how many there are means all of them.
    pub limit: Option<u32>,
}

impl FilterQuery {
    fn filter(&self) -> ApiResult<ProfileFilter> {
        Ok(ProfileFilter {
            speeds: list(self.speeds.as_deref(), "a speed", Speed::parse)?,
            rated: self.rated,
            since: self
                .since
                .as_deref()
                .map(|s| {
                    Timestamp::parse(s).ok_or_else(|| ApiError(Error::bad_request(format!("{s:?} is not a time"))))
                })
                .transpose()?,
            limit: self.limit.unwrap_or(dameless_core::profile::DEFAULT_LIMIT),
        })
    }
}

/// A player's games added up: results over every finished game, engine figures over the reviewed ones, weak spots
/// for you.
#[utoipa::path(get, path = "/api/players/{key}/profile", operation_id = "get_profile", tag = "players",
    params(PlayerPath, FilterQuery), responses((status = 200, body = Profile), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn get_profile(
    State(app): State<Arc<App>>,
    Path(p): Path<PlayerPath>,
    Query(q): Query<FilterQuery>,
) -> ApiResult<Json<Profile>> {
    let key = parse::<PlayerKey>("a player", &p.key)?;
    let mut profile = app.profiles.profile(key.clone(), q.filter()?).await?;
    profile.unreviewed_total = app.reviews.unreviewed_count(&key).await.ok();
    if key == PlayerKey::You {
        profile.history_complete =
            Some(app.hub.store.meta::<bool>(dameless_play::import::COMPLETE_KEY).await?.unwrap_or(false));
    }
    Ok(Json(profile))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompareQuery {
    /// Two to eight players, comma-separated: `you,dameless,lichess:cutemouse83`.
    pub players: String,
    pub speeds: Option<String>,
    pub rated: Option<bool>,
    pub since: Option<String>,
    pub limit: Option<u32>,
}

/// Profiles side by side, with who leads each row.
#[utoipa::path(get, path = "/api/compare", operation_id = "compare_players", tag = "players", params(CompareQuery),
    responses((status = 200, body = Comparison), (status = 400, body = dameless_core::error::ErrorBody)))]
pub async fn compare_players(
    State(app): State<Arc<App>>,
    Query(q): Query<CompareQuery>,
) -> ApiResult<Json<Comparison>> {
    let keys = list(Some(&q.players), "a player", |s| s.parse::<PlayerKey>().ok())?;
    let filter = FilterQuery { speeds: q.speeds, rated: q.rated, since: q.since, limit: q.limit }.filter()?;
    Ok(Json(app.profiles.compare(keys, filter).await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TopQuery {
    /// `bullet`, `blitz` (default), `rapid`, `classical` or `ultra_bullet`.
    pub speed: Option<String>,
    /// How many, 1 to 200 (default 10).
    pub nb: Option<u32>,
}

/// One row of Lichess's leaderboard.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LeaderboardEntry {
    /// The player's key: `lichess:<id>`.
    pub key: PlayerKey,
    pub username: String,
    pub title: Option<String>,
    pub rating: Option<u32>,
    /// Rating change over the last games.
    pub progress: Option<i32>,
}

/// Lichess's leaderboard at a speed.
#[utoipa::path(get, path = "/api/players/top", operation_id = "top_players", tag = "players", params(TopQuery),
    responses((status = 200, body = Vec<LeaderboardEntry>), (status = 400, body = dameless_core::error::ErrorBody), (status = 502, body = dameless_core::error::ErrorBody)))]
pub async fn top_players(
    State(app): State<Arc<App>>,
    Query(q): Query<TopQuery>,
) -> ApiResult<Json<Vec<LeaderboardEntry>>> {
    let speed = match q.speed.as_deref() {
        None => Speed::Blitz,
        Some(s) => Speed::parse(s).ok_or_else(|| ApiError(Error::bad_request(format!("{s:?} is not a speed"))))?,
    };
    let perf = speed.lichess_key();
    let users = app.connections.leaderboard(perf, q.nb.unwrap_or(10).clamp(1, 200)).await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| {
                let p = u.perfs.get(perf);
                LeaderboardEntry {
                    key: PlayerKey::Lichess(u.id.to_lowercase()),
                    username: if u.username.is_empty() { u.id.clone() } else { u.username.clone() },
                    title: u.title.clone(),
                    rating: p.and_then(|p| p.rating),
                    progress: p.and_then(|p| p.prog),
                }
            })
            .collect(),
    ))
}

/// `POST /api/players/{key}/import`.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct ImportBody {
    /// At most this many games, newest first; all when none.
    #[serde(default)]
    pub max: Option<u32>,
    /// Games started at or after this time (ms since 1970). Your history, when none (and no `max`, `speeds` or
    /// `rated`): the games newer than the newest imported before and, until your whole history is here, every older
    /// one.
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub speeds: Vec<Speed>,
    #[serde(default)]
    pub rated: Option<bool>,
}

/// Imports a player's finished games from Lichess (work, network only): `you` (your history, needs your login) or
/// `lichess:<name>` (theirs; refused while you play on Lichess).
#[utoipa::path(post, path = "/api/players/{key}/import", operation_id = "import_player", tag = "players",
    params(PlayerPath), request_body(content = Option<ImportBody>),
    responses((status = 202, body = Work), (status = 400, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn import_player(
    State(app): State<Arc<App>>,
    Path(p): Path<PlayerPath>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<Work>)> {
    let key = parse::<PlayerKey>("a player", &p.key)?;
    let b: ImportBody = super::optional_body(&body)?;
    let title = match &key {
        PlayerKey::You => "Importing your Lichess games".to_owned(),
        PlayerKey::Lichess(id) => {
            app.hub.policy.check(AssistKind::PlayerImport)?;
            format!("Importing {id}'s Lichess games")
        }
        other => return Err(ApiError(Error::bad_request(format!("{other} has no Lichess history to import")))),
    };
    let spec = ImportSpec { player: key.clone(), max: b.max, since: b.since, speeds: b.speeds, rated: b.rated };
    let work = app
        .scheduler
        .submit(NewWork {
            kind: WorkKind::Import,
            priority: Priority::Interactive,
            title,
            subject: Subject::Player { key },
            spec: serde_json::to_value(spec).map_err(|e| Error::internal(e.to_string()))?,
        })
        .await?;
    Ok((StatusCode::ACCEPTED, Json(work)))
}
