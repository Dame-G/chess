//! Taking upstream, against a fake upstream: a local git repository the test fork has as its `upstream` remote.
//! Nothing here reaches the network or the real fork.

use std::sync::Arc;

use dameless_core::{ErrorCode, events::EventKind, upstream::SyncOutcome, version::Line, work::WorkStatus};
use dameless_engine::git;
use dameless_hub::{Hub, Next as BusNext, TopicFilter};

use super::*;
use crate::{
    scheduler::Scheduler,
    testing::{Fake, TestRepo, fake_nodes, hub, repo},
};

struct Lab {
    fork: TestRepo,
    upstream: TestRepo,
    hub: Arc<Hub>,
    _dir: tempfile::TempDir,
    versions: Versions,
    sync: Upstream,
    fake: Arc<Fake>,
}

/// The fork (the user's checkout, `main` checked out) with an `upstream` remote: a clone of it, where upstream's new
/// commits are made.
async fn lab() -> Lab {
    let fork = repo();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap().to_owned();
    let upstream = TestRepo { dir };
    upstream.git(&["init", "-q", "-b", "main"]);
    upstream.git(&["config", "user.name", "Upstream"]);
    upstream.git(&["config", "user.email", "upstream@example.com"]);
    upstream.git(&["config", "core.autocrlf", "false"]);
    upstream.git(&["pull", "-q", fork.path().to_str().unwrap(), "main"]);
    fork.git(&["remote", "add", "upstream", &path]);
    let (hub, data) = hub(&fork, 2);
    let scheduler = Scheduler::new(hub.clone());
    let fake = Fake::new();
    let versions = Versions::with_toolchain(hub.clone(), scheduler.clone(), fake.clone());
    scheduler.register(versions.executor());
    let sync = Upstream::new(versions.clone());
    sync.register();
    scheduler.start().await.unwrap();
    Lab { fork, upstream, hub, _dir: data, versions, sync, fake }
}

/// Runs a sync to its end.
async fn synced(lab: &Lab) -> (Work, UpstreamSync) {
    let work = lab.sync.sync().await.unwrap();
    let work = lab.versions.scheduler().wait(work.id).await.unwrap();
    let last = lab.sync.status().await.unwrap().last.unwrap();
    assert_eq!(last.work, work.id);
    (work, last)
}

#[tokio::test]
async fn an_up_to_date_champion_changes_nothing() {
    let lab = lab().await;
    let before = lab.versions.champion().await.unwrap();
    let (work, last) = synced(&lab).await;
    assert_eq!(work.status, WorkStatus::Done);
    assert_eq!(last.outcome, SyncOutcome::UpToDate, "{}", last.detail);
    assert!(last.detail.starts_with("Up to date"), "{}", last.detail);
    assert_eq!(lab.versions.champion().await.unwrap().id, before.id);
}

