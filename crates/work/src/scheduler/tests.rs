use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use dameless_core::{
    GameId,
    work::{Priority, Subject},
};

use super::*;
use crate::testing::{bare_hub, eventually};

/// What the fake executor does with each item: its spec says.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
struct Job {
    /// Units of work; each asks for cores, holds them a moment, then passes a safe point.
    units: u32,
    /// Loop until stopped instead.
    #[serde(default)]
    forever: bool,
    #[serde(default)]
    fail: bool,
    #[serde(default)]
    panic: bool,
}

/// A fake executor of one kind that records what it started, in order, and the checkpoints it found.
struct Fake {
    kind: WorkKind,
    started: Mutex<Vec<WorkId>>,
    checkpoints: Mutex<Vec<Option<u32>>>,
    units: AtomicU32,
    /// Units are held until this is set.
    release: AtomicBool,
}

impl Fake {
    fn new(kind: WorkKind) -> Arc<Self> {
        Arc::new(Self {
            kind,
            started: Mutex::default(),
            checkpoints: Mutex::default(),
            units: AtomicU32::new(0),
            release: AtomicBool::new(true),
        })
    }
}

#[async_trait]
impl Executor for Fake {
    fn kind(&self) -> WorkKind {
        self.kind
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let job: Job = cx.spec()?;
        self.started.lock().unwrap().push(cx.id);
        let from = cx.checkpoint::<u32>();
        self.checkpoints.lock().unwrap().push(from);
        if job.fail {
            return Err(Error::internal("the engine crashed"));
        }
        if job.panic {
            panic!("boom");
        }
        let mut done = from.unwrap_or(0);
        while job.forever || done < job.units {
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            // Network work (imports) takes no cores.
            let grant = if self.kind.uses_cores() {
                let Some(grant) = cx.cores(1, cx.hub.ledger.total()).await else { return Err(cx.stopped()) };
                Some(grant)
            } else {
                None
            };
            loop {
                tokio::time::sleep(Duration::from_millis(20)).await;
                if self.release.load(Ordering::SeqCst) || cx.cancel.is_cancelled() {
                    break;
                }
            }
            drop(grant);
            done += 1;
            self.units.fetch_add(1, Ordering::SeqCst);
            cx.save_checkpoint(&done).await?;
        }
        Ok(serde_json::json!({ "units": done }))
    }
}

fn new(kind: WorkKind, priority: Priority, job: Job) -> NewWork {
    NewWork {
        kind,
        priority,
        title: format!("{kind:?}"),
        subject: Subject::Game { id: GameId(1) },
        spec: serde_json::to_value(job).unwrap(),
    }
}

fn units(n: u32) -> Job {
    Job { units: n, ..Default::default() }
}

fn forever() -> Job {
    Job { forever: true, ..Default::default() }
}

/// Waits until the item's log holds `needle` at least `times` times: the executor writes "Paused: …" once it has
/// stopped at its safe point, which is when it can no longer do units.
async fn logged(s: &Scheduler, id: WorkId, needle: &str, times: usize) {
    let path = s.hub().config.work_log_path(id);
    eventually(needle, || {
        let path = path.clone();
        async move { std::fs::read_to_string(path).unwrap_or_default().matches(needle).count() >= times }
    })
    .await;
}

async fn status(s: &Scheduler, id: WorkId) -> WorkStatus {
    s.get(id).await.unwrap().status
}

#[tokio::test]
async fn runs_by_priority_then_age_one_at_a_time_per_lane() {
    let (hub, _dir) = bare_hub(4);
    let s = Scheduler::new(hub);
    let reviews = Fake::new(WorkKind::Review);
    s.register(reviews.clone());
    let a = s.submit(new(WorkKind::Review, Priority::Review, units(1))).await.unwrap();
    let b = s.submit(new(WorkKind::Review, Priority::Review, units(1))).await.unwrap();
    let c = s.submit(new(WorkKind::Review, Priority::Live, units(1))).await.unwrap();
    let d = s.submit(new(WorkKind::Review, Priority::Review, units(1))).await.unwrap();
    assert_eq!(a.status, WorkStatus::Queued);
    s.start().await.unwrap();
    for id in [a.id, b.id, c.id, d.id] {
        assert_eq!(s.wait(id).await.unwrap().status, WorkStatus::Done);
    }
    assert_eq!(*reviews.started.lock().unwrap(), [c.id, a.id, b.id, d.id]);
    let log = std::fs::read_to_string(s.hub().config.work_log_path(a.id)).unwrap();
    assert!(log.contains("Queued") && log.contains("Started.") && log.contains("Done."), "{log}");
    let lists = s.list(10).await.unwrap();
    assert_eq!((lists.running.len(), lists.queued.len(), lists.recent.len()), (0, 0, 4));
}

