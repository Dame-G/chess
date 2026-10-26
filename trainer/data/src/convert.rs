//! `data convert` (alias `filter`): games from one format into another, through the game transforms, and through
//! the position filter when the output is bulletformat.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::{
    filter::{GameTransform, PositionFilter},
    formats::{Format, GameReader, GameWriter, pgn::ScoreUnits},
};

#[derive(Clone, Debug)]
pub struct ConvertOptions {
    pub input_format: Option<Format>,
    pub output_format: Format,
    pub units: ScoreUnits,
    pub transform: GameTransform,
    /// Applied when the output is bulletformat.
    pub filter: PositionFilter,
}

#[derive(Debug, Default, Serialize)]
pub struct ConvertReport {
    pub games_in: u64,
    pub positions_in: u64,
    pub games_out: u64,
    pub positions_out: u64,
    /// Games the readers skipped (unreadable, no result) and games the transform dropped.
    pub skipped: u64,
    pub dropped: u64,
    /// Games the writer refused (binpack and Chess960 starts).
    pub refused: u64,
}

pub fn convert(inputs: &[PathBuf], output: &Path, options: &ConvertOptions) -> Result<ConvertReport> {
    let mut writer = GameWriter::create(output, options.output_format, options.filter.clone())?;
    let mut report = ConvertReport::default();
    for input in inputs {
        let format = Format::detect(input, options.input_format)?;
        let mut reader = GameReader::open(input, format, options.units)?;
        while let Some(game) = reader.next_game()? {
            report.games_in += 1;
            report.positions_in += game.len() as u64;
            let Some(game) = options.transform.apply(&game) else {
                report.dropped += 1;
                continue;
            };
            match writer.write_game(&game) {
                Ok(written) => {
                    report.games_out += 1;
                    report.positions_out += written;
                }
                Err(_) if options.output_format == Format::Binpack => report.refused += 1,
                Err(e) => return Err(e),
            }
        }
        report.skipped += reader.skipped();
    }
    writer.finish()?;
    Ok(report)
}
