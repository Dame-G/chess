//! Versions: a branch of the engine is a version, and the loop is branch, test against main, merge. This gathers what
//! the runner knows about each branch (its builds, its latest verdict, its latest tune, whether the Lichess bot plays
//! it) in one pass, and tidies: hiding a version, deleting a build's binary, deleting a branch.
//!
//! A build belongs to a branch when it was built from that name (`ref`) or is of the branch's tip commit. A deleted
//! build keeps its record, so everything that used it still reads; only its binary goes.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    api::{ApiError, AppState},
    engine::{
        build::{self, Build},
        git::{self, Branch},
    },
    experiments::ExperimentStatus,
    funnel::Kind,
    stats::{self, Estimate, Pentanomial},
    tune::TuneStatus,
};

/// The meta key holding the hidden versions, a JSON array of branch names.
const HIDDEN_KEY: &str = "versions.hidden";

/// The file whose `spsa` variant of `define!` makes a commit's tunables tunable, and what marks it.
const TUNABLES_FILE: &str = "src/parameters.rs";
const TUNABLES_MARK: &str = "feature = \"spsa\"";

/// One branch of the engine, with what has been done with it.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Version {
    /// The branch, such as `main`, `test/no-nmp` or `upstream/main`.
    pub name: String,
    /// Its tip commit.
    pub sha: String,
    pub subject: String,
    /// When the tip was committed, ISO 8601.
    pub date: String,
    /// The tip's `Bench:` line.
    pub bench: Option<u64>,
    /// Upstream's branch rather than the fork's.
    pub upstream: bool,
    /// Hidden from the Versions view; still listed, for the client to filter.
    pub hidden: bool,
    /// `main`, the baseline.
    pub is_main: bool,
    /// Its builds, newest first, deleted ones included.
    pub builds: Vec<VersionBuild>,
    /// Its newest plain (not `spsa`) build that still has its binary.
    pub latest_build_id: Option<i64>,
    /// Whether it can be tuned: its tip declares tunables the `spsa` feature exposes (`src/parameters.rs` has the
    /// feature's `define!`), or a tune of it read parameters.
    pub tunables: bool,
    /// The latest experiment with this branch as `dev`.
    pub verdict: Option<VersionVerdict>,
    /// The base ref of that experiment.
    pub tested_against: Option<String>,
    /// The latest tune of this branch (by name, or by the id of one of its builds).
    pub tune: Option<VersionTune>,
    /// The Lichess bot's `build` setting names this branch or resolves to one of its builds.
    pub on_lichess: bool,
}

/// A build as a version lists it.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct VersionBuild {
    pub id: i64,
    pub commit: String,
    pub bench: u64,
    /// Whether the bench equals the commit's `Bench:` line; absent when the commit has none.
    pub bench_ok: Option<bool>,
    pub nps: u64,
    pub created_at: String,
    /// Built with the `spsa` feature, for tuning.
    pub spsa: bool,
    /// Its binary is in the data directory. False once deleted.
    pub on_disk: bool,
    /// When its binary was deleted.
    pub deleted_at: Option<String>,
}

/// The latest experiment of a version.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct VersionVerdict {
    pub experiment: i64,
    pub name: String,
    pub status: ExperimentStatus,
    pub kind: Kind,
    /// Why it stopped, once finished.
    pub summary: Option<String>,
    /// The Elo estimate of its last match, if it played one.
    pub estimate: Option<Estimate>,
    /// Games played in its last match.
    pub games: u64,
    pub finished_at: Option<String>,
}

/// The latest tune of a version.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct VersionTune {
    pub id: i64,
    pub status: TuneStatus,
    pub iteration: u32,
    pub iterations: u32,
}

struct ExperimentRow {
    id: i64,
    name: String,
    kind: Kind,
    status: ExperimentStatus,
    base: String,
    dev: String,
    summary: Option<String>,
    finished_at: Option<String>,
    /// Its last match's totals.
    penta: Option<Pentanomial>,
}

struct TuneRow {
    id: i64,
    refname: String,
    status: TuneStatus,
    iteration: u32,
    iterations: u32,
    has_params: bool,
}

/// What the database knows, read with one query per table.
struct Records {
    builds: Vec<Build>,
    /// The latest experiment of each dev ref.
    experiments: Vec<ExperimentRow>,
    /// Newest first.
    tunes: Vec<TuneRow>,
    hidden: HashSet<String>,
}

