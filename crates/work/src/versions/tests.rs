use std::sync::atomic::Ordering;

use dameless_core::{events::EventKind, version::Line};
use dameless_hub::TopicFilter;

use super::*;
use crate::testing::{Fake, TestRepo, fake_nodes, git, hub, repo};

async fn setup(cores: u32) -> (TestRepo, Arc<Hub>, tempfile::TempDir, Scheduler, Versions, Arc<Fake>) {
    let r = repo();
    let (hub, dir) = hub(&r, cores);
    let scheduler = Scheduler::new(hub.clone());
    let fake = Fake::new();
    let versions = Versions::with_toolchain(hub.clone(), scheduler.clone(), fake.clone());
    scheduler.register(versions.executor());
    scheduler.start().await.unwrap();
    (r, hub, dir, scheduler, versions, fake)
}

#[tokio::test]
async fn the_tip_of_main_is_the_champion_numbered_in_order() {
    let (r, hub, _dir, _s, versions, _fake) = setup(2).await;
    let v1 = versions.champion().await.unwrap();
    assert_eq!((v1.name.as_str(), v1.line, v1.branch.as_deref()), ("v1", Line::Champion { number: 1 }, Some("main")));
    assert_eq!(v1.commit, r.git(&["rev-parse", "main"]));
    assert!(v1.bench.is_some());
    assert_eq!(versions.resolve(&VersionRef::Champion).await.unwrap().id, v1.id);
    assert_eq!(versions.resolve(&VersionRef::parse("main")).await.unwrap().id, v1.id);
    assert_eq!(versions.resolve(&VersionRef::parse("v1")).await.unwrap().id, v1.id);
    assert_eq!(versions.resolve(&VersionRef::parse(&format!("#{}", v1.id.0))).await.unwrap().id, v1.id);

    // A commit made on main in the checkout makes the next champion.
    let tip = r.commit("main", "main", &[("src/search.rs", "fn search() { 1; }\n")], None);
    let v2 = versions.champion().await.unwrap();
    assert_eq!((v2.name.as_str(), v2.commit.as_str(), v2.parent), ("v2", tip.as_str(), Some(v1.id)));
    let line: Vec<String> = versions.list(Some("champion")).await.unwrap().into_iter().map(|v| v.name).collect();
    assert_eq!(line, ["v2", "v1"]);
    // A promotion crowns a commit with its Elo.
    let v3 = versions.crown("abc123", Some(5), Some(Estimate::plus_minus(3.0, 1.5))).await.unwrap();
    assert_eq!((v3.name.as_str(), v3.parent, v3.elo.map(|e| e.elo)), ("v3", Some(v2.id), Some(3.0)));
    assert_eq!(hub.store.champion().await.unwrap().unwrap().id, v3.id);
}

#[tokio::test]
async fn branches_and_commits_become_versions_that_follow_their_branch() {
    let (r, _hub, _dir, _s, versions, _fake) = setup(2).await;
    let sha = r.commit("lmr", "main", &[("src/search.rs", "fn search() { 2; }\n")], None);
    let v = versions.resolve(&VersionRef::parse("lmr")).await.unwrap();
    assert_eq!(
        (v.name.as_str(), v.commit.as_str(), v.line, v.branch.as_deref()),
        ("lmr", sha.as_str(), Line::Other, Some("lmr"))
    );
    let moved = r.commit("lmr", "main", &[("src/search.rs", "fn search() { 3; }\n")], None);
    let again = versions.resolve(&VersionRef::parse("lmr")).await.unwrap();
    assert_eq!((again.id, again.commit.as_str()), (v.id, moved.as_str()));
    // A commit prefix of an existing version is that version.
    let by_sha = versions.resolve(&VersionRef::Name(moved[..10].to_owned())).await.unwrap();
    assert_eq!(by_sha.id, v.id);
    let err = versions.resolve(&VersionRef::parse("nope")).await.unwrap_err();
    assert_eq!(err.status(), 404);
    // An idea's branch is recorded on the idea's line.
    let idea = versions.idea_version("lmr", IdeaId(7), None).await.unwrap();
    assert_eq!((idea.id, idea.line), (v.id, Line::Idea { idea: IdeaId(7) }));
}

