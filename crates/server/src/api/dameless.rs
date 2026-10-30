//! Dameless the engine: its logbook (champion, line, ideas, the bot, strength), versions and their builds, the
//! tunables a branch declares, and pushing the fork.

use std::{path::Path as FsPath, sync::Arc};

use axum::{extract::State, http::StatusCode};
use dameless_core::{
    BuildId, ConnectionKey, Error, VersionId,
    ideas::{Idea, IdeaStatus},
    lichess::BotStatus,
    progress::{ProgressPoint, dameless_series},
    regression::RegressionCheck,
    spsa::{self, Tunable},
    upstream::{UpstreamSettings, UpstreamStatus, UpstreamSync},
    version::{Build, Version},
    work::Work,
};
use dameless_store::IdeaFilter;
use dameless_work::{PruneRequest, VersionRef};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::{ApiError, ApiResult, Json, Path, Query};
use crate::App;

/// Ideas by where they are.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct IdeaLists {
    pub running: Vec<Idea>,
    pub queued: Vec<Idea>,
    /// Ended ideas, newest first (at most 20).
    pub recent: Vec<Idea>,
}

/// Dameless's profile: `GET /api/dameless`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct DamelessView {
    pub champion: Version,
    /// The champion line, newest first. A champion taken from upstream carries `upstream`: the commits it brought.
    pub line: Vec<Version>,
    /// The last sync with upstream Reckless, whatever its outcome (a failure says why, having changed nothing).
    pub upstream: Option<UpstreamSync>,
    pub ideas: IdeaLists,
    /// The bot, when its Lichess account is connected.
    pub connection: Option<BotStatus>,
    /// Each champion version and its cumulative Elo over `v1` (each promotion's deciding estimate added up, the
    /// errors in quadrature), oldest first. A champion a regression check measured is placed by that check instead
    /// (`measured`), and those after it add on from there.
    pub strength: Vec<ProgressPoint>,
    /// Finished regression checks (after every fifth promotion, the champion against the champion five promotions
    /// earlier), newest first. One that did not pass says the champion is measurably worse; nothing was reverted.
    pub regressions: Vec<RegressionCheck>,
}

/// Dameless's logbook: champion, line, ideas, the bot and its strength over versions.
#[utoipa::path(get, path = "/api/dameless", operation_id = "get_dameless", tag = "dameless",
    responses((status = 200, body = DamelessView)))]
pub async fn get_dameless(State(app): State<Arc<App>>) -> ApiResult<Json<DamelessView>> {
    let champion = app.versions.champion().await?;
    let line = app.versions.list(Some("champion")).await?;
    let ideas = |statuses: Vec<IdeaStatus>, limit: u32| {
        let app = app.clone();
        async move { app.ideas.list(IdeaFilter { statuses, tag: None, limit }).await }
    };
    let ended = vec![IdeaStatus::Kept, IdeaStatus::Dropped, IdeaStatus::Stopped, IdeaStatus::Failed];
    let ideas = IdeaLists {
        running: ideas(vec![IdeaStatus::Running], 0).await?,
        queued: ideas(vec![IdeaStatus::Queued], 0).await?,
        recent: ideas(ended, 20).await?,
    };
    let bot = app.connections.status(ConnectionKey::Bot);
    let connection = if bot.configured { bot.bot } else { None };
    let regressions = app.regression.checks().await?;
    let strength = dameless_series(&line, &regressions, None).points;
    let upstream = app.upstream.status().await?.last;
    Ok(Json(DamelessView { champion, line, upstream, ideas, connection, strength, regressions }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct VersionsQuery {
    /// One line: `champion`, `idea`, `upstream` or `other`.
    pub line: Option<String>,
}

/// Versions, newest first.
#[utoipa::path(get, path = "/api/versions", operation_id = "list_versions", tag = "dameless", params(VersionsQuery),
    responses((status = 200, body = Vec<Version>)))]
pub async fn list_versions(
    State(app): State<Arc<App>>,
    Query(q): Query<VersionsQuery>,
) -> ApiResult<Json<Vec<Version>>> {
    Ok(Json(app.versions.list(q.line.as_deref().filter(|l| !l.trim().is_empty())).await?))
}

/// `POST /api/versions`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct MakeVersion {
    /// A branch, tag or commit of the fork (or `champion`, `v14`, `#12`).
    #[serde(rename = "ref")]
    pub reference: String,
}