#[tokio::test]
async fn upstream_commits_fast_forward_main_and_make_the_next_champion() {
    let lab = lab().await;
    let v1 = lab.versions.champion().await.unwrap();
    let mut events = lab.hub.bus.subscribe(0, TopicFilter::of(["versions"]));
    lab.upstream.commit("main", "main", &[("src/search.rs", "fn search() { faster(); }\n")], None);
    let tip = lab.upstream.commit("main", "main", &[("src/search.rs", "fn search() { fastest(); }\n")], None);

    let (work, last) = synced(&lab).await;
    assert_eq!(work.status, WorkStatus::Done, "{:?}", work.error);
    assert_eq!(last.outcome, SyncOutcome::Merged, "{}", last.detail);
    // main is upstream's commit, and the user's checkout followed.
    assert_eq!(lab.fork.git(&["rev-parse", "main"]), tip);
    assert_eq!(std::fs::read_to_string(lab.fork.path().join("src/search.rs")).unwrap(), "fn search() { fastest(); }\n");
    assert_eq!(lab.fork.git(&["status", "--porcelain"]), "");

    let v2 = lab.versions.champion().await.unwrap();
    assert_eq!((v2.name.as_str(), v2.line, v2.parent), ("v2", Line::Champion { number: 2 }, Some(v1.id)));
    assert_eq!(v2.commit, tip);
    assert_eq!(v2.bench, git::bench_line(&lab.fork.git(&["log", "-1", "--format=%B", &tip])));
    let merge = v2.upstream.clone().unwrap();
    assert!(merge.fast_forward && merge.base == v1.commit && merge.upstream == tip);
    assert_eq!(merge.commits.len(), 2);
    assert_eq!(merge.label(), "from upstream · 2 commits");
    assert_eq!((last.version, last.champion.as_deref()), (Some(v2.id), Some("v2")));
    assert!(last.detail.starts_with("v2 from upstream · 2 commits"), "{}", last.detail);
    // The champion's binary is the one built and benched for the check.
    let builds = lab.fake.builds.load(std::sync::atomic::Ordering::SeqCst);
    lab.versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    assert_eq!(lab.fake.builds.load(std::sync::atomic::Ordering::SeqCst), builds);

    let kinds: Vec<EventKind> = std::iter::from_fn(|| events.try_recv())
        .filter_map(|n| match n {
            BusNext::Event(e) => Some(e.kind),
            BusNext::Lagged(_) => None,
        })
        .collect();
    assert!(kinds.contains(&EventKind::UpstreamSynced) && kinds.contains(&EventKind::VersionUpserted), "{kinds:?}");
    // Nothing was pushed: the fork has no other remote, and upstream did not move.
    assert_eq!(lab.upstream.git(&["rev-parse", "main"]), tip);
}

#[tokio::test]
async fn a_fork_with_its_own_commits_gets_a_merge_commit_with_its_measured_bench() {
    let lab = lab().await;
    let own = lab.fork.commit("main", "main", &[("src/parameters.rs", "define! { i32 own: 1, 0, 2, 1; }\n")], None);
    let v_own = lab.versions.champion().await.unwrap();
    assert_eq!(v_own.commit, own);
    let theirs = lab.upstream.commit("main", "main", &[("src/search.rs", "fn search() { upstream(); }\n")], None);

    let (work, last) = synced(&lab).await;
    assert_eq!(work.status, WorkStatus::Done, "{:?}", work.error);
    assert_eq!(last.outcome, SyncOutcome::Merged, "{}", last.detail);
    let tip = lab.fork.git(&["rev-parse", "main"]);
    assert_eq!(lab.fork.git(&["log", "-1", "--format=%P", &tip]), format!("{own} {theirs}"));
    let message = lab.fork.git(&["log", "-1", "--format=%B", &tip]);
    assert!(message.starts_with("Merge upstream Reckless ("), "{message}");
    // The Bench: line is what the merged tree benches.
    let merged = "fn search() { upstream(); }\n\ndefine! { i32 own: 1, 0, 2, 1; }\n";
    assert_eq!(git::bench_line(&message), Some(fake_nodes(merged)));

    let champion = lab.versions.champion().await.unwrap();
    assert_eq!((champion.commit.as_str(), champion.bench), (tip.as_str(), Some(fake_nodes(merged))));
    let merge = champion.upstream.unwrap();
    assert!(!merge.fast_forward && merge.commits.len() == 1 && merge.commits[0].sha == theirs);
    // Built once, as the merge; the champion shares that build.
    let builds = lab.fake.builds.load(std::sync::atomic::Ordering::SeqCst);
    lab.versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    assert_eq!(lab.fake.builds.load(std::sync::atomic::Ordering::SeqCst), builds);
    assert_eq!(lab.fork.git(&["status", "--porcelain"]), "");
}

