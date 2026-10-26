//! The formats, one module each, behind one reader and one writer.

pub mod bullet;
pub mod pgn;
pub mod rbinpack;
pub mod sfbinpack;
pub mod viri;

use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use shakmaty::{Color, Position};

use crate::{filter::PositionFilter, formats::pgn::ScoreUnits, game::Game};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// Viridithas's game chains (`.vf`), what bullet loads.
    Viriformat,
    /// Reckless's game records (`.rbinpack`).
    Rbinpack,
    /// Stockfish's binpack (`.binpack`).
    Binpack,
    /// bullet's 32-byte positions (`.bullet`), for filtered positions.
    Bulletformat,
    /// PGN with engine scores in the comments, read only (`.pgn`).
    Pgn,
}

impl Format {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        let ext = name.rsplit('.').next()?;
        Some(match ext {
            "vf" | "viri" | "viriformat" => Self::Viriformat,
            "rbinpack" => Self::Rbinpack,
            "binpack" => Self::Binpack,
            "bullet" | "bf" => Self::Bulletformat,
            "pgn" => Self::Pgn,
            _ => return None,
        })
    }

    pub fn detect(path: &Path, given: Option<Format>) -> Result<Self> {
        match given.or_else(|| Self::from_path(path)) {
            Some(f) => Ok(f),
            None => bail!("cannot tell the format of {} from its name; pass --format", path.display()),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Viriformat => "vf",
            Self::Rbinpack => "rbinpack",
            Self::Binpack => "binpack",
            Self::Bulletformat => "bullet",
            Self::Pgn => "pgn",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Viriformat => "viriformat",
            Self::Rbinpack => "rbinpack",
            Self::Binpack => "binpack",
            Self::Bulletformat => "bulletformat",
            Self::Pgn => "pgn",
        }
    }

    /// Games in, games out: every format but bulletformat.
    pub fn has_games(self) -> bool {
        self != Self::Bulletformat
    }
}

/// Reads games from any game format.
pub enum GameReader {
    Viri(BufReader<File>),
    Rbinpack(BufReader<File>),
    Binpack(Box<sfbinpack::Reader>),
    Pgn(Box<pgn::Reader<BufReader<File>>>),
}

impl GameReader {
    pub fn open(path: &Path, format: Format, units: ScoreUnits) -> Result<Self> {
        let open = || -> Result<BufReader<File>> {
            Ok(BufReader::with_capacity(1 << 20, File::open(path).with_context(|| format!("open {}", path.display()))?))
        };
        Ok(match format {
            Format::Viriformat => Self::Viri(open()?),
            Format::Rbinpack => Self::Rbinpack(open()?),
            Format::Binpack => Self::Binpack(Box::new(sfbinpack::Reader::open(path)?)),
            Format::Pgn => Self::Pgn(Box::new(pgn::Reader::new(open()?, units))),
            Format::Bulletformat => bail!("bulletformat holds positions, not games: it can be written, not converted"),
        })
    }

    pub fn next_game(&mut self) -> Result<Option<Game>> {
        match self {
            Self::Viri(r) => viri::read_game(r),
            Self::Rbinpack(r) => rbinpack::read_game(r),
            Self::Binpack(r) => r.next_game(),
            Self::Pgn(r) => r.next_game(),
        }
    }

    /// Games the reader could not use and skipped.
    pub fn skipped(&self) -> u64 {
        match self {
            Self::Binpack(r) => r.skipped,
            Self::Pgn(r) => r.counts.skipped,
            _ => 0,
        }
    }
}

impl Iterator for GameReader {
    type Item = Result<Game>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_game().transpose()
    }
}

/// Writes games into any format but PGN. Bulletformat writes the positions that pass its filter.
pub enum GameWriter {
    Viri(BufWriter<File>),
    Rbinpack(BufWriter<File>),
    Binpack(Box<sfbinpack::Writer>),
    Bullet(BufWriter<File>, PositionFilter),
}

impl GameWriter {
    pub fn create(path: &Path, format: Format, filter: PositionFilter) -> Result<Self> {
        Self::open(path, format, filter, false)
    }

    /// Opens `path` for writing, appending to what is there when `append` (not for binpack).
    pub fn open(path: &Path, format: Format, filter: PositionFilter, append: bool) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = || -> Result<BufWriter<File>> {
            let f = OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(path)
                .with_context(|| format!("create {}", path.display()))?;
            Ok(BufWriter::with_capacity(1 << 20, f))
        };
        Ok(match format {
            Format::Viriformat => Self::Viri(file()?),
            Format::Rbinpack => Self::Rbinpack(file()?),
            Format::Binpack if append => bail!("binpack cannot be appended to"),
            Format::Binpack => Self::Binpack(Box::new(sfbinpack::Writer::create(path)?)),
            Format::Bulletformat => Self::Bullet(file()?, filter),
            Format::Pgn => bail!("PGN is read only here"),
        })
    }

    /// Writes a game; returns how many positions were written.
    pub fn write_game(&mut self, game: &Game) -> Result<u64> {
        match self {
            Self::Viri(w) => viri::write_game(w, game).map(|_| game.len() as u64),
            Self::Rbinpack(w) => rbinpack::write_game(w, game).map(|_| game.len() as u64),
            Self::Binpack(w) => w.write_game(game).map(|_| game.len() as u64),
            Self::Bullet(w, filter) => {
                let mut written = 0;
                let mut error = None;
                game.for_each_position(|pos, entry, _| {
                    if error.is_some() || !filter.keep(pos, entry) {
                        return;
                    }
                    let white = if pos.turn() == Color::White { entry.score } else { entry.score.saturating_neg() };
                    match bullet::encode(pos, white, game.result).and_then(|r| bullet::write(w, &r)) {
                        Ok(()) => written += 1,
                        Err(e) => error = Some(e),
                    }
                });
                match error {
                    Some(e) => Err(e),
                    None => Ok(written),
                }
            }
        }
    }

    pub fn finish(self) -> Result<()> {
        match self {
            Self::Viri(mut w) | Self::Rbinpack(mut w) | Self::Bullet(mut w, _) => w.flush()?,
            Self::Binpack(w) => w.finish()?,
        }
        Ok(())
    }
}

/// Positions and games in a file, as fast as the format allows (bulletformat has no games: 0).
pub fn count(path: &Path, format: Format) -> Result<(u64, u64)> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    match format {
        Format::Viriformat => viri::count(&mut file),
        Format::Rbinpack => rbinpack::count(&mut file),
        Format::Bulletformat => Ok((file.metadata()?.len() / bullet::SIZE as u64, 0)),
        Format::Binpack | Format::Pgn => {
            drop(file);
            let mut reader = GameReader::open(path, format, ScoreUnits::Cp)?;
            let (mut positions, mut games) = (0, 0);
            while let Some(game) = reader.next_game()? {
                positions += game.len() as u64;
                games += 1;
            }
            Ok((positions, games))
        }
    }
}
