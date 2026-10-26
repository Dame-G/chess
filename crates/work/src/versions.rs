//! Versions and builds: turning a branch, tag or commit of the fork into a version, and a version into a binary.
//!
//! The fork's `main` is the **champion line**. Its tip is the champion; versions on the line are numbered `v1`, `v2`, …
//! in order of promotion. On first use the current tip of `main` becomes `v1`, and if `main` moved without a promotion
//! here (a commit made in the checkout), its new tip becomes the next number, so the champion is always `main`'s tip.
//!
//! Builds are a cache keyed by commit and cargo features. [`Versions::binary`] returns a ready binary, or queues a
//! `Build` work item and waits for it: the [`BuildExecutor`] checks the commit out in the build worktree under the data
//! directory (never the user's checkout), builds, benches once, and compares the bench with the commit's `Bench:` line.
//! A build that disagrees is suspect: it is kept, the log says so, and the fingerprint stage of a trial fails on it.
//!
//! A build in use (a live game's engine, a running trial, the bot's version) cannot be deleted. Whoever uses one holds
//! an [`InUseGuard`] from [`InUse::hold`] for as long as it does, or registers a check with [`InUse::add_check`].

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use dameless_core::{
    BuildId, Error, IdeaId, VersionId,
    events::Payload,
    training::NetId,
    version::{Build, BuildStatus, Estimate, Line, Version},
    work::{Priority, Subject, WorkKind, WorkStatus},
};
use dameless_engine::{
    build::BuildSpec,
    git::{self, Repo},
};
use dameless_hub::{Executor, Hub, WorkContext};
use dameless_store::{NewBuild, NewVersion, NewWork};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    scheduler::Scheduler,
    toolchain::{Reckless, Toolchain},
};

/// The fork's champion branch.
pub const MAIN: &str = "main";

/// Which version is meant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum VersionRef {
    /// The current champion: the tip of the fork's `main`.
    Champion,
    Id(VersionId),
    /// A version's name (`v14`, `deeper-lmr`), or a branch, tag or commit, which becomes a version on first use.
    Name(String),
}

impl VersionRef {
    /// From what people type: `champion` (or `main`), `#12` for an id, anything else a name.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.is_empty() || text == "champion" || text == MAIN {
            Self::Champion
        } else if let Some(id) = text.strip_prefix('#').and_then(|n| n.parse().ok()) {
            Self::Id(VersionId(id))
        } else {
            Self::Name(text.to_owned())
        }
    }
}

/// What a `Build` work item builds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildWork {
    pub version: VersionId,
    pub commit: String,
    pub features: Vec<String>,
    /// The net to embed; none for the commit's own.
    #[serde(default)]
    pub net: Option<NetId>,
}

/// What pruning removes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PruneRequest {
    /// Only binaries at least this old.
    pub older_than_days: Option<u32>,
    /// Keep every version's newest ready build of each feature set (the default).
    #[serde(default = "yes")]
    pub keep_latest: bool,
    /// Say what would go, delete nothing.
    #[serde(default)]
    pub dry_run: bool,
}

fn yes() -> bool {
    true
}

/// What pruning removed (or would have).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PruneReport {
    pub deleted: Vec<BuildId>,
    /// Builds skipped because something uses them.
    pub in_use: Vec<BuildId>,
    pub freed_bytes: u64,
    pub dry_run: bool,
}

/// Who uses which builds now. Holding a guard keeps a build from being deleted.
#[derive(Clone, Default)]
pub struct InUse {
    inner: Arc<InUseInner>,
}

type Check = Arc<dyn Fn(BuildId) -> Option<String> + Send + Sync>;

#[derive(Default)]
struct InUseInner {
    next: AtomicU64,
    holds: Mutex<HashMap<u64, (BuildId, String)>>,
    checks: Mutex<Vec<Check>>,
}

impl InUse {
    /// Marks `build` as used by `who` ("game:12", "trial:3") until the guard drops.
    pub fn hold(&self, build: BuildId, who: impl Into<String>) -> InUseGuard {
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        self.inner.holds.lock().unwrap().insert(id, (build, who.into()));
        InUseGuard { inner: self.inner.clone(), id }
    }

