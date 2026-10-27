//! `data`: the training-data tools. Every command takes `--json` and then prints one JSON document to stdout.

use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use data::{
    convert::{ConvertOptions, convert},
    datagen::{Adjudication, DatagenConfig},
    filter::{GameTransform, PositionFilter},
    formats::{Format, pgn::ScoreUnits},
    manifest::{self, ManifestOptions},
    openings::Opening,
    parse_count, parse_size,
    rescore::{Limit, Scorer, UciScorer},
    rng::Rng,
    shuffle, stats,
};

#[derive(Parser)]
#[command(name = "data", version, about = "Training data for Dameless: formats, conversion, filters, stats, datagen")]
struct Cli {
    /// Print one JSON document to stdout instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert games between formats (PGN, binpack, rbinpack, viriformat in; viriformat, rbinpack, binpack,
    /// bulletformat out), trimming games on the way; bulletformat output also applies the position filter.
    #[command(visible_alias = "filter")]
    Convert {
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        /// The inputs' format (default: from each name).
        #[arg(long)]
        from: Option<Format>,
        /// The output's format (default: from its name).
        #[arg(long)]
        to: Option<Format>,
        /// Units of PGN scores.
        #[arg(long, value_enum, default_value_t = ScoreUnits::Cp)]
        units: ScoreUnits,
        #[command(flatten)]
        transform: TransformArgs,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Positions, games, results, score and piece-count histograms of a file.
    Stats {
        file: PathBuf,
        #[arg(long)]
        format: Option<Format>,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Shuffle the records (games, or bulletformat positions) of one or more files into one, within a memory budget.
    Shuffle {
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        format: Option<Format>,
        /// Memory to use (beyond it the shuffle goes through temporary buckets beside the output).
        #[arg(long, value_parser = parse_size, default_value = "2G")]
        memory: u64,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Interleave files record by record at random, in proportion to their sizes; constant memory.
    Interleave {
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        format: Option<Format>,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Replace every score with an engine's, searched at fixed nodes or depth; reports MSE and MAE.
    Rescore {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        engine: PathBuf,
        #[arg(long, conflicts_with = "depth")]
        nodes: Option<u64>,
        #[arg(long)]
        depth: Option<u32>,
        #[arg(long, default_value_t = 1)]
        threads: usize,
        #[arg(long, default_value_t = 16)]
        hash: u32,
        #[arg(long, value_enum, default_value_t = ScoreUnits::Cp)]
        units: ScoreUnits,
        #[arg(long)]
        from: Option<Format>,
        #[arg(long)]
        to: Option<Format>,
    },
    /// Self-play data with an engine built from Reckless's `datagen` branch, written as viriformat; resumable.
    Datagen(DatagenArgs),
    /// (Re)build a dataset directory's manifest.json.
    Manifest {
        dir: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long, value_enum)]
        units: Option<ScoreUnits>,
    },
}

#[derive(Args)]
struct TransformArgs {
    /// Drop this many plies from the start of every game.
    #[arg(long, default_value_t = 0)]
    skip_plies: u32,
    /// Cut each game at the first score this large in absolute value (0: never).
    #[arg(long, default_value_t = 0)]
    cut_tail_at: u16,
    /// Drop games left with fewer positions.
    #[arg(long, default_value_t = 0)]
    min_positions: u32,
}

impl TransformArgs {
    fn get(&self) -> GameTransform {
        GameTransform { skip_plies: self.skip_plies, cut_tail_at: self.cut_tail_at, min_positions: self.min_positions }
    }
}

#[derive(Args)]
struct FilterArgs {
    /// Position filter: keep everything.
    #[arg(long)]
    no_filter: bool,
    /// Position filter: skip positions before this ply.
    #[arg(long)]
    min_ply: Option<u32>,
    /// Position filter: skip positions with fewer pieces.
    #[arg(long)]
    min_pieces: Option<u32>,
    /// Position filter: skip positions whose absolute score is at least this (Reckless: 4096).
    #[arg(long)]
    max_score: Option<u16>,
    /// Position filter: keep positions whose move captures or promotes.
    #[arg(long)]
    keep_tactical: bool,
    /// Position filter: keep positions in check.
    #[arg(long)]
    keep_check: bool,
}

impl FilterArgs {
    fn get(&self) -> PositionFilter {
        let mut f = if self.no_filter { PositionFilter::NONE } else { PositionFilter::RECKLESS };
        if let Some(v) = self.min_ply {
            f.min_ply = v;
        }
        if let Some(v) = self.min_pieces {
            f.min_pieces = v;
        }
        if let Some(v) = self.max_score {
            f.max_score = v;
        }
        if self.keep_tactical {
            f.skip_tactical = false;
        }
        if self.keep_check {
            f.skip_check = false;
        }
        f
    }
}

#[derive(Args)]
struct DatagenArgs {
    /// The engine, built from Reckless's `datagen` branch.
    #[arg(long)]
    engine: PathBuf,
    /// The dataset directory (created; a run on a directory with datagen.json resumes it).
    #[arg(short, long)]
    out: PathBuf,
    #[arg(long, default_value_t = 1)]
    threads: usize,
    /// Positions to reach (5M, 200k, ...).
    #[arg(long, value_parser = parse_count, default_value = "1M")]
    positions: u64,
    /// Disk cap for the dataset's files.
    #[arg(long, value_parser = parse_size, default_value = "10G")]
    max_size: u64,
    #[arg(long, default_value_t = 5000)]
    soft_nodes: u64,
    #[arg(long, default_value_t = 16)]
    hash: u32,
    /// dfrc, chess960 or startpos (or --book).
    #[arg(long, default_value = "dfrc")]
    opening: String,
    /// An EPD/FEN book to start from instead.
    #[arg(long)]
    book: Option<PathBuf>,
    #[arg(long, default_value_t = 8)]
    random_plies: u32,
    /// Throw away openings scored beyond this (UCI cp; 0: no check).
    #[arg(long, default_value_t = 1000)]
    max_opening_score: i32,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long, value_enum, default_value_t = ScoreUnits::Cp)]
    units: ScoreUnits,
    #[arg(long, default_value_t = 1000)]
    win_score: i32,
    #[arg(long, default_value_t = 4)]
    win_plies: u32,
    #[arg(long, default_value_t = 10)]
    draw_score: i32,
    #[arg(long, default_value_t = 12)]
    draw_plies: u32,
    #[arg(long, default_value_t = 60)]
    draw_after_ply: u32,
    #[arg(long, default_value_t = 400)]
    max_plies: u32,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn print(json: bool, value: &impl serde::Serialize, text: impl FnOnce() -> String) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", text());
    }
    Ok(())
}

