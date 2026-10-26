//! `bullet-train`: train Reckless's network with bullet, or check the model against the CPU reference.
//!
//! Every command prints JSON on stdout: `train` one JSON object per line as it goes (see `train.rs`), `parity` one
//! document at the end. Failures print `{"type":"error","message":…}` and exit with 1.

use std::path::PathBuf;

use anyhow::ensure;
use bullet::net::FloatNet;
use bullet::parity;
use bullet::train::{self, DataFormat, LrSchedule, TrainConfig};
use clap::{Parser, Subcommand};
use reference::{Network, positions};
use serde_json::json;

#[derive(Parser)]
#[command(about = "Train Reckless's network with bullet (CUDA)")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Load a net into the bullet model and compare it with the CPU reference: every layer on `--positions`
    /// positions, then save it back from the GPU and compare the file (and the reference's evals of both files).
    Parity {
        #[arg(long, env = "RECKLESS_NET")]
        net: PathBuf,
        /// Positions compared layer by layer (one GPU pass each).
        #[arg(long, default_value_t = 4_000)]
        positions: usize,
        /// Positions compared through the reference between the original and the re-saved file.
        #[arg(long, default_value_t = 10_000)]
        eval_positions: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Also compare without the quantisation-aware forward pass.
        #[arg(long)]
        both: bool,
        /// Where to write the re-saved net.
        #[arg(long)]
        resave: Option<PathBuf>,
    },
    /// Train a net (from scratch, or fine-tune a parent), streaming progress as JSON lines.
    Train(Box<TrainArgs>),
}

#[derive(clap::Args)]
struct TrainArgs {
    /// Training data files (all of one format).
    #[arg(long, required = true, num_args = 1..)]
    data: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = DataFormat::Viriformat)]
    format: DataFormat,
    /// The run's directory: `run.json`, `checkpoints/` and the final `<name>.nnue`.
    #[arg(long)]
    out: PathBuf,
    /// The net's name (checkpoints are `<name>-<superbatch>`).
    #[arg(long, default_value = "net")]
    name: String,
    /// Fine-tune this net (a Reckless net file) instead of starting from random weights.
    #[arg(long)]
    parent: Option<PathBuf>,
    /// Carry on from this checkpoint directory (`<out>/checkpoints/<name>-<sb>`); pass the run's other arguments
    /// again unchanged (without `--parent`).
    #[arg(long)]
    resume: Option<PathBuf>,
    #[arg(long, default_value_t = 100)]
    superbatches: usize,
    #[arg(long, default_value_t = 6104)]
    batches_per_superbatch: usize,
    #[arg(long, default_value_t = 16_384)]
    batch_size: usize,
    #[arg(long, default_value_t = 0.001)]
    lr: f32,
    #[arg(long, default_value_t = 0.00001)]
    final_lr: f32,
    #[arg(long, value_enum, default_value_t = LrSchedule::Cosine)]
    lr_schedule: LrSchedule,
    /// Step schedule: multiply by `--lr-gamma` every this many superbatches.
    #[arg(long, default_value_t = 30)]
    lr_step: usize,
    #[arg(long, default_value_t = 0.3)]
    lr_gamma: f32,
    /// Batches of linear warm-up from 0.
    #[arg(long, default_value_t = 0)]
    warmup_batches: usize,
    /// The result's weight in the target at the start (and, without `--final-wdl`, throughout).
    #[arg(long, default_value_t = 0.3)]
    wdl: f32,
    /// The result's weight at the end; linear in between.
    #[arg(long)]
    final_wdl: Option<f32>,
    /// Sigmoid scale: targets are `sigmoid(score / eval_scale)`; 380 is the engine's own scale.
    #[arg(long, default_value_t = 380.0)]
    eval_scale: f32,
    #[arg(long, default_value_t = 2.0)]
    loss_power: f32,
    /// Turn off the quantisation-aware forward pass.
    #[arg(long)]
    no_qat: bool,
    #[arg(long, default_value_t = 0.01)]
    weight_decay: f32,
    /// Save a checkpoint every this many superbatches (and at the end).
    #[arg(long, default_value_t = 10)]
    save_rate: usize,
    /// Checkpoints kept (each holds the optimiser's state, about 0.9 GB); older ones are deleted.
    #[arg(long, default_value_t = 2)]
    keep_checkpoints: usize,
    /// A progress line every this many batches (and at the end of each superbatch).
    #[arg(long, default_value_t = 32)]
    log_rate: usize,
    /// Threads decoding data.
    #[arg(long, default_value_t = 4)]
    read_threads: usize,
    /// Threads turning positions into inputs.
    #[arg(long, default_value_t = 4)]
    map_threads: u8,
    /// The loader's shuffle buffer, in MB.
    #[arg(long, default_value_t = 1024)]
    buffer_mb: usize,
    /// Seed of the initial weights (from scratch).
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Skip gradient norms (a little faster).
    #[arg(long)]
    no_grad_norms: bool,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        println!("{}", json!({ "type": "error", "message": format!("{error:#}") }));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Cmd::Parity { net, positions: n, eval_positions, seed, both, resave } => {
            let network = Network::load(&net)?;
            let ranges: Vec<_> = FloatNet::from_network(&network)
                .ranges()
                .into_iter()
                .map(|(name, lo, hi)| json!({ "weight": name, "min": lo, "max": hi }))
                .collect();
            let fens = positions::random_fens(seed, n);
            let mut layers = vec![parity::compare_layers(&network, &fens, true)?];
            if both {
                layers.push(parity::compare_layers(&network, &fens, false)?);
            }

            let original = std::fs::read(&net)?;
            let bytes = parity::resave(&network)?;
            if let Some(path) = &resave {
                parity::write(path, &bytes)?;
            }
            let identical = bytes == original;
            let differing = original.iter().zip(&bytes).filter(|(a, b)| a != b).count();
            let eval_fens = positions::random_fens(seed + 1, eval_positions);
            let resaved = Network::from_bytes(&bytes)?;
            let evals = parity::compare_nets(&network, &resaved, &eval_fens)?;

            let report = json!({
                "net": net, "sha256": network.sha256, "ranges": ranges, "layers": layers,
                "resave": {
                    "path": resave, "sha256": parity::sha256(&bytes), "identical": identical,
                    "bytes": bytes.len(), "differing_bytes": differing, "evals": evals,
                },
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            ensure!(identical || evals.evals_equal == evals.positions, "the re-saved net differs");
            Ok(())
        }
        Cmd::Train(args) => {
            let a = *args;
            let config = TrainConfig {
                data: a.data,
                format: a.format,
                out: a.out,
                name: a.name,
                parent: a.parent,
                resume: a.resume,
                superbatches: a.superbatches,
                batches_per_superbatch: a.batches_per_superbatch,
                batch_size: a.batch_size,
                lr: a.lr,
                final_lr: a.final_lr,
                lr_schedule: a.lr_schedule,
                lr_step: a.lr_step,
                lr_gamma: a.lr_gamma,
                warmup_batches: a.warmup_batches,
                wdl: a.wdl,
                final_wdl: a.final_wdl.unwrap_or(a.wdl),
                eval_scale: a.eval_scale,
                loss_power: a.loss_power,
                qat: !a.no_qat,
                weight_decay: a.weight_decay,
                save_rate: a.save_rate,
                keep_checkpoints: a.keep_checkpoints,
                log_rate: a.log_rate,
                read_threads: a.read_threads,
                map_threads: a.map_threads,
                buffer_mb: a.buffer_mb,
                seed: a.seed,
                grad_norms: !a.no_grad_norms,
            };
            train::run(&config)
        }
    }
}
