//! Reckless's net file and the model's float weights: loading (dequantising) and saving (quantising).
//!
//! The float weights are kept in bullet's layout: every weight is an `rows × cols` matrix stored column-major, an
//! affine layer's weights being `outputs × inputs`. So the feature transformer's weights are one column of 768 per
//! input, exactly the file's row-per-input order, and only the dense layers need reordering. Nothing here touches the
//! GPU, so it is tested without CUDA.

use anyhow::{bail, ensure};
use reference::Network;
use reference::arch::{
    FT_QUANT, L1_QUANT, L1_SIZE, L2_SIZE, L3_SIZE, NET_FILE_SIZE, OUTPUT_BUCKETS, PSQ_INPUTS, THREAT_INPUTS,
};

/// The model's weight names (bullet's `ModelWeights` ids), shapes as `(rows, cols)`.
pub mod names {
    /// Threat inputs → accumulator, `(768, 66864)`; no bias (the engine's threat accumulator starts at zero).
    pub const THREAT_W: &str = "l0/threat/w";
    /// Piece-square inputs → accumulator, `(768, 7680)`.
    pub const PSQ_W: &str = "l0/psq/w";
    /// The accumulator's biases, `(768, 1)`.
    pub const PSQ_B: &str = "l0/psq/b";
    /// First layer, all buckets at once: `(8 × 16, 768)`; bucket `b` is rows `16b..16b + 16`.
    pub const L1_W: &str = "l1/w";
    pub const L1_B: &str = "l1/b";
    /// `(8 × 32, 16)`.
    pub const L2_W: &str = "l2/w";
    pub const L2_B: &str = "l2/b";
    /// `(8, 32)`.
    pub const L3_W: &str = "l3/w";
    pub const L3_B: &str = "l3/b";

    pub const ALL: [&str; 9] = [THREAT_W, PSQ_W, PSQ_B, L1_W, L1_B, L2_W, L2_B, L3_W, L3_B];
}

/// The largest float a threat weight may take: it is saved as `i8` at scale 255.
pub const THREAT_MAX: f32 = i8::MAX as f32 / FT_QUANT as f32;
pub const THREAT_MIN: f32 = i8::MIN as f32 / FT_QUANT as f32;
/// The first layer's weights are saved as `i8` at scale 64.
pub const L1_MAX: f32 = i8::MAX as f32 / L1_QUANT as f32;
pub const L1_MIN: f32 = i8::MIN as f32 / L1_QUANT as f32;

/// Every weight of the network as floats, in bullet's layout (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct FloatNet {
    pub threat_w: Vec<f32>,
    pub psq_w: Vec<f32>,
    pub psq_b: Vec<f32>,
    pub l1_w: Vec<f32>,
    pub l1_b: Vec<f32>,
    pub l2_w: Vec<f32>,
    pub l2_b: Vec<f32>,
    pub l3_w: Vec<f32>,
    pub l3_b: Vec<f32>,
}

/// Row `r`, column `c` of a column-major matrix with `rows` rows.
const fn cm(rows: usize, r: usize, c: usize) -> usize {
    c * rows + r
}

/// Where the file keeps the first layer's weight from input `i` to neuron `j` of bucket `b`: `[b][i / 4][j][i % 4]`.
const fn file_l1(b: usize, i: usize, j: usize) -> usize {
    b * L2_SIZE * L1_SIZE + (i / 4) * L2_SIZE * 4 + j * 4 + i % 4
}

impl FloatNet {
    /// The shape `(rows, cols)` of each weight, by name.
    pub fn shape(name: &str) -> (usize, usize) {
        match name {
            names::THREAT_W => (L1_SIZE, THREAT_INPUTS),
            names::PSQ_W => (L1_SIZE, PSQ_INPUTS),
            names::PSQ_B => (L1_SIZE, 1),
            names::L1_W => (OUTPUT_BUCKETS * L2_SIZE, L1_SIZE),
            names::L1_B => (OUTPUT_BUCKETS * L2_SIZE, 1),
            names::L2_W => (OUTPUT_BUCKETS * L3_SIZE, L2_SIZE),
            names::L2_B => (OUTPUT_BUCKETS * L3_SIZE, 1),
            names::L3_W => (OUTPUT_BUCKETS, L3_SIZE),
            names::L3_B => (OUTPUT_BUCKETS, 1),
            _ => panic!("no weight named {name}"),
        }
    }

