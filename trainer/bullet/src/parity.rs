//! Parity of the bullet model with the CPU reference: the same net loaded into both, compared layer by layer.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use anyhow::{Context, anyhow};
use bullet_gpu::runtime::Device;
use bullet_lib::game::formats::bulletformat::ChessBoard;
use bullet_lib::nn::ExecutionContext;
use bullet_trainer::model::{ModelEvaluator, ModelWeights};
use bullet_trainer::optimiser::Optimiser;
use bullet_trainer::optimiser::adam::{AdamW, AdamWParams};
use bullet_trainer::run::{DefaultDevice, HostPool, Step};
use reference::Network;
use reference::arch::{FT_QUANT, L1_SIZE, L2_SIZE, L3_SIZE, NETWORK_SCALE};
use reference::features::threat_features;
use reference::parity::parse_fen;
use serde::Serialize;
use shakmaty::{Color, Position};

use crate::model::{self, FT_STEP, ModelConfig, outputs};
use crate::net::FloatNet;

/// How far the model is from the reference on each layer, over a set of positions.
#[derive(Clone, Debug, Default, Serialize)]
pub struct LayerReport {
    pub positions: usize,
    pub qat: bool,
    /// Accumulators (both perspectives, before clipping), in the engine's units (1/255): the largest difference, and
    /// how many of the 1,536 values per position round to a different integer.
    pub acc_max_diff: f64,
    pub acc_mismatches: usize,
    /// The pairwise layer, in the engine's units (512/255²): the largest difference, and how many values differ from
    /// the engine's integer (with `qat`; without it the float product is never truncated, so it differs by < 1).
    pub ft_max_diff: f64,
    pub ft_mismatches: usize,
    pub l1_max_diff: f64,
    pub l2_max_diff: f64,
    pub output_max_diff: f64,
    /// `(output × 380) as i32` equal to the reference's eval, and the largest difference in eval units.
    pub evals_equal: usize,
    pub eval_max_diff: i64,
    /// The most threat inputs one perspective had.
    pub max_threats: usize,
}

/// The GPU model with a net loaded, evaluating one position at a time with every intermediate layer.
pub struct Evaluator {
    device: Arc<Device<ExecutionContext>>,
    pool: Arc<HostPool<ExecutionContext>>,
    evaluator: ModelEvaluator<ExecutionContext>,
    mapper: bullet_trainer::model::ModelInputsMapper<ChessBoard>,
}

/// Every layer of one forward pass, as floats.
pub struct Layers {
    pub acc: [Vec<f32>; 2],
    pub ft: Vec<f32>,
    pub l1: Vec<f32>,
    pub l2: Vec<f32>,
    pub output: f32,
}

fn gpu<E: std::fmt::Debug>(e: E) -> anyhow::Error {
    anyhow!("GPU: {e:?}")
}

impl Evaluator {
    pub fn new(net: &FloatNet, qat: bool) -> anyhow::Result<Self> {
        let config = ModelConfig { qat, diagnostics: true, ..ModelConfig::default() };
        let definition = model::definition(config);
        let weights = model::weights_from(&definition, net)?;
        let device = DefaultDevice::new(0).map_err(gpu)?;
        let mut evaluator = ModelEvaluator::new(&definition, device.clone()).map_err(gpu)?;
        evaluator.load_weights(&weights).map_err(gpu)?;
        let pool = HostPool::new(device.clone());
        let mapper = model::mapper(config.eval_scale, Arc::new(|_| 0.0), Arc::new(AtomicUsize::new(0)));
        Ok(Self { device, pool, evaluator, mapper })
    }

    pub fn layers(&mut self, position: &ChessBoard) -> anyhow::Result<Layers> {
        let batch = self.mapper.map(&self.pool, &[*position], Step::default(), 1).map_err(gpu)?;
        let inputs = batch.to_device(&self.device).map_err(gpu)?;
        let out = self.evaluator.evaluate(&inputs).map_err(gpu)?;
        let read = |name: &str| -> anyhow::Result<Vec<f32>> {
            let buffer = out.get(name).with_context(|| format!("no output {name}"))?;
            Ok(buffer.to_host().map_err(gpu)?.f32().to_vec())
        };
        Ok(Layers {
            acc: [read(outputs::ACC_STM)?, read(outputs::ACC_NTM)?],
            ft: read(outputs::FT)?,
            l1: read(outputs::L1)?,
            l2: read(outputs::L2)?,
            output: read(outputs::OUTPUT)?[0],
        })
    }
}

