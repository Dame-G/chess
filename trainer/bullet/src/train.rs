//! The training runner: a bullet training loop over the model, with Reckless's net format in and out, checkpoints,
//! resuming, and progress as JSON lines on stdout.
//!
//! Lines (one JSON object each, `type` first):
//!
//! - `{"type":"started","device":…,"start_superbatch":…,"superbatches":…,"batch_size":…,"batches_per_superbatch":…,"parent":…,"resumed_from":…}`
//! - `{"type":"progress","superbatch":…,"batch":…,"loss":…,"lr":…,"wdl":…,"positions_per_sec":…,"grad_norm":…,"grad_norms":{…},"positions":…,"elapsed_seconds":…}`
//!   every `log_rate` batches: `loss` is the mean batch loss since the last line, `grad_norm` the norm of the whole
//!   gradient (null without grad norms) and `grad_norms` per weight; `positions` is counted from this run's start.
//! - `{"type":"superbatch","superbatch":…,"loss":…,"positions_per_sec":…,"elapsed_seconds":…}` after each superbatch.
//! - `{"type":"checkpoint","superbatch":…,"path":…,"net":…,"sha256":…}` after each checkpoint.
//! - `{"type":"done","net":…,"sha256":…,"superbatches":…,"positions":…,"elapsed_seconds":…,"dropped_inputs":…}` at the end.
//! - `{"type":"error","message":…}` (and exit code 1) on failure.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, anyhow, bail, ensure};
use bullet_lib::game::formats::bulletformat::ChessBoard;
use bullet_lib::nn::ExecutionContext;
use bullet_lib::value::loader::sfbinpack::{MoveType, PieceType, SfBinpackLoader, TrainingDataEntry};
use bullet_lib::value::loader::viribinpack::Filter;
use bullet_lib::value::loader::{DirectSequentialDataLoader, ViriBinpackLoader, ViriFilter};
use bullet_trainer::model::ModelWeights;
use bullet_trainer::optimiser::Optimiser;
use bullet_trainer::optimiser::adam::{AdamW, AdamWParams};
use bullet_trainer::reader::{DataReader, ReadMapLoader};
use bullet_trainer::run::{DefaultDevice, Step, TrainingEvent, TrainingSchedule, TrainingSteps, train_with_observer};
use reference::Network;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::model::{self, ModelConfig};
use crate::net::{FloatNet, L1_MAX, L1_MIN, THREAT_MAX, THREAT_MIN, names};
use crate::parity::{sha256, write};

/// The bullet commit this crate is built against (`trainer/Cargo.toml`).
pub const BULLET_REV: &str = "579b78b37a694cacd6e9ede4706386c15c34837c";

/// Piece-square weights and the accumulator's biases are clipped to bullet's default (±1.98, ×255 in the file), as the
/// shipped net was: its weights reach 1.98 and its biases ±0.65 (`parity` prints every weight's range).
pub const PSQ_RANGE: f32 = 1.98;
/// The second and third layers' weights: the shipped net's sit exactly at ±0.99, the clip it was trained with.
pub const DENSE_RANGE: f32 = 0.99;
/// The float biases (first, second and third layers): bullet's default.
pub const BIAS_RANGE: f32 = 1.98;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum DataFormat {
    /// Viridithas's game chains (`.vf`), what `data datagen` writes.
    Viriformat,
    /// Stockfish binpacks.
    Binpack,
    /// bullet's 32-byte positions (`.bullet`), read in file order (shuffle them first).
    Bulletformat,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LrSchedule {
    Constant,
    /// `lr` × `lr_gamma` every `lr_step` superbatches.
    Step,
    Linear,
    Cosine,
}

/// Everything a run needs. Serialised into `<out>/run.json` and each checkpoint's `state.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrainConfig {
    pub data: Vec<PathBuf>,
    pub format: DataFormat,
    pub out: PathBuf,
    pub name: String,
    pub parent: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub superbatches: usize,
    pub batches_per_superbatch: usize,
    pub batch_size: usize,
    pub lr: f32,
    pub final_lr: f32,
    pub lr_schedule: LrSchedule,
    pub lr_step: usize,
    pub lr_gamma: f32,
    pub warmup_batches: usize,
    pub wdl: f32,
    pub final_wdl: f32,
    pub eval_scale: f32,
    pub loss_power: f32,
    pub qat: bool,
    pub weight_decay: f32,
    pub save_rate: usize,
    pub keep_checkpoints: usize,
    pub log_rate: usize,
    pub read_threads: usize,
    pub map_threads: u8,
    pub buffer_mb: usize,
    pub seed: u64,
    pub grad_norms: bool,
}

