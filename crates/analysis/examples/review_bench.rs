//! Times reviews of a fixed set of games with a real engine, and writes what they came to, so two methods (or two
//! revisions of this crate) can be compared on speed and on results.
//!
//! ```text
//! cargo run --release -p dameless-analysis --example review_bench -- \
//!     --engine reckless.exe --games games.json --mode app --cores 6 [--nodes 200000] [--out app.json]
//! ```
//!
//! `games.json` is an array of games as `GET /api/games/{id}` returns them. Modes:
//!
//! - `app`: the `Reviews` executor end to end (the position cache, the judge, the store) in a fresh data directory,
//!   with `--cores` cores and engines: what the app does.
//! - `cold`: every position on a clean engine (`ucinewgame` before each search) on `--cores` engines, judged with the
//!   same judge: the reproducible reference, independent of order and scheduling. Run it with a large `--nodes` for a
//!   reference to measure the others against.
//! - `ordered`: each game's positions in order on one engine after one `ucinewgame`, a game per engine at a time:
//!   reproducible too.
//!
//! `cold` and `ordered` take `--hash` (MiB, 16 by default).
//!
//! The output has, per game, both accuracies and each move's judgement, classification and score before it.

use std::{collections::VecDeque, sync::Mutex, time::Instant};

use dameless_analysis::{Eval, ReviewRequest, ReviewResult, Reviews, search};
use dameless_core::{
    game::Game,
    review::{PositionEval, ReviewInput, ReviewedMove, judge_game, to_search},
};
use dameless_engine::pool::{EngineSpec, Pool, Reset};
use dameless_hub::{Config, Executor, Hub, WorkContext};
use dameless_store::NewGame;
use dameless_work::FixedEngines;
use serde::Serialize;

#[derive(Serialize)]
struct Ply {
    judgement: Option<String>,
    classification: String,
    before: String,
    best: String,
    depth: u32,
}

#[derive(Serialize)]
struct GameOut {
    id: i64,
    white: Option<f64>,
    black: Option<f64>,
    plies: Vec<Ply>,
}

#[derive(Serialize)]
struct Out {
    mode: String,
    cores: usize,
    nodes: u64,
    games: usize,
    positions: usize,
    searched: u64,
    seconds: f64,
    reviews: Vec<GameOut>,
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn plies(plies: &[ReviewedMove]) -> Vec<Ply> {
    plies
        .iter()
        .map(|p| Ply {
            judgement: p.judgement.map(|j| format!("{j:?}")),
            classification: format!("{:?}", p.classification),
            before: format!("{:?}", p.eval_before),
            best: p.best_uci.clone(),
            depth: p.depth,
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let engine = std::fs::canonicalize(arg(&args, "--engine").expect("--engine")).unwrap();
    let games: Vec<Game> =
        serde_json::from_str(&std::fs::read_to_string(arg(&args, "--games").expect("--games")).unwrap()).unwrap();
    let mode = arg(&args, "--mode").unwrap_or_else(|| "app".into());
    let cores: usize = arg(&args, "--cores").map_or(1, |c| c.parse().unwrap());
    let nodes: u64 = arg(&args, "--nodes").map_or(200_000, |c| c.parse().unwrap());
    let positions: usize = games.iter().map(|g| g.moves.len() + 1).sum();

    let started = Instant::now();
    let (reviews, searched) = match mode.as_str() {
        "app" => app(&engine, &games, cores, nodes).await,
        "cold" | "ordered" => {
            let hash: u32 = arg(&args, "--hash").map_or(16, |c| c.parse().unwrap());
            engine_only(&engine, &games, cores, nodes, hash, mode == "ordered").await
        }
        other => panic!("no mode {other}"),
    };
    let seconds = started.elapsed().as_secs_f64();
    eprintln!(
        "{mode}: {} games, {positions} positions ({searched} searched) on {cores} cores in {seconds:.1} s: \
         {:.2} s a game, {:.1} positions a second, {:.2} s a game per core",
        games.len(),
        seconds / games.len() as f64,
        searched as f64 / seconds,
        seconds * cores as f64 / games.len() as f64,
    );
    let out = Out { mode, cores, nodes, games: games.len(), positions, searched, seconds, reviews };
    let json = serde_json::to_string_pretty(&out).unwrap();
    match arg(&args, "--out") {
        Some(path) => std::fs::write(path, json).unwrap(),
        None => println!("{json}"),
    }
}

/// The app's own review, end to end, in a fresh data directory.
async fn app(engine: &std::path::Path, games: &[Game], cores: usize, nodes: u64) -> (Vec<GameOut>, u64) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), cores as u32);
    let hub = Hub::open(config).unwrap();
    let eval = Eval::new(hub.clone(), FixedEngines::new().with("bench", engine, &[]).shared());
    let mut ids = Vec::new();
    for g in games {
        let s = &g.summary;
        let mut new =
            NewGame::starting(s.white.clone(), s.black.clone(), s.source.clone(), s.time, s.started_at.clone());
        new.status = s.status;
        new.outcome = s.outcome;
        new.ending = s.ending;
        new.opening = s.opening.clone();
        new.start_fen = g.start_fen.clone();
        new.moves = g.moves.clone();
        new.ended_at = s.ended_at.clone();
        ids.push(hub.store.insert_game(new).await.unwrap());
    }
    let reviews = Reviews::new(eval);
    let req =
        ReviewRequest { games: ids.clone(), nodes: Some(nodes), concurrency: Some(cores as u32), ..Default::default() };
    let new = reviews.prepare(req).await.unwrap();
    let spec = new.spec.clone();
    let work = hub.store.insert_work(new).await.unwrap();
    let (cx, _cancel) = WorkContext::detached(hub.clone(), work.id, work.kind, spec);
    let result: ReviewResult = serde_json::from_value(reviews.run(cx).await.unwrap()).unwrap();
    let mut out = Vec::new();
    for (g, id) in games.iter().zip(ids) {
        let review = hub.store.review(id, None, None).await.unwrap().expect("reviewed");
        out.push(GameOut {
            id: g.summary.id.0,
            white: review.summary.white.accuracy,
            black: review.summary.black.accuracy,
            plies: plies(&review.plies),
        });
    }
    (out, result.positions_searched)
}

