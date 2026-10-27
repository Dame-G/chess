//! The network: loading a net file and the forward pass, with the engine's arithmetic.
//!
//! The arithmetic is that of Reckless built for AVX2 (`-C target-cpu=native` on x86-64 without AVX-512), which is how
//! the harness builds it. Every step is written out in plain Rust, in the engine's order of operations, so the result
//! matches bit for bit. Where another build of the engine computes differently, the step says so.

use std::path::Path;

use anyhow::{Context, bail, ensure};
use sha2::{Digest, Sha256};
use shakmaty::{Board, Chess, Color, Position};

use crate::arch::{
    DEQUANT_MULTIPLIER, FT_QUANT, FT_SHIFT, L1_SIZE, L2_SIZE, L3_SIZE, NET_FILE_SIZE, NETWORK_SCALE, OUTPUT_BUCKETS,
    PSQ_INPUTS, THREAT_INPUTS,
};
use crate::features::{output_bucket, psq_features, threat_features};

/// A net's parameters, laid out as in the file (row-major, as the engine's `Parameters` struct).
pub struct Network {
    /// `[THREAT_INPUTS][L1_SIZE]`: one row of 768 per threat input.
    pub ft_threat_weights: Vec<i8>,
    /// `[PSQ_INPUTS][L1_SIZE]`: one row per piece-square input (10 king buckets × 768).
    pub ft_piece_weights: Vec<i16>,
    /// `[L1_SIZE]`: added to the piece-square accumulator (the threat accumulator starts at zero).
    pub ft_biases: Vec<i16>,
    /// `[OUTPUT_BUCKETS][L1_SIZE / 4][L2_SIZE][4]`: the weight from transformer output `i` to neuron `j` of bucket `b`
    /// is at `b × 12288 + (i / 4) × 64 + j × 4 + i % 4` (see [`Network::l1_weight`]).
    pub l1_weights: Vec<i8>,
    /// `[OUTPUT_BUCKETS][L2_SIZE]`.
    pub l1_biases: Vec<f32>,
    /// `[OUTPUT_BUCKETS][L2_SIZE][L3_SIZE]`: input-major.
    pub l2_weights: Vec<f32>,
    /// `[OUTPUT_BUCKETS][L3_SIZE]`.
    pub l2_biases: Vec<f32>,
    /// `[OUTPUT_BUCKETS][L3_SIZE]`.
    pub l3_weights: Vec<f32>,
    /// `[OUTPUT_BUCKETS]`.
    pub l3_biases: Vec<f32>,
    /// The file's SHA-256, lowercase hex.
    pub sha256: String,
}

/// Every intermediate of one forward pass, for checking another implementation layer by layer.
#[derive(Clone, Debug)]
pub struct Forward {
    /// The output bucket used.
    pub bucket: usize,
    /// The two accumulators, side to move first: piece-square plus threat, before clipping.
    pub accumulators: [Vec<i16>; 2],
    /// The feature transformer's output (pairwise products), side to move's half first.
    pub ft_out: Vec<u8>,
    /// The first layer's integer sums, before dequantisation.
    pub l1_sums: [i32; L2_SIZE],
    /// Whether the AVX2 sums wrapped in 16 bits anywhere (see [`Network::forward_with_bucket`]); when false, the sums
    /// equal the plain 32-bit dot products.
    pub l1_wrapped: bool,
    /// Activations after each hidden layer, clipped to [0, 1].
    pub l1_out: [f32; L2_SIZE],
    pub l2_out: [f32; L3_SIZE],
    /// The network's float output.
    pub output: f32,
    /// The evaluation, from the side to move, in the engine's units: `output × 380` truncated toward zero.
    pub eval: i32,
}

/// A little-endian reader over the file, aware of the struct's 64-byte field alignment.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> &'a [u8] {
        let slice = &self.bytes[self.at..self.at + len];
        // Each field of `Parameters` is an `Aligned<T>` (64-byte aligned), so the next one starts on a 64-byte line.
        self.at = (self.at + len).next_multiple_of(64);
        slice
    }

    fn i8s(&mut self, n: usize) -> Vec<i8> {
        self.take(n).iter().map(|&b| b as i8).collect()
    }

    fn i16s(&mut self, n: usize) -> Vec<i16> {
        self.take(2 * n).as_chunks::<2>().0.iter().map(|&c| i16::from_le_bytes(c)).collect()
    }

    fn f32s(&mut self, n: usize) -> Vec<f32> {
        self.take(4 * n).as_chunks::<4>().0.iter().map(|&c| f32::from_le_bytes(c)).collect()
    }
}