/// A checkpoint's `state.json`.
#[derive(Serialize, Deserialize)]
struct CheckpointState {
    superbatch: usize,
    net: PathBuf,
    sha256: String,
    config: TrainConfig,
}

impl TrainConfig {
    /// How far through the run a step is, in [0, 1).
    fn progress(&self, step: Step) -> f32 {
        let done = (step.superbatch() - 1) * self.batches_per_superbatch + step.batch();
        done as f32 / (self.superbatches * self.batches_per_superbatch) as f32
    }

    pub fn lr_at(&self, step: Step) -> f32 {
        let p = self.progress(step);
        let lr = match self.lr_schedule {
            LrSchedule::Constant => self.lr,
            LrSchedule::Step => self.lr * self.lr_gamma.powi(((step.superbatch() - 1) / self.lr_step.max(1)) as i32),
            LrSchedule::Linear => self.lr + (self.final_lr - self.lr) * p,
            LrSchedule::Cosine => {
                self.final_lr + (self.lr - self.final_lr) * 0.5 * (1.0 + (std::f32::consts::PI * p).cos())
            }
        };
        let batch = (step.superbatch() - 1) * self.batches_per_superbatch + step.batch();
        if batch < self.warmup_batches { lr * (batch + 1) as f32 / self.warmup_batches as f32 } else { lr }
    }

    pub fn wdl_at(&self, step: Step) -> f32 {
        self.wdl + (self.final_wdl - self.wdl) * self.progress(step)
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(!self.data.is_empty(), "no data files");
        for path in &self.data {
            ensure!(path.is_file(), "no data file {}", path.display());
        }
        ensure!(self.superbatches > 0 && self.batches_per_superbatch > 0 && self.batch_size > 0, "empty schedule");
        ensure!((0.0..=1.0).contains(&self.wdl) && (0.0..=1.0).contains(&self.final_wdl), "wdl must be in [0, 1]");
        ensure!(self.eval_scale > 0.0, "eval scale must be positive");
        ensure!(self.parent.is_none() || self.resume.is_none(), "--parent and --resume exclude each other");
        Ok(())
    }
}

fn gpu<E: std::fmt::Debug>(e: E) -> anyhow::Error {
    anyhow!("GPU: {e:?}")
}

fn emit(value: serde_json::Value) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// Reckless's position filter at load time: no capture or promotion, not in check, |score| < 4096.
fn viri_filter() -> Filter {
    Filter { max_eval: 4095, filter_tactical: true, filter_check: true, ..Filter::UNRESTRICTED }
}

fn binpack_filter(entry: &TrainingDataEntry) -> bool {
    !entry.pos.is_checked(entry.pos.side_to_move())
        && entry.score.unsigned_abs() < 4096
        && entry.mv.mtype() == MoveType::Normal
        && entry.pos.piece_at(entry.mv.to()).piece_type() == PieceType::None
}

type Opt = Optimiser<ExecutionContext, AdamW<ExecutionContext>>;