/// Makes a version of a branch, tag or commit and queues its build (see `GET /api/work`). Answers the version.
#[utoipa::path(post, path = "/api/versions", operation_id = "make_version", tag = "dameless", request_body = MakeVersion,
    responses((status = 201, body = Version), (status = 404, body = dameless_core::error::ErrorBody)))]
pub async fn make_version(
    State(app): State<Arc<App>>,
    Json(b): Json<MakeVersion>,
) -> ApiResult<(StatusCode, Json<Version>)> {
    let version = app.versions.resolve(&VersionRef::parse(&b.reference)).await?;
    let (versions, v) = (app.versions.clone(), version.clone());
    tokio::spawn(async move {
        if let Err(e) = versions.binary_of(&v, &[]).await {
            tracing::warn!("building {}: {e}", v.name);
        }
    });
    Ok((StatusCode::CREATED, Json(version)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct VersionPath {
    pub id: i64,
}

/// A version's builds, newest first (diagnostics).
#[utoipa::path(get, path = "/api/versions/{id}/builds", operation_id = "list_builds", tag = "dameless", params(VersionPath),
    responses((status = 200, body = Vec<Build>)))]
pub async fn list_builds(State(app): State<Arc<App>>, Path(p): Path<VersionPath>) -> ApiResult<Json<Vec<Build>>> {
    Ok(Json(app.versions.builds(VersionId(p.id)).await?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct BuildPath {
    pub id: i64,
}

/// Deletes a build's binary; its record stays for what used it. 409 while something uses it.
#[utoipa::path(delete, path = "/api/builds/{id}", operation_id = "delete_build", tag = "dameless", params(BuildPath),
    responses((status = 200, body = Build), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn delete_build(State(app): State<Arc<App>>, Path(p): Path<BuildPath>) -> ApiResult<Json<Build>> {
    Ok(Json(app.versions.delete_build(BuildId(p.id)).await?))
}

/// `POST /api/builds/prune`.
#[derive(Default, Serialize, Deserialize, ToSchema)]
pub struct PruneBody {
    /// Only binaries at least this old.
    #[serde(default)]
    pub older_than_days: Option<u32>,
    /// Keep every version's newest build of each feature set (default true).
    #[serde(default)]
    pub keep_latest: Option<bool>,
    /// Say what would go, delete nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// What pruning removed (or would have).
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Pruned {
    /// The builds whose binaries were deleted (or would be).
    pub builds: Vec<BuildId>,
    /// Bytes freed (or that would be).
    pub bytes: u64,
    /// Builds left alone because something uses them.
    pub in_use: Vec<BuildId>,
    pub dry_run: bool,
}

/// Deletes the binaries nothing uses: every version's older builds, or all of them with `keep_latest: false`.
#[utoipa::path(post, path = "/api/builds/prune", operation_id = "prune_builds", tag = "dameless", request_body = PruneBody,
    responses((status = 200, body = Pruned)))]
pub async fn prune_builds(State(app): State<Arc<App>>, Json(b): Json<PruneBody>) -> ApiResult<Json<Pruned>> {
    let r = app
        .versions
        .prune(PruneRequest {
            older_than_days: b.older_than_days,
            keep_latest: b.keep_latest.unwrap_or(true),
            dry_run: b.dry_run,
        })
        .await?;
    Ok(Json(Pruned { builds: r.deleted, bytes: r.freed_bytes, in_use: r.in_use, dry_run: r.dry_run }))
}

/// What a push did.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Pushed {
    pub remote: String,
    /// The commit `main` is at, there and here.
    pub head: String,
}

/// Pushes the fork's `main` to `origin`. Only ever on this explicit request (the UI confirms first).
#[utoipa::path(post, path = "/api/dameless/push", operation_id = "push", tag = "dameless",
    responses((status = 200, body = Pushed), (status = 502, body = dameless_core::error::ErrorBody)))]
pub async fn push(State(app): State<Arc<App>>) -> ApiResult<Json<Pushed>> {
    dameless_work::push(&app.hub).await?;
    let head = app.versions.repo().resolve(dameless_work::versions::MAIN).await.map(|c| c.sha).unwrap_or_default();
    Ok(Json(Pushed { remote: "origin".into(), head }))
}

/// Taking upstream Reckless: the automatic sync's setting, the last sync (merged, up to date or failed, with the
/// upstream commits it brought), one running, and when the next automatic one is due.
#[utoipa::path(get, path = "/api/upstream", operation_id = "upstream_status", tag = "dameless",
    responses((status = 200, body = UpstreamStatus)))]
