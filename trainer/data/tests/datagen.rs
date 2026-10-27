mod common;

use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

use common::fake_engine;
use data::{
    datagen::{self, DatagenConfig, State},
    filter::PositionFilter,
    formats::{Format, GameReader, pgn::ScoreUnits},
    manifest,
    openings::Opening,
    rescore::{Limit, Scorer, UciScorer},
    stats,
};

fn config(engine: PathBuf, threads: usize, target: u64) -> DatagenConfig {
    DatagenConfig {
        engine,
        threads,
        target_positions: target,
        soft_nodes: 100,
        opening_nodes: 100,
        seed: 42,
        ..Default::default()
    }
}

fn read_games(dir: &Path) -> (u64, u64) {
    let (mut positions, mut games) = (0, 0);
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if Format::from_path(&path) != Some(Format::Viriformat) {
            continue;
        }
        for g in GameReader::open(&path, Format::Viriformat, ScoreUnits::Cp).unwrap() {
            let g = g.unwrap();
            g.validate().unwrap();
            positions += g.len() as u64;
            games += 1;
        }
    }
    (positions, games)
}

#[test]
fn datagen_with_a_fake_engine() {
    let dir = tempfile::tempdir().unwrap();
    let stop = AtomicBool::new(false);
    let report = datagen::run(&config(fake_engine(), 2, 2000), dir.path(), &stop, &|_| {}).unwrap();
    assert_eq!(report.finished, "target");
    assert!(report.positions >= 2000, "{report:?}");
    assert_eq!(report.failures, 0);
    assert_eq!(read_games(dir.path()), (report.positions, report.games));

    // The state and the manifest agree with the files.
    let state = State::load(dir.path()).unwrap().unwrap();
    assert_eq!(state.positions(), report.positions);
    assert_eq!(state.parts.len(), 2);
    let m = manifest::read(dir.path()).unwrap().unwrap();
    assert_eq!(m.format, "viriformat");
    assert_eq!(m.source, "datagen");
    assert_eq!((m.positions, m.games, m.size_bytes), (report.positions, report.games, report.bytes));
    for f in &m.files {
        assert_eq!(f.sha256, manifest::sha256_file(&dir.path().join(&f.path)).unwrap());
    }
    assert!(m.generator.unwrap()["config"]["soft_nodes"] == 100);

    // DFRC starts: castling rights that are not the standard ones show up.
    let s =
        stats::stats(&dir.path().join(datagen::part_name(0)), Format::Viriformat, &PositionFilter::RECKLESS).unwrap();
    assert!(s.games > 0 && s.wdl.iter().sum::<u64>() == s.games);
}

#[test]
fn datagen_resumes_after_a_torn_write() {
    let dir = tempfile::tempdir().unwrap();
    let stop = AtomicBool::new(false);
    let first = datagen::run(&config(fake_engine(), 1, 600), dir.path(), &stop, &|_| {}).unwrap();
    let part = datagen::part_path(dir.path(), 0);
    let before = std::fs::read(&part).unwrap();

    // A game half written when the process died.
    let mut f = OpenOptions::new().append(true).open(&part).unwrap();
    f.write_all(&[1, 2, 3, 4, 5, 6, 7]).unwrap();
    drop(f);

    let second = datagen::run(&config(fake_engine(), 1, 1500), dir.path(), &stop, &|_| {}).unwrap();
    assert_eq!(second.finished, "target");
    assert!(second.positions >= 1500 && second.positions > first.positions);
    assert_eq!(second.positions, first.positions + second.session_positions);
    let after = std::fs::read(&part).unwrap();
    assert_eq!(&after[..before.len()], &before[..], "the first run's games are kept as they were");
    assert_eq!(read_games(dir.path()), (second.positions, second.games));
    let state = State::load(dir.path()).unwrap().unwrap();
    assert_eq!(state.sessions.len(), 2);
}

#[test]
fn datagen_stops_at_the_disk_cap() {
    let dir = tempfile::tempdir().unwrap();
    let stop = AtomicBool::new(false);
    let mut c = config(fake_engine(), 2, 1_000_000);
    c.max_bytes = 4000;
    c.opening = Opening::Startpos;
    let report = datagen::run(&c, dir.path(), &stop, &|_| {}).unwrap();
    assert_eq!(report.finished, "disk-cap");
    assert!(report.bytes <= 4000 && report.bytes > 1000, "{report:?}");
}

#[test]
fn datagen_stops_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let stop = AtomicBool::new(true);
    let report = datagen::run(&config(fake_engine(), 1, 1_000_000), dir.path(), &stop, &|_| {}).unwrap();
    assert_eq!(report.finished, "stopped");
    assert_eq!(report.positions, 0);
}