/// Runs the training described by `config`, printing JSON lines.
pub fn run(config: &TrainConfig) -> anyhow::Result<()> {
    config.validate()?;
    std::fs::create_dir_all(config.out.join("checkpoints"))?;

    let model_config = ModelConfig {
        qat: config.qat,
        eval_scale: config.eval_scale,
        loss_power: config.loss_power,
        diagnostics: false,
    };
    let definition = model::definition(model_config);

    let mut start_superbatch = 1;
    let weights = if let Some(parent) = &config.parent {
        let network = Network::load(parent)?;
        model::weights_from(&definition, &FloatNet::from_network(&network))?
    } else {
        ModelWeights::new(&definition, config.seed)
    };

    let device = DefaultDevice::new(0).map_err(gpu)?;
    let params = AdamWParams { decay: config.weight_decay, ..AdamWParams::default() };
    let mut optimiser = Opt::new(definition, weights, device.clone(), params).map_err(gpu)?;
    let clip = |min: f32, max: f32| AdamWParams { min_weight: min, max_weight: max, ..params };
    optimiser.set_params_for_weight(names::THREAT_W, clip(THREAT_MIN, THREAT_MAX));
    optimiser.set_params_for_weight(names::PSQ_W, clip(-PSQ_RANGE, PSQ_RANGE));
    optimiser.set_params_for_weight(names::PSQ_B, clip(-PSQ_RANGE, PSQ_RANGE));
    optimiser.set_params_for_weight(names::L1_W, clip(L1_MIN, L1_MAX));
    for name in [names::L2_W, names::L3_W] {
        optimiser.set_params_for_weight(name, clip(-DENSE_RANGE, DENSE_RANGE));
    }
    for name in [names::L1_B, names::L2_B, names::L3_B] {
        optimiser.set_params_for_weight(name, clip(-BIAS_RANGE, BIAS_RANGE));
    }

    if let Some(checkpoint) = &config.resume {
        let state: CheckpointState = serde_json::from_slice(
            &std::fs::read(checkpoint.join("state.json")).context("reading the checkpoint's state.json")?,
        )?;
        ensure!(state.superbatch < config.superbatches, "the checkpoint is already at the last superbatch");
        let optimiser_state = checkpoint.join("optimiser_state");
        ensure!(optimiser_state.join("weights.bin").is_file(), "no optimiser state in {}", checkpoint.display());
        optimiser.load_from_checkpoint(&path_str(&optimiser_state)?).map_err(gpu)?;
        start_superbatch = state.superbatch + 1;
    }

    write(
        &config.out.join("run.json"),
        &serde_json::to_vec_pretty(&json!({ "bullet": BULLET_REV, "config": config }))?,
    )?;

    let dropped = Arc::new(AtomicUsize::new(0));
    let wdl_config = config.clone();
    let mapper = model::mapper(config.eval_scale, Arc::new(move |step| wdl_config.wdl_at(step)), dropped.clone());
    let paths: Vec<String> = config.data.iter().map(|p| path_str(p)).collect::<anyhow::Result<_>>()?;
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();

    let mut session = Session::new(config, start_superbatch, dropped);
    match config.format {
        DataFormat::Viriformat => {
            let filter = ViriFilter::Builtin(viri_filter());
            let reader = ViriBinpackLoader::new_concat_multiple(&paths, config.buffer_mb, config.read_threads, filter);
            session.train(&mut optimiser, reader, mapper)?;
        }
        DataFormat::Binpack => {
            let filter: fn(&TrainingDataEntry) -> bool = binpack_filter;
            let reader = SfBinpackLoader::new_concat_multiple(&paths, config.buffer_mb, config.read_threads, filter);
            session.train(&mut optimiser, reader, mapper)?;
        }
        DataFormat::Bulletformat => {
            let reader = DirectSequentialDataLoader::new(&paths);
            session.train(&mut optimiser, reader, mapper)?;
        }
    }
    session.finish()
}

fn path_str(path: &Path) -> anyhow::Result<String> {
    path.to_str().map(str::to_owned).with_context(|| format!("{} is not UTF-8", path.display()))
}

/// The state of one run while bullet's loop drives it.
struct Session<'a> {
    config: &'a TrainConfig,
    start_superbatch: usize,
    dropped: Arc<AtomicUsize>,
    started: Instant,
    last_net: Option<(PathBuf, String)>,
    checkpoints: Vec<PathBuf>,
    error: Option<anyhow::Error>,
    positions: usize,
}

/// What the observer accumulates between progress lines.
#[derive(Default)]
struct Window {
    loss: f64,
    batches: usize,
    since: Option<std::time::Duration>,
}

