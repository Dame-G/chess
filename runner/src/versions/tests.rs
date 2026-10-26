use std::{path::PathBuf, process::Command};

use axum::http::StatusCode;

use super::*;
use crate::{config::Config, lichess::rules::Settings, store::Store};

fn branch(name: &str, sha: &str) -> Branch {
    Branch {
        name: name.into(),
        sha: sha.into(),
        subject: format!("{name} tip"),
        date: "2026-10-01T00:00:00+00:00".into(),
        bench: Some(1),
        hidden: false,
    }
}

fn insert_build(c: &Connection, key: &str, refname: &str, commit: &str, features: &str) -> i64 {
    c.execute(
        "INSERT INTO builds (key, ref, commit_sha, subject, features, bench, bench_expected, nps)
         VALUES (?1, ?2, ?3, 's', ?4, 100, 100, 1)",
        [key, refname, commit, features],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn insert_experiment(c: &Connection, dev: &str, base: &str, status: &str) -> i64 {
    c.execute(
        "INSERT INTO experiments (name, kind, base_ref, dev_ref, funnel, status, results, summary)
         VALUES (?1, 'gainer', ?2, ?1, '[]', ?3, '[]', 'why')",
        [dev, base, status],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn insert_tune(c: &Connection, refname: &str, status: &str, params: &str) -> i64 {
    c.execute(
        "INSERT INTO tunes (name, ref, inputs, params, spec, iterations, pairs, concurrency, alpha, gamma, a_ratio,
         iteration, status) VALUES ('t', ?1, '[]', ?2, '{}', 100, 8, 1, 0.6, 0.1, 0.1, 7, ?3)",
        [refname, params, status],
    )
    .unwrap();
    c.last_insert_rowid()
}

#[tokio::test]
async fn versions_gather_builds_verdicts_tunes_and_the_bot() {
    let store = Store::memory().unwrap();
    let (records, ids) = store
        .call(|c| {
            let old = insert_build(c, "t1", "feature", "aaa", "no-syzygy");
            let spsa = insert_build(c, "t2", "feature", "bbb", "no-syzygy+spsa");
            let tip = insert_build(c, "t3", "feature", "bbb", "no-syzygy");
            let main = insert_build(c, "t4", "main", "mmm", "no-syzygy");
            build::delete(c, Path::new("nowhere"), &build::get(c, tip)?.unwrap())?;
            // Two experiments of `feature`: the later one is its verdict, with its last match's estimate.
            insert_experiment(c, "feature", "main", "failed");
            let latest = insert_experiment(c, "feature", "upstream/main", "passed");
            c.execute(
                "INSERT INTO tests (experiment_id, stage, spec, base_build, dev_build, running, penta)
                 VALUES (?1, 'stc', '{}', ?2, ?3, 0, '[1,2,10,5,2]')",
                rusqlite::params![latest, main, old],
            )?;
            // A tune by build id counts for the branch that build belongs to; the newer one wins.
            insert_tune(c, "feature", "done", "[]");
            let tune = insert_tune(c, &old.to_string(), "running", "[{\"name\":\"x\"}]");
            set_hidden_names(c, &HashSet::from(["main".to_owned()]))?;
            Ok((load(c)?, (old, spsa, tip, main, latest, tune)))
        })
        .await
        .unwrap();
    let (old, spsa, tip, main, latest, tune) = ids;
    let branches = [branch("feature", "bbb"), branch("main", "mmm"), branch("upstream/main", "mmm")];

    let versions = aggregate(&branches, &records, Path::new("nowhere"), Some("main"), &HashSet::new());
    let [feature, main_v, upstream] = &versions[..] else { panic!("three versions") };
    assert_eq!(feature.builds.iter().map(|b| b.id).collect::<Vec<_>>(), vec![tip, spsa, old]);
    assert!(feature.builds.iter().all(|b| !b.on_disk));
    assert!(feature.builds[0].deleted_at.is_some());
    // The tip's plain build is deleted and the spsa one is not plain: the newest usable is the old one.
    assert_eq!(feature.latest_build_id, Some(old));
    let verdict = feature.verdict.as_ref().unwrap();
    assert_eq!((verdict.experiment, verdict.status, verdict.games), (latest, ExperimentStatus::Passed, 40));
    assert!(verdict.estimate.is_some());
    assert_eq!(feature.tested_against.as_deref(), Some("upstream/main"));
    assert_eq!(feature.tune.as_ref().map(|t| (t.id, t.status, t.iteration)), Some((tune, TuneStatus::Running, 7)));
    assert!(feature.tunables, "a tune of it read parameters");
    assert!(!feature.hidden && !feature.is_main && !feature.on_lichess);

    assert!(main_v.is_main && main_v.hidden && main_v.on_lichess);
    assert_eq!(main_v.latest_build_id, Some(main));
    assert!(main_v.verdict.is_none() && main_v.tune.is_none() && !main_v.tunables);
    // `main` names that branch alone, though upstream's main shares its commit (and so its build).
    assert!(upstream.upstream && !upstream.on_lichess);
    assert_eq!(upstream.builds.len(), 1);

    // A build id resolves to the branches its build belongs to; a tip with the spsa define! is tunable.
    let by_id =
        aggregate(&branches, &records, Path::new("nowhere"), Some(&main.to_string()), &HashSet::from(["mmm".into()]));
    assert!(by_id[1].on_lichess && by_id[2].on_lichess && !by_id[0].on_lichess);
    assert!(by_id[1].tunables && by_id[2].tunables);
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap().success();
    assert!(ok, "git {args:?}");
}

/// A temp Reckless stand-in: `main`, a `feature` branch, an `upstream/main`, and a runner state on it.
fn repo_state() -> (Arc<AppState>, PathBuf) {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).unwrap();
    let root = std::env::temp_dir().join(format!("chess-versions-test-{}", u64::from_le_bytes(bytes)));
    let repo = root.join("reckless");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@example.com"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "Start\n\nBench: 5"]);
    git(&repo, &["update-ref", "refs/remotes/upstream/main", "HEAD"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "Feature\n\nBench: 6"]);
    git(&repo, &["checkout", "-q", "main"]);
    let config = Config {
        data: root.join("data"),
        reckless: repo,
        listen: "127.0.0.1:0".parse().unwrap(),
        token: "t".into(),
        lichess_url: String::new(),
    };
    std::fs::create_dir_all(&config.data).unwrap();
    (Arc::new(AppState::new(config, Store::memory().unwrap(), crate::lichess::engines())), root)
}

fn confirm(name: &str) -> DeleteVersion {
    DeleteVersion { confirm: name.into() }
}

#[tokio::test]
async fn hiding_persists_and_shows_in_branches() {
    let (state, root) = repo_state();
    assert_eq!(set_hidden(&state, "nope".into(), true).await.unwrap_err().status, StatusCode::NOT_FOUND);
    set_hidden(&state, "feature".into(), true).await.unwrap();
    let hidden: Vec<String> =
        branches(&state).await.unwrap().into_iter().filter(|b| b.hidden).map(|b| b.name).collect();
    assert_eq!(hidden, vec!["feature"]);
    assert!(list(&state).await.unwrap().iter().any(|v| v.name == "feature" && v.hidden));
    set_hidden(&state, "feature".into(), false).await.unwrap();
    assert!(branches(&state).await.unwrap().iter().all(|b| !b.hidden));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn deleting_a_branch_is_guarded() {
    let (state, root) = repo_state();
    let status = |r: Result<DeletedVersion, ApiError>| r.unwrap_err().status;
    assert_eq!(status(delete_version(&state, "main", &confirm("main"), false, false).await), StatusCode::BAD_REQUEST);
    assert_eq!(
        status(delete_version(&state, "upstream/main", &confirm("upstream/main"), false, false).await),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(delete_version(&state, "feature", &confirm("featur"), false, false).await),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(status(delete_version(&state, "gone", &confirm("gone"), false, false).await), StatusCode::NOT_FOUND);

    // A running experiment holds it.
    state.store.call(|c| Ok(insert_experiment(c, "feature", "main", "running"))).await.unwrap();
    assert_eq!(
        status(delete_version(&state, "feature", &confirm("feature"), false, false).await),
        StatusCode::BAD_REQUEST
    );
    state.store.call(|c| Ok(c.execute("UPDATE experiments SET status = 'passed'", [])?)).await.unwrap();

    // So does the Lichess bot.
    let sha = git::resolve(&state.config.reckless, "feature").await.unwrap().sha;
    let id = state.store.call(move |c| Ok(insert_build(c, "k", "feature", &sha, "no-syzygy"))).await.unwrap();
    let settings = Settings { build: Some("feature".into()), ..Default::default() };
    crate::lichess::put_settings(&state, settings).await.unwrap();
    assert_eq!(status(delete_version(&state, "feature", &confirm("feature"), false, true).await), StatusCode::CONFLICT);
    // And the bot's build cannot be deleted on its own.
    assert_eq!(delete_build(&state, id).await.unwrap_err().status, StatusCode::CONFLICT);
    crate::lichess::put_settings(&state, Settings::default()).await.unwrap();

    // Free now: the branch goes, with its build's binary; the experiment stays on record.
    set_hidden(&state, "feature".into(), true).await.unwrap();
    let done = delete_version(&state, "feature", &confirm("feature"), false, true).await.unwrap();
    assert_eq!((done.remote, done.builds), (false, vec![id]));
    assert!(!git::has_branch(&state.config.reckless, "feature").await);
    let (hidden, experiments, deleted) = state
        .store
        .call(move |c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM experiments", [], |r| r.get(0))?;
            Ok((hidden(c)?, n, build::get(c, id)?.unwrap().deleted_at))
        })
        .await
        .unwrap();
    assert!(hidden.is_empty());
    assert_eq!(experiments, 1);
    assert!(deleted.is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn a_build_in_use_is_not_deleted_and_prune_keeps_the_newest() {
    let (state, root) = repo_state();
    let (old, newer, busy) = state
        .store
        .call(|c| {
            let old = insert_build(c, "a", "main", "x1", "no-syzygy");
            let newer = insert_build(c, "b", "main", "x2", "no-syzygy");
            let busy = insert_build(c, "c", "other", "x3", "no-syzygy");
            c.execute(
                "INSERT INTO tunes (name, ref, build_id, inputs, params, spec, iterations, pairs, concurrency,
                       alpha, gamma, a_ratio, iteration, status)
                       VALUES ('t', 'other', ?1, '[]', '[]', '{}', 1, 1, 1, 0.6, 0.1, 0.1, 0, 'running')",
                [busy],
            )?;
            Ok((old, newer, busy))
        })
        .await
        .unwrap();
    assert_eq!(delete_build(&state, busy).await.unwrap_err().status, StatusCode::CONFLICT);
    assert_eq!(delete_build(&state, 999).await.unwrap_err().status, StatusCode::NOT_FOUND);

    let dry = prune(&state, PruneRequest { older_than_days: None, keep_latest_per_branch: true, dry_run: true })
        .await
        .unwrap();
    assert_eq!((dry.deleted, dry.in_use), (vec![old], vec![busy]));
    let done = prune(&state, PruneRequest { older_than_days: Some(30), keep_latest_per_branch: true, dry_run: false })
        .await
        .unwrap();
    assert!(done.deleted.is_empty(), "nothing is 30 days old");
    let done = prune(&state, PruneRequest { older_than_days: None, keep_latest_per_branch: false, dry_run: false })
        .await
        .unwrap();
    assert_eq!(done.deleted, vec![newer, old]);
    assert!(delete_build(&state, old).await.unwrap().deleted_at.is_some(), "deleting again changes nothing");
    let _ = std::fs::remove_dir_all(root);
}
