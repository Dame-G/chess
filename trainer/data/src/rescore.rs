//! `data rescore`: replace every position's score with a new one, as Reckless's `rescore` tool does
//! (`src/tools/rescore.rs` on `nnue-tools-rsn-3`), and say how far the old scores were from the new ones (MSE, MAE
//! and the scores' moments, over the positions Reckless's filter keeps).
//!
//! The scorer is a hook (`Scorer`): `UciScorer` searches each position with an engine at fixed nodes or depth, from
//! its FEN without history; a net's static eval (the trainer's CPU reference) can be plugged in the same way.

use std::{path::Path, time::Duration};

use anyhow::Result;
use serde::Serialize;
use shakmaty::Chess;

use crate::{
    filter::PositionFilter,
    formats::{Format, GameReader, GameWriter, pgn::ScoreUnits},
    game::{self, Game, MATE_SCORE},
    uci::{self, Engine, Score},
};

/// Scores one position from the side to move's point of view.
pub trait Scorer: Send {
    fn score(&mut self, pos: &Chess) -> Result<i16>;
}

#[derive(Copy, Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Limit {
    Nodes(u64),
    Depth(u32),
}

pub struct UciScorer {
    engine: Engine,
    go: String,
    units: ScoreUnits,
    timeout: Duration,
}

impl UciScorer {
    pub fn start(path: &Path, limit: Limit, hash_mb: u32, units: ScoreUnits) -> Result<Self> {
        let mut engine = Engine::start(path)?;
        engine.set_option("Threads", 1)?;
        engine.set_option("Hash", hash_mb)?;
        for (name, value) in [("UCI_Chess960", "true"), ("Minimal", "true")] {
            if engine.has_option(name) {
                engine.set_option(name, value)?;
            }
        }
        engine.new_game()?;
        let go = match limit {
            Limit::Nodes(n) => format!("go nodes {n}"),
            Limit::Depth(d) => format!("go depth {d}"),
        };
        Ok(Self { engine, go, units, timeout: Duration::from_secs(60) })
    }
}

impl Scorer for UciScorer {
    fn score(&mut self, pos: &Chess) -> Result<i16> {
        let result = self.engine.search(&uci::position_command(&game::fen(pos), &[]), &self.go, self.timeout)?;
        Ok(match result.score {
            Some(Score::Cp(cp)) => self.units.convert(cp, pos),
            Some(Score::Mate(n)) if n > 0 => MATE_SCORE,
            Some(Score::Mate(_)) => -MATE_SCORE,
            None => 0,
        })
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Moments {
    pub count: u64,
    pub mean: f64,
    pub abs_mean: f64,
    pub stddev: f64,
    pub min: i16,
    pub max: i16,
    #[serde(skip)]
    sums: (f64, f64, f64),
}

impl Moments {
    fn add(&mut self, v: i16) {
        if self.count == 0 {
            self.min = v;
            self.max = v;
        }
        self.count += 1;
        let x = f64::from(v);
        self.sums.0 += x;
        self.sums.1 += x.abs();
        self.sums.2 += x * x;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }

    fn finish(&mut self) {
        if self.count > 0 {
            let n = self.count as f64;
            self.mean = self.sums.0 / n;
            self.abs_mean = self.sums.1 / n;
            self.stddev = (self.sums.2 / n - self.mean * self.mean).max(0.0).sqrt();
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct RescoreReport {
    pub games: u64,
    pub positions: u64,
    /// Positions compared (those Reckless's filter keeps, judged on the old score).
    pub compared: u64,
    pub mse: f64,
    pub mae: f64,
    pub original: Moments,
    pub rescored: Moments,
}

/// Rescores `input` into `output`, `threads` scorers at once (each made by `make_scorer`).
pub fn rescore(
    input: &Path,
    input_format: Format,
    output: &Path,
    output_format: Format,
    threads: usize,
    make_scorer: &(dyn Fn() -> Result<Box<dyn Scorer>> + Sync),
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<RescoreReport> {
    anyhow::ensure!(input != output, "rescoring in place is not supported");
    let threads = threads.max(1);
    let mut reader = GameReader::open(input, input_format, ScoreUnits::Cp)?;
    let mut writer = GameWriter::create(output, output_format, PositionFilter::NONE)?;
    let mut scorers = (0..threads).map(|_| make_scorer()).collect::<Result<Vec<_>>>()?;
    let mut report = RescoreReport::default();
    let (mut sq, mut abs) = (0f64, 0f64);
    let filter = PositionFilter::RECKLESS;

    loop {
        let mut batch: Vec<Game> = Vec::new();
        while batch.len() < 16 * threads {
            match reader.next_game()? {
                Some(g) => batch.push(g),
                None => break,
            }
        }
        if batch.is_empty() {
            break;
        }
        let originals = batch.clone();
        let chunk = batch.len().div_ceil(threads);
        let results: Vec<Result<()>> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .chunks_mut(chunk)
                .zip(scorers.iter_mut())
                .map(|(games, scorer)| {
                    scope.spawn(move || -> Result<()> {
                        for game in games.iter_mut() {
                            let mut pos = game.start.clone();
                            for entry in game.entries.iter_mut() {
                                entry.score = scorer.score(&pos)?;
                                shakmaty::Position::play_unchecked(&mut pos, entry.mv);
                            }
                        }
                        Ok(())
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(anyhow::anyhow!("scorer panicked")))).collect()
        });
        for r in results {
            r?;
        }
        for (old, new) in originals.iter().zip(&batch) {
            old.for_each_position(|pos, entry, i| {
                let rescored = new.entries[i].score;
                if filter.keep(pos, entry) {
                    report.original.add(entry.score);
                    report.rescored.add(rescored);
                    let e = f64::from(rescored) - f64::from(entry.score);
                    sq += e * e;
                    abs += e.abs();
                    report.compared += 1;
                }
            });
            writer.write_game(new)?;
            report.games += 1;
            report.positions += new.len() as u64;
        }
        progress(report.games, report.positions);
    }
    writer.finish()?;
    if report.compared > 0 {
        report.mse = sq / report.compared as f64;
        report.mae = abs / report.compared as f64;
    }
    report.original.finish();
    report.rescored.finish();
    Ok(report)
}