fn enum_of<T: serde::de::DeserializeOwned>(s: String) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s)).ok()
}

fn load(c: &Connection) -> Result<Records> {
    let builds = build::list(c)?;
    let mut stmt = c.prepare(
        "SELECT e.id, e.name, e.kind, e.status, e.base_ref, e.dev_ref, e.summary, e.finished_at,
                (SELECT penta FROM tests t WHERE t.experiment_id = e.id ORDER BY t.id DESC LIMIT 1)
         FROM experiments e WHERE e.id IN (SELECT MAX(id) FROM experiments GROUP BY dev_ref)",
    )?;
    let experiments = stmt
        .query_map([], |r| {
            Ok(ExperimentRow {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: enum_of(r.get(2)?).unwrap_or(Kind::Gainer),
                status: enum_of(r.get(3)?).unwrap_or(ExperimentStatus::Failed),
                base: r.get(4)?,
                dev: r.get(5)?,
                summary: r.get(6)?,
                finished_at: r.get(7)?,
                penta: r
                    .get::<_, Option<String>>(8)?
                    .and_then(|p| serde_json::from_str::<[u64; 5]>(&p).ok())
                    .map(Pentanomial),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut stmt =
        c.prepare("SELECT id, ref, status, iteration, iterations, params <> '[]' FROM tunes ORDER BY id DESC")?;
    let tunes = stmt
        .query_map([], |r| {
            Ok(TuneRow {
                id: r.get(0)?,
                refname: r.get(1)?,
                status: enum_of(r.get(2)?).unwrap_or(TuneStatus::Failed),
                iteration: r.get(3)?,
                iterations: r.get(4)?,
                has_params: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Records { builds, experiments, tunes, hidden: hidden(c)? })
}

fn hidden(c: &Connection) -> Result<HashSet<String>> {
    let json: Option<String> =
        c.query_row("SELECT value FROM meta WHERE key = ?1", [HIDDEN_KEY], |r| r.get(0)).optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

fn set_hidden_names(c: &Connection, names: &HashSet<String>) -> Result<()> {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    c.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [HIDDEN_KEY, &serde_json::to_string(&sorted)?],
    )?;
    Ok(())
}

/// Whether a build belongs to a branch: built from its name, or of its tip commit.
fn belongs(b: &Build, branch: &Branch) -> bool {
    b.refname == branch.name || b.commit == branch.sha
}

/// Whether the Lichess bot's `build` setting is this branch. A setting that is a branch name is that branch alone;
/// an id or a commit prefix is the branch its build belongs to.
fn plays(setting: &str, builds: &[Build], branches: &[Branch], branch: &Branch) -> bool {
    if branches.iter().any(|b| b.name == setting) {
        return setting == branch.name;
    }
    let resolved = match setting.parse::<i64>() {
        Ok(id) => builds.iter().find(|b| b.id == id),
        Err(_) => {
            builds.iter().find(|b| b.deleted_at.is_none() && (b.refname == setting || b.commit.starts_with(setting)))
        }
    };
    resolved.is_some_and(|b| belongs(b, branch))
}

fn aggregate(
    branches: &[Branch],
    records: &Records,
    data: &Path,
    lichess: Option<&str>,
    tunable: &HashSet<String>,
) -> Vec<Version> {
    let experiments: HashMap<&str, &ExperimentRow> = records.experiments.iter().map(|e| (e.dev.as_str(), e)).collect();
    let by_id: HashMap<i64, &Build> = records.builds.iter().map(|b| (b.id, b)).collect();
    branches
        .iter()
        .map(|branch| {
            let mine: Vec<&Build> = records.builds.iter().filter(|b| belongs(b, branch)).collect();
            let tune = records.tunes.iter().find(|t| {
                t.refname == branch.name
                    || t.refname.parse::<i64>().ok().and_then(|id| by_id.get(&id)).is_some_and(|b| belongs(b, branch))
            });
            let tuned = records.tunes.iter().any(|t| {
                t.has_params
                    && (t.refname == branch.name
                        || t.refname
                            .parse::<i64>()
                            .ok()
                            .and_then(|id| by_id.get(&id))
                            .is_some_and(|b| belongs(b, branch)))
            });
            let experiment = experiments.get(branch.name.as_str());
            Version {
                name: branch.name.clone(),
                sha: branch.sha.clone(),
                subject: branch.subject.clone(),
                date: branch.date.clone(),
                bench: branch.bench,
                upstream: git::is_upstream(&branch.name),
                hidden: records.hidden.contains(&branch.name),
                is_main: branch.name == "main",
                latest_build_id: mine.iter().find(|b| b.deleted_at.is_none() && !build::is_spsa(b)).map(|b| b.id),
                builds: mine
                    .iter()
                    .map(|b| VersionBuild {
                        id: b.id,
                        commit: b.commit.clone(),
                        bench: b.bench,
                        bench_ok: b.bench_expected.map(|e| e == b.bench),
                        nps: b.nps,
                        created_at: b.created_at.clone(),
                        spsa: build::is_spsa(b),
                        on_disk: b.deleted_at.is_none() && build::binary(data, b).exists(),
                        deleted_at: b.deleted_at.clone(),
                    })
                    .collect(),
                tunables: tuned || tunable.contains(&branch.sha),
                verdict: experiment.map(|e| VersionVerdict {
                    experiment: e.id,
                    name: e.name.clone(),
                    status: e.status,
                    kind: e.kind,
                    summary: e.summary.clone(),
                    estimate: e.penta.as_ref().and_then(stats::estimate),
                    games: e.penta.map_or(0, |p| p.pairs() * 2),
                    finished_at: e.finished_at.clone(),
                }),
                tested_against: experiment.map(|e| e.base.clone()),
                tune: tune.map(|t| VersionTune {
                    id: t.id,
                    status: t.status,
                    iteration: t.iteration,
                    iterations: t.iterations,
                }),
                on_lichess: lichess.is_some_and(|s| plays(s, &records.builds, branches, branch)),
            }
        })
        .collect()
}

/// Every branch the runner knows, newest commit first, with its builds, verdict, tune and Lichess state.
pub async fn list(state: &AppState) -> Result<Vec<Version>> {
    let branches = git::branches(&state.config.reckless).await?;
    let shas: Vec<&str> = branches.iter().map(|b| b.sha.as_str()).collect::<HashSet<_>>().into_iter().collect();
    let tunable = git::commits_containing(&state.config.reckless, &shas, TUNABLES_FILE, TUNABLES_MARK).await;
    let records = state.store.call(|c| load(c)).await?;
    let lichess = crate::lichess::build_setting(state);
    Ok(aggregate(&branches, &records, &state.config.data, lichess.as_deref(), &tunable))
}

/// The branches, with whether each is hidden.
pub async fn branches(state: &AppState) -> Result<Vec<Branch>> {
    let mut branches = git::branches(&state.config.reckless).await?;
    let hidden = state.store.call(|c| hidden(c)).await?;
    for b in &mut branches {
        b.hidden = hidden.contains(&b.name);
    }
    Ok(branches)
}

/// Tells clients following the events that the versions changed in a way no other event shows.
fn changed(state: &AppState, data: serde_json::Value) {
    state.events.emit("versions.changed", data);
}

/// A version's hidden flag, as set.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Hidden {
    #[serde(default)]
    pub name: String,
    pub hidden: bool,
}

/// Hides a version from the Versions view, or shows it again. Hiding needs a branch the runner knows; showing does
/// not, so a stale name can always be cleared.
pub async fn set_hidden(state: &AppState, name: String, hide: bool) -> Result<Hidden, ApiError> {
    if hide && !git::branches(&state.config.reckless).await?.iter().any(|b| b.name == name) {
        return Err(ApiError::not_found(&format!("no branch {name}")));
    }
    let key = name.clone();
    state
        .store
        .call(move |c| {
            let mut names = hidden(c)?;
            if hide {
                names.insert(key)
            } else {
                names.remove(&key)
            };
            set_hidden_names(c, &names)
        })
        .await?;
    changed(state, serde_json::json!({ "reason": "hidden", "name": name, "hidden": hide }));
    Ok(Hidden { name, hidden: hide })
}

/// Whether the Lichess bot plays this build now.
async fn lichess_uses(state: &AppState, id: i64) -> Result<bool> {
    let Some(setting) = crate::lichess::build_setting(state) else { return Ok(false) };
    Ok(crate::lichess::resolve_build(state, &setting).await?.is_some_and(|b| b.id == id))
}

/// Deletes a build's binary and marks the build deleted; its record stays. 409 while something uses it. Deleting a
/// deleted build changes nothing.
pub async fn delete_build(state: &AppState, id: i64) -> Result<Build, ApiError> {
    if lichess_uses(state, id).await? {
        return Err(ApiError::conflict(&format!("build {id} is the Lichess bot's build: choose another first")));
    }
    let data = state.config.data.clone();
    let outcome = state
        .store
        .call(move |c| {
            let Some(b) = build::get(c, id)? else { return Ok(Err(ApiError::not_found("no such build"))) };
            if b.deleted_at.is_some() {
                return Ok(Ok((b, None)));
            }
            if let Some(user) = build::user(c, id)? {
                return Ok(Err(ApiError::conflict(&format!("build {id} is in use: {user}"))));
            }
            let freed = build::delete(c, &data, &b)?;
            Ok(Ok((build::get(c, id)?.expect("build exists"), Some(freed))))
        })
        .await?;
    let (b, freed) = outcome?;
    if let Some(freed) = freed {
        changed(state, serde_json::json!({ "reason": "build.deleted", "builds": [id], "freed_bytes": freed }));
    }
    Ok(b)
}

/// Which build binaries to delete.
#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct PruneRequest {
    /// Only builds made at least this many days ago. Left out, any age.
    pub older_than_days: Option<u32>,
    /// Keep the newest plain build and the newest `spsa` build of every branch. Default true.
    #[serde(default = "yes")]
    pub keep_latest_per_branch: bool,
    /// Say what would go, deleting nothing.
    #[serde(default)]
    pub dry_run: bool,
}

fn yes() -> bool {
    true
}

/// What a prune deleted.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PruneResult {
    /// The builds whose binaries were deleted (or, with `dry_run`, would be).
    pub deleted: Vec<i64>,
    /// Builds that matched but are in use (a running experiment, test, tune or match, or the Lichess bot).
    pub in_use: Vec<i64>,
    /// Bytes freed (or, with `dry_run`, that would be).
    pub freed_bytes: u64,
    pub dry_run: bool,
}

/// Deletes the binaries of builds nothing uses: old ones, keeping each branch's newest.
pub async fn prune(state: &AppState, req: PruneRequest) -> Result<PruneResult, ApiError> {
    let branches = git::branches(&state.config.reckless).await?;
    let lichess = match crate::lichess::build_setting(state) {
        Some(s) => crate::lichess::resolve_build(state, &s).await?.map(|b| b.id),
        None => None,
    };
    let data = state.config.data.clone();
    let result = state
        .store
        .call(move |c| {
            let builds: Vec<Build> = build::list(c)?.into_iter().filter(|b| b.deleted_at.is_none()).collect();
            let mut keep: HashSet<i64> = lichess.into_iter().collect();
            if req.keep_latest_per_branch {
                for branch in &branches {
                    for spsa in [false, true] {
                        let newest = builds.iter().find(|b| build::is_spsa(b) == spsa && belongs(b, branch));
                        keep.extend(newest.map(|b| b.id));
                    }
                }
            }
            let old: HashSet<i64> = {
                let mut stmt = c.prepare(
                    "SELECT id FROM builds WHERE deleted_at IS NULL
                     AND (?1 IS NULL OR julianday('now') - julianday(created_at) >= ?1)",
                )?;
                stmt.query_map([req.older_than_days], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            };
            let mut out = PruneResult { deleted: vec![], in_use: vec![], freed_bytes: 0, dry_run: req.dry_run };
            for b in builds.iter().filter(|b| old.contains(&b.id)) {
                if lichess == Some(b.id) || build::user(c, b.id)?.is_some() {
                    out.in_use.push(b.id);
                    continue;
                }
                if keep.contains(&b.id) {
                    continue;
                }
                out.freed_bytes += if req.dry_run { build::size(&data, b) } else { build::delete(c, &data, b)? };
                out.deleted.push(b.id);
            }
            Ok(out)
        })
        .await?;
    if !result.dry_run && !result.deleted.is_empty() {
        changed(
            state,
            serde_json::json!({ "reason": "build.deleted", "builds": result.deleted, "freed_bytes": result.freed_bytes }),
        );
    }
    Ok(result)
}

/// What deleting a version asks for.
#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
pub struct DeleteVersion {
    /// The branch name again, as a guard against deleting the wrong one.
    pub confirm: String,
}

/// What deleting a version did.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct DeletedVersion {
    pub name: String,
    /// It was deleted on `origin` too (false when not asked, or when `origin` did not have it).
    pub remote: bool,
    /// The builds whose binaries were deleted with it.
    pub builds: Vec<i64>,
}

/// Deletes a fork branch locally and, with `remote`, on `origin`; with `builds`, its builds' binaries too (those no
/// other branch shares). Experiments and tunes that used it keep their records.
pub async fn delete_version(
    state: &Arc<AppState>,
    name: &str,
    req: &DeleteVersion,
    remote: bool,
    with_builds: bool,
) -> Result<DeletedVersion, ApiError> {
    if req.confirm != name {
        return Err(ApiError::bad_request(&format!("confirm must repeat the branch name ({name})")));
    }
    if name == "main" {
        return Err(ApiError::bad_request("main is the baseline: it is never deleted"));
    }
    if git::is_upstream(name) {
        return Err(ApiError::bad_request("an upstream branch is upstream's: only the fork's own can be deleted"));
    }
    let repo = &state.config.reckless;
    if !git::has_branch(repo, name).await {
        return Err(ApiError::not_found(&format!("no branch {name} in the fork")));
    }
    let key = name.to_owned();
    let busy = state
        .store
        .call(move |c| {
            let found = |sql: &str| -> Result<Option<i64>> { Ok(c.query_row(sql, [&key], |r| r.get(0)).optional()?) };
            if let Some(e) = found(
                "SELECT id FROM experiments WHERE status IN ('queued', 'running') AND (dev_ref = ?1 OR base_ref = ?1)",
            )? {
                return Ok(Some(format!("experiment {e} is queued or running with it")));
            }
            if let Some(t) = found("SELECT id FROM tunes WHERE status IN ('queued', 'running') AND ref = ?1")? {
                return Ok(Some(format!("tune {t} is queued or running on it")));
            }
            Ok(None)
        })
        .await?;
    if let Some(busy) = busy {
        return Err(ApiError::bad_request(&format!("{name} is in use: {busy}")));
    }

    let branches = git::branches(repo).await?;
    let branch = branches.iter().find(|b| b.name == name).ok_or(ApiError::not_found("no such branch"))?;
    let records = state.store.call(|c| load(c)).await?;
    if let Some(setting) = crate::lichess::build_setting(state)
        && plays(&setting, &records.builds, &branches, branch)
    {
        return Err(ApiError::conflict(&format!("{name} is the Lichess bot's build: choose another first")));
    }
    if git::checked_out(repo).await.as_deref() == Some(name) {
        return Err(ApiError::conflict(&format!("{name} is checked out in {}: switch away first", repo.display())));
    }

    // Its builds, less any another branch also claims.
    let mut doomed = Vec::new();
    if with_builds {
        let others: Vec<&Branch> = branches.iter().filter(|b| b.name != name).collect();
        doomed = records
            .builds
            .iter()
            .filter(|b| b.deleted_at.is_none() && belongs(b, branch) && !others.iter().any(|o| belongs(b, o)))
            .cloned()
            .collect::<Vec<_>>();
        let ids: Vec<i64> = doomed.iter().map(|b| b.id).collect();
        let user = state
            .store
            .call(move |c| {
                for id in ids {
                    if let Some(user) = build::user(c, id)? {
                        return Ok(Some(format!("build {id}: {user}")));
                    }
                }
                Ok(None)
            })
            .await?;
        if let Some(user) = user {
            return Err(ApiError::conflict(&format!("a build of {name} is in use ({user})")));
        }
    }

    // The remote first: if it fails, nothing is gone.
    let on_origin = if remote {
        git::delete_remote_branch(repo, name).await.map_err(|e| ApiError::bad_gateway(&format!("{e:#}")))?
    } else {
        false
    };
    git::delete_branch(repo, name).await?;

    let data = state.config.data.clone();
    let key = name.to_owned();
    let deleted = state
        .store
        .call(move |c| {
            let mut deleted = Vec::new();
            for b in &doomed {
                build::delete(c, &data, b)?;
                deleted.push(b.id);
            }
            let mut names = hidden(c)?;
            if names.remove(&key) {
                set_hidden_names(c, &names)?;
            }
            Ok(deleted)
        })
        .await?;
    changed(state, serde_json::json!({ "reason": "deleted", "name": name, "remote": on_origin, "builds": deleted }));
    Ok(DeletedVersion { name: name.to_owned(), remote: on_origin, builds: deleted })
}

#[cfg(test)]
mod tests;