    /// The weights by name, in [`names::ALL`]'s order.
    pub fn named(&self) -> [(&'static str, &[f32]); 9] {
        [
            (names::THREAT_W, &self.threat_w),
            (names::PSQ_W, &self.psq_w),
            (names::PSQ_B, &self.psq_b),
            (names::L1_W, &self.l1_w),
            (names::L1_B, &self.l1_b),
            (names::L2_W, &self.l2_w),
            (names::L2_B, &self.l2_b),
            (names::L3_W, &self.l3_w),
            (names::L3_B, &self.l3_b),
        ]
    }

    /// Builds the net from a lookup by name (e.g. bullet's `ModelWeights`), checking every size.
    pub fn from_lookup(mut get: impl FnMut(&str) -> Vec<f32>) -> anyhow::Result<Self> {
        let mut take = |name: &str| {
            let values = get(name);
            let (rows, cols) = Self::shape(name);
            ensure!(values.len() == rows * cols, "{name} has {} values, not {rows} × {cols}", values.len());
            Ok(values)
        };
        Ok(Self {
            threat_w: take(names::THREAT_W)?,
            psq_w: take(names::PSQ_W)?,
            psq_b: take(names::PSQ_B)?,
            l1_w: take(names::L1_W)?,
            l1_b: take(names::L1_B)?,
            l2_w: take(names::L2_W)?,
            l2_b: take(names::L2_B)?,
            l3_w: take(names::L3_W)?,
            l3_b: take(names::L3_B)?,
        })
    }

    /// Dequantises a net file: feature transformer ÷ 255, first layer ÷ 64, the rest as is.
    pub fn from_network(net: &Network) -> Self {
        let ft = FT_QUANT as f32;
        let l1q = L1_QUANT as f32;

        let mut l1_w = vec![0.0; OUTPUT_BUCKETS * L2_SIZE * L1_SIZE];
        for b in 0..OUTPUT_BUCKETS {
            for i in 0..L1_SIZE {
                for j in 0..L2_SIZE {
                    l1_w[cm(OUTPUT_BUCKETS * L2_SIZE, b * L2_SIZE + j, i)] =
                        net.l1_weights[file_l1(b, i, j)] as f32 / l1q;
                }
            }
        }

        // The file's L2 is `[bucket][input][output]`; bullet's is column-major `(bucket·32 + output, input)`.
        let mut l2_w = vec![0.0; OUTPUT_BUCKETS * L2_SIZE * L3_SIZE];
        for b in 0..OUTPUT_BUCKETS {
            for i in 0..L2_SIZE {
                for j in 0..L3_SIZE {
                    l2_w[cm(OUTPUT_BUCKETS * L3_SIZE, b * L3_SIZE + j, i)] =
                        net.l2_weights[(b * L2_SIZE + i) * L3_SIZE + j];
                }
            }
        }

        // The file's L3 is `[bucket][input]`; bullet's is column-major `(bucket, input)`.
        let mut l3_w = vec![0.0; OUTPUT_BUCKETS * L3_SIZE];
        for b in 0..OUTPUT_BUCKETS {
            for i in 0..L3_SIZE {
                l3_w[cm(OUTPUT_BUCKETS, b, i)] = net.l3_weights[b * L3_SIZE + i];
            }
        }

        Self {
            threat_w: net.ft_threat_weights.iter().map(|&w| w as f32 / ft).collect(),
            psq_w: net.ft_piece_weights.iter().map(|&w| w as f32 / ft).collect(),
            psq_b: net.ft_biases.iter().map(|&w| w as f32 / ft).collect(),
            l1_w,
            l1_b: net.l1_biases.clone(),
            l2_w,
            l2_b: net.l2_biases.clone(),
            l3_w,
            l3_b: net.l3_biases.clone(),
        }
    }

    /// Quantises the net into the engine's file: the `#[repr(C)] Parameters` struct, little-endian, each field on a
    /// 64-byte boundary (zero padding), the first layer interleaved `[bucket][768 / 4][16][4]`. Integers are rounded
    /// to nearest; a value outside its type's range is an error (training clips the weights so it cannot happen).
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let ft = FT_QUANT as f32;
        let mut out = Vec::with_capacity(NET_FILE_SIZE);

        let threat = quantise_i8(&self.threat_w, ft, names::THREAT_W)?;
        push_field(&mut out, threat.iter().map(|&w| w as u8));
        let psq = quantise_i16(&self.psq_w, ft, names::PSQ_W)?;
        push_field(&mut out, psq.iter().flat_map(|w| w.to_le_bytes()));
        let bias = quantise_i16(&self.psq_b, ft, names::PSQ_B)?;
        push_field(&mut out, bias.iter().flat_map(|w| w.to_le_bytes()));

        let l1 = quantise_i8(&self.l1_w, L1_QUANT as f32, names::L1_W)?;
        let mut file = vec![0u8; l1.len()];
        for b in 0..OUTPUT_BUCKETS {
            for i in 0..L1_SIZE {
                for j in 0..L2_SIZE {
                    file[file_l1(b, i, j)] = l1[cm(OUTPUT_BUCKETS * L2_SIZE, b * L2_SIZE + j, i)] as u8;
                }
            }
        }
        push_field(&mut out, file);
        push_field(&mut out, self.l1_b.iter().flat_map(|w| w.to_le_bytes()));

        let mut l2 = vec![0f32; self.l2_w.len()];
        for b in 0..OUTPUT_BUCKETS {
            for i in 0..L2_SIZE {
                for j in 0..L3_SIZE {
                    l2[(b * L2_SIZE + i) * L3_SIZE + j] = self.l2_w[cm(OUTPUT_BUCKETS * L3_SIZE, b * L3_SIZE + j, i)];
                }
            }
        }
        push_field(&mut out, l2.iter().flat_map(|w| w.to_le_bytes()));
        push_field(&mut out, self.l2_b.iter().flat_map(|w| w.to_le_bytes()));

        let mut l3 = vec![0f32; self.l3_w.len()];
        for b in 0..OUTPUT_BUCKETS {
            for i in 0..L3_SIZE {
                l3[b * L3_SIZE + i] = self.l3_w[cm(OUTPUT_BUCKETS, b, i)];
            }
        }
        push_field(&mut out, l3.iter().flat_map(|w| w.to_le_bytes()));
        push_field(&mut out, self.l3_b.iter().flat_map(|w| w.to_le_bytes()));

        ensure!(out.len() == NET_FILE_SIZE, "wrote {} bytes, not {NET_FILE_SIZE}", out.len());
        Ok(out)
    }