#[tokio::test]
async fn a_background_review_yields_to_trials_and_never_holds_up_a_review() {
    let (hub, _dir) = bare_hub(4);
    let s = Scheduler::new(hub);
    let reviews = Fake::new(WorkKind::Review);
    let trials = Fake::new(WorkKind::Calibrate);
    s.register(reviews.clone());
    s.register(trials.clone());
    s.start().await.unwrap();
    // Reviewing everything takes every core for each unit, hands them back and asks again at once.
    let all = s.submit(new(WorkKind::Review, Priority::Background, forever())).await.unwrap();
    eventually("the background review going", || async { reviews.units.load(Ordering::SeqCst) > 0 }).await;
    // A trial starts beside it and is served first.
    let t = s.submit(new(WorkKind::Calibrate, Priority::Background, forever())).await.unwrap();
    eventually("the trial playing", || async { trials.units.load(Ordering::SeqCst) > 2 }).await;
    // A review asked for now runs at once in its own lane, and ends, while the background one waits its turn.
    let one = s.submit(new(WorkKind::Review, Priority::Review, units(1))).await.unwrap();
    assert_eq!(s.wait(one.id).await.unwrap().status, WorkStatus::Done);
    assert!(s.is_running(all.id) && s.is_running(t.id));
    s.stop(t.id).await.unwrap();
    s.stop(all.id).await.unwrap();
}

#[tokio::test]
async fn imports_run_at_once_and_builds_do_not_wait_for_trials() {
    let (hub, _dir) = bare_hub(4);
    let s = Scheduler::new(hub);
    let trials = Fake::new(WorkKind::Calibrate);
    s.register(trials.clone());
    for kind in [WorkKind::Import, WorkKind::Review] {
        s.register(Fake::new(kind));
    }
    s.start().await.unwrap();
    // The trial asks for every core for each unit, hands them back and asks again at once.
    let t = s.submit(new(WorkKind::Calibrate, Priority::Background, forever())).await.unwrap();
    eventually("the trial playing", || async { trials.units.load(Ordering::SeqCst) > 0 }).await;
    let i1 = s.submit(new(WorkKind::Import, Priority::Review, forever())).await.unwrap();
    let i2 = s.submit(new(WorkKind::Import, Priority::Review, forever())).await.unwrap();
    let b = s.submit(new(WorkKind::Review, Priority::Live, units(1))).await.unwrap();
    // The build runs although a trial holds its lane and takes every core between units: the ledger serves the
    // waiting build first.
    assert_eq!(s.wait(b.id).await.unwrap().status, WorkStatus::Done);
    eventually("both imports running", || async { s.is_running(i1.id) && s.is_running(i2.id) }).await;
    assert!(s.is_running(t.id));
    // And the trial gets its cores again once the build is done.
    let after = trials.units.load(Ordering::SeqCst);
    eventually("trial units after the build", || async { trials.units.load(Ordering::SeqCst) > after + 1 }).await;
    for id in [t.id, i1.id, i2.id] {
        s.stop(id).await.unwrap();
        assert_eq!(s.wait(id).await.unwrap().status, WorkStatus::Stopped);
    }
}

