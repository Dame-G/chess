//! The order reviews search positions in, against the fake engine writing down what it is told: each run on one engine
//! in the order given, a clean table only at the start of a run, a run split over idle engines, and what is left given
//! back when the engines are told to stop.
//!
//! One test, since it sets the fake engine's log for the whole process.

mod common;

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use common::{QUIET_DRAW, SCHOLARS_MATE, harness};
use dameless_analysis::{Job, runs::RunQueue};
use dameless_core::chess::{Position, position_key};

/// The positions of a game, from the start to the end.
fn jobs(san: &[&str]) -> Vec<Job> {
    (0..=san.len())
        .map(|i| {
            let fen = Position::after(None, &san[..i]).unwrap().fen();
            Job { key: position_key(&fen), fen }
        })
        .collect()
}

/// The log, as (engine, what it was told): the FEN of a `position`, or `new` for `ucinewgame`.
fn told(log: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let (pid, rest) = l.split_once(' ').unwrap();
            let what = match rest.strip_prefix("position fen ") {
                Some(fen) => fen.to_owned(),
                None => "new".to_owned(),
            };
            (pid.to_owned(), what)
        })
        .collect()
}

#[tokio::test]
async fn runs_go_in_order_on_one_engine_split_over_idle_ones_and_stop_when_told() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("told.log");
    // SAFETY: set before any engine starts, and this file's only test is the only thing running in the process.
    unsafe { std::env::set_var("FAKE_EVAL_LOG", &log) };
    let h = harness(2).await;
    let spec = h.eval.review_engines(&h.engine, 1_000, 2);
    let (draw, mate) = (jobs(&QUIET_DRAW), jobs(&SCHOLARS_MATE[..4]));
    let fens = |jobs: &[Job]| jobs.iter().map(|j| j.fen.clone()).collect::<Vec<_>>();

    // One engine, two runs: each in order, a clean table between them and not within one.
    let queue = Mutex::new(RunQueue::new(0));
    queue.lock().unwrap().push(draw.clone());
    queue.lock().unwrap().push(mate.clone());
    let found = Mutex::new(Vec::new());
    h.eval.search_runs(&spec, &queue, 1, &|| true, &found).await;
    let found = found.into_inner().unwrap();
    assert_eq!(found.len(), draw.len() + mate.len());
    assert!(found.iter().all(|(_, r)| r.is_ok()));
    let mut expected: Vec<String> = fens(&draw);
    expected.push("new".into());
    expected.extend(fens(&mate));
    let first = told(&log);
    assert_eq!(first.iter().map(|(_, w)| w.clone()).collect::<Vec<_>>(), expected);
    assert!(first.iter().all(|(pid, _)| *pid == first[0].0), "one engine");

    // Told to stop after two positions: the rest of the run goes back to the queue, to start afresh.
    std::fs::remove_file(&log).unwrap();
    let queue = Mutex::new(RunQueue::new(0));
    queue.lock().unwrap().push(draw.clone());
    let asked = AtomicUsize::new(0);
    let found = Mutex::new(Vec::new());
    h.eval.search_runs(&spec, &queue, 1, &|| asked.fetch_add(1, Ordering::SeqCst) < 2, &found).await;
    assert_eq!(found.into_inner().unwrap().len(), 2);
    let mut queue = queue.into_inner().unwrap();
    assert_eq!(queue.len(), draw.len() - 2);
    let next = queue.next(0).unwrap();
    assert_eq!((next.job.fen.as_str(), next.fresh), (draw[2].fen.as_str(), true));

    // Two engines, one run: both search, each its part in order, every position once.
    std::fs::remove_file(&log).unwrap();
    let queue = Mutex::new(RunQueue::new(0));
    queue.lock().unwrap().push(draw.clone());
    let found = Mutex::new(Vec::new());
    h.eval.search_runs(&spec, &queue, 2, &|| true, &found).await;
    assert_eq!(found.into_inner().unwrap().len(), draw.len());
    let index: HashMap<String, usize> = fens(&draw).into_iter().enumerate().map(|(i, f)| (f, i)).collect();
    let mut by_engine: HashMap<String, Vec<usize>> = HashMap::new();
    for (pid, what) in told(&log) {
        if let Some(&i) = index.get(&what) {
            by_engine.entry(pid).or_default().push(i);
        }
    }
    assert_eq!(by_engine.len(), 2, "both engines searched: {by_engine:?}");
    let mut all: Vec<usize> = by_engine.values().flatten().copied().collect();
    all.sort_unstable();
    assert_eq!(all, (0..draw.len()).collect::<Vec<_>>());
    for part in by_engine.values() {
        assert!(part.windows(2).all(|w| w[0] < w[1]), "in order: {part:?}");
    }
}
