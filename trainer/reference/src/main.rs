//! `reference`: evaluate positions with the CPU reference, or check it against the engine.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use reference::{Network, arch::OUTPUT_BUCKETS, parity, positions};

#[derive(Parser)]
#[command(about = "A CPU reference of Reckless's network")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Evaluate one position: the eval from the side to move (as Reckless's `eval` × 100), and every bucket.
    Eval {
        /// The net file (Reckless's `networks/<name>.nnue`).
        #[arg(long, env = "RECKLESS_NET")]
        net: PathBuf,
        #[arg(long)]
        fen: String,
    },
    /// Compare the reference with the engine's `eval` command on random positions (or a file of FENs).
    Parity {
        /// A Reckless binary built from the net's commit.
        #[arg(long, env = "RECKLESS_BIN")]
        engine: PathBuf,
        #[arg(long, env = "RECKLESS_NET")]
        net: PathBuf,
        /// How many random positions.
        #[arg(long, default_value_t = 10_000)]
        positions: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// One FEN per line instead of random positions.
        #[arg(long)]
        fens: Option<PathBuf>,
        /// Also write the engine's evaluations to this file, as a fixture for `tests/fixed_positions.rs`.
        #[arg(long)]
        write_expected: Option<PathBuf>,
        /// How many mismatches to print.
        #[arg(long, default_value_t = 10)]
        show: usize,
    },
    /// Print random positions as FENs, one per line (the ones `parity` uses for the same seed).
    Positions {
        #[arg(long, default_value_t = 100)]
        positions: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Cmd::Eval { net, fen } => {
            let network = Network::load(&net)?;
            let position = parity::parse_fen(&fen)?;
            let (all, wrapped) = parity::reference_eval(&network, &position);
            let forward = network.forward(&position);
            println!("net        {}", network.sha256);
            println!("bucket     {}", forward.bucket);
            for (bucket, value) in all.buckets.iter().enumerate() {
                println!("bucket {bucket}   {value:+}");
            }
            println!("output     {}", forward.output);
            println!("l1 wrapped {wrapped}");
            println!("eval       {:+} (side to move)", all.eval);
        }
        Cmd::Parity { engine, net, positions: n, seed, fens, write_expected, show } => {
            let network = Network::load(&net)?;
            let fens = match fens {
                Some(path) => std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect(),
                None => positions::random_fens(seed, n),
            };

            let started = Instant::now();
            let engine_evals = parity::engine_evals(&engine, &fens)?;
            let engine_time = started.elapsed();
            if let Some(path) = &write_expected {
                let header = format!("Reckless's `eval` for net {}, from the side to move.", network.sha256);
                parity::write_expected(path, &header, &fens, &engine_evals)?;
            }
            let started = Instant::now();
            let report = parity::compare(&network, &fens, &engine_evals)?;
            let reference_time = started.elapsed();

            println!("net        {}", network.sha256);
            println!("engine     {} ({:.1}s)", engine.display(), engine_time.as_secs_f64());
            println!("reference  {:.1}s", reference_time.as_secs_f64());
            println!("positions  {}", report.positions);
            println!("matched    {} of {} (eval and all {OUTPUT_BUCKETS} buckets)", report.matched, report.positions);
            println!("buckets    {} of {} per-bucket outputs", report.bucket_matched, report.bucket_values);
            println!("by bucket  {:?}", report.per_bucket);
            println!("l1 wrapped {} positions", report.l1_wrapped);
            for m in report.mismatches.iter().take(show) {
                println!(
                    "mismatch   {}\n           engine {:?} / {:?}\n           reference {:?} / {:?} (l1 wrapped: {})",
                    m.fen, m.engine.eval, m.engine.buckets, m.reference.eval, m.reference.buckets, m.l1_wrapped
                );
            }
            if !report.passed() {
                bail!("{} of {} positions differ", report.positions - report.matched, report.positions);
            }
            println!("parity     passed");
        }
        Cmd::Positions { positions: n, seed } => {
            for fen in positions::random_fens(seed, n) {
                println!("{fen}");
            }
        }
    }
    Ok(())
}
