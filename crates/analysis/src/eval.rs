//! The evaluation service: live sessions for the board, and cached searches of positions for reviews and profiles.
//!
//! A **session** is one search the board watches (`POST /api/eval`): it reserves its cores in the ledger for as long
//! as it runs (live evaluation never waits for work), runs on a warm engine from the pool, and streams `eval.info` on
//! topic `eval:<session>` at most five times a second, every score White's. Each client (a browser tab) has one
//! session: starting another replaces it. Sessions are assistance, so a session is refused while the fair-play policy
//! forbids it, and a running one stops the moment the policy turns off (a Lichess game of yours starts).
//!
//! **Evaluating positions** is what reviews do: each position searched from its bare FEN to a node budget, through the
//! position cache (`positions`), keyed by the position, the engine (its id and what it calls itself), the budget and
//! MultiPV, so a position
//! shared by many games is searched once. Scores are flipped to White's before they are cached. The searches go in
//! [runs](crate::runs) ([`Eval::search_runs`]): positions in the order played, on an engine that keeps its table.
//! [`Eval::evaluate_positions`] does it for one list of positions; the review drives [`Eval::search_runs`] itself, to
//! keep several games in flight.

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use dameless_core::{
    Error, Side,
    chess::{Position, position_key},
    events::{AssistKind, EvalInfo, Payload, Topic},
    review::{PositionEval, ToSearch},
};
use dameless_engine::{
    pool::{EngineSpec, Pool, PooledEngine, Reset},
    uci::{self, Info, Limits, SearchEvent},
};
use dameless_hub::{Hub, Reservation};
use dameless_store::positions::SearchKey;
use dameless_work::{EngineBinary, EngineSource};
use serde::{Deserialize, Serialize};
use tokio::{task::JoinHandle, time::Instant};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{
    runs::{Next, RunQueue},
    search::{engine_spec, live_lines, record, review_spec, search_fen},
};

/// `eval.info` is sent at most this often (five a second); the newest report always goes out in the end.
pub const INFO_EVERY: Duration = Duration::from_millis(200);

/// How long a session runs with no event stream watching its topic before it stops: longer than a page takes to
/// reconnect. A stream open for anything else (another tab, the CLI) does not keep it.
pub const UNWATCHED: Duration = Duration::from_secs(15);

/// How long a stopped search has to send its `bestmove` before its engine is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Hash for live sessions, in MiB.
const SESSION_HASH: u32 = 64;

/// Lines a session may ask for, at most.
pub const MAX_MULTIPV: u32 = 10;

/// Moves a session's position may have after its FEN, at most.
const MAX_MOVES: usize = 2000;

/// Idle engines older than this are killed, to give memory back when the board is quiet.
const IDLE_ENGINES: Duration = Duration::from_secs(300);

/// Where a session's search stops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Budget {
    Depth {
        depth: u32,
    },
    Nodes {
        nodes: u64,
    },
    /// Until stopped (the board's engine switch).
    #[default]
    Infinite,
}

impl Budget {
    fn limits(self) -> Limits {
        match self {
            Self::Depth { depth } => Limits::depth(depth.max(1)),
            Self::Nodes { nodes } => Limits::nodes(nodes.max(1)),
            Self::Infinite => Limits::infinite(),
        }
    }
}

/// `POST /api/eval`: a live search of a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EvalRequest {
    /// The start position as FEN; none for the standard start.
    #[serde(default)]
    pub fen: Option<String>,
    /// Moves from it, UCI or SAN. The engine gets the whole history, so it sees repetitions.
    #[serde(default)]
    pub moves: Vec<String>,
    /// The engine to search with, by id; the analysis engine when none.
    #[serde(default)]
    pub engine: Option<String>,
    /// Lines to report, 1 to 10.
    #[serde(default = "one")]
    pub multipv: u32,
    #[serde(default)]
    pub budget: Budget,
    /// Cores (search threads) to reserve; 1 when none.
    #[serde(default)]
    pub threads: Option<u32>,
    /// Who asks (a browser tab): one session per client, a new one replaces the old.
    pub client: String,
}

fn one() -> u32 {
    1
}

/// A started session: its events come on topic `eval:<session>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EvalSession {
    pub session: String,
    /// The engine searching, by what it calls itself: `Stockfish 19`.
    pub engine: String,
    /// The position searched, after the moves.
    pub fen: String,
}

