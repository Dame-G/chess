//! Evaluating positions for reviews: a pool of engines, one search thread each, sharing a queue of positions, and a
//! cache of every evaluation by position, build, node budget and line count (`position_evals`), so a position seen in
//! another game (openings, mostly) costs nothing.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, anyhow, bail};
use futures::future::BoxFuture;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc, oneshot};
use utoipa::ToSchema;

use crate::engine::uci::{Engine, Info, Limits, Score, Search};

/// One line of a search, best first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Line {
    /// The line's first move, in UCI notation.
    pub uci: String,
    /// From the side to move's view.
    pub score: Score,
    /// The whole line, the first move included (at most 12 moves are kept).
    pub pv: Vec<String>,
}

/// What a search of one position found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PositionEval {
    /// The best line first, then the second best when MultiPV was 2 and there was a second legal move.
    pub lines: Vec<Line>,
    pub depth: u32,
    pub nodes: u64,
}

/// Something that evaluates positions given as FEN: an engine, or a fake one in tests.
pub trait Analyser: Send {
    fn analyse<'a>(&'a mut self, fen: &'a str) -> BoxFuture<'a, Result<PositionEval>>;
}

/// A build over UCI: `MultiPV` lines, a fixed node budget per position, one thread.
pub struct UciAnalyser {
    engine: Engine,
    limits: Limits,
    multipv: u32,
}

impl UciAnalyser {
    pub async fn start(binary: &Path, nodes: u64, multipv: u32, hash: u32) -> Result<Self> {
        let mut engine = Engine::start(binary).await?;
        engine.set_option("Threads", 1).await?;
        engine.set_option("Hash", hash).await?;
        engine.set_option("MultiPV", multipv).await?;
        engine.ready().await?;
        Ok(Self { engine, limits: Limits { nodes: Some(nodes), ..Limits::default() }, multipv })
    }
}

/// The newest report of each line asked for.
fn record(lines: &mut Vec<Info>, info: Info, multipv: u32) {
    if info.multipv == 0 || info.multipv > multipv.max(1) {
        return;
    }
    let i = info.multipv as usize - 1;
    if lines.len() <= i {
        lines.resize(i + 1, info.clone());
    }
    lines[i] = info;
}

impl Analyser for UciAnalyser {
    fn analyse<'a>(&'a mut self, fen: &'a str) -> BoxFuture<'a, Result<PositionEval>> {
        Box::pin(async move {
            self.engine.go(fen, &[], &self.limits).await?;
            let mut infos: Vec<Info> = Vec::new();
            let best = loop {
                match self.engine.next().await? {
                    Search::Info(info) => record(&mut infos, info, self.multipv),
                    Search::Done(best) => break best,
                }
            };
            if infos.is_empty() || best.is_empty() || best == "(none)" {
                bail!("the engine found no move at {fen}");
            }
            // A line reported at a lower depth than the best one (the search stopped mid-iteration) still counts:
            // it is the best the engine has for it.
            let depth = infos[0].depth;
            let nodes = infos.iter().map(|i| i.nodes).max().unwrap_or(0);
            let mut lines: Vec<Line> = infos
                .into_iter()
                .filter(|i| !i.pv.is_empty())
                .map(|i| Line { uci: i.pv[0].clone(), score: i.score, pv: i.pv.into_iter().take(12).collect() })
                .collect();
            // The move the engine plays is the best line, whatever the last report said.
            if let Some(i) = lines.iter().position(|l| l.uci == best)
                && i != 0
            {
                lines.swap(0, i);
            }
            Ok(PositionEval { lines, depth, nodes })
        })
    }
}

/// Says whether searching must wait (you are playing on Lichess); a worker checks before each position.
pub type Gate = Arc<dyn Fn() -> bool + Send + Sync>;

/// Makes a fresh analyser, for a worker whose engine died.
pub type Factory = Arc<dyn Fn() -> BoxFuture<'static, Result<Box<dyn Analyser>>> + Send + Sync>;

struct Task {
    fen: String,
    reply: oneshot::Sender<Result<PositionEval>>,
}

/// Engines sharing one queue of positions. Dropping the pool stops them.
pub struct Pool {
    tx: mpsc::UnboundedSender<Task>,
    workers: Vec<tokio::task::JoinHandle<()>>,
}