#[test]
fn rescore_with_a_fake_engine() {
    let dir = tempfile::tempdir().unwrap();
    let input = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/sample.vf"));
    let output = dir.path().join("rescored.rbinpack");
    let engine = fake_engine();
    let make = || -> anyhow::Result<Box<dyn Scorer>> {
        Ok(Box::new(UciScorer::start(&engine, Limit::Nodes(10), 16, ScoreUnits::Cp)?))
    };
    let report =
        data::rescore::rescore(input, Format::Viriformat, &output, Format::Rbinpack, 3, &make, &|_, _| {}).unwrap();
    let originals: Vec<_> =
        GameReader::open(input, Format::Viriformat, ScoreUnits::Cp).unwrap().map(|g| g.unwrap()).collect();
    let rescored: Vec<_> =
        GameReader::open(&output, Format::Rbinpack, ScoreUnits::Cp).unwrap().map(|g| g.unwrap()).collect();
    assert_eq!(report.games as usize, originals.len());
    assert_eq!(originals.len(), rescored.len());
    for (a, b) in originals.iter().zip(&rescored) {
        assert_eq!(a.start, b.start);
        assert_eq!(a.len(), b.len());
        // The fake engine's score is the material balance for the side to move.
        b.for_each_position(|pos, e, _| {
            use shakmaty::{Position, Role};
            let board = pos.board();
            let value = |c| {
                [(Role::Pawn, 100), (Role::Knight, 300), (Role::Bishop, 300), (Role::Rook, 500), (Role::Queen, 900)]
                    .iter()
                    .map(|&(r, v)| (board.by_role(r) & board.by_color(c)).count() as i32 * v)
                    .sum::<i32>()
            };
            assert_eq!(i32::from(e.score), value(pos.turn()) - value(!pos.turn()));
        });
    }
    assert!(report.compared > 0 && report.mae > 0.0);
}

/// Datagen with a real engine from Reckless's `datagen` branch, for a few seconds. Builds it in a temporary shared
/// clone of `RECKLESS_DIR` (default `C:\dev\reckless`, never modified), or uses `RECKLESS_DATAGEN_BIN`.
///
/// `cargo test -p data --release --test datagen -- --ignored --nocapture real_reckless`
#[test]
#[ignore]
fn real_reckless_datagen() {
    let build_dir = tempfile::tempdir().unwrap();
    let engine = match std::env::var_os("RECKLESS_DATAGEN_BIN") {
        Some(bin) => PathBuf::from(bin),
        None => build_reckless_datagen(build_dir.path()),
    };
    let dir = tempfile::tempdir().unwrap();
    let threads = std::env::var("DATAGEN_THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or(4);
    let seconds: u64 = std::env::var("DATAGEN_SECONDS").ok().and_then(|t| t.parse().ok()).unwrap_or(20);
    let c = DatagenConfig {
        engine: engine.clone(),
        threads,
        target_positions: u64::MAX,
        soft_nodes: 5000,
        opening_nodes: 5000,
        seed: 1,
        ..Default::default()
    };
    let stop = AtomicBool::new(false);
    let started = Instant::now();
    let report = std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        datagen::run(&c, dir.path(), &stop, &|p| {
            eprintln!("{} positions, {} games, {:.0}/s", p.positions, p.games, p.positions_per_second)
        })
        .unwrap()
    });
    eprintln!("{}", serde_json::to_string_pretty(&report).unwrap());
    eprintln!("wall {:.1}s", started.elapsed().as_secs_f64());
    assert_eq!(report.finished, "stopped");
    assert!(report.games > 0);
    assert_eq!(read_games(dir.path()), (report.positions, report.games));
    let s =
        stats::stats(&dir.path().join(datagen::part_name(0)), Format::Viriformat, &PositionFilter::RECKLESS).unwrap();
    eprintln!("{s}");
    if let Some(keep) = std::env::var_os("DATAGEN_KEEP") {
        std::fs::copy(datagen::part_path(dir.path(), 0), keep).unwrap();
    }
}

fn build_reckless_datagen(work: &Path) -> PathBuf {
    let reckless =
        std::env::var_os("RECKLESS_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\dev\reckless"));
    let clone = work.join("reckless");
    let run = |cmd: &mut std::process::Command| {
        let status = cmd.status().unwrap();
        assert!(status.success(), "{cmd:?}");
    };
    run(std::process::Command::new("git")
        .args(["clone", "--quiet", "--shared", "--no-checkout"])
        .arg(&reckless)
        .arg(&clone));
    // The clone's remote is the checkout, whose remote branches it does not have: check out the commit itself (its
    // objects are shared).
    let reference = std::env::var("RECKLESS_REF").unwrap_or_else(|_| "origin/datagen".to_string());
    let out =
        std::process::Command::new("git").current_dir(&reckless).args(["rev-parse", &reference]).output().unwrap();
    assert!(out.status.success(), "no {reference} in {}", reckless.display());
    let commit = String::from_utf8(out.stdout).unwrap().trim().to_string();
    run(std::process::Command::new("git").current_dir(&clone).args(["checkout", "--quiet", &commit]));
    let mut cargo = std::process::Command::new("cargo");
    cargo.current_dir(&clone).args(["build", "--release", "--no-default-features"]).env_remove("CARGO_TARGET_DIR");
    let net = std::fs::read_dir(reckless.join("networks")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok());
    if let Some(net) = net {
        cargo.env("EVALFILE", net.path());
    }
    run(&mut cargo);
    clone.join("target/release").join(if cfg!(windows) { "reckless.exe" } else { "reckless" })
}