pub async fn upstream_status(State(app): State<Arc<App>>) -> ApiResult<Json<UpstreamStatus>> {
    Ok(Json(app.upstream.status().await?))
}

/// Turns the automatic sync with upstream on or off (on by default: every six hours).
#[utoipa::path(put, path = "/api/upstream", operation_id = "set_upstream_settings", tag = "dameless",
    request_body = UpstreamSettings, responses((status = 200, body = UpstreamStatus)))]
pub async fn set_upstream_settings(
    State(app): State<Arc<App>>,
    Json(b): Json<UpstreamSettings>,
) -> ApiResult<Json<UpstreamStatus>> {
    Ok(Json(app.upstream.set_settings(b).await?))
}

/// Takes upstream Reckless now: fetches the fork's `upstream` remote and, when `upstream/main` has commits the champion
/// lacks, merges them into the fork's `main` locally (fast-forward, or a merge commit with its measured `Bench:` line),
/// builds and benches the result and makes it the next champion. No trials: upstream's commits are tested on its own
/// fleet. A conflict, a failed build or bench, or a dirty checkout of `main` changes nothing. Never pushes. Answers the
/// work; 409 while a sync runs or without an `upstream` remote.
#[utoipa::path(post, path = "/api/upstream/sync", operation_id = "sync_upstream", tag = "dameless",
    responses((status = 201, body = Work), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn sync_upstream(State(app): State<Arc<App>>) -> ApiResult<(StatusCode, Json<Work>)> {
    Ok((StatusCode::CREATED, Json(app.upstream.sync().await?)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TunablesQuery {
    /// The branch (or any version) whose tunables to list.
    pub branch: String,
}

/// The tunables a branch declares (`src/parameters.rs`), read from its `spsa` build. When that build is not there
/// yet, it is queued and the answer is 409: ask again once it is done (`GET /api/work`).
#[utoipa::path(get, path = "/api/tunables", operation_id = "list_tunables", tag = "dameless", params(TunablesQuery),
    responses((status = 200, body = Vec<Tunable>), (status = 404, body = dameless_core::error::ErrorBody), (status = 409, body = dameless_core::error::ErrorBody)))]
pub async fn list_tunables(
    State(app): State<Arc<App>>,
    Query(q): Query<TunablesQuery>,
) -> ApiResult<Json<Vec<Tunable>>> {
    let version = app.versions.resolve(&VersionRef::parse(&q.branch)).await?;
    Ok(Json(tunables_of(&app, &version, true).await?))
}

/// A version's declared tunables from its ready `spsa` build. Without one: queued (when `build`) and a 409.
pub(crate) async fn tunables_of(app: &Arc<App>, version: &Version, build: bool) -> ApiResult<Vec<Tunable>> {
    let features = vec!["spsa".to_owned()];
    match app.hub.store.cached_build(&version.commit, &features).await? {
        Some(b) if FsPath::new(&b.path).is_file() => {
            let out = app
                .versions
                .toolchain()
                .spsa(FsPath::new(&b.path))
                .await
                .map_err(|e| Error::internal(format!("read the tunables of {}: {e:#}", version.name)))?;
            Ok(spsa::parse_openbench(&out))
        }
        _ => {
            if build {
                let (versions, v) = (app.versions.clone(), version.clone());
                tokio::spawn(async move {
                    if let Err(e) = versions.binary_of(&v, &["spsa"]).await {
                        tracing::warn!("building {} with its tunables: {e}", v.name);
                    }
                });
            }
            Err(ApiError(Error::conflict(format!(
                "{} is being built with its tunables; ask again when that work is done",
                version.name
            ))))
        }
    }
}