#[tokio::test]
async fn a_conflict_changes_nothing_and_says_where() {
    let lab = lab().await;
    let own = lab.fork.commit("main", "main", &[("src/search.rs", "fn search() { ours(); }\n")], None);
    let before = lab.versions.champion().await.unwrap();
    lab.upstream.commit("main", "main", &[("src/search.rs", "fn search() { theirs(); }\n")], None);
    let (work, last) = synced(&lab).await;
    assert_eq!(work.status, WorkStatus::Failed);
    assert_eq!(last.outcome, SyncOutcome::Failed);
    assert!(
        last.detail.contains("conflicts in src/search.rs") && last.detail.contains("nothing changed"),
        "{}",
        last.detail
    );
    assert_eq!(last.merge.as_ref().map(|m| m.commits.len()), Some(1));
    assert_eq!(lab.fork.git(&["rev-parse", "main"]), own);
    assert_eq!(lab.versions.champion().await.unwrap().id, before.id);
    assert_eq!(lab.fork.git(&["status", "--porcelain"]), "");
    let log = std::fs::read_to_string(lab.hub.config.work_log_path(work.id)).unwrap();
    assert!(log.contains("Taking upstream failed"), "{log}");
}

#[tokio::test]
async fn a_bench_that_disagrees_or_a_dirty_checkout_changes_nothing() {
    let lab = lab().await;
    let main = lab.fork.git(&["rev-parse", "main"]);
    lab.upstream.commit("main", "main", &[("src/search.rs", "fn search() { stale(); }\n")], Some(7));
    let (work, last) = synced(&lab).await;
    assert_eq!((work.status, last.outcome), (WorkStatus::Failed, SyncOutcome::Failed));
    assert!(last.detail.contains("says Bench: 7"), "{}", last.detail);
    assert_eq!(lab.fork.git(&["rev-parse", "main"]), main);

    // A right line, but uncommitted work in the user's checkout of main: refused, as a promotion is.
    lab.upstream.commit("main", "main", &[("src/search.rs", "fn search() { fine(); }\n")], None);
    std::fs::write(lab.fork.path().join("src/parameters.rs"), "// mine, not committed\n").unwrap();
    let (work, last) = synced(&lab).await;
    assert_eq!((work.status, last.outcome), (WorkStatus::Failed, SyncOutcome::Failed));
    assert!(last.detail.contains("uncommitted changes"), "{}", last.detail);
    assert_eq!(lab.fork.git(&["rev-parse", "main"]), main);
    assert_eq!(lab.versions.champion().await.unwrap().name, "v1");
}

#[tokio::test]
async fn the_automatic_sync_follows_its_setting_and_the_clock() {
    let lab = lab().await;
    let status = lab.sync.status().await.unwrap();
    assert!(status.settings.auto && status.remote && status.last.is_none() && status.next_at.is_some());
    // Never synced: due now.
    let work = lab.sync.sync_if_due().await.unwrap().expect("due");
    lab.versions.scheduler().wait(work.id).await.unwrap();
    // Synced just now: not due for six hours.
    assert!(lab.sync.sync_if_due().await.unwrap().is_none());
    let next = lab.sync.status().await.unwrap().next_at.unwrap().unix_ms().unwrap();
    assert!(next - now().unix_ms().unwrap() > upstream::EVERY_MS - 60_000);

    let off = lab.sync.set_settings(UpstreamSettings { auto: false }).await.unwrap();
    assert!(!off.settings.auto && off.next_at.is_none());
    assert!(lab.sync.sync_if_due().await.unwrap().is_none(), "off means off");
    // On demand works either way.
    let work = lab.sync.sync().await.unwrap();
    lab.versions.scheduler().wait(work.id).await.unwrap();
}

#[tokio::test]
async fn a_fork_without_an_upstream_remote_has_nothing_to_sync() {
    let lab = lab().await;
    lab.fork.git(&["remote", "remove", "upstream"]);
    assert!(!lab.sync.status().await.unwrap().remote);
    let e = lab.sync.sync().await.unwrap_err();
    assert_eq!(e.code(), ErrorCode::Conflict);
    assert!(e.message().contains("no remote named upstream"), "{}", e.message());
    assert!(lab.sync.sync_if_due().await.unwrap().is_none());
}