    /// Adds a check asked on every delete: it names the user of a build, or says none (the bot's version setting).
    pub fn add_check(&self, check: impl Fn(BuildId) -> Option<String> + Send + Sync + 'static) {
        self.inner.checks.lock().unwrap().push(Arc::new(check));
    }

    /// Who uses a build now.
    pub fn users(&self, build: BuildId) -> Vec<String> {
        let mut who: Vec<String> =
            self.inner.holds.lock().unwrap().values().filter(|(b, _)| *b == build).map(|(_, w)| w.clone()).collect();
        let checks = self.inner.checks.lock().unwrap().clone();
        who.extend(checks.iter().filter_map(|c| c(build)));
        who.sort();
        who.dedup();
        who
    }
}

/// A build held in use; released when dropped.
pub struct InUseGuard {
    inner: Arc<InUseInner>,
    id: u64,
}

impl Drop for InUseGuard {
    fn drop(&mut self) {
        self.inner.holds.lock().unwrap().remove(&self.id);
    }
}

/// Versions and builds. Cheap to clone.
#[derive(Clone)]
pub struct Versions {
    inner: Arc<Inner>,
}

struct Inner {
    hub: Arc<Hub>,
    scheduler: Scheduler,
    toolchain: Arc<dyn Toolchain>,
    repo: Repo,
    in_use: InUse,
    /// Builds queued and not yet ended, by commit and features, so two callers wait on one build.
    pending: tokio::sync::Mutex<HashMap<(String, Vec<String>), dameless_core::WorkId>>,
    /// Serialises changes to the champion line.
    champion: tokio::sync::Mutex<()>,
}

