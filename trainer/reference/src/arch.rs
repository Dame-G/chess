//! The architecture's constants, as `src/nnue.rs` in Reckless declares them.

/// King buckets of the piece-square inputs (per perspective, after mirroring the king to the queenside).
pub const INPUT_BUCKETS: usize = 10;
/// Output buckets, chosen by the number of pieces on the board.
pub const OUTPUT_BUCKETS: usize = 8;

/// Piece-square inputs per king bucket: 2 colours × 6 piece types × 64 squares.
pub const PSQ_INPUTS_PER_BUCKET: usize = 768;
/// All piece-square inputs.
pub const PSQ_INPUTS: usize = INPUT_BUCKETS * PSQ_INPUTS_PER_BUCKET;
/// All threat inputs: (attacking piece, square, attacked piece, square) pairs, deduplicated.
pub const THREAT_INPUTS: usize = 66_864;

/// Accumulator width per perspective (the feature transformer's output).
pub const L1_SIZE: usize = 768;
/// Hidden layer widths.
pub const L2_SIZE: usize = 16;
pub const L3_SIZE: usize = 32;

/// The feature transformer's weights and biases are scaled by this (1.0 = 255).
pub const FT_QUANT: i32 = 255;
/// The first layer's weights are scaled by this.
pub const L1_QUANT: i32 = 64;
/// The pairwise product of two clipped accumulator values is shifted right by this.
pub const FT_SHIFT: u32 = 9;
/// Turns the first layer's integer sums back into floats (`2^9 / (255 × 255 × 64)`), as an `f32` the way the engine
/// computes it.
pub const DEQUANT_MULTIPLIER: f32 = (1 << FT_SHIFT) as f32 / (FT_QUANT * FT_QUANT * L1_QUANT) as f32;
/// The network's float output times this, truncated toward zero, is the evaluation in the engine's internal units.
pub const NETWORK_SCALE: i32 = 380;

/// King bucket by square (from the perspective's own side, king mirrored to files a–d).
#[rustfmt::skip]
pub const INPUT_BUCKETS_LAYOUT: [u8; 64] = [
    0, 1, 2, 3, 3, 2, 1, 0,
    4, 5, 6, 7, 7, 6, 5, 4,
    8, 8, 8, 8, 8, 8, 8, 8,
    9, 9, 9, 9, 9, 9, 9, 9,
    9, 9, 9, 9, 9, 9, 9, 9,
    9, 9, 9, 9, 9, 9, 9, 9,
    9, 9, 9, 9, 9, 9, 9, 9,
    9, 9, 9, 9, 9, 9, 9, 9,
];

/// Output bucket by the number of pieces on the board (kings included).
#[rustfmt::skip]
pub const OUTPUT_BUCKETS_LAYOUT: [usize; 33] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0,
    1, 1, 1, 1,
    2, 2, 2, 2,
    3, 3, 3,
    4, 4, 4,
    5, 5, 5,
    6, 6, 6,
    7, 7, 7, 7,
];

/// The size of a net file: the engine's `Parameters` struct, each field aligned to 64 bytes.
pub const NET_FILE_SIZE: usize = 63_266_880;

/// The net `main` ships (Reckless's `build/build.rs`, `NETWORK_NAME`); the name is the first 8 hex digits of its SHA-256.
pub const MAIN_NET_NAME: &str = "v60-7f587dfb.nnue";
/// Where Reckless's build script downloads it from when `networks/` lacks it and `EVALFILE` is not set.
pub const MAIN_NET_URL: &str =
    "https://github.com/codedeliveryservice/RecklessNetworks/releases/download/networks/v60-7f587dfb.nnue";
/// Its full SHA-256.
pub const MAIN_NET_SHA256: &str = "7f587dfb1fe5d74d53909328afa6fd51650c8c7f45907602db7fbb1e52948c61";