impl Pool {
    /// Starts `size` analysers from `factory`. Fails if none starts.
    pub async fn start(factory: Factory, size: usize, gate: Gate) -> Result<Self> {
        let mut analysers = Vec::new();
        let mut error = None;
        for _ in 0..size.max(1) {
            match factory().await {
                Ok(a) => analysers.push(a),
                Err(e) => error = Some(e),
            }
        }
        if analysers.is_empty() {
            return Err(error.unwrap_or_else(|| anyhow!("no engine started")));
        }
        let (tx, rx) = mpsc::unbounded_channel::<Task>();
        let rx = Arc::new(Mutex::new(rx));
        let alive = Arc::new(AtomicUsize::new(analysers.len()));
        let workers = analysers
            .into_iter()
            .map(|mut analyser| {
                let (rx, alive, factory, gate) = (rx.clone(), alive.clone(), factory.clone(), gate.clone());
                tokio::spawn(async move {
                    loop {
                        let task = rx.lock().await.recv().await;
                        let Some(task) = task else { break };
                        while gate() {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        match analyser.analyse(&task.fen).await {
                            Ok(eval) => drop(task.reply.send(Ok(eval))),
                            Err(e) => {
                                let _ = task.reply.send(Err(e));
                                // The engine may be dead: one fresh start, else this worker is gone.
                                match factory().await {
                                    Ok(fresh) => analyser = fresh,
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                    // The last worker out closes the queue, so nothing waits on positions no one will search.
                    if alive.fetch_sub(1, Ordering::SeqCst) == 1 {
                        let mut rx = rx.lock().await;
                        rx.close();
                        while let Ok(task) = rx.try_recv() {
                            let _ = task.reply.send(Err(anyhow!("every engine of the review stopped")));
                        }
                    }
                })
            })
            .collect();
        Ok(Self { tx, workers })
    }

    /// Evaluates every position, in parallel across the engines; the answers are in the order asked.
    pub async fn analyse_all(&self, fens: &[String]) -> Result<Vec<PositionEval>> {
        let mut replies = Vec::with_capacity(fens.len());
        for fen in fens {
            let (reply, rx) = oneshot::channel();
            self.tx
                .send(Task { fen: fen.clone(), reply })
                .map_err(|_| anyhow!("every engine of the review stopped"))?;
            replies.push(rx);
        }
        let mut out = Vec::with_capacity(replies.len());
        for (rx, fen) in replies.into_iter().zip(fens) {
            out.push(rx.await.context("an engine of the review stopped")?.with_context(|| format!("at {fen}"))?);
        }
        Ok(out)
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        for w in &self.workers {
            w.abort();
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The cache
// ---------------------------------------------------------------------------------------------------------------------

/// What a position is cached under: its FEN without the move counters, so the same position reached at another move
/// number is the same entry.
pub fn cache_key(fen: &str) -> String {
    fen.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
}

/// The cached evaluations among `keys`, by key.
pub fn cached(
    c: &Connection,
    keys: &[String],
    build: i64,
    nodes: u64,
    multipv: u32,
) -> Result<HashMap<String, PositionEval>> {
    let mut stmt = c.prepare_cached(
        "SELECT data FROM position_evals WHERE fen = ?1 AND build_id = ?2 AND nodes = ?3 AND multipv = ?4",
    )?;
    let mut out = HashMap::new();
    for key in keys {
        if out.contains_key(key) {
            continue;
        }
        let mut rows = stmt.query(params![key, build, nodes as i64, multipv])?;
        if let Some(row) = rows.next()?
            && let Ok(eval) = serde_json::from_str::<PositionEval>(&row.get::<_, String>(0)?)
        {
            out.insert(key.clone(), eval);
        }
    }
    Ok(out)
}

pub fn store(c: &mut Connection, evals: &[(String, PositionEval)], build: i64, nodes: u64, multipv: u32) -> Result<()> {
    let tx = c.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO position_evals (fen, build_id, nodes, multipv, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (key, eval) in evals {
            stmt.execute(params![key, build, nodes as i64, multipv, serde_json::to_string(eval)?])?;
        }
    }
    tx.commit()?;
    Ok(())
}