fn one_format(inputs: &[PathBuf], output: &Path, given: Option<Format>) -> Result<Format> {
    if let Some(f) = given {
        return Ok(f);
    }
    let f = Format::detect(output, None)?;
    for input in inputs {
        if Format::from_path(input).is_some_and(|i| i != f) {
            bail!("{} and {} differ in format; convert first", input.display(), output.display());
        }
    }
    Ok(f)
}

fn run(cli: Cli) -> Result<()> {
    let json = cli.json;
    match cli.command {
        Command::Convert { inputs, output, from, to, units, transform, filter } => {
            if inputs.is_empty() {
                bail!("no inputs");
            }
            let options = ConvertOptions {
                input_format: from,
                output_format: Format::detect(&output, to)?,
                units,
                transform: transform.get(),
                filter: filter.get(),
            };
            let report = convert(&inputs, &output, &options)?;
            print(json, &report, || {
                format!(
                    "{} games ({} positions) in, {} games ({} positions) out; {} skipped, {} dropped, {} refused",
                    report.games_in,
                    report.positions_in,
                    report.games_out,
                    report.positions_out,
                    report.skipped,
                    report.dropped,
                    report.refused
                )
            })
        }
        Command::Stats { file, format, filter } => {
            let format = Format::detect(&file, format)?;
            let s = stats::stats(&file, format, &filter.get())?;
            print(json, &s, || s.to_string())
        }
        Command::Shuffle { inputs, output, format, memory, seed } => {
            let format = one_format(&inputs, &output, format)?;
            let r = shuffle::shuffle(&inputs, &output, format, memory, seed.unwrap_or_else(Rng::time_seed))?;
            print(json, &r, || format!("{} records ({} bytes) shuffled, {} bucket(s)", r.records, r.bytes, r.buckets))
        }
        Command::Interleave { inputs, output, format, seed } => {
            let format = one_format(&inputs, &output, format)?;
            let r = shuffle::interleave(&inputs, &output, format, seed.unwrap_or_else(Rng::time_seed))?;
            print(json, &r, || format!("{} records ({} bytes) interleaved", r.records, r.bytes))
        }
        Command::Rescore { input, output, engine, nodes, depth, threads, hash, units, from, to } => {
            let limit = match (nodes, depth) {
                (Some(n), _) => Limit::Nodes(n),
                (None, Some(d)) => Limit::Depth(d),
                (None, None) => bail!("give --nodes or --depth"),
            };
            let in_format = Format::detect(&input, from)?;
            let out_format = Format::detect(&output, to)?;
            let make = || -> Result<Box<dyn Scorer>> { Ok(Box::new(UciScorer::start(&engine, limit, hash, units)?)) };
            let quiet = json;
            let report = data::rescore::rescore(&input, in_format, &output, out_format, threads, &make, &|g, p| {
                if !quiet {
                    eprintln!("{g} games, {p} positions");
                }
            })?;
            print(json, &report, || {
                format!(
                    "{} games, {} positions rescored; over {} filtered positions: MSE {:.1}, MAE {:.2}\n\
                     original: mean {:.1}, |mean| {:.1}, sd {:.1}\nrescored: mean {:.1}, |mean| {:.1}, sd {:.1}",
                    report.games,
                    report.positions,
                    report.compared,
                    report.mse,
                    report.mae,
                    report.original.mean,
                    report.original.abs_mean,
                    report.original.stddev,
                    report.rescored.mean,
                    report.rescored.abs_mean,
                    report.rescored.stddev
                )
            })
        }
        Command::Datagen(args) => {
            let opening = match (&args.book, args.opening.as_str()) {
                (Some(book), _) => Opening::Book(book.clone()),
                (None, "dfrc") => Opening::Dfrc,
                (None, "chess960" | "frc") => Opening::Chess960,
                (None, "startpos" | "standard") => Opening::Startpos,
                (None, other) => bail!("unknown opening {other:?}: dfrc, chess960 or startpos"),
            };
            let config = DatagenConfig {
                engine: args.engine,
                threads: args.threads,
                target_positions: args.positions,
                max_bytes: args.max_size,
                soft_nodes: args.soft_nodes,
                hash_mb: args.hash,
                opening,
                random_plies: args.random_plies,
                max_opening_score: args.max_opening_score,
                opening_nodes: args.soft_nodes,
                adjudication: Adjudication {
                    win_score: args.win_score,
                    win_plies: args.win_plies,
                    draw_score: args.draw_score,
                    draw_plies: args.draw_plies,
                    draw_after_ply: args.draw_after_ply,
                    max_plies: args.max_plies,
                },
                seed: args.seed.unwrap_or_else(Rng::time_seed),
                units: args.units,
                ..Default::default()
            };
            let stop = AtomicBool::new(false);
            let report = data::datagen::run(&config, &args.out, &stop, &|p| {
                if json {
                    // One JSON object per line on stderr, for the app's datagen executor; stdout keeps the report.
                    if let Ok(line) = serde_json::to_string(p) {
                        eprintln!("{line}");
                    }
                } else {
                    eprintln!(
                        "{} positions, {} games, {:.1} MB, {:.0} positions/s",
                        p.positions,
                        p.games,
                        p.bytes as f64 / 1e6,
                        p.positions_per_second
                    );
                }
            })?;
            print(json, &report, || {
                format!(
                    "{}: {} positions in {} games, {} bytes ({:.2} per position); this run {} positions at {:.0}/s",
                    report.finished,
                    report.positions,
                    report.games,
                    report.bytes,
                    report.bytes_per_position,
                    report.session_positions,
                    report.positions_per_second
                )
            })
        }
        Command::Manifest { dir, name, source, units } => {
            let m = manifest::build_and_write(
                &dir,
                &ManifestOptions { name, source, generator: None, score_units: units },
            )?;
            print(json, &m, || {
                format!(
                    "{}: {} files, {} positions, {} games, {} bytes",
                    m.name,
                    m.files.len(),
                    m.positions,
                    m.games,
                    m.size_bytes
                )
            })
        }
    }
}