/// Loads `network` into the model and compares every layer with the reference on `fens`.
pub fn compare_layers(network: &Network, fens: &[String], qat: bool) -> anyhow::Result<LayerReport> {
    let mut evaluator = Evaluator::new(&FloatNet::from_network(network), qat)?;
    let mut report = LayerReport { qat, ..LayerReport::default() };
    let ft_quant = FT_QUANT as f64;

    for fen in fens {
        let position = parse_fen(fen)?;
        let expected = network.forward(&position);
        let cb: ChessBoard = format!("{fen} | 0 | 0.5").parse().map_err(|e: String| anyhow!(e))?;
        let ours = evaluator.layers(&cb)?;

        let stm = if position.turn() == Color::White { 0 } else { 1 };
        for pov in [stm, 1 - stm] {
            report.max_threats = report.max_threats.max(threat_features(position.board(), pov).len());
        }

        for (side, acc) in ours.acc.iter().enumerate() {
            for (k, &value) in acc.iter().enumerate() {
                let theirs = expected.accumulators[side][k] as f64;
                let units = value as f64 * ft_quant;
                report.acc_max_diff = report.acc_max_diff.max((units - theirs).abs());
                report.acc_mismatches += usize::from(units.round() != theirs);
            }
        }
        for (k, &value) in ours.ft.iter().enumerate() {
            let units = value as f64 / FT_STEP as f64;
            let theirs = expected.ft_out[k] as f64;
            report.ft_max_diff = report.ft_max_diff.max((units - theirs).abs());
            report.ft_mismatches += usize::from(units.round() != theirs);
        }
        for (a, b) in ours.l1.iter().zip(&expected.l1_out) {
            report.l1_max_diff = report.l1_max_diff.max((a - b).abs() as f64);
        }
        for (a, b) in ours.l2.iter().zip(&expected.l2_out) {
            report.l2_max_diff = report.l2_max_diff.max((a - b).abs() as f64);
        }
        report.output_max_diff = report.output_max_diff.max((ours.output - expected.output).abs() as f64);
        let eval = (ours.output * NETWORK_SCALE as f32) as i32;
        report.evals_equal += usize::from(eval == expected.eval);
        report.eval_max_diff = report.eval_max_diff.max((eval as i64 - expected.eval as i64).abs());
        report.positions += 1;
    }
    debug_assert_eq!(L1_SIZE, 768);
    debug_assert_eq!((L2_SIZE, L3_SIZE), (16, 32));
    Ok(report)
}

/// Loads `network` into the model on the GPU, reads the weights back and quantises them into a net file.
pub fn resave(network: &Network) -> anyhow::Result<Vec<u8>> {
    let definition = model::definition(ModelConfig::default());
    let weights = model::weights_from(&definition, &FloatNet::from_network(network))?;
    let device = DefaultDevice::new(0).map_err(gpu)?;
    let optimiser = Optimiser::<ExecutionContext, AdamW<_>>::new(definition, weights, device, AdamWParams::default())
        .map_err(gpu)?;
    let back: ModelWeights = optimiser.cpu_weights().map_err(gpu)?.clone();
    model::net_from(&back)?.to_bytes()
}

/// How two nets compare through the reference on the same positions.
#[derive(Clone, Debug, Serialize)]
pub struct EvalComparison {
    pub positions: usize,
    pub evals_equal: usize,
    pub max_diff: i64,
}

pub fn compare_nets(a: &Network, b: &Network, fens: &[String]) -> anyhow::Result<EvalComparison> {
    let mut comparison = EvalComparison { positions: 0, evals_equal: 0, max_diff: 0 };
    for fen in fens {
        let position = parse_fen(fen)?;
        let (x, y) = (a.evaluate(&position), b.evaluate(&position));
        comparison.positions += 1;
        comparison.evals_equal += usize::from(x == y);
        comparison.max_diff = comparison.max_diff.max((x as i64 - y as i64).abs());
    }
    Ok(comparison)
}

/// The SHA-256 of a file's bytes, lowercase hex.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes bytes to a file, creating its directory.
pub fn write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
