//! Training with the fakes: datasets (datagen, import, the cap), training runs (GPU, progress, nets), builds keyed by
//! net, and a net idea through its funnel. The trainer's tools are the fake program of [`crate::fake_trainer`]; builds
//! and games are the fake toolchain's.

use std::{path::Path, sync::Arc, time::Duration};

use dameless_core::{
    IdeaId,
    events::EventKind,
    ideas::{Author, IdeaKind, IdeaStatus, Stage, TrialStatus},
    training::{DatasetSource, TrainSchedule, TrainSpec},
    work::{WorkKind, WorkStatus},
};
use dameless_hub::{Config, Hub, Next as BusNext, TopicFilter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    data::{DatagenRequest, Datasets, ImportRequest},
    fake_trainer::FakeTrainer,
    ideas::{Ideas, NewIdeaRequest},
    nets::Nets,
    scheduler::Scheduler,
    testing::{Fake, FakeProbes, TestRepo, eventually, repo},
    train::Training,
    versions::{VersionRef, Versions},
};

struct Lab {
    _repo: TestRepo,
    hub: Arc<Hub>,
    _dir: tempfile::TempDir,
    scheduler: Scheduler,
    versions: Versions,
    ideas: Ideas,
    data: Datasets,
    nets: Nets,
    training: Training,
    trainer: Arc<FakeTrainer>,
    fake: Arc<Fake>,
    probes: Arc<FakeProbes>,
}

/// A lab with `gpus` GPUs and a datasets' cap of `cap` bytes, over a repository whose champion names the net
/// `test-net.nnue`, found in its `networks/`.
async fn lab(gpus: u32, cap: u64) -> Lab {
    let repo = repo();
    repo.commit("main", "main", &[("build/build.rs", "const NETWORK_NAME: &str = \"test-net.nnue\";\n")], None);
    std::fs::create_dir_all(repo.path().join("networks")).unwrap();
    std::fs::write(repo.path().join("networks").join("test-net.nnue"), "the shipped net").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), repo.path(), 8);
    config.gpus = gpus;
    config.data_cap_bytes = cap;
    let hub = Hub::open(config).unwrap();
    let scheduler = Scheduler::new(hub.clone());
    let fake = Fake::new();
    let versions = Versions::with_toolchain(hub.clone(), scheduler.clone(), fake.clone());
    scheduler.register(versions.executor());
    let ideas = Ideas::new(versions.clone());
    ideas.register();
    let trainer = FakeTrainer::new();
    let probes = FakeProbes::new();
    let data = Datasets::new(versions.clone(), trainer.clone(), probes.clone());
    data.register();
    let nets = Nets::new(hub.clone());
    let training = Training::new(ideas.clone(), data.clone(), nets.clone(), probes.clone());
    training.register();
    scheduler.start().await.unwrap();
    Lab { _repo: repo, hub, _dir: dir, scheduler, versions, ideas, data, nets, training, trainer, fake, probes }
}

const GB: u64 = 1_000_000_000;

fn datagen(name: &str, positions: u64) -> DatagenRequest {
    DatagenRequest {
        name: Some(name.into()),
        positions,
        threads: Some(2),
        soft_nodes: None,
        opening: None,
        version: Some("main".into()),
    }
}

fn args_log(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("args.log")).unwrap_or_default().lines().map(str::to_owned).collect()
}

/// A dataset of `positions` in the trainer's units, written by hand.
async fn dataset(lab: &Lab, name: &str, positions: u64) -> dameless_core::training::Dataset {
    let w = lab.data.datagen(datagen(name, positions)).await.unwrap();
    assert_eq!(lab.scheduler.wait(w.id).await.unwrap().status, WorkStatus::Done);
    lab.hub.store.dataset_by_name(name).await.unwrap().unwrap()
}

fn small_run(superbatches: u32) -> TrainSpec {
    TrainSpec {
        schedule: TrainSchedule { superbatches, batches_per_superbatch: 10, batch_size: 16, ..Default::default() },
        save_rate: 1,
        keep_checkpoints: 1,
        ..Default::default()
    }
}