    /// The smallest and largest value of each weight.
    pub fn ranges(&self) -> Vec<(&'static str, f32, f32)> {
        self.named()
            .into_iter()
            .map(|(name, values)| {
                let (lo, hi) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
                (name, lo, hi)
            })
            .collect()
    }
}

/// Appends a field and pads to the next 64-byte boundary, as `Aligned<T>` fields lie in the struct.
fn push_field(out: &mut Vec<u8>, bytes: impl IntoIterator<Item = u8>) {
    out.extend(bytes);
    out.resize(out.len().next_multiple_of(64), 0);
}

fn quantise(values: &[f32], scale: f32, min: i32, max: i32, name: &str) -> anyhow::Result<Vec<i32>> {
    let mut out = Vec::with_capacity(values.len());
    for (index, &v) in values.iter().enumerate() {
        let q = (v as f64 * scale as f64).round();
        if !(min as f64..=max as f64).contains(&q) {
            bail!("{name}[{index}] = {v} is {q} quantised, outside {min}..={max}");
        }
        out.push(q as i32);
    }
    Ok(out)
}

fn quantise_i8(values: &[f32], scale: f32, name: &str) -> anyhow::Result<Vec<i8>> {
    Ok(quantise(values, scale, i8::MIN as i32, i8::MAX as i32, name)?.into_iter().map(|q| q as i8).collect())
}

