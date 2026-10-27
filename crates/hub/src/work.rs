//! How background work runs: the contract between the scheduler (crate `work`) and whatever does the work (a build,
//! a trial or tune in `work`, a review in `analysis`, an import in `play`).
//!
//! An [`Executor`] knows how to run one [`WorkKind`]. The scheduler registers one per kind, picks the next queued item,
//! and calls [`Executor::run`] with a [`WorkContext`]: the item's spec and last checkpoint, and the means to report
//! progress, write the log, ask for cores, honour pausing and the fair-play policy, and notice being stopped. An
//! executor never touches the `work` table itself; the context does, so status, progress and events stay consistent.
//!
//! Work is resumable: an executor saves a checkpoint after each unit (a trial's round, a tune's iteration, a review's
//! game) and, when started again after a restart or a pause, carries on from [`WorkContext::checkpoint`].

use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use dameless_core::{
    Error,
    events::{AssistKind, Payload, WorkLog},
    ids::WorkId,
    work::{Priority, Progress, WorkKind},
};
use dameless_store::work::WorkUpdate;
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::{GpuGrant, Grant, Hub};

/// Runs one kind of work. Implemented by the crate that owns that kind.
#[async_trait]
pub trait Executor: Send + Sync {
    fn kind(&self) -> WorkKind;

    /// Does the work and returns its result (stored in the log and shown by `GET /api/work/{id}`).
    ///
    /// When [`WorkContext::cancel`] fires (or [`WorkContext::safe_point`] returns `false`), finish the current unit if it is cheap, save a checkpoint, and return [`WorkContext::stopped`]. The
    /// scheduler then records the item as stopped (or paused, when it was a pause) rather than failed.
    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error>;
}

/// What the scheduler asks of running work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Run,
    /// Stop at the next safe point and wait; `reason` is for people ("Live analysis").
    Pause {
        reason: String,
    },
}

/// Everything a running work item may use. Made by the scheduler; [`WorkContext::detached`] makes one for tests.
pub struct WorkContext {
    pub id: WorkId,
    pub kind: WorkKind,
    pub hub: Arc<Hub>,
    /// Fires when the item is stopped (by a person, or the server shutting down).
    pub cancel: CancellationToken,
    /// The item's priority: where its requests for cores and the GPU stand in the ledger's line.
    pub priority: Priority,
    spec: serde_json::Value,
    checkpoint: Option<serde_json::Value>,
    control: watch::Receiver<Control>,
    log: Mutex<Option<std::fs::File>>,
    last_progress: Mutex<Option<Instant>>,
}

/// Progress is written and sent at most this often; the last report of a unit always goes through
/// ([`WorkContext::progress_now`]).
const PROGRESS_EVERY: Duration = Duration::from_millis(1000);

impl WorkContext {
    /// Made by the scheduler for each run of an item, at [`Priority::Interactive`] until
    /// [`WorkContext::with_priority`] says otherwise.
    pub fn new(
        hub: Arc<Hub>,
        id: WorkId,
        kind: WorkKind,
        spec: serde_json::Value,
        checkpoint: Option<serde_json::Value>,
        cancel: CancellationToken,
        control: watch::Receiver<Control>,
    ) -> Self {
        let log = std::fs::OpenOptions::new().create(true).append(true).open(hub.config.work_log_path(id)).ok();
        Self {
            id,
            kind,
            hub,
            cancel,
            priority: Priority::Interactive,
            spec,
            checkpoint,
            control,
            log: Mutex::new(log),
            last_progress: Mutex::new(None),
        }
    }

    /// The item's priority, which orders its requests for cores and the GPU against other work's.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// A context with no scheduler behind it, for tests of an executor: never paused, cancelled through the returned
    /// token. The item must exist in the store (insert it first) for progress and checkpoints to be saved.
    pub fn detached(hub: Arc<Hub>, id: WorkId, kind: WorkKind, spec: serde_json::Value) -> (Self, CancellationToken) {
        let cancel = CancellationToken::new();
        let (_tx, rx) = watch::channel(Control::Run);
        // The sender is dropped: `rx` keeps its last value, Run, for ever.
        (Self::new(hub, id, kind, spec, None, cancel.clone(), rx), cancel)
    }

    /// The item's spec, as the route that queued it wrote it.
    pub fn spec<T: DeserializeOwned>(&self) -> Result<T, Error> {
        serde_json::from_value(self.spec.clone())
            .map_err(|e| Error::Internal(format!("work {} has an unreadable spec: {e}", self.id.0)))
    }

    /// The last checkpoint saved, when the item ran before.
    pub fn checkpoint<T: DeserializeOwned>(&self) -> Option<T> {
        self.checkpoint.as_ref().and_then(|c| serde_json::from_value(c.clone()).ok())
    }

    /// Saves where the work stands, so a restart or a resume carries on from here.
    pub async fn save_checkpoint<T: Serialize>(&self, checkpoint: &T) -> Result<(), Error> {
        let value = serde_json::to_value(checkpoint).map_err(|e| Error::Internal(e.to_string()))?;
        self.hub.store.set_work_checkpoint(self.id, value).await
    }