#[tokio::test]
async fn datagen_writes_a_dataset_and_a_rerun_resumes_it() {
    let lab = lab(0, 10 * GB).await;
    let mut sub = lab.hub.bus.subscribe(0, TopicFilter::of(["training", "work"]));
    let d = dataset(&lab, "dfrc", 1000).await;
    assert_eq!((d.positions, d.size_bytes, d.format.as_str()), (1000, 4000, "viriformat"));
    assert_eq!(d.score_units.as_deref(), Some("reckless-internal"));
    assert!(matches!(d.source, DatasetSource::Datagen { .. }));
    let dir = lab.data.dir("dfrc");
    let first = args_log(&dir);
    assert_eq!(first.len(), 1);
    for flag in ["datagen --json --engine", "--threads 2", "--positions 1000", "--units reckless-internal"] {
        assert!(first[0].contains(flag), "{flag} in {}", first[0]);
    }

    // More of the same dataset: the tool runs again on the same directory.
    let again = dataset(&lab, "dfrc", 2500).await;
    assert_eq!((again.id, again.positions), (d.id, 2500));
    assert_eq!(args_log(&dir).len(), 2);
    let status = lab.data.status().await.unwrap();
    assert_eq!((status.used_bytes, status.cap_bytes), (10_000, 10 * GB));

    let kinds: Vec<EventKind> = std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            BusNext::Event(e) => Some(e.kind),
            BusNext::Lagged(_) => None,
        })
        .collect();
    assert!(kinds.contains(&EventKind::DatasetUpserted));
    let w = lab.scheduler.list(10).await.unwrap().recent.into_iter().find(|w| w.kind == WorkKind::Datagen).unwrap();
    let p = w.progress.expect("datagen reports progress");
    assert_eq!((p.unit.as_str(), p.total), ("positions", Some(2500)));

    // Deleting needs a confirmation, then removes the files.
    assert_eq!(lab.data.delete(d.id, false).await.unwrap_err().status(), 400);
    lab.data.delete(d.id, true).await.unwrap();
    assert!(!dir.exists());
    assert!(lab.data.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn stopping_datagen_kills_it_and_keeps_what_it_wrote() {
    let lab = lab(0, 10 * GB).await;
    lab.trainer.set_env("FAKE_SLEEP_MS", "150");
    let w = lab.data.datagen(datagen("slow", 20_000)).await.unwrap();
    eventually("datagen to write", || async {
        lab.hub.store.work(w.id).await.unwrap().and_then(|w| w.progress).is_some_and(|p| p.done > 0)
    })
    .await;
    lab.scheduler.stop(w.id).await.unwrap();
    assert_eq!(lab.scheduler.wait(w.id).await.unwrap().status, WorkStatus::Stopped);
    let d = lab.hub.store.dataset_by_name("slow").await.unwrap().expect("registered from what was written");
    assert!(d.positions > 0 && d.positions < 20_000, "{}", d.positions);
}

#[tokio::test]
async fn datagen_and_imports_stay_within_the_cap_and_the_disk() {
    let lab = lab(0, 800).await;
    // The tool is given the cap left as its --max-size.
    let d = dataset(&lab, "tiny", 200).await;
    assert_eq!(d.size_bytes, 800);
    assert!(args_log(&lab.data.dir("tiny"))[0].contains("--max-size 800"));
    let e = lab.data.datagen(datagen("more", 100)).await.unwrap_err();
    assert_eq!(e.status(), 409, "{}", e.message());
    let e = lab
        .data
        .import(ImportRequest {
            url: "http://127.0.0.1:1/x.binpack".into(),
            expected_bytes: 10,
            sha256: None,
            name: "shard".into(),
            confirm: true,
        })
        .await
        .unwrap_err();
    assert_eq!(e.status(), 409, "{}", e.message());

    // The disk keeps 15 GB free whatever the cap says.
    let lab = lab_big_cap_full_disk().await;
    let e = lab.data.datagen(datagen("d", 100)).await.unwrap_err();
    assert_eq!(e.status(), 409);
}

async fn lab_big_cap_full_disk() -> Lab {
    let lab = lab(0, 100 * GB).await;
    *lab.probes.disk.lock().unwrap() = Some(crate::health::DiskSpace { free_bytes: 15 * GB, total_bytes: 500 * GB });
    lab
}

/// Serves `body` at any path, once per connection, saying it is `length` bytes long.
async fn serve(body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut got = Vec::new();
                while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                    let Ok(n) = socket.read(&mut buf).await else { return };
                    if n == 0 {
                        return;
                    }
                    got.extend_from_slice(&buf[..n]);
                }
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn an_import_downloads_checks_and_converts_one_shard() {
    let lab = lab(0, 10 * GB).await;
    let body: Vec<u8> = (0..4000u32).map(|i| (i % 251) as u8).collect();
    let sha = crate::nets::hex(&<sha2::Sha256 as sha2::Digest>::digest(&body));
    let base = serve(body.clone()).await;
    let request = |file: &str, bytes: u64, sha: Option<String>| ImportRequest {
        url: format!("{base}/sets/{file}"),
        expected_bytes: bytes,
        sha256: sha,
        name: "vondele-test".into(),
        confirm: true,
    };

    // Without your go-ahead, nothing is queued.
    let e = lab.data.import(ImportRequest { confirm: false, ..request("a.binpack", 4000, None) }).await.unwrap_err();
    assert_eq!(e.status(), 400);

    // The size you confirmed is checked against the server's.
    let w = lab.data.import(request("a.binpack", 3999, None)).await.unwrap();
    let w = lab.scheduler.wait(w.id).await.unwrap();
    assert_eq!(w.status, WorkStatus::Failed);
    assert!(w.error.as_deref().unwrap_or_default().contains("3999"), "{:?}", w.error);

    // So is the SHA-256.
    let w = lab.data.import(request("a.binpack", 4000, Some("0".repeat(64)))).await.unwrap();
    let w = lab.scheduler.wait(w.id).await.unwrap();
    assert_eq!(w.status, WorkStatus::Failed);
    assert!(w.error.as_deref().unwrap_or_default().contains("SHA-256"), "{:?}", w.error);

    let w = lab.data.import(request("a.binpack", 4000, Some(sha.clone()))).await.unwrap();
    let done = lab.scheduler.wait(w.id).await.unwrap();
    assert_eq!(done.status, WorkStatus::Done, "{:?}", done.error);
    let d = lab.hub.store.dataset_by_name("vondele-test").await.unwrap().unwrap();
    // Converted (the fake writes it twice over), and the download is gone.
    assert_eq!(d.size_bytes, 8000);
    assert_eq!(d.source, DatasetSource::Import { url: format!("{base}/sets/a.binpack") });
    assert_eq!(d.score_units, None, "a binpack's scores are not in Reckless's units");
    assert!(lab.data.dir("vondele-test").join("a.vf").is_file());
    let downloads = lab.hub.config.downloads_dir();
    assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    assert!(args_log(&lab.data.dir("vondele-test"))[0].starts_with("convert"));
}

#[tokio::test]
async fn a_training_run_holds_the_gpu_streams_progress_and_registers_its_net() {
    let lab = lab(1, 10 * GB).await;
    // Without a GPU, nothing is queued.
    let no_gpu = self::lab(0, GB).await;
    assert_eq!(no_gpu.training.submit(small_run(2)).await.unwrap_err().status(), 409);

    let d = dataset(&lab, "dfrc", 1000).await;
    let mut sub = lab.hub.bus.subscribe(0, TopicFilter::of(["work", "training"]));
    lab.trainer.set_env("FAKE_SLEEP_MS", "50");
    let w = lab.training.submit(TrainSpec { name: Some("smoke".into()), ..small_run(3) }).await.unwrap();
    eventually("the run to take the GPU", || async {
        lab.hub.ledger.snapshot().gpu_grants == vec![w.id]
            || lab.hub.store.work(w.id).await.unwrap().is_some_and(|w| w.status.is_final())
    })
    .await;
    let done = lab.scheduler.wait(w.id).await.unwrap();
    assert_eq!(done.status, WorkStatus::Done, "{:?}", done.error);
    assert_eq!(lab.hub.ledger.free_gpus(), 1, "the GPU is given back");

    let nets = lab.nets.list().await.unwrap();
    let shipped = nets.iter().find(|n| n.name == "test-net").expect("the champion's shipped net, registered");
    let trained = nets.iter().find(|n| n.name == "smoke").unwrap();
    assert_eq!((trained.parent, trained.dataset), (Some(shipped.id), Some(d.id)));
    let run = trained.run.clone().unwrap();
    assert_eq!((run.superbatches, run.work, run.final_loss), (3, Some(w.id), Some(1.0 / 3.0)));
    assert!(Path::new(&trained.path).is_file() && trained.path.ends_with(&format!("{}.nnue", trained.sha256)));
    // The run's directory, with its checkpoints, is gone once the net is in nets/.
    assert!(!lab.hub.config.runs_dir().join(format!("work-{}", w.id)).exists());

    let steps = lab.training.steps(w.id).await.unwrap();
    assert_eq!(steps.iter().map(|s| s.superbatch).collect::<Vec<_>>(), [1, 2, 3]);
    let events: Vec<_> = std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            BusNext::Event(e) => Some(e),
            BusNext::Lagged(_) => None,
        })
        .collect();
    let progress: Vec<_> = events.iter().filter(|e| e.kind == EventKind::TrainProgress).collect();
    assert!(!progress.is_empty() && progress.iter().all(|e| e.topic == "work"));
    assert_eq!(progress[0].data["superbatches"], 3);
    assert!(events.iter().any(|e| e.kind == EventKind::NetUpserted && e.topic == "training"));
    let args = std::fs::read_to_string(lab.hub.config.logs_dir().join(format!("work-{}.log", w.id))).unwrap();
    assert!(args.contains("--parent") && args.contains("--eval-scale 380"), "{args}");

    // bullet's error fails the run.
    lab.trainer.set_env("FAKE_TRAIN_FAIL", "1");
    let w = lab.training.submit(small_run(2)).await.unwrap();
    let failed = lab.scheduler.wait(w.id).await.unwrap();
    assert_eq!(failed.status, WorkStatus::Failed);
    assert!(failed.error.unwrap_or_default().contains("no data in the files"));
}

