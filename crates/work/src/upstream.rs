//! Taking upstream: the `Upstream` work item, which merges upstream Reckless's `main` into the fork's and crowns the
//! result the next champion.
//!
//! Upstream's commits are already tested on its own fleet, so no trials are run on them. What is checked is that the
//! result builds and is what it claims: the fork's `upstream` remote is fetched, and when `upstream/main` has commits
//! the champion lacks, the merge is made the way a promotion makes one, in the scratch worktree under the data
//! directory:
//!
//! - **The fork has nothing of its own on top:** `main` fast-forwards to upstream's commit. Its build's bench must equal
//!   that commit's `Bench:` line (upstream's CI checks the same thing), else nothing changes.
//! - **The fork has its own commits:** a merge commit on no branch is built, and the commit that goes onto `main` is
//!   the same merge with its `Bench:` line written from the measured bench (the build is shared with it).
//!
//! Then `main` moves as a promotion moves it (`engine::git::Repo::advance`): only from where the sync found it, and
//! the user's checkout of `main` is fast-forwarded only when clean, else the sync is refused. The new tip is champion
//! `v<n+1>`, recorded with the upstream commits it brought ([`UpstreamMerge`]). A conflict, a failed build, a bench that
//! disagrees or a refused checkout changes nothing: the sync fails, and its sentence is in the work's log, the
//! logbook (`GET /api/upstream`, `GET /api/dameless`) and the `upstream.synced` event. Nothing is ever pushed.
//!
//! Syncs share the promotions' lane, so a sync never runs while an idea is being promoted. The app looks for new
//! upstream commits every six hours by itself while the setting is on (it is by default), and on demand
//! (`POST /api/upstream/sync`).

use std::time::Duration;

use async_trait::async_trait;
use dameless_core::{
    Error,
    events::Payload,
    upstream::{self, SyncOutcome, UpstreamCommit, UpstreamMerge, UpstreamSettings, UpstreamStatus, UpstreamSync},
    version::Version,
    work::{Priority, Subject, Work, WorkKind},
};
use dameless_engine::git::CheckedOut;
use dameless_hub::{Executor, WorkContext};
use dameless_store::{NewWork, now};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{
    ideas::with_bench_line,
    versions::{MAIN, VersionRef, Versions},
};

/// Where the automatic sync's setting is kept.
pub const SETTINGS_KEY: &str = "upstream.settings";
/// Where the last sync is kept.
pub const LAST_KEY: &str = "upstream.last";
/// The name an upstream merge is built under before it becomes the champion.
pub const MERGE_VERSION: &str = "upstream-merge";

/// How often the automatic sync looks whether one is due.
const LOOK_EVERY: Duration = Duration::from_secs(60);

/// What an `Upstream` work item was queued by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamWork {
    /// The six-hourly sync rather than a request.
    #[serde(default)]
    pub auto: bool,
}

/// Syncing with upstream Reckless. Cheap to clone.
#[derive(Clone)]
pub struct Upstream {
    versions: Versions,
}

impl Upstream {
    pub fn new(versions: Versions) -> Self {
        Self { versions }
    }

    /// Registers the `Upstream` executor with the scheduler.
    pub fn register(&self) {
        self.versions.scheduler().register(std::sync::Arc::new(UpstreamExecutor { upstream: self.clone() }));
    }

    async fn settings(&self) -> Result<UpstreamSettings, Error> {
        Ok(self.versions.hub().store.meta(SETTINGS_KEY).await?.unwrap_or_default())
    }

    async fn last(&self) -> Result<Option<UpstreamSync>, Error> {
        self.versions.hub().store.meta(LAST_KEY).await
    }

    /// The sync queued or running now, if any.
    async fn active(&self) -> Result<Option<Work>, Error> {
        let lists = self.versions.scheduler().list(0).await?;
        Ok(lists.running.into_iter().chain(lists.queued).find(|w| w.kind == WorkKind::Upstream))
    }

    /// The setting, the last sync, one running, and when the next automatic one is due.
    pub async fn status(&self) -> Result<UpstreamStatus, Error> {
        let settings = self.settings().await?;
        let last = self.last().await?;
        let now_ms = now().unix_ms().unwrap_or_default();
        let next_at = settings.auto.then(|| {
            let due = upstream::next_due(last.as_ref().and_then(|l| l.at.unix_ms()), now_ms);
            dameless_core::Timestamp::from_unix_ms(due)
        });
        Ok(UpstreamStatus {
            settings,
            remote: self.versions.repo().has_remote(upstream::REMOTE).await,
            last,
            running: self.active().await?.map(|w| w.id),
            next_at,
        })
    }

    /// Turns the automatic sync on or off.
    pub async fn set_settings(&self, settings: UpstreamSettings) -> Result<UpstreamStatus, Error> {
        self.versions.hub().store.put_meta(SETTINGS_KEY, &settings).await?;
        self.status().await
    }

    /// Queues a sync now. 409 while one is queued or running, or when the engine checkout has no `upstream` remote.
    pub async fn sync(&self) -> Result<Work, Error> {
        self.submit(false).await
    }

