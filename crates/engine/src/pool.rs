//! A pool of warm engine processes, for the evaluation service and reviews: starting Reckless and allocating its hash
//! takes longer than many of the searches they ask for, so engines are kept running and handed out again.
//!
//! Engines are pooled by [`EngineSpec`] (a binary, its arguments and its UCI options), so an engine is only ever
//! reused with the settings it was started with. Each spec has a limit on how many engines may exist at once (checked
//! out plus idle); [`Pool::checkout`] waits while it is reached. The caller sizes it to the cores it was granted with
//! [`Pool::resize`].
//!
//! A returned engine is cleaned up lazily, when it is next checked out: a search still running is stopped and read to
//! its `bestmove`, and (unless asked to keep its state) it gets `ucinewgame`. An engine that fails any of that, or that
//! died while out, is killed and replaced by a fresh one.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use tokio::sync::Notify;

use crate::{
    proc,
    uci::{Engine, UCIOK_TIMEOUT},
};

/// How long a returned engine has to stop a search it was left running.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// What engines in the pool are: one binary started with these arguments and set up with these options.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EngineSpec {
    pub path: PathBuf,
    pub args: Vec<String>,
    /// UCI options set once after the handshake, in order (`Hash`, `Threads`, `MultiPV`…).
    pub options: Vec<(String, String)>,
}

impl EngineSpec {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), args: Vec::new(), options: Vec::new() }
    }

    pub fn option(mut self, name: impl Into<String>, value: impl ToString) -> Self {
        self.options.push((name.into(), value.to_string()));
        self
    }

    async fn start(&self) -> Result<Engine> {
        let command = proc::Command::new(&self.path).args(&self.args);
        let mut engine = Engine::start_command(&command, UCIOK_TIMEOUT).await?;
        engine.configure(&self.options).await?;
        Ok(engine)
    }
}

/// What a reused engine gets before it is handed out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reset {
    /// `ucinewgame`: a clean hash table and history, as for a new game.
    #[default]
    NewGame,
    /// Keep the hash table: positions searched before may speed this search up (and make it less reproducible).
    Keep,
}

/// How many engines of a spec exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolStats {
    pub limit: usize,
    pub checked_out: usize,
    pub idle: usize,
}

struct Idle {
    engine: Engine,
    since: Instant,
}

struct Slot {
    limit: usize,
    out: usize,
    idle: Vec<Idle>,
}

struct Inner {
    slots: Mutex<HashMap<EngineSpec, Slot>>,
    notify: Notify,
    default_limit: usize,
}

impl Inner {
    fn slots(&self) -> std::sync::MutexGuard<'_, HashMap<EngineSpec, Slot>> {
        self.slots.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Gives up one checked-out place, keeping the engine if there is room for it.
    fn give_back(&self, spec: &EngineSpec, engine: Option<Engine>) {
        let dropped = {
            let mut slots = self.slots();
            let slot = slots.get_mut(spec).expect("a checked-out engine has a slot");
            slot.out -= 1;
            match engine {
                Some(engine) if slot.out + slot.idle.len() < slot.limit => {
                    slot.idle.push(Idle { engine, since: Instant::now() });
                    None
                }
                other => other,
            }
        };
        // Killed outside the lock (dropping kills its process tree).
        drop(dropped);
        self.notify.notify_waiters();
    }
}

/// Warm engines, shared: clone it to hand it around.
#[derive(Clone)]
pub struct Pool {
    inner: Arc<Inner>,
}

