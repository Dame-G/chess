//! Reckless's network as a bullet model.
//!
//! Per perspective (side to move first), the accumulator is the threat inputs' columns plus the king-bucketed
//! piece-square inputs' columns plus the biases; each half of 384 is clipped to [0, 1] and the halves are multiplied
//! (the pairwise layer). The 768 products go through 768 → 16 → 32 → 1, each layer through the output bucket the piece
//! count selects, with clipped ReLU between. The output times 380 is the engine's eval.
//!
//! **Quantisation-aware** (`qat`): the pairwise products are truncated to the engine's grid (`(a·b) >> 9` on values
//! scaled by 255, so steps of 512 / 255²) and the first layer's weights rounded to steps of 1/64, both with the
//! gradient passed straight through. With a quantised net loaded, the forward pass then computes what the engine
//! computes, up to float rounding.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bullet_compiler::tensor::TValue;
use bullet_lib::game::formats::bulletformat::ChessBoard;
use bullet_trainer::model::{
    DenseInput, InitSettings, ModelDefinition, ModelInputs, ModelInputsMapper, ModelWeights, SparseInput,
};
use bullet_trainer::run::Step;
use reference::arch::{FT_QUANT, FT_SHIFT, L1_QUANT, L1_SIZE, L2_SIZE, L3_SIZE, NETWORK_SCALE, OUTPUT_BUCKETS};
use reference::arch::{PSQ_INPUTS, THREAT_INPUTS};

use crate::inputs::{self, MAX_PSQ, MAX_THREATS};
use crate::net::{FloatNet, names};

/// The inputs, in order: threats (side to move, other side), piece-squares (same), the output bucket, the target.
pub type InputTy = (((((SparseInput, SparseInput), SparseInput), SparseInput), SparseInput), DenseInput<f32>);

/// Names of the forward pass's outputs.
pub mod outputs {
    /// The network's float output (before the eval scale).
    pub const OUTPUT: &str = "output";
    /// Diagnostics only: each perspective's accumulator before clipping (768).
    pub const ACC_STM: &str = "acc/stm";
    pub const ACC_NTM: &str = "acc/ntm";
    /// Diagnostics only: the pairwise layer's 768 outputs, side to move's first.
    pub const FT: &str = "ft";
    /// Diagnostics only: the hidden layers' activations through the position's bucket.
    pub const L1: &str = "l1";
    pub const L2: &str = "l2";
}

/// How the model is built.
#[derive(Clone, Copy, Debug)]
pub struct ModelConfig {
    /// Quantisation-aware forward pass (see the module docs).
    pub qat: bool,
    /// The scale of the sigmoid: the prediction is `sigmoid(output × 380 / eval_scale)` against a target of
    /// `sigmoid(score / eval_scale)` blended with the result. 380 (the engine's own scale) makes it `sigmoid(output)`.
    pub eval_scale: f32,
    /// The loss is `|prediction − target|^power` (2: squared error).
    pub loss_power: f32,
    /// Register the intermediate layers as outputs (for parity), at some cost in speed.
    pub diagnostics: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self { qat: true, eval_scale: NETWORK_SCALE as f32, loss_power: 2.0, diagnostics: false }
    }
}

/// The step between two values of the pairwise layer's output in the engine, as a float: `2^9 / 255²`.
pub const FT_STEP: f32 = (1 << FT_SHIFT) as f32 / (FT_QUANT * FT_QUANT) as f32;

pub fn model_inputs() -> ModelInputs<InputTy> {
    ModelInputs::default()
        .add_sparse("stm/threats", (THREAT_INPUTS, 1), MAX_THREATS)
        .add_sparse("ntm/threats", (THREAT_INPUTS, 1), MAX_THREATS)
        .add_sparse("stm/psq", (PSQ_INPUTS, 1), MAX_PSQ)
        .add_sparse("ntm/psq", (PSQ_INPUTS, 1), MAX_PSQ)
        .add_sparse("buckets", (OUTPUT_BUCKETS, 1), 1)
        .add_dense("targets", (1, 1))
}

