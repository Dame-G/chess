//! The scheduler: the one queue every piece of background work goes through.
//!
//! Work is stored first (`work` table) and then started by a loop when its executor is free: queued items by priority,
//! then oldest first. Each kind has a lane that runs one item at a time (builds; trials and tunes together; reviews;
//! promotions), so a trial waiting for its build never blocks the build, and imports, which only use the network, run
//! at once. Cores are not handed out here: a running item asks the ledger for them itself (`cx.cores`), between units.
//!
//! Live play comes first. When a live game or live evaluation reserves cores and the ledger is oversubscribed, the
//! lowest-priority running CPU work is paused at its next safe point; it is resumed once cores are free again. A person
//! can pause, resume and stop work too. Every transition is written to the item's log.
//!
//! At startup, work that was running when the app stopped is queued again and resumes from its checkpoint.

use std::{
    collections::HashMap,
    io::Write,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use dameless_core::{
    Error, WorkId,
    events::{Payload, WorkLog},
    work::{Work, WorkKind, WorkStatus},
};
use dameless_hub::{Control, Executor, Hub, WorkContext};
use dameless_store::{WorkLists, WorkUpdate, now};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

pub use dameless_store::NewWork;

/// Why work paused itself for live play, as people read it. Work paused for this reason resumes by itself.
pub const LIVE_PLAY: &str = "A live game";

/// Why a person paused work.
pub const PAUSED_BY_YOU: &str = "Paused by you";

/// Called when a work item of a kind ends: done (with the executor's result), failed or stopped. Not called when the
/// app shuts down, since that work resumes at the next start.
#[async_trait]
pub trait EndHook: Send + Sync {
    async fn ended(&self, work: &Work, result: Option<&serde_json::Value>);
}

/// The queue. Cheap to clone: every clone shares the loop and the running items.
#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}

struct Inner {
    hub: Arc<Hub>,
    executors: Mutex<HashMap<WorkKind, Arc<dyn Executor>>>,
    hooks: Mutex<HashMap<WorkKind, Vec<Arc<dyn EndHook>>>>,
    running: Mutex<HashMap<WorkId, Running>>,
    /// Something changed that may let queued work start.
    wake: Notify,
    /// Some work ended.
    ended: Notify,
    shutdown: CancellationToken,
    started: Mutex<bool>,
}

/// A running item, as the scheduler steers it.
struct Running {
    kind: WorkKind,
    rank: u8,
    cancel: CancellationToken,
    control: watch::Sender<Control>,
    /// A person stopped it: its end is recorded as stopped whatever it returns.
    stopping: bool,
    /// A person paused it: only a person resumes it.
    paused_by_you: bool,
    /// Paused for live play, holding this many cores when it was.
    preempted: Option<u32>,
}

/// One item at a time per lane; `None` runs at once.
fn lane(kind: WorkKind) -> Option<u8> {
    match kind {
        WorkKind::Build => Some(0),
        WorkKind::Trial | WorkKind::Tune | WorkKind::Calibrate => Some(1),
        WorkKind::Review => Some(2),
        WorkKind::Promote => Some(3),
        WorkKind::Import => None,
    }
}

/// Whether an item waits for a free core before it starts. Promotions are git work, imports network work.
fn needs_a_core(kind: WorkKind) -> bool {
    kind.uses_cores() && kind != WorkKind::Promote
}

impl Scheduler {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self {
            inner: Arc::new(Inner {
                hub,
                executors: Mutex::default(),
                hooks: Mutex::default(),
                running: Mutex::default(),
                wake: Notify::new(),
                ended: Notify::new(),
                shutdown: CancellationToken::new(),
                started: Mutex::new(false),
            }),
        }
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.inner.hub
    }

    /// Adds the executor for its kind (replacing one registered before).
    pub fn register(&self, executor: Arc<dyn Executor>) {
        self.inner.executors.lock().unwrap().insert(executor.kind(), executor);
        self.inner.wake.notify_one();
    }

    /// Calls `hook` whenever an item of `kind` ends.
    pub fn on_end(&self, kind: WorkKind, hook: Arc<dyn EndHook>) {
        self.inner.hooks.lock().unwrap().entry(kind).or_default().push(hook);
    }

    /// Queues work that was running when the app stopped (and work paused for live play), then starts the loop. Call
    /// once, after registering the executors; later calls do nothing.
    pub async fn start(&self) -> Result<(), Error> {
        {
            let mut started = self.inner.started.lock().unwrap();
            if *started {
                return Ok(());
            }
            *started = true;
        }
        let store = &self.inner.hub.store;
        for w in store.requeue_running_work().await? {
            self.log(w.id, "The app restarted: queued to resume from its checkpoint.");
            self.inner.hub.bus.emit(Payload::WorkUpserted(Box::new(w)));
        }
        // Live play is gone after a restart, so work paused for it carries on; work a person paused stays paused.
        for w in store.work_lists(0).await?.running {
            if w.status == WorkStatus::Paused && w.paused_for.as_deref() == Some(LIVE_PLAY) {
                let w = store
                    .update_work(
                        w.id,
                        WorkUpdate { status: Some(WorkStatus::Queued), paused_for: Some(None), ..Default::default() },
                    )
                    .await?;
                self.log(w.id, "The app restarted: queued to resume from its checkpoint.");
                self.inner.hub.bus.emit(Payload::WorkUpserted(Box::new(w)));
            }
        }
        let this = self.clone();
        tokio::spawn(async move { this.run_loop().await });
        Ok(())
    }

    /// Stops the loop and every running item without recording them as stopped: they are queued again, to resume, at
    /// the next start.
    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        for r in self.inner.running.lock().unwrap().values() {
            r.cancel.cancel();
        }
    }

    /// Stores new work and wakes the loop; returns it, queued.
    pub async fn submit(&self, new: NewWork) -> Result<Work, Error> {
        let work = self.inner.hub.store.insert_work(new).await?;
        self.log(work.id, format!("Queued: {}.", work.title));
        self.emit(&work);
        self.inner.wake.notify_one();
        Ok(work)
    }

    pub async fn get(&self, id: WorkId) -> Result<Work, Error> {
        self.inner.hub.store.work(id).await?.ok_or_else(|| Error::not_found(format!("No work {id}")))
    }

    /// Running, queued and recently finished work.
    pub async fn list(&self, recent: u32) -> Result<WorkLists, Error> {
        self.inner.hub.store.work_lists(recent).await
    }

    /// Whether the item is running now (started and not ended).
    pub fn is_running(&self, id: WorkId) -> bool {
        self.inner.running.lock().unwrap().contains_key(&id)
    }

    /// Stops an item: a queued or held one at once, a running one at its next safe point (its end is recorded as
    /// stopped). Stopping finished work changes nothing.
    pub async fn stop(&self, id: WorkId) -> Result<Work, Error> {
        let work = self.get(id).await?;
        {
            let mut running = self.inner.running.lock().unwrap();
            if let Some(r) = running.get_mut(&id) {
                r.stopping = true;
                r.cancel.cancel();
                drop(running);
                self.log(id, "Stopping…");
                return Ok(work);
            }
        }
        if work.status.is_final() {
            return Ok(work);
        }
        let work = self
            .inner
            .hub
            .store
            .update_work(
                id,
                WorkUpdate {
                    status: Some(WorkStatus::Stopped),
                    paused_for: Some(None),
                    ended_at: Some(now()),
                    ..Default::default()
                },
            )
            .await?;
        self.log(id, "Stopped before it started.");
        self.emit(&work);
        self.inner.ended.notify_waiters();
        self.call_hooks(&work, None).await;
        Ok(work)
    }

    /// Pauses an item: a running one waits at its next safe point, a queued one is held. Only [`Scheduler::resume`]
    /// carries it on.
    pub async fn pause(&self, id: WorkId, reason: impl Into<String>) -> Result<Work, Error> {
        let reason = reason.into();
        let work = self.get(id).await?;
        if work.status.is_final() {
            return Err(Error::conflict(format!("Work {id} has ended")));
        }
        {
            let mut running = self.inner.running.lock().unwrap();
            if let Some(r) = running.get_mut(&id) {
                r.paused_by_you = true;
                r.preempted = None;
                r.control.send_replace(Control::Pause { reason: reason.clone() });
            }
        }
        let work = self.set_paused(id, Some(reason.clone())).await?;
        self.log(id, format!("Pause asked: {reason}."));
        Ok(work)
    }

    /// Carries on paused work: a running item continues from its safe point, a held one is queued again.
    pub async fn resume(&self, id: WorkId) -> Result<Work, Error> {
        let work = self.get(id).await?;
        if work.status.is_final() {
            return Err(Error::conflict(format!("Work {id} has ended")));
        }
        let was_running = {
            let mut running = self.inner.running.lock().unwrap();
            match running.get_mut(&id) {
                Some(r) => {
                    r.paused_by_you = false;
                    r.preempted = None;
                    r.control.send_replace(Control::Run);
                    true
                }
                None => false,
            }
        };
        let status = if was_running { WorkStatus::Running } else { WorkStatus::Queued };
        if work.status == status && work.paused_for.is_none() {
            return Ok(work);
        }
        let work = self
            .inner
            .hub
            .store
            .update_work(id, WorkUpdate { status: Some(status), paused_for: Some(None), ..Default::default() })
            .await?;
        self.log(id, "Resume asked.");
        self.emit(&work);
        self.inner.wake.notify_one();
        Ok(work)
    }

    /// Waits until the item has ended and returns it.
    pub async fn wait(&self, id: WorkId) -> Result<Work, Error> {
        loop {
            let ended = self.inner.ended.notified();
            tokio::pin!(ended);
            ended.as_mut().enable();
            let work = self.get(id).await?;
            if work.status.is_final() {
                return Ok(work);
            }
            ended.await;
        }
    }

    async fn set_paused(&self, id: WorkId, reason: Option<String>) -> Result<Work, Error> {
        let status = if reason.is_some() { WorkStatus::Paused } else { WorkStatus::Running };
        let work = self
            .inner
            .hub
            .store
            .update_work(id, WorkUpdate { status: Some(status), paused_for: Some(reason), ..Default::default() })
            .await?;
        self.emit(&work);
        Ok(work)
    }

    fn emit(&self, work: &Work) {
        self.inner.hub.bus.emit(Payload::WorkUpserted(Box::new(work.clone())));
    }

    /// Writes a line to the item's log and sends it live, as [`WorkContext::log`] does for running work.
    fn log(&self, id: WorkId, line: impl AsRef<str>) {
        let hub = &self.inner.hub;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(hub.config.work_log_path(id)) {
            let _ = writeln!(f, "{}", line.as_ref());
        }
        hub.bus.emit(Payload::WorkLog(WorkLog { id, line: line.as_ref().to_owned() }));
    }

    async fn call_hooks(&self, work: &Work, result: Option<&serde_json::Value>) {
        let hooks = self.inner.hooks.lock().unwrap().get(&work.kind).cloned().unwrap_or_default();
        for hook in hooks {
            hook.ended(work, result).await;
        }
    }

    async fn run_loop(self) {
        let mut ledger = self.inner.hub.ledger.changed();
        loop {
            if let Err(e) = self.fill().await {
                eprintln!("scheduler: {e}");
            }
            self.rebalance().await;
            tokio::select! {
                _ = self.inner.shutdown.cancelled() => return,
                _ = self.inner.wake.notified() => {}
                r = ledger.changed() => if r.is_err() { return },
            }
        }
    }

    /// Starts every queued item whose lane is free, in queue order.
    async fn fill(&self) -> Result<(), Error> {
        let queued = self.inner.hub.store.queued_work().await?;
        for w in queued {
            if self.inner.shutdown.is_cancelled() {
                return Ok(());
            }
            let Some(executor) = self.inner.executors.lock().unwrap().get(&w.kind).cloned() else { continue };
            let busy = {
                let running = self.inner.running.lock().unwrap();
                lane(w.kind).is_some_and(|l| running.values().any(|r| lane(r.kind) == Some(l)))
            };
            if busy || (needs_a_core(w.kind) && self.inner.hub.ledger.free() == 0) {
                continue;
            }
            self.launch(w, executor).await?;
        }
        Ok(())
    }

    async fn launch(&self, w: Work, executor: Arc<dyn Executor>) -> Result<(), Error> {
        let hub = &self.inner.hub;
        let spec = hub.store.work_spec(w.id).await?.unwrap_or(serde_json::Value::Null);
        let checkpoint = hub.store.work_checkpoint(w.id).await?;
        let cancel = CancellationToken::new();
        let (control, rx) = watch::channel(Control::Run);
        self.inner.running.lock().unwrap().insert(
            w.id,
            Running {
                kind: w.kind,
                rank: w.priority.rank(),
                cancel: cancel.clone(),
                control,
                stopping: false,
                paused_by_you: false,
                preempted: None,
            },
        );
        let work = hub
            .store
            .update_work(
                w.id,
                WorkUpdate {
                    status: Some(WorkStatus::Running),
                    started_at: w.started_at.is_none().then(now),
                    ..Default::default()
                },
            )
            .await?;
        self.log(w.id, if checkpoint.is_some() { "Started again, from its checkpoint." } else { "Started." });
        self.emit(&work);

        let cx = WorkContext::new(hub.clone(), w.id, w.kind, spec, checkpoint, cancel, rx);
        let this = self.clone();
        let id = w.id;
        tokio::spawn(async move {
            let outcome = match tokio::spawn(async move { executor.run(cx).await }).await {
                Ok(outcome) => outcome,
                Err(e) => Err(Error::internal(format!("the work panicked: {e}"))),
            };
            this.finish(id, outcome).await;
        });
        Ok(())
    }

    /// Records how a run ended, unless the app is shutting down (then it stays running, to be queued at the next start).
    async fn finish(&self, id: WorkId, outcome: Result<serde_json::Value, Error>) {
        let Some(r) = self.inner.running.lock().unwrap().remove(&id) else { return };
        if self.inner.shutdown.is_cancelled() {
            return;
        }
        let (status, error, line) = match &outcome {
            Ok(_) => (WorkStatus::Done, None, "Done.".to_owned()),
            Err(e) if r.stopping || WorkContext::is_stopped(e) => (WorkStatus::Stopped, None, "Stopped.".to_owned()),
            Err(e) => (WorkStatus::Failed, Some(e.to_string()), format!("Failed: {e}")),
        };
        if let Ok(v) = &outcome
            && !v.is_null()
        {
            self.log(id, format!("Result: {v}"));
        }
        self.log(id, line);
        let update = WorkUpdate {
            status: Some(status),
            cores: Some(0),
            paused_for: Some(None),
            error,
            ended_at: Some(now()),
            ..Default::default()
        };
        match self.inner.hub.store.update_work(id, update).await {
            Ok(work) => {
                self.emit(&work);
                // Callbacks first, so whoever waits on this item sees what they did (the next trial queued).
                self.call_hooks(&work, outcome.as_ref().ok()).await;
            }
            Err(e) => eprintln!("scheduler: record the end of work {id}: {e}"),
        }
        self.inner.ended.notify_waiters();
        self.inner.wake.notify_one();
    }

    /// Pauses the lowest-priority CPU work while live play oversubscribes the machine, and resumes work paused for it
    /// once cores are free again.
    async fn rebalance(&self) {
        let snap = self.inner.hub.ledger.snapshot();
        let held = |id: WorkId| snap.grants.iter().filter(|g| g.work == id).map(|g| g.cores).sum::<u32>();
        let overflow = (snap.reserved + snap.granted).saturating_sub(snap.total);
        let mut changes: Vec<(WorkId, Option<String>)> = Vec::new();
        {
            let mut running = self.inner.running.lock().unwrap();
            if overflow > 0 && snap.reserved > 0 {
                // Already paused work gives its cores back at its next safe point: pause more only if that is not enough.
                let freeing: u32 = running.iter().filter(|(_, r)| r.preempted.is_some()).map(|(id, _)| held(*id)).sum();
                if freeing < overflow {
                    let victim = running
                        .iter()
                        .filter(|(id, r)| {
                            needs_a_core(r.kind) && r.preempted.is_none() && !r.paused_by_you && held(**id) > 0
                        })
                        .max_by_key(|(id, r)| (r.rank, id.0))
                        .map(|(id, _)| *id);
                    if let Some(id) = victim
                        && let Some(r) = running.get_mut(&id)
                    {
                        r.preempted = Some(held(id));
                        r.control.send_replace(Control::Pause { reason: LIVE_PLAY.to_owned() });
                        changes.push((id, Some(LIVE_PLAY.to_owned())));
                    }
                }
            } else {
                for (id, r) in running.iter_mut() {
                    let Some(had) = r.preempted else { continue };
                    if snap.reserved == 0 || snap.free >= had.clamp(1, 2) {
                        r.preempted = None;
                        r.control.send_replace(Control::Run);
                        changes.push((*id, None));
                    }
                }
            }
        }
        for (id, reason) in changes {
            match &reason {
                Some(r) => self.log(id, format!("Pausing for live play: {r}.")),
                None => self.log(id, "Cores are free again: carrying on."),
            }
            let _ = self.set_paused(id, reason).await;
        }
    }
}

#[cfg(test)]
mod tests;
