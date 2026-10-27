//! `data stats`: what a file holds.

use std::{collections::BTreeMap, fs::File, io::BufReader, path::Path};

use anyhow::Result;
use serde::Serialize;
use shakmaty::Position;

use crate::{
    filter::PositionFilter,
    formats::{Format, GameReader, bullet, pgn::ScoreUnits},
};

/// Width of a score histogram bucket.
pub const SCORE_BUCKET: i32 = 100;

#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub format: String,
    pub bytes: u64,
    pub positions: u64,
    /// Games (0 for bulletformat, which has none).
    pub games: u64,
    /// Games the reader skipped (PGN without a result, binpack chains that do not convert).
    pub skipped: u64,
    /// Game results from White's side: `[black wins, draws, white wins]` (bulletformat: positions, from the side to
    /// move's side: `[losses, draws, wins]`).
    pub wdl: [u64; 3],
    /// Positions that pass the filter (Reckless's unless given).
    pub filtered_positions: u64,
    pub filter: PositionFilter,
    /// Positions in check, and positions whose move captures or promotes.
    pub in_check: u64,
    pub tactical: u64,
    /// Score histogram: bucket start (multiples of 100, from the side to move's side) to positions.
    pub scores: BTreeMap<i32, u64>,
    /// Pieces on the board (kings included) to positions.
    pub pieces: BTreeMap<u32, u64>,
    /// Game length in positions to games.
    pub lengths: BTreeMap<usize, u64>,
    pub mean_score: f64,
    pub mean_abs_score: f64,
    pub bytes_per_position: f64,
}

pub fn stats(path: &Path, format: Format, filter: &PositionFilter) -> Result<Stats> {
    let mut s = Stats {
        format: format.name().to_string(),
        bytes: std::fs::metadata(path)?.len(),
        filter: filter.clone(),
        ..Default::default()
    };
    let (mut sum, mut abs_sum) = (0f64, 0f64);
    let mut add_score = |s: &mut Stats, score: i16| {
        let bucket = (i32::from(score)).div_euclid(SCORE_BUCKET) * SCORE_BUCKET;
        *s.scores.entry(bucket).or_default() += 1;
        sum += f64::from(score);
        abs_sum += f64::from(score).abs();
    };

    if format == Format::Bulletformat {
        let mut input = BufReader::with_capacity(1 << 20, File::open(path)?);
        while let Some(record) = bullet::read(&mut input)? {
            let d = bullet::decode(&record);
            s.positions += 1;
            s.wdl[usize::from(d.result.min(2))] += 1;
            *s.pieces.entry(d.pieces).or_default() += 1;
            add_score(&mut s, d.score);
        }
        s.filtered_positions = s.positions;
    } else {
        let mut reader = GameReader::open(path, format, ScoreUnits::Cp)?;
        while let Some(game) = reader.next_game()? {
            s.games += 1;
            s.positions += game.len() as u64;
            s.wdl[usize::from(game.result.as_u8())] += 1;
            *s.lengths.entry(game.len()).or_default() += 1;
            game.for_each_position(|pos, entry, _| {
                *s.pieces.entry(pos.board().occupied().count() as u32).or_default() += 1;
                s.in_check += u64::from(pos.is_check());
                s.tactical += u64::from(crate::game::is_tactical(entry.mv));
                s.filtered_positions += u64::from(filter.keep(pos, entry));
                add_score(&mut s, entry.score);
            });
        }
        s.skipped = reader.skipped();
    }
    if s.positions > 0 {
        s.mean_score = sum / s.positions as f64;
        s.mean_abs_score = abs_sum / s.positions as f64;
        s.bytes_per_position = s.bytes as f64 / s.positions as f64;
    }
    Ok(s)
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "format     {}", self.format)?;
        writeln!(f, "size       {} bytes ({:.2} bytes per position)", self.bytes, self.bytes_per_position)?;
        writeln!(f, "positions  {}", self.positions)?;
        if self.games > 0 {
            writeln!(f, "games      {} (mean length {:.1})", self.games, self.positions as f64 / self.games as f64)?;
            writeln!(f, "results    1-0 {}  1/2 {}  0-1 {}", self.wdl[2], self.wdl[1], self.wdl[0])?;
            writeln!(f, "in check   {}", self.in_check)?;
            writeln!(f, "tactical   {}", self.tactical)?;
        } else {
            writeln!(f, "results    win {}  draw {}  loss {} (side to move)", self.wdl[2], self.wdl[1], self.wdl[0])?;
        }
        if self.skipped > 0 {
            writeln!(f, "skipped    {}", self.skipped)?;
        }
        writeln!(f, "filtered   {} pass {:?}", self.filtered_positions, self.filter)?;
        writeln!(f, "score      mean {:.1}, mean |score| {:.1}", self.mean_score, self.mean_abs_score)?;
        writeln!(f, "scores (side to move, buckets of {SCORE_BUCKET}):")?;
        histogram(f, self.scores.iter().map(|(k, v)| (format!("{k:>6}"), *v)), self.positions)?;
        writeln!(f, "pieces:")?;
        histogram(f, self.pieces.iter().map(|(k, v)| (format!("{k:>6}"), *v)), self.positions)?;
        Ok(())
    }
}

fn histogram(
    f: &mut std::fmt::Formatter<'_>,
    rows: impl Iterator<Item = (String, u64)>,
    total: u64,
) -> std::fmt::Result {
    let rows: Vec<_> = rows.collect();
    let max = rows.iter().map(|r| r.1).max().unwrap_or(1).max(1);
    for (label, count) in rows {
        let bar = "#".repeat(((count * 40) / max) as usize);
        let share = if total > 0 { 100.0 * count as f64 / total as f64 } else { 0.0 };
        writeln!(f, "  {label} {count:>10} {share:>5.1}% {bar}")?;
    }
    Ok(())
}