    async fn submit(&self, auto: bool) -> Result<Work, Error> {
        if let Some(w) = self.active().await? {
            return Err(Error::conflict(format!("Upstream is being taken already (work {})", w.id)));
        }
        if !self.versions.repo().has_remote(upstream::REMOTE).await {
            return Err(Error::conflict(format!(
                "The engine checkout has no remote named {}: add it (git remote add upstream \
                 https://github.com/codedeliveryservice/Reckless.git)",
                upstream::REMOTE
            )));
        }
        let champion = self.versions.champion().await?;
        self.versions
            .scheduler()
            .submit(NewWork {
                kind: WorkKind::Upstream,
                priority: Priority::Interactive,
                title: "Taking upstream Reckless".into(),
                subject: Subject::Version { id: champion.id },
                spec: serde_json::to_value(UpstreamWork { auto }).map_err(|e| Error::internal(e.to_string()))?,
            })
            .await
    }

    /// Queues the automatic sync when it is due: the setting on, an `upstream` remote, none running, and six hours
    /// since the last. Returns the work when it queued one.
    pub async fn sync_if_due(&self) -> Result<Option<Work>, Error> {
        let settings = self.settings().await?;
        if !settings.auto || self.active().await?.is_some() {
            return Ok(None);
        }
        if !self.versions.repo().has_remote(upstream::REMOTE).await {
            return Ok(None);
        }
        let now_ms = now().unix_ms().unwrap_or_default();
        let last = self.last().await?.and_then(|l| l.at.unix_ms());
        if upstream::next_due(last, now_ms) > now_ms {
            return Ok(None);
        }
        self.submit(true).await.map(Some)
    }