/// Features sorted and without repeats, as the build cache keys them.
fn features_of(features: &[&str]) -> Vec<String> {
    let mut f: Vec<String> = features.iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
    f.sort();
    f.dedup();
    f
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn git_error(e: anyhow::Error) -> Error {
    Error::internal(format!("git: {e:#}"))
}

impl Versions {
    /// Versions over the hub's engine checkout, building with cargo.
    pub fn new(hub: Arc<Hub>, scheduler: Scheduler) -> Self {
        Self::with_toolchain(hub, scheduler, Arc::new(Reckless))
    }

    /// Versions building with another toolchain (a fake, in tests).
    pub fn with_toolchain(hub: Arc<Hub>, scheduler: Scheduler, toolchain: Arc<dyn Toolchain>) -> Self {
        let repo = Repo::new(&hub.config.reckless_dir);
        Self {
            inner: Arc::new(Inner {
                hub,
                scheduler,
                toolchain,
                repo,
                in_use: InUse::default(),
                pending: tokio::sync::Mutex::default(),
                champion: tokio::sync::Mutex::default(),
            }),
        }
    }

    /// The `Build` executor, to register with the scheduler.
    pub fn executor(&self) -> Arc<dyn Executor> {
        Arc::new(BuildExecutor { versions: self.clone() })
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.inner.hub
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.inner.scheduler
    }

    pub fn toolchain(&self) -> &Arc<dyn Toolchain> {
        &self.inner.toolchain
    }

    /// The fork's checkout.
    pub fn repo(&self) -> &Repo {
        &self.inner.repo
    }

    /// Who uses which builds.
    pub fn in_use(&self) -> &InUse {
        &self.inner.in_use
    }

    fn emit(&self, v: &Version) {
        self.inner.hub.bus.emit(Payload::VersionUpserted(Box::new(v.clone())));
    }

    /// The version a reference means, making one of a branch, tag or commit on first use.
    pub async fn resolve(&self, r: &VersionRef) -> Result<Version, Error> {
        let store = &self.inner.hub.store;
        let name = match r {
            VersionRef::Champion => return self.champion().await,
            VersionRef::Id(id) => {
                return store.version(*id).await?.ok_or_else(|| Error::not_found(format!("No version {id}")));
            }
            VersionRef::Name(n) if n == MAIN || n == "champion" => return self.champion().await,
            VersionRef::Name(n) => n.trim().to_owned(),
        };
        if let Some(v) = store.version_by_name(&name).await? {
            // A branch moves: the version follows its tip (champion versions name fixed commits).
            if v.branch.as_deref() == Some(name.as_str())
                && !matches!(v.line, Line::Champion { .. })
                && let Ok(c) = self.inner.repo.resolve(&name).await
                && c.sha != v.commit
            {
                let moved = store
                    .upsert_version(NewVersion {
                        name: v.name.clone(),
                        commit: c.sha,
                        branch: v.branch.clone(),
                        line: v.line,
                        parent: v.parent,
                        bench: if v.net.is_some() { None } else { c.bench },
                        net: v.net,
                    })
                    .await?;
                self.emit(&moved);
                return Ok(moved);
            }
            return Ok(v);
        }
        let commit =
            self.inner.repo.resolve(&name).await.map_err(|_| {
                Error::not_found(format!("No branch, tag or commit named {name} in the engine checkout"))
            })?;
        // A commit (or a prefix of one) that already is a version is that version.
        if commit.sha.starts_with(&name)
            && let Some(v) = store.versions_by_commit(&commit.sha).await?.into_iter().next()
        {
            return Ok(v);
        }
        let upstream = git::is_upstream(&name);
        let branch = (upstream || self.inner.repo.has_branch(&name).await).then(|| name.clone());
        let parent = self.champion().await.ok().map(|c| c.id);
        let v = store
            .upsert_version(NewVersion {
                name,
                commit: commit.sha,
                branch,
                line: if upstream { Line::Upstream } else { Line::Other },
                parent,
                bench: commit.bench,
                net: None,
            })
            .await?;
        self.emit(&v);
        Ok(v)
    }

    /// Records an idea's branch as its version (or moves it to the branch's tip). It embeds its parent's net, so an
    /// idea on a champion that is code plus a trained net is tested with that net.
    pub async fn idea_version(&self, branch: &str, idea: IdeaId, parent: Option<VersionId>) -> Result<Version, Error> {
        let net = match parent {
            Some(p) => self.inner.hub.store.version(p).await?.and_then(|v| v.net),
            None => None,
        };
        let commit = self
            .inner
            .repo
            .resolve(branch)
            .await
            .map_err(|_| Error::bad_request(format!("No branch {branch} in the engine checkout")))?;
        let v = self
            .inner
            .hub
            .store
            .upsert_version(NewVersion {
                name: branch.to_owned(),
                commit: commit.sha,
                branch: Some(branch.to_owned()),
                line: Line::Idea { idea },
                parent,
                // A Bench: line is measured with the commit's own net: it says nothing of a build with another.
                bench: if net.is_some() { None } else { commit.bench },
                net,
            })
            .await?;
        self.emit(&v);
        Ok(v)
    }

    /// The champion: the tip of the fork's `main`, numbered on the champion line.
    pub async fn champion(&self) -> Result<Version, Error> {
        let _line = self.inner.champion.lock().await;
        let store = &self.inner.hub.store;
        let current = store.champion().await?;
        let tip = match self.inner.repo.resolve(MAIN).await {
            Ok(tip) => tip,
            Err(e) => return current.ok_or_else(|| git_error(e)),
        };
        if let Some(c) = &current
            && c.commit == tip.sha
        {
            return Ok(c.clone());
        }
        let number = current.as_ref().map_or(1, |c| match c.line {
            Line::Champion { number } => number + 1,
            _ => 1,
        });
        let v = store
            .upsert_version(NewVersion {
                name: Line::champion_name(number),
                commit: tip.sha,
                branch: Some(MAIN.to_owned()),
                line: Line::Champion { number },
                // The champion's net carries over to new code.
                net: current.as_ref().and_then(|c| c.net),
                bench: if current.as_ref().is_some_and(|c| c.net.is_some()) { None } else { tip.bench },
                parent: current.map(|c| c.id),
            })
            .await?;
        self.emit(&v);
        Ok(v)
    }

    /// Records a promotion: `commit` (the new tip of `main`) is the next champion, with its Elo over the last.
    pub async fn crown(&self, commit: &str, bench: Option<u64>, elo: Option<Estimate>) -> Result<Version, Error> {
        let _line = self.inner.champion.lock().await;
        let store = &self.inner.hub.store;
        let current = store.champion().await?;
        if let Some(c) = current.as_ref().filter(|c| c.commit == commit) {
            return Ok(c.clone());
        }
        let number = current.as_ref().map_or(1, |c| match c.line {
            Line::Champion { number } => number + 1,
            _ => 1,
        });
        let v = store
            .upsert_version(NewVersion {
                name: Line::champion_name(number),
                commit: commit.to_owned(),
                branch: Some(MAIN.to_owned()),
                line: Line::Champion { number },
                net: current.as_ref().and_then(|c| c.net),
                bench: if current.as_ref().is_some_and(|c| c.net.is_some()) { None } else { bench },
                parent: current.map(|c| c.id),
            })
            .await?;
        store.set_version_elo(v.id, elo).await?;
        let v = store.version(v.id).await?.unwrap_or(v);
        self.emit(&v);
        Ok(v)
    }

    /// Versions, newest first; one line's (`champion`, `idea`, `upstream`, `other`) when asked.
    pub async fn list(&self, line: Option<&str>) -> Result<Vec<Version>, Error> {
        self.inner.hub.store.versions(line).await
    }

    /// A version's builds, newest first.
    pub async fn builds(&self, version: VersionId) -> Result<Vec<Build>, Error> {
        self.inner.hub.store.builds(version).await
    }

    /// A ready binary of a version with these cargo features, building it first (as `Build` work, awaited) when there is
    /// none.
    pub async fn binary(&self, r: &VersionRef, features: &[&str]) -> Result<(Build, PathBuf), Error> {
        let version = self.resolve(r).await?;
        self.binary_of(&version, features).await
    }

    /// [`Versions::binary`] for a version in hand.
    pub async fn binary_of(&self, version: &Version, features: &[&str]) -> Result<(Build, PathBuf), Error> {
        let features = features_of(features);
        let store = &self.inner.hub.store;
        if let Some(b) = store.cached_build(&version.commit, &features).await?
            && Path::new(&b.path).is_file()
        {
            let path = PathBuf::from(&b.path);
            return Ok((b, path));
        }
        let id = {
            let mut pending = self.inner.pending.lock().await;
            let key = (version.commit.clone(), features.clone());
            let live = match pending.get(&key) {
                Some(id) => store.work(*id).await?.filter(|w| !w.status.is_final()).map(|w| w.id),
                None => None,
            };
            match live {
                Some(id) => id,
                None => {
                    let label = if features.is_empty() { String::new() } else { format!(" +{}", features.join("+")) };
                    let work = self
                        .inner
                        .scheduler
                        .submit(NewWork {
                            kind: WorkKind::Build,
                            priority: Priority::Interactive,
                            title: format!("Building {}{label}", version.name),
                            subject: Subject::Version { id: version.id },
                            spec: serde_json::to_value(BuildWork {
                                version: version.id,
                                commit: version.commit.clone(),
                                features: features.clone(),
                                net: None,
                            })
                            .map_err(|e| Error::internal(e.to_string()))?,
                        })
                        .await?;
                    pending.insert(key, work.id);
                    work.id
                }
            }
        };
        let work = self.inner.scheduler.wait(id).await?;
        match work.status {
            WorkStatus::Done => match store.cached_build(&version.commit, &features).await? {
                Some(b) => {
                    let path = PathBuf::from(&b.path);
                    Ok((b, path))
                }
                None => Err(Error::internal(format!("{} was built, but its binary is not recorded", version.name))),
            },
            WorkStatus::Stopped => Err(Error::conflict(format!("Building {} was stopped", version.name))),
            _ => Err(Error::internal(format!(
                "Building {} failed: {}",
                version.name,
                work.error.unwrap_or_else(|| "see its log".into())
            ))),
        }
    }

    /// Records `build`'s binary as also being the build of `commit` (a reworded commit: same tree, new message), so it
    /// is not built again.
    pub async fn alias_build(&self, build: &Build, version: VersionId, commit: &str) -> Result<Build, Error> {
        let store = &self.inner.hub.store;
        let alias = store
            .insert_build(NewBuild {
                version,
                commit: commit.to_owned(),
                features: build.features.clone(),
                net: build.net,
                path: build.path.clone(),
                status: BuildStatus::Building,
                log_path: None,
            })
            .await?;
        store.update_build(alias.id, BuildStatus::Ready, build.bench, build.nps).await
    }

    /// Deletes a build's binary; the record stays for what used it. 409 while something uses it.
    pub async fn delete_build(&self, id: BuildId) -> Result<Build, Error> {
        let store = &self.inner.hub.store;
        let build = store.build(id).await?.ok_or_else(|| Error::not_found(format!("No build {id}")))?;
        if build.status == BuildStatus::Deleted {
            return Ok(build);
        }
        let users = self.inner.in_use.users(id);
        if !users.is_empty() {
            return Err(Error::conflict(format!("Build {id} is in use: {}", users.join(", "))));
        }
        self.remove_binary(&build);
        store.mark_build_deleted(id).await?;
        if let Some(v) = store.version(build.version).await? {
            self.emit(&v);
        }
        store.build(id).await?.ok_or_else(|| Error::not_found(format!("No build {id}")))
    }

    /// Removes a build's binary, and its folder when it has one of its own under the builds directory. Another build
    /// record sharing the path (an alias) loses it too, which is what deleting means.
    fn remove_binary(&self, build: &Build) {
        let path = Path::new(&build.path);
        let builds = self.inner.hub.config.builds_dir();
        match path.parent() {
            Some(dir) if dir.parent() == Some(builds.as_path()) && dir.file_name().is_some_and(|n| n != "target") => {
                let _ = std::fs::remove_dir_all(dir);
            }
            _ => {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// Deletes the binaries nothing uses: by default every version's older builds, keeping its newest of each feature
    /// set; with `keep_latest` off, all of them.
    pub async fn prune(&self, req: PruneRequest) -> Result<PruneReport, Error> {
        let store = &self.inner.hub.store;
        let mut report = PruneReport { dry_run: req.dry_run, ..Default::default() };
        let cutoff = req.older_than_days.map(|d| SystemTime::now() - Duration::from_secs(u64::from(d) * 86_400));
        for version in store.versions(None).await? {
            let mut kept: Vec<Vec<String>> = Vec::new();
            for build in store.builds(version.id).await? {
                if build.status == BuildStatus::Deleted {
                    continue;
                }
                if req.keep_latest && build.status == BuildStatus::Ready && !kept.contains(&build.features) {
                    kept.push(build.features.clone());
                    continue;
                }
                if matches!(build.status, BuildStatus::Queued | BuildStatus::Building) {
                    continue;
                }
                let meta = std::fs::metadata(&build.path).ok();
                if let (Some(cutoff), Some(modified)) = (cutoff, meta.as_ref().and_then(|m| m.modified().ok()))
                    && modified > cutoff
                {
                    continue;
                }
                if !self.inner.in_use.users(build.id).is_empty() {
                    report.in_use.push(build.id);
                    continue;
                }
                report.freed_bytes += meta.map_or(0, |m| m.len());
                report.deleted.push(build.id);
                if !req.dry_run {
                    self.remove_binary(&build);
                    store.mark_build_deleted(build.id).await?;
                }
            }
        }
        Ok(report)
    }
}

/// Builds a commit into a binary: the `Build` work kind.
pub struct BuildExecutor {
    versions: Versions,
}

#[async_trait]
impl Executor for BuildExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Build
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: BuildWork = cx.spec()?;
        let v = &self.versions;
        let hub = &v.inner.hub;
        let store = &hub.store;
        let version = store.version(spec.version).await?.ok_or_else(|| Error::not_found("The version is gone"))?;
        if let Some(b) = store.cached_build(&spec.commit, &spec.features).await?
            && Path::new(&b.path).is_file()
        {
            cx.log(format!("Already built: build {}.", b.id));
            return Ok(serde_json::json!({ "build": b.id, "cached": true }));
        }
        let Some(grant) = cx.cores(1, hub.ledger.total()).await else { return Err(cx.stopped()) };
        let builds = hub.config.builds_dir();
        let label = if spec.features.is_empty() { "plain".to_owned() } else { spec.features.join("+") };
        let out_dir = builds.join(format!("{}-{label}", short(&spec.commit)));
        let binary = out_dir.join(dameless_engine::exe("reckless"));
        let record = store
            .insert_build(NewBuild {
                version: version.id,
                commit: spec.commit.clone(),
                features: spec.features.clone(),
                net: spec.net,
                path: binary.display().to_string(),
                status: BuildStatus::Building,
                log_path: Some(hub.config.work_log_path(cx.id).display().to_string()),
            })
            .await?;
        let build_spec = BuildSpec {
            repo: hub.config.reckless_dir.clone(),
            commit: spec.commit.clone(),
            features: spec.features.clone(),
            syzygy: None,
            native: true,
            worktree: hub.config.build_worktree(),
            target_dir: builds.join("target"),
            out_dir,
            networks_dir: Some(builds.join("networks")),
        };
        cx.log(format!("Building {} ({}) {label}", version.name, short(&spec.commit)));
        let mut log = |line: &str| cx.log(line);
        let built = match v.inner.toolchain.build(&build_spec, &cx.cancel, &mut log).await {
            Ok(built) => built,
            Err(e) => {
                store.update_build(record.id, BuildStatus::Failed, None, None).await?;
                if dameless_engine::is_cancelled(&e) || cx.cancel.is_cancelled() {
                    return Err(cx.stopped());
                }
                return Err(Error::internal(format!("build failed: {e:#}")));
            }
        };
        let bench = match v.inner.toolchain.bench(&built.binary).await {
            Ok(b) => b,
            Err(e) => {
                store.update_build(record.id, BuildStatus::Failed, None, None).await?;
                return Err(Error::internal(format!("bench failed: {e:#}")));
            }
        };
        drop(grant);
        let expected = v.inner.repo.resolve(&spec.commit).await.ok().and_then(|c| c.bench);
        let suspect = expected.is_some_and(|e| e != bench.nodes);
        match expected {
            Some(e) if suspect => cx.log(format!(
                "Suspect: bench {} but the commit says Bench: {e} (a stale Bench: line, or non-determinism)",
                bench.nodes
            )),
            Some(_) => cx.log(format!("Bench {} matches the commit, {} nps", bench.nodes, bench.nps)),
            None => cx.log(format!("Bench {}, {} nps (the commit has no Bench: line)", bench.nodes, bench.nps)),
        }
        let b = store.update_build(record.id, BuildStatus::Ready, Some(bench.nodes), Some(bench.nps)).await?;
        v.emit(&version);
        Ok(serde_json::json!({ "build": b.id, "bench": bench.nodes, "nps": bench.nps, "suspect": suspect }))
    }
}

/// Pushes the fork's `main` to `origin`. Only ever called on an explicit request (`POST /api/dameless/push`).
pub async fn push(hub: &Hub) -> Result<(), Error> {
    Repo::new(&hub.config.reckless_dir).push(MAIN).await.map_err(|e| Error::upstream(format!("push failed: {e:#}")))
}

/// Downloads fastchess and the opening book into the tools directory (`dameless setup`). Never called by tests.
pub async fn setup(hub: &Hub, log: &mut (dyn FnMut(&str) + Send)) -> Result<(), Error> {
    dameless_engine::tools::setup(&hub.config.tools_dir(), log)
        .await
        .map(drop)
        .map_err(|e| Error::upstream(format!("setup failed: {e:#}")))
}

#[cfg(test)]
mod tests;