/// The engine alone, `cores` engines sharing one queue. Cold: each position on a clean engine (`ucinewgame` before
/// each search), every position its own job. Ordered: each game one job, its positions searched in order on one
/// engine after one `ucinewgame`, so each search starts from the table the one before left.
async fn engine_only(
    engine: &std::path::Path,
    games: &[Game],
    cores: usize,
    nodes: u64,
    hash: u32,
    ordered: bool,
) -> (Vec<GameOut>, u64) {
    let spec = EngineSpec::new(engine).option("Threads", 1).option("Hash", hash).option("MultiPV", 2);
    let pool = Pool::new(cores);
    let mut jobs: VecDeque<Vec<(usize, usize, String)>> = VecDeque::new();
    let mut found: Vec<Vec<Option<PositionEval>>> = Vec::new();
    let mut searched = 0;
    for (gi, g) in games.iter().enumerate() {
        let positions = to_search(ReviewInput::from(g)).unwrap();
        found.push(vec![None; positions.len()]);
        let mine: Vec<(usize, usize, String)> =
            positions.into_iter().enumerate().filter(|(_, p)| !p.terminal).map(|(pi, p)| (gi, pi, p.fen)).collect();
        searched += mine.len() as u64;
        if ordered {
            jobs.push_back(mine);
        } else {
            jobs.extend(mine.into_iter().map(|j| vec![j]));
        }
    }
    let jobs = Mutex::new(jobs);
    let found = Mutex::new(found);
    let worker = || async {
        loop {
            let Some(job) = jobs.lock().unwrap().pop_front() else { break };
            let mut engine = pool.checkout_with(&spec, Reset::NewGame).await.unwrap();
            for (gi, pi, fen) in job {
                let eval = search::search_fen(&mut engine, &fen, nodes, 2).await.unwrap();
                found.lock().unwrap()[gi][pi] = Some(eval);
            }
        }
    };
    futures::future::join_all((0..cores).map(|_| worker())).await;
    let found = found.into_inner().unwrap();
    let out = games
        .iter()
        .zip(found)
        .map(|(g, evals)| {
            let (summary, ps) = judge_game(ReviewInput::from(g), &evals, "bench", nodes).unwrap();
            GameOut {
                id: g.summary.id.0,
                white: summary.white.accuracy,
                black: summary.black.accuracy,
                plies: plies(&ps),
            }
        })
        .collect();
    (out, searched)
}