impl<'a> Session<'a> {
    fn new(config: &'a TrainConfig, start_superbatch: usize, dropped: Arc<AtomicUsize>) -> Self {
        Self {
            config,
            start_superbatch,
            dropped,
            started: Instant::now(),
            last_net: None,
            checkpoints: existing_checkpoints(config),
            error: None,
            positions: 0,
        }
    }

    fn train<R: DataReader<ChessBoard>>(
        &mut self,
        optimiser: &mut Opt,
        reader: R,
        mapper: bullet_trainer::model::ModelInputsMapper<ChessBoard>,
    ) -> anyhow::Result<()> {
        let config = self.config;
        let lr_config = config.clone();
        let schedule = TrainingSchedule {
            steps: TrainingSteps {
                batch_size: config.batch_size,
                batches_per_superbatch: config.batches_per_superbatch,
                start_superbatch: self.start_superbatch,
                end_superbatch: config.superbatches,
            },
            lr_schedule: Box::new(move |step| lr_config.lr_at(step)),
            log_rate: 0,
            track_grad_norms: config.grad_norms,
        };
        let loader = ReadMapLoader::new(reader, mapper, config.map_threads);

        let window = RefCell::new(Window::default());
        let session = RefCell::new(&mut *self);
        let mut observer = |event: &TrainingEvent| match event {
            TrainingEvent::RunStarted { steps, device_name, .. } => emit(json!({
                "type": "started", "device": device_name, "start_superbatch": steps.start_superbatch,
                "superbatches": steps.end_superbatch, "batch_size": steps.batch_size,
                "batches_per_superbatch": steps.batches_per_superbatch, "parent": config.parent,
                "resumed_from": config.resume, "bullet": BULLET_REV,
            })),
            TrainingEvent::BatchCompleted { step, loss, learning_rate, elapsed, grad_norm, .. } => {
                let mut w = window.borrow_mut();
                w.loss += *loss as f64;
                w.batches += 1;
                let mut s = session.borrow_mut();
                s.positions += config.batch_size;
                let last_of_superbatch = step.batch() + 1 == step.batches_per_superbatch();
                if config.log_rate > 0 && ((step.batch() + 1) % config.log_rate == 0 || last_of_superbatch) {
                    let since = w.since.unwrap_or_default();
                    let seconds = (*elapsed - since).as_secs_f64();
                    let norms: Option<&BTreeMap<String, f32>> = grad_norm.as_ref();
                    let total = norms.map(|n| n.values().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt());
                    emit(json!({
                        "type": "progress", "superbatch": step.superbatch(), "batch": step.batch() + 1,
                        "loss": w.loss / w.batches as f64, "lr": learning_rate, "wdl": config.wdl_at(*step),
                        "positions_per_sec": if seconds > 0.0 { (w.batches * config.batch_size) as f64 / seconds } else { 0.0 },
                        "grad_norm": total, "grad_norms": norms, "positions": s.positions,
                        "elapsed_seconds": elapsed.as_secs_f64(),
                    }));
                    *w = Window { since: Some(*elapsed), ..Window::default() };
                }
            }
            TrainingEvent::SuperbatchCompleted { step, loss, elapsed, superbatch_elapsed, .. } => emit(json!({
                "type": "superbatch", "superbatch": step.superbatch(), "loss": loss,
                "positions_per_sec": (config.batch_size * config.batches_per_superbatch) as f64 / superbatch_elapsed.as_secs_f64(),
                "elapsed_seconds": elapsed.as_secs_f64(),
            })),
            _ => {}
        };

        let result = train_with_observer(
            optimiser,
            schedule,
            loader,
            |_, _, _| {},
            |optimiser, step| {
                let superbatch = step.superbatch();
                if superbatch.is_multiple_of(config.save_rate.max(1)) || superbatch == step.final_superbatch() {
                    let mut s = session.borrow_mut();
                    if let Err(error) = s.checkpoint(optimiser, superbatch) {
                        emit(json!({ "type": "warning", "message": format!("checkpoint failed: {error:#}") }));
                        s.error = Some(error);
                    }
                }
            },
            &mut observer,
        );
        result.map_err(|e| anyhow!("training failed: {e:?}"))
    }