#[tokio::test]
async fn builds_are_keyed_by_net() {
    let lab = lab(1, 10 * GB).await;
    let champion = lab.versions.champion().await.unwrap();
    let net = lab
        .nets
        .register_file(
            &lab._repo.path().join("networks").join("test-net.nnue"),
            crate::nets::NetSource { name: "other".into(), ..Default::default() },
        )
        .await
        .unwrap();
    let with_net = lab.versions.net_version(IdeaId(1), &champion, net.id, &net.name).await.unwrap();
    assert_eq!(with_net.name, "v1 + other");
    assert_eq!(
        (with_net.commit.as_str(), with_net.net, with_net.bench),
        (champion.commit.as_str(), Some(net.id), None)
    );
    let (plain, _) = lab.versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    let (netted, path) = lab.versions.binary_of(&with_net, &[]).await.unwrap();
    assert_ne!(plain.id, netted.id);
    assert_eq!((plain.net, netted.net), (None, Some(net.id)));
    assert_ne!(plain.bench, netted.bench, "the net changes the bench");
    assert!(path.to_string_lossy().contains(&format!("-net{}", net.id)));
    // Cached: asking again builds nothing.
    let builds = lab.fake.builds.load(std::sync::atomic::Ordering::SeqCst);
    lab.versions.binary_of(&with_net, &[]).await.unwrap();
    assert_eq!(lab.fake.builds.load(std::sync::atomic::Ordering::SeqCst), builds);
}