fn quantise_i16(values: &[f32], scale: f32, name: &str) -> anyhow::Result<Vec<i16>> {
    Ok(quantise(values, scale, i16::MIN as i32, i16::MAX as i32, name)?.into_iter().map(|q| q as i16).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic net file: every field filled with a pattern, so a layout mistake shows.
    fn synthetic_file() -> Vec<u8> {
        let mut net = FloatNet {
            threat_w: vec![0.0; L1_SIZE * THREAT_INPUTS],
            psq_w: vec![0.0; L1_SIZE * PSQ_INPUTS],
            psq_b: vec![0.0; L1_SIZE],
            l1_w: vec![0.0; OUTPUT_BUCKETS * L2_SIZE * L1_SIZE],
            l1_b: vec![0.0; OUTPUT_BUCKETS * L2_SIZE],
            l2_w: vec![0.0; OUTPUT_BUCKETS * L2_SIZE * L3_SIZE],
            l2_b: vec![0.0; OUTPUT_BUCKETS * L3_SIZE],
            l3_w: vec![0.0; OUTPUT_BUCKETS * L3_SIZE],
            l3_b: vec![0.0; OUTPUT_BUCKETS],
        };
        let pattern = |i: usize, m: i32| ((i as i64 * 7919 % (2 * m as i64 + 1)) - m as i64) as f32;
        for (i, w) in net.threat_w.iter_mut().enumerate() {
            *w = pattern(i, 127) / 255.0;
        }
        for (i, w) in net.psq_w.iter_mut().enumerate() {
            *w = pattern(i, 900) / 255.0;
        }
        for (i, w) in net.psq_b.iter_mut().enumerate() {
            *w = pattern(i, 300) / 255.0;
        }
        for (i, w) in net.l1_w.iter_mut().enumerate() {
            *w = pattern(i, 127) / 64.0;
        }
        for v in [&mut net.l1_b, &mut net.l2_w, &mut net.l2_b, &mut net.l3_w, &mut net.l3_b] {
            for (i, w) in v.iter_mut().enumerate() {
                *w = pattern(i, 1000) / 997.0;
            }
        }
        net.to_bytes().unwrap()
    }

    #[test]
    fn a_net_file_round_trips_byte_for_byte() {
        let bytes = synthetic_file();
        let network = Network::from_bytes(&bytes).unwrap();
        let float = FloatNet::from_network(&network);
        assert_eq!(float.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn the_first_layer_is_interleaved_as_the_engine_reads_it() {
        let network = Network::from_bytes(&synthetic_file()).unwrap();
        let float = FloatNet::from_network(&network);
        for (b, i, j) in [(0, 0, 0), (3, 5, 7), (7, 767, 15), (2, 130, 9)] {
            let ours = float.l1_w[cm(OUTPUT_BUCKETS * L2_SIZE, b * L2_SIZE + j, i)];
            assert_eq!(ours, network.l1_weight(b, i, j) as f32 / 64.0);
        }
        for (b, i, j) in [(0, 0, 0), (5, 15, 31), (7, 3, 2)] {
            let ours = float.l2_w[cm(OUTPUT_BUCKETS * L3_SIZE, b * L3_SIZE + j, i)];
            assert_eq!(ours, network.l2_weights[(b * L2_SIZE + i) * L3_SIZE + j]);
        }
    }

    #[test]
    fn out_of_range_weights_are_refused() {
        let network = Network::from_bytes(&synthetic_file()).unwrap();
        let mut float = FloatNet::from_network(&network);
        float.threat_w[10] = 0.6;
        assert!(float.to_bytes().is_err());
    }

    #[test]
    fn the_clip_ranges_quantise_into_range() {
        assert_eq!((THREAT_MAX * 255.0).round(), 127.0);
        assert_eq!((THREAT_MIN * 255.0).round(), -128.0);
        assert_eq!(L1_MAX * 64.0, 127.0);
        assert_eq!(L1_MIN * 64.0, -128.0);
    }
}