    /// Reports progress, at most once a second (extra reports in between are dropped).
    pub async fn progress(&self, progress: Progress) -> Result<(), Error> {
        {
            let mut last = self.last_progress.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < PROGRESS_EVERY) {
                return Ok(());
            }
            *last = Some(Instant::now());
        }
        self.progress_now(progress).await
    }

    /// Reports progress now, whatever the throttle: use it at the end of a unit.
    pub async fn progress_now(&self, progress: Progress) -> Result<(), Error> {
        *self.last_progress.lock().unwrap() = Some(Instant::now());
        self.update(WorkUpdate { progress: Some(progress), ..Default::default() }).await
    }

    /// Changes the title people see ("Trying deeper-lmr · long games").
    pub async fn set_title(&self, title: impl Into<String>) -> Result<(), Error> {
        self.update(WorkUpdate { title: Some(title.into()), ..Default::default() }).await
    }

    /// Writes a line to the item's log file and sends it live (`work.log`).
    pub fn log(&self, line: impl AsRef<str>) {
        let line = line.as_ref();
        if let Some(f) = self.log.lock().unwrap().as_mut() {
            let _ = writeln!(f, "{line}");
        }
        self.hub.bus.emit(Payload::WorkLog(WorkLog { id: self.id, line: line.to_owned() }));
    }

    /// Asks for cores from the ledger, waiting until at least `min` are free and no work of higher priority (or of the
    /// same priority, asking earlier) waits ahead of this item. `None` when stopped while waiting. Hold the grant only
    /// while using it (a trial's round), so the scheduler can rebalance between units and others get their turn.
    pub async fn cores(&self, min: u32, max: u32) -> Option<Grant> {
        let ledger = self.hub.ledger.clone();
        let mut changed = ledger.changed();
        // In line from now until granted; dropped (leaving the line) if this wait is stopped or cancelled.
        let request = ledger.request_cores(self.id, self.priority, min, max);
        loop {
            if let Some(grant) = request.grant() {
                let _ = self.update(WorkUpdate { cores: Some(grant.cores()), ..Default::default() }).await;
                return Some(grant);
            }
            tokio::select! {
                _ = self.cancel.cancelled() => return None,
                r = changed.changed() => if r.is_err() { return None },
            }
        }
    }

    /// Asks for a GPU, waiting until one is free. `None` when stopped while waiting, or at once when the machine has
    /// none (check `hub.ledger.gpus()` first to say so). Hold it for the whole run.
    pub async fn gpu(&self) -> Option<GpuGrant> {
        let ledger = self.hub.ledger.clone();
        if ledger.gpus() == 0 {
            return None;
        }
        let mut changed = ledger.changed();
        let request = ledger.request_gpu(self.id, self.priority);
        let mut said = false;
        loop {
            if let Some(grant) = request.grant() {
                return Some(grant);
            }
            if !said {
                said = true;
                self.log("Waiting for the GPU.");
            }
            tokio::select! {
                _ = self.cancel.cancelled() => return None,
                r = changed.changed() => if r.is_err() { return None },
            }
        }
    }

    /// A safe point: call between units. Waits while the scheduler has paused the item, and while the fair-play policy
    /// forbids `assist` (for work that gives assistance: reviews, other players' imports). Returns `false` when the item
    /// was stopped: save a checkpoint and return [`WorkContext::stopped`].
    pub async fn safe_point(&self, assist: Option<AssistKind>) -> bool {
        let mut control = self.control.clone();
        let mut policy = self.hub.policy.watch();
        let mut paused = false;
        loop {
            if self.cancel.is_cancelled() {
                return false;
            }
            let why = match &*control.borrow_and_update() {
                Control::Pause { reason } => Some(reason.clone()),
                Control::Run => None,
            }
            .or_else(|| {
                let p = policy.borrow_and_update().clone();
                assist.filter(|k| !p.allows(*k)).map(|_| p.reason.unwrap_or_else(|| "Your Lichess game".to_owned()))
            });
            match why {
                None => {
                    if paused {
                        let _ = self.update(WorkUpdate { paused_for: Some(None), ..Default::default() }).await;
                        self.log("Resumed.");
                    }
                    return true;
                }
                Some(reason) if !paused => {
                    paused = true;
                    self.log(format!("Paused: {reason}."));
                    let _ = self.update(WorkUpdate { paused_for: Some(Some(reason)), ..Default::default() }).await;
                }
                Some(_) => {}
            }
            tokio::select! {
                _ = self.cancel.cancelled() => return false,
                _ = control.changed() => {}
                _ = policy.changed() => {}
            }
        }
    }

    /// Whether the scheduler has asked the item to pause, without waiting: work that keeps several units in flight (a
    /// tune's workers) stops starting new ones, lets those running finish, then calls [`WorkContext::safe_point`].
    pub fn pause_asked(&self) -> bool {
        matches!(*self.control.borrow(), Control::Pause { .. })
    }

    /// The error an executor returns after noticing [`WorkContext::cancel`]; the scheduler records the item as stopped.
    pub fn stopped(&self) -> Error {
        Error::Conflict(STOPPED.to_owned())
    }

    /// Whether an error from [`Executor::run`] is [`WorkContext::stopped`].
    pub fn is_stopped(error: &Error) -> bool {
        matches!(error, Error::Conflict(m) if m == STOPPED)
    }

    async fn update(&self, update: WorkUpdate) -> Result<(), Error> {
        let work = self.hub.store.update_work(self.id, update).await?;
        self.hub.bus.emit(Payload::WorkUpserted(Box::new(work)));
        Ok(())
    }
}

const STOPPED: &str = "stopped";