pub fn definition(config: ModelConfig) -> ModelDefinition {
    let inputs = model_inputs();
    ModelDefinition::build(&inputs, |builder, (((((stm_threats, ntm_threats), stm_psq), ntm_psq), buckets), target)| {
        // Sparse inputs: about 60 threats and 32 piece-squares are active, so initialise for that, within the clips.
        let threat_w = builder.new_weights(
            names::THREAT_W,
            (L1_SIZE, THREAT_INPUTS),
            InitSettings::Normal { mean: 0.0, stdev: (1.0f32 / 64.0).sqrt() / 2.0 },
        );
        let psq_w = builder.new_weights(
            names::PSQ_W,
            (L1_SIZE, PSQ_INPUTS),
            InitSettings::Normal { mean: 0.0, stdev: (1.0f32 / 32.0).sqrt() },
        );
        let psq_b = builder.new_weights(names::PSQ_B, (L1_SIZE, 1), InitSettings::Zeroed);
        let l1 = builder.new_affine("l1/", L1_SIZE, OUTPUT_BUCKETS * L2_SIZE);
        let l2 = builder.new_affine("l2/", L2_SIZE, OUTPUT_BUCKETS * L3_SIZE);
        let l3 = builder.new_affine("l3/", L3_SIZE, OUTPUT_BUCKETS);

        // Rows `start..end` of a perspective's accumulator, clipped. Slicing the weights rather than the sum lets bullet
        // use strided sparse matmuls without materialising the full accumulator.
        let half = |threats, psq, start, end| {
            let acc = threat_w.slice_rows(start, end).matmul(threats)
                + psq_w.slice_rows(start, end).matmul(psq)
                + psq_b.slice_rows(start, end);
            acc.crelu()
        };
        let pairwise = |threats, psq| {
            let product = half(threats, psq, 0, L1_SIZE / 2) * half(threats, psq, L1_SIZE / 2, L1_SIZE);
            // The engine truncates exact integers; a float product lands a hair below one now and then, so it is nudged up by
            // a thousandth of a step (true products are multiples of 1/512 of a step) before truncating.
            if config.qat { (FT_STEP * 1e-3 + product).faux_quantise(1.0 / FT_STEP, false) } else { product }
        };
        let ft = pairwise(stm_threats, stm_psq).concat(pairwise(ntm_threats, ntm_psq));

        let l1_w = if config.qat { l1.weights.faux_quantise(L1_QUANT as f32, true) } else { l1.weights };
        let l1_out = (l1_w.matmul(ft) + l1.bias).select(buckets).crelu();
        let l2_out = l2.forward(l1_out).select(buckets).crelu();
        let output = l3.forward(l2_out).select(buckets);

        let prediction = (NETWORK_SCALE as f32 / config.eval_scale * output).sigmoid();
        let loss = if config.loss_power == 2.0 {
            prediction.squared_error(target)
        } else {
            prediction.power_error(target, config.loss_power)
        };

        let mut named = vec![(outputs::OUTPUT.to_string(), output)];
        if config.diagnostics {
            let acc = |threats, psq| threat_w.matmul(threats) + psq_w.matmul(psq) + psq_b;
            named.push((outputs::ACC_STM.to_string(), acc(stm_threats, stm_psq)));
            named.push((outputs::ACC_NTM.to_string(), acc(ntm_threats, ntm_psq)));
            named.push((outputs::FT.to_string(), ft));
            named.push((outputs::L1.to_string(), l1_out));
            named.push((outputs::L2.to_string(), l2_out));
        }

        (Some(loss.reduce_sum_batch()), named)
    })
}

/// The training target of one position: `wdl × result + (1 − wdl) × sigmoid(score / eval_scale)`, all from the side
/// to move.
pub fn target(position: &ChessBoard, wdl: f32, eval_scale: f32) -> f32 {
    let result = f32::from(position.result) / 2.0;
    let score = 1.0 / (1.0 + (-f32::from(position.score) / eval_scale).exp());
    wdl * result + (1.0 - wdl) * score
}

/// Turns positions into the model's inputs. `wdl` gives the result's weight in the target at each step; inputs that
/// did not fit their slots are counted in `dropped`.
pub fn mapper(
    eval_scale: f32,
    wdl: Arc<dyn Fn(Step) -> f32 + Send + Sync>,
    dropped: Arc<AtomicUsize>,
) -> ModelInputsMapper<ChessBoard> {
    ModelInputsMapper::build(
        &model_inputs(),
        move |position: &ChessBoard,
              step,
              (((((stm_threats, ntm_threats), stm_psq), ntm_psq), bucket), target_slot)| {
            let board = inputs::board(position);
            let lost = inputs::fill(&board, 0, stm_threats, stm_psq) + inputs::fill(&board, 1, ntm_threats, ntm_psq);
            if lost > 0 {
                dropped.fetch_add(lost, Ordering::Relaxed);
            }
            bucket[0] = inputs::output_bucket(position) as i32;
            target_slot[0] = target(position, wdl(step), eval_scale);
        },
    )
}

/// Model weights holding `net` (every other weight of the definition, if any, from `seed`'s initialisation).
pub fn weights_from(definition: &ModelDefinition, net: &FloatNet) -> anyhow::Result<ModelWeights> {
    let mut weights = ModelWeights::new(definition, 0);
    for (name, values) in net.named() {
        anyhow::ensure!(weights.set(name, TValue::F32(values.to_vec())), "the model has no weight {name} of that size");
    }
    Ok(weights)
}

/// The net in model weights.
pub fn net_from(weights: &ModelWeights) -> anyhow::Result<FloatNet> {
    FloatNet::from_lookup(|name| weights.get(name).values.f32().to_vec())
}