/// `_mm256_maddubs_epi16` on one pair: two u8 × i8 products, summed with signed saturation to 16 bits.
fn maddubs(a0: u8, w0: i8, a1: u8, w1: i8) -> i16 {
    let sum = a0 as i32 * w0 as i32 + a1 as i32 * w1 as i32;
    sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// `min_ps` then `max_ps`, operand order and all: `min(x, 1)` is `x < 1 ? x : 1`, `max(m, 0)` is `m > 0 ? m : 0`.
fn clip(x: f32) -> f32 {
    let m = if x < 1.0 { x } else { 1.0 };
    if m > 0.0 { m } else { 0.0 }
}

impl Network {
    /// Reads a net file as the engine embeds it (`include_bytes!` of `EVALFILE`, transmuted to `Parameters`).
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_bytes(&bytes).with_context(|| format!("loading {}", path.display()))
    }

    /// Parses a net from its bytes.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() != NET_FILE_SIZE {
            bail!("a net of this architecture is {NET_FILE_SIZE} bytes; this one is {}", bytes.len());
        }

        let mut r = Reader { bytes, at: 0 };
        let network = Self {
            ft_threat_weights: r.i8s(THREAT_INPUTS * L1_SIZE),
            ft_piece_weights: r.i16s(PSQ_INPUTS * L1_SIZE),
            ft_biases: r.i16s(L1_SIZE),
            l1_weights: r.i8s(OUTPUT_BUCKETS * L2_SIZE * L1_SIZE),
            l1_biases: r.f32s(OUTPUT_BUCKETS * L2_SIZE),
            l2_weights: r.f32s(OUTPUT_BUCKETS * L2_SIZE * L3_SIZE),
            l2_biases: r.f32s(OUTPUT_BUCKETS * L3_SIZE),
            l3_weights: r.f32s(OUTPUT_BUCKETS * L3_SIZE),
            l3_biases: r.f32s(OUTPUT_BUCKETS),
            sha256: Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect(),
        };
        ensure!(r.at == NET_FILE_SIZE, "the layout covers {} bytes, not the file's {NET_FILE_SIZE}", r.at);
        Ok(network)
    }

    /// The first layer's weight from transformer output `input` (0..768) to neuron `neuron` (0..16) of `bucket`.
    pub fn l1_weight(&self, bucket: usize, input: usize, neuron: usize) -> i8 {
        self.l1_weights[bucket * L2_SIZE * L1_SIZE + (input / 4) * L2_SIZE * 4 + neuron * 4 + input % 4]
    }

    /// One perspective's accumulator: the biases plus the rows of its active piece-square inputs, plus the rows of its
    /// active threat inputs. All sums wrap in 16 bits, as the engine's `add_epi16`; wrapping sums are exact in any
    /// order, so this equals the engine's accumulator however it was updated.
    pub fn accumulator(&self, board: &Board, pov: usize) -> Vec<i16> {
        let mut psq = self.ft_biases.clone();
        for feature in psq_features(board, pov) {
            let row = &self.ft_piece_weights[feature * L1_SIZE..][..L1_SIZE];
            for (value, &weight) in psq.iter_mut().zip(row) {
                *value = value.wrapping_add(weight);
            }
        }

        let mut threat = vec![0i16; L1_SIZE];
        for feature in threat_features(board, pov) {
            let row = &self.ft_threat_weights[feature * L1_SIZE..][..L1_SIZE];
            for (value, &weight) in threat.iter_mut().zip(row) {
                *value = value.wrapping_add(weight as i16);
            }
        }

        psq.iter().zip(&threat).map(|(&p, &t)| p.wrapping_add(t)).collect()
    }

    /// The evaluation of `position` from the side to move, in the engine's units: what Reckless's `eval` command prints
    /// as "NNUE evaluation" (there divided by 100 and turned to White's point of view).
    pub fn evaluate(&self, position: &Chess) -> i32 {
        self.forward(position).eval
    }

    /// The full forward pass with the output bucket the position selects.
    pub fn forward(&self, position: &Chess) -> Forward {
        self.forward_with_bucket(position, output_bucket(position.board()))
    }

    /// The full forward pass through a given output bucket (the engine's `eval_with_bucket`).
    ///
    /// The first layer adds its inputs as AVX2 Reckless does: the transformer's nonzero 4-byte chunks are taken two at
    /// a time; per chunk, `maddubs` sums byte pairs with 16-bit saturation; the two chunks' 16-bit results are added
    /// with 16-bit wrapping, then widened to 32 bits (an odd last chunk is widened alone). Transformer outputs are at
    /// most 127, so a single `maddubs` never saturates, but the pairwise add can wrap when large outputs meet large
    /// weights. A VNNI build (exact 32-bit sums) and the scalar build (4 products in 16 bits) differ from AVX2 only
    /// then; [`Forward::l1_wrapped`] says whether it happened.
    pub fn forward_with_bucket(&self, position: &Chess, bucket: usize) -> Forward {
        self.propagate(self.accumulators(position), bucket)
    }

    /// Both accumulators of `position`, side to move first.
    pub fn accumulators(&self, position: &Chess) -> [Vec<i16>; 2] {
        let board = position.board();
        let stm = match position.turn() {
            Color::White => 0,
            Color::Black => 1,
        };
        [self.accumulator(board, stm), self.accumulator(board, 1 - stm)]
    }

    /// Everything after the accumulators (side to move first), through output bucket `bucket`.
    pub fn propagate(&self, accumulators: [Vec<i16>; 2], bucket: usize) -> Forward {
        // Pairwise: each perspective's first half times its second half, both clipped to [0, 255], shifted right by 9.
        // AVX2 clips the second half only from above and lets `packus` turn the negative products into zero: the same.
        let mut ft_out = vec![0u8; L1_SIZE];
        for (half, acc) in accumulators.iter().enumerate() {
            for i in 0..L1_SIZE / 2 {
                let left = acc[i].clamp(0, FT_QUANT as i16) as i32;
                let right = acc[i + L1_SIZE / 2].clamp(0, FT_QUANT as i16) as i32;
                ft_out[i + half * L1_SIZE / 2] = ((left * right) >> FT_SHIFT) as u8;
            }
        }

        // First layer: sparse over the nonzero 4-byte chunks, in ascending order.
        let nonzero: Vec<usize> =
            (0..L1_SIZE / 4).filter(|&c| ft_out[c * 4..c * 4 + 4].iter().any(|&b| b != 0)).collect();
        let weights = &self.l1_weights[bucket * L2_SIZE * L1_SIZE..][..L2_SIZE * L1_SIZE];
        let chunk_pairs = |chunk: usize, neuron: usize| -> [i16; 2] {
            let a = &ft_out[chunk * 4..chunk * 4 + 4];
            let w = &weights[chunk * L2_SIZE * 4 + neuron * 4..][..4];
            [maddubs(a[0], w[0], a[1], w[1]), maddubs(a[2], w[2], a[3], w[3])]
        };

        let mut l1_sums = [0i32; L2_SIZE];
        let mut l1_wrapped = false;
        for pair in nonzero.chunks(2) {
            for (neuron, sum) in l1_sums.iter_mut().enumerate() {
                let first = chunk_pairs(pair[0], neuron);
                match pair.get(1) {
                    Some(&second) => {
                        let second = chunk_pairs(second, neuron);
                        for k in 0..2 {
                            let (wrapped, overflow) = first[k].overflowing_add(second[k]);
                            l1_wrapped |= overflow;
                            *sum += wrapped as i32;
                        }
                    }
                    None => *sum += first[0] as i32 + first[1] as i32,
                }
            }
        }

        // From here on the arithmetic is f32, with fused multiply-adds where the engine uses them.
        let mut l1_out = [0f32; L2_SIZE];
        for (j, out) in l1_out.iter_mut().enumerate() {
            let bias = self.l1_biases[bucket * L2_SIZE + j];
            *out = clip((l1_sums[j] as f32).mul_add(DEQUANT_MULTIPLIER, bias));
        }

        // Second layer: start from the biases and accumulate input by input.
        let mut l2_out = [0f32; L3_SIZE];
        l2_out.copy_from_slice(&self.l2_biases[bucket * L3_SIZE..][..L3_SIZE]);
        for (i, &input) in l1_out.iter().enumerate() {
            let row = &self.l2_weights[(bucket * L2_SIZE + i) * L3_SIZE..][..L3_SIZE];
            for (out, &weight) in l2_out.iter_mut().zip(row) {
                *out = weight.mul_add(input, *out);
            }
        }
        for out in &mut l2_out {
            *out = clip(*out);
        }

        // Output: sixteen running sums (two AVX2 registers), reduced by halving (8, 4, 2, 1), then the bias.
        let l3_weights = &self.l3_weights[bucket * L3_SIZE..][..L3_SIZE];
        let mut sums = [0f32; 16];
        for start in (0..L3_SIZE).step_by(16) {
            for (m, sum) in sums.iter_mut().enumerate() {
                *sum = l3_weights[start + m].mul_add(l2_out[start + m], *sum);
            }
        }
        let mut stride = 8;
        while stride > 0 {
            for i in 0..stride {
                sums[i] += sums[i + stride];
            }
            stride /= 2;
        }
        let output = sums[0] + self.l3_biases[bucket];
        let eval = (output * NETWORK_SCALE as f32) as i32;

        Forward { bucket, accumulators, ft_out, l1_sums, l1_wrapped, l1_out, l2_out, output, eval }
    }
}