#[tokio::test]
async fn records_failures_panics_and_waits_for_missing_executors() {
    let (hub, _dir) = bare_hub(2);
    let s = Scheduler::new(hub);
    s.register(Fake::new(WorkKind::Review));
    s.start().await.unwrap();
    let failed =
        s.submit(new(WorkKind::Review, Priority::Review, Job { fail: true, ..Default::default() })).await.unwrap();
    let w = s.wait(failed.id).await.unwrap();
    assert_eq!((w.status, w.error.as_deref()), (WorkStatus::Failed, Some("the engine crashed")));
    let panicked =
        s.submit(new(WorkKind::Review, Priority::Review, Job { panic: true, ..Default::default() })).await.unwrap();
    let w = s.wait(panicked.id).await.unwrap();
    assert_eq!(w.status, WorkStatus::Failed);
    assert!(w.error.unwrap().contains("panicked"));
    // No executor for the kind: it waits in the queue.
    let orphan = s.submit(new(WorkKind::Setup, Priority::Background, Job::default())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(status(&s, orphan.id).await, WorkStatus::Queued);
}

struct Ends(Mutex<Vec<(WorkId, WorkStatus, Option<serde_json::Value>)>>);

#[async_trait]
impl EndHook for Ends {
    async fn ended(&self, work: &Work, result: Option<&serde_json::Value>) {
        self.0.lock().unwrap().push((work.id, work.status, result.cloned()));
    }
}

#[tokio::test]
async fn stops_running_and_queued_work_and_calls_back() {
    let (hub, _dir) = bare_hub(2);
    let s = Scheduler::new(hub);
    let fake = Fake::new(WorkKind::Review);
    s.register(fake.clone());
    let ends = Arc::new(Ends(Mutex::default()));
    s.on_end(WorkKind::Review, ends.clone());
    s.start().await.unwrap();
    let running = s.submit(new(WorkKind::Review, Priority::Review, forever())).await.unwrap();
    let queued = s.submit(new(WorkKind::Review, Priority::Review, units(1))).await.unwrap();
    eventually("the first running", || async { fake.units.load(Ordering::SeqCst) > 0 }).await;
    assert_eq!(status(&s, queued.id).await, WorkStatus::Queued);
    assert_eq!(s.stop(queued.id).await.unwrap().status, WorkStatus::Stopped);
    s.stop(running.id).await.unwrap();
    assert_eq!(s.wait(running.id).await.unwrap().status, WorkStatus::Stopped);
    // Stopping ended work changes nothing.
    assert_eq!(s.stop(running.id).await.unwrap().status, WorkStatus::Stopped);
    let done = s.submit(new(WorkKind::Review, Priority::Review, units(2))).await.unwrap();
    s.wait(done.id).await.unwrap();
    eventually("three ends", || async { ends.0.lock().unwrap().len() == 3 }).await;
    let ends = ends.0.lock().unwrap().clone();
    assert_eq!(
        ends,
        [
            (queued.id, WorkStatus::Stopped, None),
            (running.id, WorkStatus::Stopped, None),
            (done.id, WorkStatus::Done, Some(serde_json::json!({ "units": 2 })))
        ]
    );
}

#[tokio::test]
async fn pauses_and_resumes_running_and_queued_work() {
    let (hub, _dir) = bare_hub(2);
    let s = Scheduler::new(hub);
    let fake = Fake::new(WorkKind::Review);
    s.register(fake.clone());
    s.start().await.unwrap();
    let w = s.submit(new(WorkKind::Review, Priority::Review, forever())).await.unwrap();
    eventually("a unit done", || async { fake.units.load(Ordering::SeqCst) > 0 }).await;
    let paused = s.pause(w.id, PAUSED_BY_YOU).await.unwrap();
    assert_eq!((paused.status, paused.paused_for.as_deref()), (WorkStatus::Paused, Some(PAUSED_BY_YOU)));
    logged(&s, w.id, "Paused: Paused by you", 1).await;
    let at = fake.units.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fake.units.load(Ordering::SeqCst), at, "no units while paused");
    let resumed = s.resume(w.id).await.unwrap();
    assert_eq!((resumed.status, resumed.paused_for), (WorkStatus::Running, None));
    eventually("units again", || async { fake.units.load(Ordering::SeqCst) > at }).await;

    // A queued item that is paused is held until resumed.
    let held = s.submit(new(WorkKind::Review, Priority::Live, units(1))).await.unwrap();
    assert_eq!(s.pause(held.id, PAUSED_BY_YOU).await.unwrap().status, WorkStatus::Paused);
    s.stop(w.id).await.unwrap();
    s.wait(w.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(status(&s, held.id).await, WorkStatus::Paused);
    assert_eq!(s.resume(held.id).await.unwrap().status, WorkStatus::Queued);
    assert_eq!(s.wait(held.id).await.unwrap().status, WorkStatus::Done);
    assert!(s.pause(held.id, "x").await.is_err(), "ended work cannot pause");
}

#[tokio::test]
async fn live_play_pauses_the_lowest_priority_work_until_cores_are_back() {
    let (hub, _dir) = bare_hub(4);
    let s = Scheduler::new(hub.clone());
    let trials = Fake::new(WorkKind::Calibrate);
    // Units are held until released, so the trial holds its cores when live play arrives.
    trials.release.store(false, Ordering::SeqCst);
    s.register(trials.clone());
    s.start().await.unwrap();
    let trial = s.submit(new(WorkKind::Calibrate, Priority::Background, forever())).await.unwrap();
    eventually("the trial holds cores", || async { hub.ledger.snapshot().granted == 4 }).await;

    let game = hub.ledger.reserve("game:1", 2);
    eventually("the trial paused", || async {
        let w = s.get(trial.id).await.unwrap();
        w.status == WorkStatus::Paused && w.paused_for.as_deref() == Some(LIVE_PLAY)
    })
    .await;
    // At its safe point it gives the cores back and, with two free, carries on with those.
    trials.release.store(true, Ordering::SeqCst);
    eventually("the trial running again", || async { status(&s, trial.id).await == WorkStatus::Running }).await;
    let before = trials.units.load(Ordering::SeqCst);
    eventually("trial units", || async { trials.units.load(Ordering::SeqCst) > before + 1 }).await;

    // With every core reserved, it stays paused until the games end.
    trials.release.store(false, Ordering::SeqCst);
    let all = hub.ledger.reserve("game:2", 4);
    eventually("paused again", || async { status(&s, trial.id).await == WorkStatus::Paused }).await;
    // The first pause may have been lifted before the trial reached its safe point (and so never logged): count from
    // here. The trial is still in its unit, so it cannot have logged this pause yet.
    let log_path = hub.config.work_log_path(trial.id);
    let pauses = std::fs::read_to_string(&log_path).unwrap_or_default().matches("Paused: A live game").count();
    trials.release.store(true, Ordering::SeqCst);
    logged(&s, trial.id, "Paused: A live game", pauses + 1).await;
    let at = trials.units.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(status(&s, trial.id).await, WorkStatus::Paused);
    assert_eq!(trials.units.load(Ordering::SeqCst), at, "no units while live play holds every core");
    drop(all);
    drop(game);
    eventually("running after the games", || async { status(&s, trial.id).await == WorkStatus::Running }).await;
    let log = std::fs::read_to_string(hub.config.work_log_path(trial.id)).unwrap();
    assert!(log.contains("Pausing for live play") && log.contains("carrying on"), "{log}");

    // Work a person paused is not resumed by the ledger.
    s.pause(trial.id, PAUSED_BY_YOU).await.unwrap();
    drop(hub.ledger.reserve("game:3", 1));
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(s.get(trial.id).await.unwrap().paused_for.as_deref(), Some(PAUSED_BY_YOU));
    s.stop(trial.id).await.unwrap();
    s.wait(trial.id).await.unwrap();
}

#[tokio::test]
async fn work_running_at_a_restart_resumes_from_its_checkpoint() {
    let (hub, _dir) = bare_hub(2);
    let store = hub.store.clone();
    let job = serde_json::to_value(units(3)).unwrap();
    let mut ids = Vec::new();
    for paused_for in [None, Some(LIVE_PLAY), Some(PAUSED_BY_YOU)] {
        let w = store
            .insert_work(NewWork {
                kind: WorkKind::Review,
                priority: Priority::Review,
                title: "r".into(),
                subject: Subject::Game { id: GameId(1) },
                spec: job.clone(),
            })
            .await
            .unwrap();
        let status = if paused_for.is_some() { WorkStatus::Paused } else { WorkStatus::Running };
        let update =
            WorkUpdate { status: Some(status), paused_for: Some(paused_for.map(str::to_owned)), ..Default::default() };
        store.update_work(w.id, update).await.unwrap();
        store.set_work_checkpoint(w.id, serde_json::json!(2)).await.unwrap();
        ids.push(w.id);
    }
    let [running, live, yours] = ids[..] else { unreachable!() };

    let s = Scheduler::new(hub.clone());
    let fake = Fake::new(WorkKind::Review);
    s.register(fake.clone());
    s.start().await.unwrap();
    s.start().await.unwrap();
    assert_eq!(s.wait(running).await.unwrap().status, WorkStatus::Done);
    assert_eq!(s.wait(live).await.unwrap().status, WorkStatus::Done);
    // Each did only the unit left after its checkpoint.
    assert_eq!(fake.checkpoints.lock().unwrap().clone(), [Some(2), Some(2)]);
    assert_eq!(fake.units.load(Ordering::SeqCst), 2);
    assert_eq!(status(&s, yours).await, WorkStatus::Paused);
    let log = std::fs::read_to_string(hub.config.work_log_path(running)).unwrap();
    assert!(log.contains("restarted") && log.contains("from its checkpoint"), "{log}");
}

#[tokio::test]
async fn shutdown_leaves_running_work_to_resume() {
    let (hub, _dir) = bare_hub(2);
    let s = Scheduler::new(hub.clone());
    let fake = Fake::new(WorkKind::Review);
    s.register(fake.clone());
    s.start().await.unwrap();
    let w = s.submit(new(WorkKind::Review, Priority::Review, forever())).await.unwrap();
    eventually("running", || async { fake.units.load(Ordering::SeqCst) > 0 }).await;
    s.shutdown();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(status(&s, w.id).await, WorkStatus::Running);
    assert_eq!(hub.store.requeue_running_work().await.unwrap().len(), 1);
}