impl Pool {
    /// A pool where each spec may have up to `default_limit` engines until [`Pool::resize`] says otherwise.
    pub fn new(default_limit: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                slots: Mutex::new(HashMap::new()),
                notify: Notify::new(),
                default_limit: default_limit.max(1),
            }),
        }
    }

    /// An engine of this spec with a clean hash table, waiting while the spec's limit is reached.
    pub async fn checkout(&self, spec: &EngineSpec) -> Result<PooledEngine> {
        self.checkout_with(spec, Reset::NewGame).await
    }

    /// An engine of this spec, reset as asked, waiting while the spec's limit is reached.
    pub async fn checkout_with(&self, spec: &EngineSpec, reset: Reset) -> Result<PooledEngine> {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let taken = {
                let mut slots = self.inner.slots();
                let default = self.inner.default_limit;
                let slot =
                    slots.entry(spec.clone()).or_insert_with(|| Slot { limit: default, out: 0, idle: Vec::new() });
                if slot.out < slot.limit {
                    slot.out += 1;
                    Some(slot.idle.pop())
                } else {
                    None
                }
            };
            let Some(idle) = taken else {
                notified.await;
                continue;
            };
            let engine = match idle {
                Some(idle) => match Self::clean(idle.engine, reset).await {
                    Some(engine) => Ok(engine),
                    None => spec.start().await,
                },
                None => spec.start().await,
            };
            return match engine {
                Ok(engine) => Ok(PooledEngine { engine: Some(engine), spec: spec.clone(), pool: self.inner.clone() }),
                Err(e) => {
                    self.inner.give_back(spec, None);
                    Err(e)
                }
            };
        }
    }

    /// Readies a returned engine for its next use, or `None` when it is not fit for one.
    async fn clean(mut engine: Engine, reset: Reset) -> Option<Engine> {
        if !engine.alive() {
            return None;
        }
        engine.finish(STOP_TIMEOUT).await.ok()?;
        match reset {
            Reset::NewGame => engine.new_game().await.ok()?,
            Reset::Keep => engine.ready().await.ok()?,
        }
        Some(engine)
    }

    /// Sets how many engines of a spec may exist at once. Idle engines over the new limit are killed now; checked-out
    /// ones over it are killed when they come back.
    pub fn resize(&self, spec: &EngineSpec, limit: usize) {
        let killed = {
            let mut slots = self.inner.slots();
            let slot = slots.entry(spec.clone()).or_insert_with(|| Slot { limit, out: 0, idle: Vec::new() });
            slot.limit = limit;
            let keep = limit.saturating_sub(slot.out);
            if slot.idle.len() > keep {
                // The oldest go first.
                let excess = slot.idle.len() - keep;
                slot.idle.drain(..excess).collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        };
        drop(killed);
        self.inner.notify.notify_waiters();
    }

    /// Starts engines of a spec until `count` are idle or the limit is reached, so the first searches do not wait for
    /// a start.
    pub async fn warm(&self, spec: &EngineSpec, count: usize) -> Result<()> {
        let mut engines = Vec::new();
        for _ in 0..count {
            let room = {
                let slots = self.inner.slots();
                slots.get(spec).is_none_or(|s| s.out + s.idle.len() + engines.len() < s.limit)
            };
            if !room {
                break;
            }
            engines.push(spec.start().await?);
        }
        let mut slots = self.inner.slots();
        let default = self.inner.default_limit;
        let slot = slots.entry(spec.clone()).or_insert_with(|| Slot { limit: default, out: 0, idle: Vec::new() });
        for engine in engines {
            if slot.out + slot.idle.len() < slot.limit {
                slot.idle.push(Idle { engine, since: Instant::now() });
            }
        }
        Ok(())
    }

    /// Kills every idle engine of every spec. Checked-out engines are left alone.
    pub fn kill_idle(&self) {
        self.kill_idle_older_than(Duration::ZERO);
    }

    /// Kills idle engines that have waited at least `age`, to give memory back when the board is quiet.
    pub fn kill_idle_older_than(&self, age: Duration) {
        let now = Instant::now();
        let killed: Vec<Idle> = {
            let mut slots = self.inner.slots();
            slots
                .values_mut()
                .flat_map(|slot| {
                    let (old, young): (Vec<_>, Vec<_>) =
                        std::mem::take(&mut slot.idle).into_iter().partition(|i| now.duration_since(i.since) >= age);
                    slot.idle = young;
                    old
                })
                .collect()
        };
        drop(killed);
    }

    /// How many engines of a spec exist now.
    pub fn stats(&self, spec: &EngineSpec) -> PoolStats {
        let slots = self.inner.slots();
        match slots.get(spec) {
            Some(s) => PoolStats { limit: s.limit, checked_out: s.out, idle: s.idle.len() },
            None => PoolStats { limit: self.inner.default_limit, checked_out: 0, idle: 0 },
        }
    }
}

/// An engine checked out of a [`Pool`]. It goes back when dropped; [`PooledEngine::discard`] kills it instead.
pub struct PooledEngine {
    engine: Option<Engine>,
    spec: EngineSpec,
    pool: Arc<Inner>,
}

impl PooledEngine {
    /// Kills the engine rather than returning it (it misbehaved, or its state must not be reused).
    pub fn discard(mut self) {
        let engine = self.engine.take();
        drop(engine);
        self.pool.give_back(&self.spec, None);
    }

    pub fn spec(&self) -> &EngineSpec {
        &self.spec
    }
}

impl Deref for PooledEngine {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        self.engine.as_ref().expect("a pooled engine is present until dropped")
    }
}

impl DerefMut for PooledEngine {
    fn deref_mut(&mut self) -> &mut Engine {
        self.engine.as_mut().expect("a pooled engine is present until dropped")
    }
}

impl Drop for PooledEngine {
    fn drop(&mut self) {
        if let Some(mut engine) = self.engine.take() {
            let engine = engine.alive().then_some(engine);
            self.pool.give_back(&self.spec, engine);
        }
    }
}
