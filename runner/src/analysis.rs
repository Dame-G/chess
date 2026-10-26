//! Position analysis with a build: one-shot (a search to a limit, answered in the response, for agents and scripts)
//! and live (an infinite search streamed as `analysis.info` events, for the board). One live search at a time.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{
    api::{ApiError, AppState},
    engine::{
        build,
        uci::{Engine, Info, Limits, Search},
    },
};

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct AnalysisRequest {
    /// The build to analyse with. Defaults to the newest build.
    pub build: Option<i64>,
    /// The position as FEN, or `startpos` (the default).
    #[serde(default = "startpos")]
    pub fen: String,
    /// Moves from that position, in UCI notation.
    #[serde(default)]
    pub moves: Vec<String>,
    /// How many best lines to report.
    #[serde(default = "one")]
    pub multipv: u32,
    #[serde(default = "one")]
    pub threads: u32,
    /// Hash table size in MiB.
    #[serde(default = "hash")]
    pub hash: u32,
    /// Where a one-shot search stops. Left empty, a one-shot search stops at depth 20; a live one runs until stopped.
    #[serde(default)]
    pub limits: Limits,
}

fn startpos() -> String {
    "startpos".into()
}
fn one() -> u32 {
    1
}
fn hash() -> u32 {
    64
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct AnalysisResult {
    pub build: i64,
    pub engine: String,
    pub bestmove: String,
    /// The last report of each line, best first.
    pub lines: Vec<Info>,
}

async fn start(state: &AppState, req: &AnalysisRequest) -> Result<(build::Build, Engine), ApiError> {
    let id = req.build;
    let found = state
        .store
        .call(move |c| match id {
            Some(id) => build::get(c, id),
            None => Ok(build::list(c)?.into_iter().find(|b| b.deleted_at.is_none())),
        })
        .await?;
    let b = match found {
        Some(b) if b.deleted_at.is_some() => {
            return Err(ApiError::conflict(&format!("build {} was deleted: build its commit again", b.id)));
        }
        Some(b) => b,
        None if id.is_some() => return Err(ApiError::not_found("no such build")),
        None => return Err(ApiError::conflict("no builds yet: build one first")),
    };
    let mut engine = Engine::start(&build::binary(&state.config.data, &b)).await?;
    engine.set_option("Threads", req.threads).await?;
    engine.set_option("Hash", req.hash).await?;
    engine.set_option("MultiPV", req.multipv).await?;
    engine.ready().await?;
    engine.go(&req.fen, &req.moves, &req.limits).await?;
    Ok((b, engine))
}

/// Keeps the newest report of each line. An engine's `multipv` counts from 1; a line it numbers beyond what was asked
/// for is ignored.
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

pub async fn analyse(state: &AppState, mut req: AnalysisRequest) -> Result<AnalysisResult, ApiError> {
    // Lichess forbids engine help while you play there.
    crate::lichess::me::check_fair_play(state)?;
    let l = &req.limits;
    if l.depth.is_none() && l.nodes.is_none() && l.movetime.is_none() {
        req.limits.depth = Some(20);
    }
    let (b, mut engine) = start(state, &req).await?;
    let mut lines = Vec::new();
    loop {
        match engine.next().await? {
            Search::Info(info) => record(&mut lines, info, req.multipv),
            Search::Done(bestmove) => {
                return Ok(AnalysisResult { build: b.id, engine: engine.name.clone(), bestmove, lines });
            }
        }
    }
}

/// How long a live search runs with no event stream open before it stops: longer than a page takes to reconnect.
const UNWATCHED: Duration = Duration::from_secs(15);

pub struct Live {
    current: Mutex<Option<(u64, CancellationToken)>>,
    next: AtomicU64,
}

impl Live {
    pub fn new() -> Self {
        Self { current: Mutex::new(None), next: AtomicU64::new(1) }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct LiveSession {
    /// Events of this search carry this id.
    pub session: u64,
    pub build: i64,
}

/// Starts a live search, stopping any other. Its reports arrive as `analysis.info` events, then `analysis.stopped`. It
/// also stops by itself once no event stream has been open for [`UNWATCHED`].
pub async fn start_live(state: &Arc<AppState>, req: AnalysisRequest) -> Result<LiveSession, ApiError> {
    stop_live(state);
    // Lichess forbids engine help while you play there; a game that starts later stops this search too.
    crate::lichess::me::check_fair_play(state)?;
    let (b, mut engine) = start(state, &req).await?;
    let session = state.live.next.fetch_add(1, Ordering::Relaxed);
    let token = CancellationToken::new();
    // Two starts at once both get past `stop_live` above: the one replaced here is stopped too, or its engine would
    // search forever with nothing left to stop it.
    if let Some((_, replaced)) = state.live.current.lock().unwrap().replace((session, token.clone())) {
        replaced.cancel();
    }
    // A Lichess game that started while the engine loaded found nothing to stop yet.
    if state.me.locked() {
        token.cancel();
    }

    let state = state.clone();
    tokio::spawn(async move {
        // Once stopped, the engine has a few seconds to answer before it is killed (dropped) without its best move.
        let mut deadline = None;
        // A board closed without stopping its search (a closed tab, a lost connection) leaves no one following the
        // events: the search stops then, rather than taking the machine's threads from tests and the bot for good.
        let mut watch = tokio::time::interval(UNWATCHED / 3);
        let mut unwatched_since: Option<tokio::time::Instant> = None;
        let bestmove = loop {
            let next = tokio::select! {
                next = engine.next() => next,
                _ = watch.tick(), if deadline.is_none() => {
                    if state.events.listeners() > 0 {
                        unwatched_since = None;
                    } else if unwatched_since.get_or_insert_with(tokio::time::Instant::now).elapsed() >= UNWATCHED {
                        token.cancel();
                    }
                    continue;
                }
                _ = token.cancelled(), if deadline.is_none() => {
                    deadline = Some(tokio::time::Instant::now() + Duration::from_secs(5));
                    let _ = engine.send("stop").await;
                    continue;
                }
                _ = tokio::time::sleep_until(deadline.unwrap_or_else(tokio::time::Instant::now)), if deadline.is_some() => {
                    break None;
                }
            };
            match next {
                Ok(Search::Info(info)) => {
                    state.events.emit("analysis.info", serde_json::json!({ "session": session, "info": info }))
                }
                Ok(Search::Done(best)) => break Some(best),
                Err(_) => break None,
            }
        };
        state.events.emit("analysis.stopped", serde_json::json!({ "session": session, "bestmove": bestmove }));
        let mut current = state.live.current.lock().unwrap();
        if current.as_ref().is_some_and(|(s, _)| *s == session) {
            *current = None;
        }
    });
    Ok(LiveSession { session, build: b.id })
}

pub fn stop_live(state: &AppState) {
    if let Some((_, token)) = state.live.current.lock().unwrap().take() {
        token.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_report_of_each_line_asked_for() {
        let info =
            |multipv, depth| Info::parse(&format!("info depth {depth} multipv {multipv} score cp 1 pv e2e4")).unwrap();
        let mut lines = Vec::new();
        record(&mut lines, info(2, 5), 2);
        record(&mut lines, info(1, 6), 2);
        record(&mut lines, info(0, 7), 2);
        record(&mut lines, info(3, 7), 2);
        assert_eq!(lines.iter().map(|l| (l.multipv, l.depth)).collect::<Vec<_>>(), [(1, 6), (2, 5)]);
    }
}