fn net_idea(title: &str, train: TrainSpec) -> NewIdeaRequest {
    NewIdeaRequest {
        title: title.into(),
        hypothesis: "A fine-tune on fresh self-play helps".into(),
        kind: IdeaKind::Net,
        branch: String::new(),
        base: None,
        author: Author::Claude,
        tags: vec![],
        notes: None,
        long: false,
        tune: None,
        train: Some(train),
        custom: None,
        fix_bench: false,
    }
}

async fn ended(lab: &Lab, id: IdeaId) -> dameless_core::ideas::Idea {
    for _ in 0..2000 {
        let idea = lab.ideas.get(id).await.unwrap();
        if idea.status.is_final() {
            return idea;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("idea {id} did not end");
}

#[tokio::test]
async fn a_net_idea_trains_then_tests_the_champions_code_with_the_net_and_promotes_it() {
    let lab = lab(1, 10 * GB).await;
    dataset(&lab, "dfrc", 1000).await;
    lab.fake.rounds([0, 0, 0, 0, 1]);
    lab.fake.pair_scale.store(50, std::sync::atomic::Ordering::SeqCst);
    let mut sub = lab.hub.bus.subscribe(0, TopicFilter::of(["idea:*"]));
    let idea = lab.ideas.create(net_idea("Fine-tune on dfrc", small_run(2))).await.unwrap();
    assert_eq!(
        idea.trials.iter().map(|t| t.stage).collect::<Vec<_>>(),
        [Stage::Train, Stage::Fingerprint, Stage::Screen, Stage::Short]
    );
    assert_eq!((idea.branch.as_str(), idea.version), ("main", None));
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    assert!(idea.trials.iter().all(|t| t.status == TrialStatus::Passed));
    let train = &idea.trials[0];
    assert!(train.detail.as_deref().unwrap_or_default().starts_with(&format!("net-{}: 2 superbatches", idea.id)));

    let version = lab.hub.store.version(idea.version.unwrap()).await.unwrap().unwrap();
    let champion = lab.versions.champion().await.unwrap();
    assert_eq!(version.name, format!("v1 + net-{}", idea.id));
    assert_eq!(version.commit, champion.commit);
    let view = lab.training.idea(idea.id).await.unwrap();
    assert_eq!(view.steps.len(), 2);
    assert_eq!(view.net.as_ref().map(|n| n.id), version.net);
    let progress: Vec<_> = std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            BusNext::Event(e) if e.kind == EventKind::TrainProgress => Some(e),
            _ => None,
        })
        .collect();
    assert!(!progress.is_empty() && progress.iter().all(|e| e.topic == format!("idea:{}", idea.id)));

    // Promoting records "code plus net" on the champion line, and builds of the champion embed the net from then on.
    let w = lab.ideas.promote(idea.id).await.unwrap();
    assert_eq!(lab.scheduler.wait(w.id).await.unwrap().status, WorkStatus::Done);
    let v2 = lab.versions.champion().await.unwrap();
    assert_eq!((v2.name.as_str(), v2.commit.as_str(), v2.net), ("v2", champion.commit.as_str(), version.net));
    let (build, _) = lab.versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    assert_eq!(build.net, version.net);
    // The champion's net is now the trained one, so the next run fine-tunes it.
    assert_eq!(lab.nets.champion_net(&lab.versions).await.unwrap().id, version.net.unwrap());
    // An idea on the champion is tested with the net too.
    let other = lab.versions.idea_version("main", IdeaId(99), Some(v2.id)).await;
    assert!(other.is_ok());
}