#[tokio::test]
async fn binaries_are_built_once_and_cached() {
    let (r, hub, _dir, _s, versions, fake) = setup(2).await;
    *fake.build_delay.lock().unwrap() = std::time::Duration::from_millis(50);
    let mut sub = hub.bus.subscribe(0, TopicFilter::of(["versions", "work"]));
    // Two callers at once wait on one build.
    let (a, b) = tokio::join!(versions.binary(&VersionRef::Champion, &[]), versions.binary(&VersionRef::Champion, &[]));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.0.id, b.0.id);
    assert!(a.1.is_file());
    assert_eq!(fake.builds.load(Ordering::SeqCst), 1);
    let champion = versions.champion().await.unwrap();
    assert_eq!((a.0.bench, a.0.status, a.0.version), (champion.bench, BuildStatus::Ready, champion.id));
    // Cached now; features make another build.
    versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    assert_eq!(fake.builds.load(Ordering::SeqCst), 1);
    let (spsa, _) = versions.binary(&VersionRef::Champion, &["spsa"]).await.unwrap();
    assert_eq!((spsa.features.clone(), fake.builds.load(Ordering::SeqCst)), (vec!["spsa".to_owned()], 2));
    assert_eq!(versions.builds(champion.id).await.unwrap().len(), 2);
    let kinds: Vec<EventKind> = std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            dameless_hub::Next::Event(e) => Some(e.kind),
            _ => None,
        })
        .collect();
    assert!(kinds.contains(&EventKind::VersionUpserted) && kinds.contains(&EventKind::WorkUpserted));

    // A stale Bench: line builds, and the log calls the build suspect.
    r.commit("stale", "main", &[("src/search.rs", "fn search() { 4; }\n")], Some(42));
    let (stale, _) = versions.binary(&VersionRef::parse("stale"), &[]).await.unwrap();
    assert_ne!(stale.bench, Some(42));
    let work = hub.store.work_lists(10).await.unwrap().recent;
    let log = std::fs::read_to_string(hub.config.work_log_path(work[0].id)).unwrap();
    assert!(log.contains("Suspect: bench"), "{log}");
}

#[tokio::test]
async fn builds_in_use_are_kept_and_prune_keeps_the_newest() {
    let (r, hub, _dir, _s, versions, _fake) = setup(2).await;
    let (old, old_path) = versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    let guard = versions.in_use().hold(old.id, "game:12");
    let err = versions.delete_build(old.id).await.unwrap_err();
    assert_eq!(err.status(), 409);
    assert!(err.to_string().contains("game:12"));
    drop(guard);
    versions.in_use().add_check(move |b| (b == old.id).then(|| "the bot".to_owned()));
    assert!(versions.delete_build(old.id).await.unwrap_err().to_string().contains("the bot"));

    // A second build of the same version (a new feature set) and an older build of another version.
    r.commit("x", "main", &[("src/search.rs", "fn search() { 5; }\n")], None);
    let (x, x_path) = versions.binary(&VersionRef::parse("x"), &[]).await.unwrap();
    let report =
        versions.prune(PruneRequest { keep_latest: true, dry_run: true, older_than_days: None }).await.unwrap();
    assert!(report.deleted.is_empty(), "each version's newest stays: {report:?}");
    let report =
        versions.prune(PruneRequest { keep_latest: false, dry_run: true, older_than_days: None }).await.unwrap();
    assert_eq!((report.deleted.clone(), report.in_use.clone()), (vec![x.id], vec![old.id]));
    assert!(x_path.is_file(), "a dry run deletes nothing");
    let report =
        versions.prune(PruneRequest { keep_latest: false, dry_run: false, older_than_days: Some(1) }).await.unwrap();
    assert!(report.deleted.is_empty(), "everything is newer than a day");
    versions.prune(PruneRequest { keep_latest: false, dry_run: false, older_than_days: None }).await.unwrap();
    assert!(!x_path.exists());
    assert_eq!(hub.store.build(x.id).await.unwrap().unwrap().status, BuildStatus::Deleted);
    assert!(old_path.is_file());
    // A deleted build is built again when asked for.
    let (again, _) = versions.binary(&VersionRef::parse("x"), &[]).await.unwrap();
    assert_ne!(again.id, x.id);
    assert_eq!(versions.delete_build(BuildId(999)).await.unwrap_err().status(), 404);
}

#[tokio::test]
async fn push_sends_main_to_origin() {
    let (r, hub, _dir, _s, _versions, _fake) = setup(1).await;
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "-q", "--bare"]);
    r.git(&["remote", "add", "origin", bare.path().to_str().unwrap()]);
    push(&hub).await.unwrap();
    assert_eq!(git(bare.path(), &["rev-parse", "main"]), r.git(&["rev-parse", "main"]));
    assert!(fake_nodes("a") != fake_nodes("b"));
}
