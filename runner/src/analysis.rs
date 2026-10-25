//! Position analysis with a build: one-shot (a search to a limit, answered in the response, for agents and scripts)
//! and live (an infinite search streamed as `analysis.info` events, for the board). One live search at a time.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{
    api::AppState,
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

async fn start(state: &AppState, req: &AnalysisRequest) -> Result<(build::Build, Engine)> {
    let id = req.build;
    let found = state
        .store
        .call(move |c| match id {
            Some(id) => build::get(c, id),
            None => Ok(build::list(c)?.into_iter().next()),
        })
        .await?;
    let Some(b) = found else { bail!(if id.is_some() { "no such build" } else { "no builds yet: build one first" }) };
    let mut engine = Engine::start(&build::binary(&state.config.data, &b)).await?;
    engine.set_option("Threads", req.threads).await?;
    engine.set_option("Hash", req.hash).await?;
    engine.set_option("MultiPV", req.multipv).await?;
    engine.ready().await?;
    engine.go(&req.fen, &req.moves, &req.limits).await?;
    Ok((b, engine))
}

/// Keeps the newest report of each line.
fn record(lines: &mut Vec<Info>, info: Info) {
    let i = info.multipv as usize - 1;
    if lines.len() <= i {
        lines.resize(i + 1, info.clone());
    }
    lines[i] = info;
}

pub async fn analyse(state: &AppState, mut req: AnalysisRequest) -> Result<AnalysisResult> {
    let l = &req.limits;
    if l.depth.is_none() && l.nodes.is_none() && l.movetime.is_none() {
        req.limits.depth = Some(20);
    }
    let (b, mut engine) = start(state, &req).await?;
    let mut lines = Vec::new();
    loop {
        match engine.next().await? {
            Search::Info(info) => record(&mut lines, info),
            Search::Done(bestmove) => {
                return Ok(AnalysisResult { build: b.id, engine: engine.name.clone(), bestmove, lines });
            }
        }
    }
}

pub struct Live {
    current: Mutex<Option<(u64, CancellationToken)>>,
    next: Mutex<u64>,
}

impl Live {
    pub fn new() -> Self {
        Self { current: Mutex::new(None), next: Mutex::new(1) }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct LiveSession {
    /// Events of this search carry this id.
    pub session: u64,
    pub build: i64,
}

/// Starts a live search, stopping any other. Its reports arrive as `analysis.info` events, then `analysis.stopped`.
pub async fn start_live(state: &Arc<AppState>, req: AnalysisRequest) -> Result<LiveSession> {
    stop_live(state);
    let (b, mut engine) = start(state, &req).await?;
    let session = {
        let mut next = state.live.next.lock().unwrap();
        *next += 1;
        *next - 1
    };
    let token = CancellationToken::new();
    *state.live.current.lock().unwrap() = Some((session, token.clone()));

    let state = state.clone();
    tokio::spawn(async move {
        let mut stopping = false;
        let bestmove = loop {
            let next = tokio::select! {
                next = engine.next() => next,
                _ = token.cancelled(), if !stopping => {
                    stopping = true;
                    let _ = engine.send("stop").await;
                    continue;
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