/// A running session, for status and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub session: String,
    pub client: String,
}

/// Positions searched for a review: one evaluation per position (none where the game is over), and how many were
/// searched now and how many came from the cache.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluated {
    pub evals: Vec<Option<PositionEval>>,
    pub searched: usize,
    pub cached: usize,
}

struct Running {
    session: String,
    client: String,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}

struct Inner {
    hub: Arc<Hub>,
    engines: Arc<dyn EngineSource>,
    pool: Pool,
    sessions: Mutex<Vec<Running>>,
    next: AtomicU64,
}

impl Inner {
    fn sessions(&self) -> std::sync::MutexGuard<'_, Vec<Running>> {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Forgets a session that ended, if it is still the one listed.
    fn ended(&self, session: &str) {
        self.sessions().retain(|r| r.session != session);
    }
}

/// The evaluation service. Cheap to clone.
#[derive(Clone)]
pub struct Eval {
    inner: Arc<Inner>,
}

impl Eval {
    /// The service, with a pool of warm engines sized by the machine's cores. Called within the tokio runtime: it
    /// starts a task that kills engines left idle for a while.
    pub fn new(hub: Arc<Hub>, engines: Arc<dyn EngineSource>) -> Self {
        let pool = Pool::new(hub.ledger.total() as usize);
        let seed =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let inner =
            Arc::new(Inner { hub, engines, pool, sessions: Mutex::new(Vec::new()), next: AtomicU64::new(seed) });
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(reap(Arc::downgrade(&inner)));
        }
        Self { inner }
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.inner.hub
    }

    pub fn engines(&self) -> &Arc<dyn EngineSource> {
        &self.inner.engines
    }

    /// The warm engines, shared with nothing else: reviews and sessions check out of it.
    pub fn pool(&self) -> &Pool {
        &self.inner.pool
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------------------------------------------------------

    /// Starts a live search, replacing the client's session if it has one. Refused (409, fair play) while the policy
    /// forbids evaluation.
    pub async fn start_session(&self, req: EvalRequest) -> Result<EvalSession, Error> {
        let hub = &self.inner.hub;
        hub.policy.check(AssistKind::Eval)?;
        let client = req.client.trim().to_owned();
        if client.is_empty() {
            return Err(Error::bad_request("a session needs a client"));
        }
        if req.moves.len() > MAX_MOVES {
            return Err(Error::bad_request(format!("at most {MAX_MOVES} moves")));
        }
        if !(1..=MAX_MULTIPV).contains(&req.multipv) {
            return Err(Error::bad_request(format!("multipv is 1 to {MAX_MULTIPV}")));
        }
        let start = Position::from_start(req.fen.as_deref())?;
        let mut pos = start.clone();
        let mut ucis = Vec::with_capacity(req.moves.len());
        for m in &req.moves {
            ucis.push(pos.play(m)?.uci);
        }
        let to_move = pos.turn();
        let engine_position = if start.is_standard_start() {
            uci::Position::startpos().with_moves(ucis)
        } else {
            uci::Position::fen(start.fen()).with_moves(ucis)
        };
        let threads = req.threads.unwrap_or(1).clamp(1, hub.ledger.total());

        self.stop_client(&client).await;

        let binary = self.inner.engines.ready(req.engine.as_deref()).await?;
        if let Some(nodes) = binary.engine.nodes {
            return Err(Error::bad_request(format!(
                "{} plays a fixed {nodes} node(s) a move: it cannot analyse",
                binary.engine.name
            )));
        }
        let session = format!("{:012x}", self.inner.next.fetch_add(1, Ordering::Relaxed) & 0xffff_ffff_ffff);
        let reservation = hub.ledger.reserve(format!("eval:{session}"), threads);
        let spec =
            engine_spec(&binary).option("Threads", threads).option("Hash", SESSION_HASH).option("MultiPV", req.multipv);
        let mut engine =
            self.inner.pool.checkout(&spec).await.map_err(|e| Error::internal(format!("start the engine: {e:#}")))?;
        // A Lichess game that started while the engine loaded: nothing was running yet to stop.
        hub.policy.check(AssistKind::Eval)?;
        engine
            .go(&engine_position, &req.budget.limits())
            .await
            .map_err(|e| Error::internal(format!("start the search: {e:#}")))?;

        let cancel = CancellationToken::new();
        let task = tokio::spawn(run_session(
            self.inner.clone(),
            Live { session: session.clone(), engine, to_move, multipv: req.multipv, cancel: cancel.clone() },
            reservation,
        ));
        let replaced = {
            let mut sessions = self.inner.sessions();
            let replaced = take_client(&mut sessions, &client);
            sessions.push(Running { session: session.clone(), client, cancel, task: Some(task) });
            replaced
        };
        // Two starts at once both got past `stop_client`: the one replaced here is stopped too, or its engine would
        // search for ever with nothing left to stop it.
        for r in replaced {
            r.cancel.cancel();
        }
        Ok(EvalSession { session, engine: binary.engine.label().to_owned(), fen: pos.fen() })
    }

    /// Stops a session and waits until its engine has stopped. `false` when there was no such session (it ended).
    pub async fn stop_session(&self, session: &str) -> bool {
        let found = {
            let mut sessions = self.inner.sessions();
            sessions.iter().position(|r| r.session == session).map(|i| sessions.remove(i))
        };
        match found {
            Some(r) => {
                stop(r).await;
                true
            }
            None => false,
        }
    }

    /// Stops a client's session, if it has one.
    pub async fn stop_client(&self, client: &str) {
        let found = take_client(&mut self.inner.sessions(), client);
        for r in found {
            stop(r).await;
        }
    }

    /// Stops every session (the server shutting down).
    pub async fn stop_all(&self) {
        let all: Vec<Running> = std::mem::take(&mut *self.inner.sessions());
        for r in all {
            stop(r).await;
        }
    }

    /// The sessions running now.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        self.inner
            .sessions()
            .iter()
            .map(|r| SessionInfo { session: r.session.clone(), client: r.client.clone() })
            .collect()
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Positions, through the cache
    // -----------------------------------------------------------------------------------------------------------------

    /// One position, searched from its bare FEN to `nodes` with `engine`, through the position cache. Not gated by
    /// the fair-play policy: the work that calls it (a review) waits at its safe points instead.
    pub async fn evaluate(
        &self,
        fen: &str,
        engine: &EngineBinary,
        nodes: u64,
        multipv: u32,
    ) -> Result<PositionEval, Error> {
        let pos = Position::from_fen(fen)?;
        if pos.is_terminal() {
            return Err(Error::bad_request("the game is over in that position"));
        }
        let fen = pos.fen();
        let position = ToSearch { key: position_key(&fen), fen, terminal: false };
        let mut found = self.evaluate_positions(std::slice::from_ref(&position), engine, nodes, multipv, 1).await?;
        found.evals.pop().flatten().ok_or_else(|| Error::internal("the search found nothing"))
    }

    /// Many positions (a game's), through the position cache: the cached ones are read, the rest searched on up to
    /// `engines` engines at once (each position to whichever engine is free) and cached. Positions where the game is
    /// over get `None`. A position that appears twice is searched once.
    pub async fn evaluate_positions(
        &self,
        positions: &[ToSearch],
        engine: &EngineBinary,
        nodes: u64,
        multipv: u32,
        engines: usize,
    ) -> Result<Evaluated, Error> {
        let search = SearchKey { engine: engine.engine.cache_key(), nodes, multipv };
        let mut keys: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for p in positions.iter().filter(|p| !p.terminal) {
            if seen.insert(p.key.as_str()) {
                keys.push(p.key.clone());
            }
        }
        let mut known = self.inner.hub.store.positions(keys.clone(), &search).await?;
        let cached = known.len();
        let missing: Vec<(String, String)> = positions
            .iter()
            .filter(|p| !p.terminal && !known.contains_key(&p.key))
            .fold(Vec::<(String, String)>::new(), |mut acc, p| {
                if !acc.iter().any(|(k, _)| *k == p.key) {
                    acc.push((p.key.clone(), p.fen.clone()));
                }
                acc
            });
        if !missing.is_empty() {
            let searching = self.review_engines(engine, nodes, multipv);
            let queue = Mutex::new(RunQueue::new(0));
            // The positions in the order given (a game's, as played): one run, which idle engines split.
            queue
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(missing.iter().map(|(key, fen)| Job { key: key.clone(), fen: fen.clone() }));
            let found = Mutex::new(Vec::new());
            self.search_runs(&searching, &queue, engines, &|| true, &found).await;
            let mut fresh = Vec::with_capacity(missing.len());
            for (job, result) in found.into_inner().unwrap_or_else(|p| p.into_inner()) {
                fresh.push((job.key, result?));
            }
            self.inner.hub.store.put_positions(fresh.clone(), &search).await?;
            known.extend(fresh);
        }
        let evals = positions.iter().map(|p| (!p.terminal).then(|| known.get(&p.key).cloned()).flatten()).collect();
        Ok(Evaluated { evals, searched: missing.len(), cached })
    }

    /// How reviews search with an engine: one thread, the review hash and `multipv` lines, to `nodes` a position.
    pub fn review_engines(&self, engine: &EngineBinary, nodes: u64, multipv: u32) -> ReviewEngines {
        ReviewEngines { spec: review_spec(engine, multipv), nodes, multipv }
    }

    /// Searches the queue's runs on up to `engines` pooled engines, each run's positions one after another on one
    /// engine that keeps its hash table between them (a clean table at the start of each run), until the queue is
    /// empty or `go_on` says to stop. `go_on` is asked before each position; an engine that stops gives the rest of
    /// its run back to the queue. Each result is pushed to `found` as it comes, so a caller that drops this future
    /// (the work was stopped) keeps what was finished. An engine that fails is replaced and the position tried once
    /// more on the fresh one; a second failure is that position's result.
    pub async fn search_runs(
        &self,
        searching: &ReviewEngines,
        queue: &Mutex<RunQueue<Job>>,
        engines: usize,
        go_on: &(dyn Fn() -> bool + Sync),
        found: &Mutex<Vec<(Job, Result<PositionEval, Error>)>>,
    ) {
        let ReviewEngines { spec, nodes, multipv } = searching;
        let (nodes, multipv) = (*nodes, *multipv);
        let lock = || queue.lock().unwrap_or_else(|p| p.into_inner());
        let engines = {
            let mut q = lock();
            let engines = engines.clamp(1, q.len().max(1));
            q.resize(engines);
            engines
        };
        let pool = &self.inner.pool;
        pool.resize(spec, engines);
        let checkout = || async {
            pool.checkout_with(spec, Reset::NewGame)
                .await
                .map_err(|e| Error::internal(format!("start the engine: {e:#}")))
        };
        let worker = |i: usize| async move {
            // Checked out on the first position, with a clean table: that position starts a run anyway.
            let mut engine: Option<PooledEngine> = None;
            loop {
                if !go_on() {
                    lock().give_back(i);
                    break;
                }
                let Some(Next { job, fresh }) = lock().next(i) else { break };
                let result = async {
                    let mut current = match engine.take() {
                        Some(mut e) if fresh => match e.new_game().await {
                            Ok(()) => e,
                            Err(_) => {
                                e.discard();
                                checkout().await?
                            }
                        },
                        Some(e) => e,
                        None => checkout().await?,
                    };
                    let eval = match search_fen(&mut current, &job.fen, nodes, multipv).await {
                        Ok(eval) => eval,
                        Err(_) => {
                            // The engine may be dead or confused: one fresh start, one more try.
                            current.discard();
                            current = checkout().await?;
                            search_fen(&mut current, &job.fen, nodes, multipv)
                                .await
                                .map_err(|e| Error::internal(format!("{} at {}", e.message(), job.fen)))?
                        }
                    };
                    engine = Some(current);
                    Ok(eval)
                }
                .await;
                found.lock().unwrap_or_else(|p| p.into_inner()).push((job, result));
            }
        };
        futures::future::join_all((0..engines).map(worker)).await;
    }
}