#[tokio::test]
async fn a_stopped_net_idea_resumes_its_run_from_the_last_checkpoint() {
    let lab = lab(1, 10 * GB).await;
    dataset(&lab, "dfrc", 1000).await;
    lab.fake.rounds([0, 0, 0, 0, 1]);
    lab.fake.pair_scale.store(50, std::sync::atomic::Ordering::SeqCst);
    lab.trainer.set_env("FAKE_SLEEP_MS", "200");
    let idea = lab.ideas.create(net_idea("Long fine-tune", small_run(6))).await.unwrap();
    eventually("two superbatches", || async { lab.hub.store.idea_train_steps(idea.id).await.unwrap().len() >= 2 })
        .await;
    lab.ideas.stop(idea.id).await.unwrap();
    let stopped = ended(&lab, idea.id).await;
    assert_eq!(stopped.status, IdeaStatus::Stopped);
    let run_dir = lab.hub.config.runs_dir().join(format!("idea-{}", idea.id));
    assert!(run_dir.join("checkpoints").is_dir(), "the checkpoint stays for the resume");

    lab.trainer.clear_env();
    lab.ideas.resume(idea.id).await.unwrap();
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    let steps = lab.hub.store.idea_train_steps(idea.id).await.unwrap();
    let sbs: Vec<u32> = steps.iter().map(|s| s.superbatch).collect();
    assert_eq!(sbs.last(), Some(&6));
    // The second run carried on: no superbatch was trained twice past the checkpoint it resumed from.
    assert!(sbs.windows(2).all(|w| w[1] > w[0]), "{sbs:?}");
    let train_works: Vec<_> =
        lab.scheduler.list(50).await.unwrap().recent.into_iter().filter(|w| w.kind == WorkKind::Train).collect();
    assert_eq!(train_works.len(), 2);
    let resumed = train_works.iter().map(|w| w.id).max().unwrap();
    let log = std::fs::read_to_string(lab.hub.config.work_log_path(resumed)).unwrap();
    assert!(log.contains("--resume"), "{log}");
}