    /// Looks every minute whether the automatic sync is due, until `shutdown`. The first look is a minute after the app
    /// starts.
    pub fn spawn_auto(&self, shutdown: CancellationToken) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = tokio::time::sleep(LOOK_EVERY) => {}
                }
                if let Err(e) = this.sync_if_due().await {
                    eprintln!("upstream: {e}");
                }
            }
        });
    }

    /// Stores and sends a sync's end.
    async fn record(&self, sync: UpstreamSync) -> Result<(), Error> {
        let hub = self.versions.hub();
        hub.store.put_meta(LAST_KEY, &sync).await?;
        hub.bus.emit(Payload::UpstreamSynced(Box::new(sync)));
        Ok(())
    }

    /// The sync itself. A failure is recorded and returned, having changed nothing.
    async fn run(&self, cx: &WorkContext) -> Result<serde_json::Value, Error> {
        let work = cx.id;
        match self.take(cx).await {
            Ok((sync, value)) => {
                cx.log(&sync.detail);
                self.record(sync).await?;
                Ok(value)
            }
            Err((merge, e)) if !WorkContext::is_stopped(&e) => {
                let detail = format!("Taking upstream failed, nothing changed: {}", e.message());
                cx.log(&detail);
                let sync = UpstreamSync {
                    at: now(),
                    outcome: SyncOutcome::Failed,
                    detail,
                    work,
                    version: None,
                    champion: None,
                    merge,
                };
                self.record(sync).await?;
                Err(e)
            }
            Err((_, e)) => Err(e),
        }
    }

    async fn take(
        &self,
        cx: &WorkContext,
    ) -> Result<(UpstreamSync, serde_json::Value), (Option<UpstreamMerge>, Error)> {
        let none = |e: Error| (None, e);
        let versions = &self.versions;
        let repo = versions.repo();
        let hub = versions.hub();
        if !repo.has_remote(upstream::REMOTE).await {
            return Err(none(Error::conflict(format!("the engine checkout has no remote {}", upstream::REMOTE))));
        }
        cx.log(format!("Fetching {}", upstream::REMOTE));
        repo.fetch(upstream::REMOTE)
            .await
            .map_err(|e| none(Error::upstream(format!("fetching {} failed: {e:#}", upstream::REMOTE))))?;
        // The champion line knows main's tip before it grows.
        versions.champion().await.map_err(none)?;
        let git = |e: anyhow::Error| Error::conflict(format!("git: {e:#}"));
        let main = repo.resolve(MAIN).await.map_err(|e| none(git(e)))?;
        let theirs = repo.resolve(upstream::BRANCH).await.map_err(|e| none(git(e)))?;
        if repo.is_ancestor(&theirs.sha, &main.sha).await {
            let detail =
                format!("Up to date: the champion has every upstream commit (upstream at {})", short(&theirs.sha));
            let sync = UpstreamSync {
                at: now(),
                outcome: SyncOutcome::UpToDate,
                detail,
                work: cx.id,
                version: None,
                champion: None,
                merge: None,
            };
            return Ok((sync, serde_json::json!({ "outcome": "up_to_date", "upstream": theirs.sha })));
        }
        let commits = repo
            .commits_between(&main.sha, &theirs.sha)
            .await
            .map_err(|e| none(git(e)))?
            .into_iter()
            .map(|(sha, subject)| UpstreamCommit { sha, subject })
            .collect::<Vec<_>>();
        let fast_forward = repo.is_ancestor(&main.sha, &theirs.sha).await;
        let merge = UpstreamMerge { base: main.sha.clone(), upstream: theirs.sha.clone(), commits, fast_forward };
        let with = |e: Error| (Some(merge.clone()), e);
        cx.log(format!(
            "{} upstream commit(s) the champion lacks ({}): {}",
            merge.commits.len(),
            merge.range(),
            if fast_forward { "main fast-forwards" } else { "the fork has its own commits, so a merge commit" }
        ));
        for c in &merge.commits {
            cx.log(format!("  {} {}", short(&c.sha), c.subject));
        }

        let (commit, bench, build) = if fast_forward {
            let expected = theirs.bench.ok_or_else(|| {
                with(Error::conflict(format!("upstream's {} has no Bench: line to check", short(&theirs.sha))))
            })?;
            let version = versions.resolve(&VersionRef::Name(upstream::BRANCH.into())).await.map_err(with)?;
            if version.commit != theirs.sha {
                return Err(with(Error::conflict("upstream/main moved while it was being taken")));
            }
            // The champion will be this very commit, so this build is already its build.
            let (_, measured) = self.build(cx, &version).await.map_err(with)?;
            if measured != expected {
                return Err(with(Error::conflict(format!(
                    "upstream's {} benches {measured} but says Bench: {expected}",
                    short(&theirs.sha)
                ))));
            }
            cx.log(format!("Bench {measured} matches upstream's Bench: line"));
            (theirs.sha.clone(), measured, None)
        } else {
            let message = merge_message(&merge);
            let made = repo
                .merge_detached(&main.sha, &theirs.sha, &message, &hub.config.promote_worktree())
                .await
                .map_err(|e| with(Error::conflict(format!("{e:#}"))))?;
            let version = versions.scratch_version(MERGE_VERSION, &made).await.map_err(with)?;
            let (build, measured) = self.build(cx, &version).await.map_err(with)?;
            let commit = repo.recommit(&made, &with_bench_line(&message, measured)).await.map_err(|e| with(git(e)))?;
            cx.log(format!("The merge benches {measured}: its commit {} says so", short(&commit)));
            (commit, measured, Some(build))
        };

        repo.advance(MAIN, &commit, &main.sha, CheckedOut::FastForwardIfClean)
            .await
            .map_err(|e| with(Error::conflict(format!("{e:#}"))))?;
        cx.log(format!("{MAIN} {} → {}", short(&main.sha), short(&commit)));
        let crowned = versions.crown(&commit, Some(bench), None).await.map_err(with)?;
        if let Some(build) = &build {
            // Same tree as the merge that was built: its binary is the champion's.
            versions.alias_build(build, crowned.id, &commit).await.map_err(with)?;
        }
        hub.store.set_version_upstream(crowned.id, Some(merge.clone())).await.map_err(with)?;
        let crowned = versions.announce(crowned.id).await.map_err(with)?;
        let detail = format!(
            "{} {}: {} ({}), bench {bench}",
            crowned.name,
            merge.label(),
            merge.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>().join("; "),
            merge.range()
        );
        let value = serde_json::json!({
            "outcome": "merged",
            "version": crowned.id,
            "name": crowned.name,
            "commit": commit,
            "fast_forward": fast_forward,
            "commits": merge.commits.len(),
        });
        let sync = UpstreamSync {
            at: now(),
            outcome: SyncOutcome::Merged,
            detail,
            work: cx.id,
            version: Some(crowned.id),
            champion: Some(crowned.name.clone()),
            merge: Some(merge),
        };
        Ok((sync, value))
    }

    /// Builds a version (as `Build` work, awaited) and returns the build and its bench.
    async fn build(&self, cx: &WorkContext, version: &Version) -> Result<(dameless_core::version::Build, u64), Error> {
        cx.log(format!("Building {} ({})", version.name, short(&version.commit)));
        let (build, _) = self.versions.binary_of(version, &[]).await?;
        let bench = build.bench.ok_or_else(|| Error::internal(format!("{}'s build has no bench", version.name)))?;
        Ok((build, bench))
    }
}

/// The merge commit's message, before its `Bench:` line: the range and every upstream commit it brings.
fn merge_message(merge: &UpstreamMerge) -> String {
    let list: Vec<String> = merge.commits.iter().map(|c| format!("- {} {}", short(&c.sha), c.subject)).collect();
    format!(
        "Merge upstream Reckless ({})\n\nTaken by Dameless: upstream's commits are tested on its own fleet.\n\n{}\n",
        merge.range(),
        list.join("\n")
    )
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

struct UpstreamExecutor {
    upstream: Upstream,
}

#[async_trait]
impl Executor for UpstreamExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Upstream
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let w: UpstreamWork = cx.spec().unwrap_or_default();
        if w.auto {
            cx.log("The six-hourly look for new upstream commits.");
        }
        self.upstream.run(&cx).await
    }
}

#[cfg(test)]
mod tests;