/// How a review searches: [`Eval::review_engines`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewEngines {
    pub spec: EngineSpec,
    pub nodes: u64,
    pub multipv: u32,
}

/// A position for [`Eval::search_runs`]: its cache key and its FEN.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub key: String,
    pub fen: String,
}

/// Takes a client's sessions out of the list.
fn take_client(sessions: &mut Vec<Running>, client: &str) -> Vec<Running> {
    let (theirs, others) = std::mem::take(sessions).into_iter().partition(|r| r.client == client);
    *sessions = others;
    theirs
}

/// Stops a session's search and waits for its task to end.
async fn stop(mut r: Running) {
    r.cancel.cancel();
    if let Some(task) = r.task.take() {
        let _ = task.await;
    }
}

/// Kills engines left idle for a while, until the service is gone.
async fn reap(inner: Weak<Inner>) {
    let mut tick = tokio::time::interval(IDLE_ENGINES / 5);
    loop {
        tick.tick().await;
        let Some(inner) = inner.upgrade() else { return };
        inner.pool.kill_idle_older_than(IDLE_ENGINES);
    }
}

/// What a session's task owns.
struct Live {
    session: String,
    engine: PooledEngine,
    to_move: Side,
    multipv: u32,
    cancel: CancellationToken,
}

/// Follows a session's search: sends its reports (throttled), stops it when asked, when the policy turns off, or when
/// nobody has watched for [`UNWATCHED`], and gives its engine and cores back when the search ends.
async fn run_session(inner: Arc<Inner>, live: Live, reservation: Reservation) {
    let Live { session, mut engine, to_move, multipv, cancel } = live;
    let hub = inner.hub.clone();
    let mut policy = hub.policy.watch();
    // A game that started between the start's last check and this task's first step changed nothing it will see.
    if !policy.borrow_and_update().allows(AssistKind::Eval) {
        cancel.cancel();
    }
    let mut lines: Vec<Info> = Vec::new();
    let mut pending = false;
    let mut next_emit = Instant::now();
    let mut deadline: Option<Instant> = None;
    let mut watch = tokio::time::interval(UNWATCHED / 3);
    let mut unwatched_since: Option<Instant> = None;
    let emit = |lines: &[Info]| {
        let Some(first) = lines.first() else { return };
        hub.bus.emit(Payload::EvalInfo(EvalInfo {
            session: session.clone(),
            depth: first.depth,
            lines: live_lines(lines, to_move),
            nodes: lines.iter().map(|i| i.nodes).max().unwrap_or(0),
            nps: first.nps,
        }));
    };
    let healthy = loop {
        tokio::select! {
            next = engine.next() => match next {
                Ok(SearchEvent::Info(info)) => {
                    record(&mut lines, info, multipv);
                    let now = Instant::now();
                    if now >= next_emit {
                        emit(&lines);
                        next_emit = now + INFO_EVERY;
                        pending = false;
                    } else {
                        pending = true;
                    }
                }
                Ok(SearchEvent::BestMove(_)) => {
                    if pending {
                        emit(&lines);
                    }
                    break true;
                }
                Err(_) => break false,
            },
            _ = tokio::time::sleep_until(next_emit), if pending => {
                emit(&lines);
                next_emit = Instant::now() + INFO_EVERY;
                pending = false;
            }
            _ = cancel.cancelled(), if deadline.is_none() => {
                deadline = Some(Instant::now() + STOP_TIMEOUT);
                if engine.stop().await.is_err() {
                    break false;
                }
            }
            _ = tokio::time::sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => break false,
            changed = policy.changed(), if deadline.is_none() => {
                if changed.is_err() || !policy.borrow_and_update().allows(AssistKind::Eval) {
                    cancel.cancel();
                }
            }
            _ = watch.tick(), if deadline.is_none() => {
                if hub.bus.watched(&Topic::Eval(session.clone())) {
                    unwatched_since = None;
                } else if unwatched_since.get_or_insert_with(Instant::now).elapsed() >= UNWATCHED {
                    cancel.cancel();
                }
            }
        }
    };
    if healthy {
        // Idle again: back to the pool, warm for the next session.
        drop(engine);
    } else {
        engine.discard();
    }
    drop(reservation);
    inner.ended(&session);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_with_defaults() {
        let req: EvalRequest = serde_json::from_value(serde_json::json!({ "client": "tab-1" })).unwrap();
        assert_eq!((req.multipv, req.budget, req.fen, req.engine), (1, Budget::Infinite, None, None));
        let req: EvalRequest = serde_json::from_value(serde_json::json!({
            "client": "tab-1", "fen": "startpos", "moves": ["e4"], "engine": "stockfish",
            "budget": { "type": "depth", "depth": 20 }, "multipv": 3
        }))
        .unwrap();
        assert_eq!(req.budget, Budget::Depth { depth: 20 });
        assert_eq!(req.engine.as_deref(), Some("stockfish"));
        assert_eq!(Budget::Nodes { nodes: 5 }.limits().command(), "go nodes 5");
    }
}