    fn checkpoint(&mut self, optimiser: &Opt, superbatch: usize) -> anyhow::Result<()> {
        let config = self.config;
        let name = format!("{}-{superbatch}", config.name);
        let dir = config.out.join("checkpoints").join(&name);
        let state_dir = dir.join("optimiser_state");
        std::fs::create_dir_all(&state_dir)?;
        optimiser.write_to_checkpoint(&path_str(&state_dir)?).map_err(gpu)?;

        let net = model::net_from(&*optimiser.cpu_weights().map_err(gpu)?)?;
        let bytes = net.to_bytes()?;
        let net_path = dir.join(format!("{name}.nnue"));
        write(&net_path, &bytes)?;
        let hash = sha256(&bytes);
        let state = CheckpointState { superbatch, net: net_path.clone(), sha256: hash.clone(), config: config.clone() };
        write(&dir.join("state.json"), &serde_json::to_vec_pretty(&state)?)?;
        emit(json!({ "type": "checkpoint", "superbatch": superbatch, "path": dir, "net": net_path, "sha256": hash }));

        self.last_net = Some((net_path, hash));
        self.checkpoints.push(dir);
        while self.checkpoints.len() > config.keep_checkpoints.max(1) {
            let old = self.checkpoints.remove(0);
            std::fs::remove_dir_all(&old).with_context(|| format!("removing {}", old.display()))?;
        }
        Ok(())
    }

    fn finish(self) -> anyhow::Result<()> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let Some((net, hash)) = self.last_net else { bail!("no net was saved") };
        let final_path = self.config.out.join(format!("{}.nnue", self.config.name));
        std::fs::copy(&net, &final_path)?;
        emit(json!({
            "type": "done", "net": final_path, "sha256": hash, "superbatches": self.config.superbatches,
            "positions": self.positions, "elapsed_seconds": self.started.elapsed().as_secs_f64(),
            "dropped_inputs": self.dropped.load(Ordering::Relaxed),
        }));
        Ok(())
    }
}

/// The run's checkpoints already on disk (from an earlier session of a resumed run), oldest first, so the count kept
/// covers them too.
fn existing_checkpoints(config: &TrainConfig) -> Vec<PathBuf> {
    let prefix = format!("{}-", config.name);
    let mut found: Vec<(usize, PathBuf)> = std::fs::read_dir(config.out.join("checkpoints"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let superbatch = name.strip_prefix(&prefix)?.parse().ok()?;
            entry.path().join("state.json").is_file().then(|| (superbatch, entry.path()))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, path)| path).collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> TrainConfig {
        TrainConfig {
            data: vec![],
            format: DataFormat::Viriformat,
            out: PathBuf::new(),
            name: "t".into(),
            parent: None,
            resume: None,
            superbatches: 10,
            batches_per_superbatch: 100,
            batch_size: 1,
            lr: 0.001,
            final_lr: 0.0001,
            lr_schedule: LrSchedule::Cosine,
            lr_step: 3,
            lr_gamma: 0.5,
            warmup_batches: 0,
            wdl: 0.2,
            final_wdl: 0.6,
            eval_scale: 380.0,
            loss_power: 2.0,
            qat: true,
            weight_decay: 0.01,
            save_rate: 1,
            keep_checkpoints: 1,
            log_rate: 1,
            read_threads: 1,
            map_threads: 1,
            buffer_mb: 1,
            seed: 1,
            grad_norms: true,
        }
    }

    #[test]
    fn schedules_run_from_start_to_end() {
        let mut c = config();
        let first = Step::new(1, 0, 10, 100);
        let middle = Step::new(6, 0, 10, 100);
        assert!((c.lr_at(first) - 0.001).abs() < 1e-9);
        assert!((c.lr_at(middle) - 0.00055).abs() < 1e-7);
        assert!((c.wdl_at(middle) - 0.4).abs() < 1e-6);
        c.lr_schedule = LrSchedule::Step;
        assert!((c.lr_at(Step::new(4, 0, 10, 100)) - 0.0005).abs() < 1e-9);
        c.warmup_batches = 10;
        assert!((c.lr_at(Step::new(1, 4, 10, 100)) - 0.0005).abs() < 1e-9);
    }
}
