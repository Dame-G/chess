# trainer

Training Reckless's networks. This is its own Cargo workspace, excluded from the app's, so the app never needs CUDA to compile. Build it with its own target directory:

```sh
cd trainer
CARGO_TARGET_DIR=../target-trainer cargo test
```

Crates:

- `reference`: a CPU reference of Reckless's network in plain Rust, bit for bit with the engine. The bullet model is checked against it.
- `data`: datasets (see its own docs).

## The reference

`reference` loads a net file exactly as the engine embeds it and computes the engine's evaluation: the same inputs, the same integer arithmetic, and the same float operations in the same order. It is plain safe Rust with no SIMD, written for reading, not speed (about 60 µs a position).

```sh
reference eval --net networks/v60-7f587dfb.nnue --fen "<fen>"      # eval from the side to move, and every bucket
reference parity --engine <reckless binary> --net <net> --positions 10000 [--seed 1] [--fens file]
reference positions --positions 100 --seed 1                        # the random positions parity uses
```

The library:

- `Network::load(path)` and `Network::from_bytes(bytes)`: the parameters as public `Vec`s in file order, plus the file's `sha256`.
- `network.evaluate(&Chess) -> i32`: the evaluation from the side to move, in the engine's units (what `eval` prints, × 100).
- `network.forward(&Chess) -> Forward` and `forward_with_bucket(&Chess, bucket)`: every intermediate value (the accumulators, the transformer's output, the first layer's integer sums, each layer's activations, the float output, the eval), for checking another implementation layer by layer. `accumulators(&Chess)` and `propagate(accumulators, bucket)` split it in two.
- `features::psq_features(&Board, pov)`, `features::threat_features(&Board, pov)`, `psq_index`, `threat_index`, `king_bucket`, `output_bucket`: the inputs, in Reckless's numbering.
- `arch::*`: the architecture constants, and the shipped net's name, URL and SHA-256.
- `parity::*`: driving the engine's `eval` command and comparing. `positions::*`: the deterministic random positions.

### Parity

Reckless has a non-UCI `eval` command. It refreshes the network from scratch and prints the network's output through every output bucket, and through the position's own bucket as "NNUE evaluation", each as `raw / 100` from White's point of view with two decimals. `raw` is an integer of a few thousand at most, so the printed value gives it back exactly. No patch to the engine is needed. `reference parity` runs one engine process over all the positions (`position fen …` then `eval`) and compares the eval and all eight per-bucket outputs of each.

The positions are random playouts of 0 to 199 plies, half from the standard start and half from independently shuffled back ranks. Each playout prefers quiet moves, plays uniformly or prefers captures, a third of the time each, so every output bucket gets positions (seed 1: 2312, 846, 812, 586, 758, 1034, 1226 and 2426 positions in buckets 0 to 7).

Result on 2026-10-26, Reckless `main` at 69aea90 with its shipped net:

| Engine build                                        | Positions       | Evals matching | Per-bucket outputs matching |
| --------------------------------------------------- | --------------- | -------------- | --------------------------- |
| AVX2 (`-C target-cpu=native`, i7-10700K), no syzygy | 10,000 (seed 1) | 10,000         | 80,000 of 80,000            |
| AVX2                                                | 10,000 (seed 2) | 10,000         | 80,000 of 80,000            |
| scalar (no `target-cpu`), no syzygy                 | 10,000 (seed 1) | 10,000         | 80,000 of 80,000            |

To run it again, build Reckless in a shared clone (never in the user's checkout), embedding the net with `EVALFILE` so nothing is downloaded:

```sh
git clone --shared ../../reckless /tmp/reckless && cd /tmp/reckless
EVALFILE=<path>/v60-7f587dfb.nnue RUSTFLAGS="-C target-cpu=native" cargo build --release --bin reckless --no-default-features
cd -  # back in trainer/
CARGO_TARGET_DIR=../target-trainer cargo run --release -p reference -- parity --engine /tmp/reckless/target/release/reckless --net <path>/v60-7f587dfb.nnue
# or as a test:
RECKLESS_BIN=/tmp/reckless/target/release/reckless RECKLESS_NET=<net> CARGO_TARGET_DIR=../target-trainer cargo test --release -p reference -- --ignored parity
```

(`--no-default-features` drops Syzygy, which needs clang; it does not touch the network.) The clone must be outside this repository's directory, or cargo takes it for a member of the app's workspace.

`tests/fixed_positions.rs` runs with every `cargo test`: 16 fixed positions with the engine's eval and eight bucket outputs checked in (`tests/data/fixed-v60-7f587dfb.txt`, written by `reference parity --fens … --write-expected`). It needs the net (`RECKLESS_NET`, or Reckless's checkout beside this repository) and passes with a note when the net is absent.

## The architecture (Reckless `main`, 69aea90, net v60)

Everything below is from Reckless's `src/nnue.rs` and `src/nnue/`; the reference follows it line for line.

### Inputs

Every input is seen from a perspective (White's and Black's accumulators are separate). The perspective's squares are flipped vertically for Black and mirrored horizontally when its own king stands on files e–h, so it always sees its king on files a–d of its own side. Squares are 0 = a1 … 63 = h8; a piece is `2 × type + colour` (pawn, knight, bishop, rook, queen, king; white 0, black 1).

**King-bucketed piece-square inputs: 10 × 768 = 7,680, weights `i16`.** Index `bucket × 768 + 384 × (piece is theirs) + 64 × type + square`, square and king transformed as above. The bucket is `INPUT_BUCKETS_LAYOUT[king]`:

```
ranks 4-8:  9 9 9 9 9 9 9 9      (one bucket for the king beyond its third rank)
rank 3:     8 8 8 8 8 8 8 8
rank 2:     4 5 6 7 7 6 5 4
rank 1:     0 1 2 3 3 2 1 0
```

Mirrored in the files, so with the king on a–d only the left half is reached. Kings are inputs too.

**Threat inputs: 66,864, weights `i8`.** For every piece and every occupied square it attacks with the board's occupancy (sliders stop at the first piece; pawns by their capture squares), the pair (attacking piece, its square, attacked piece, its square) is an input unless excluded. Kings attack like any piece. Exclusions by type (attacker row, attacked column; `-1` excluded):

|        | P   | N   | B   | R   | Q   | K   |
| ------ | --- | --- | --- | --- | --- | --- |
| pawn   | 0   | 1   | -1  | 2   | -1  | -1  |
| knight | 0   | 1   | 2   | 3   | 4   | -1  |
| bishop | 0   | 1   | 2   | 3   | -1  | -1  |
| rook   | 0   | 1   | 2   | 3   | -1  | -1  |
| queen  | 0   | 1   | 2   | 3   | 4   | -1  |
| king   | 0   | 1   | 2   | 3   | -1  | -1  |

So nothing attacking a king is an input, a pawn attacking a bishop or queen is not, and neither is a bishop, rook or king attacking a queen. A pair of the same type (enemy pieces, or friendly non-pawns) is counted once: only when the attacker's (transformed) square is at least the attacked one's. The index is `base(attacker, attacked) + offset(attacker, from) + rank of to among the attacker's empty-board attacks from from`, where pieces are colour-swapped for Black's perspective; `base` lays out, per attacking piece (white pawn … black king), `targets × squares` blocks: per colour of the attacked piece, one block per allowed attacked type (the row's non-negative entries), each block as many slots as the piece has empty-board (square, target) pairs (pawns on ranks 2–7 only). The total is 66,864. `features.rs` builds the same tables as Reckless's `threats::initialize`.

### Feature transformer

Two accumulators of 768 `i16` per perspective, both wrapping in 16 bits: piece-square (starting from the 768 biases) and threats (starting from 0). The engine keeps them incrementally with a refresh cache; the sums wrap, so they equal a fresh sum in any order. Per perspective, `acc = psq + threat` (wrapping).

**Pairwise activation:** for each perspective, side to move first, for `i` in 0..384: `out[i] = (clamp(acc[i], 0, 255) × clamp(acc[i + 384], 0, 255)) >> 9`, a `u8` in 0..127. The 768 outputs are the side to move's 384 then the other side's 384.

### Layers (per output bucket)

- **Output bucket:** `OUTPUT_BUCKETS_LAYOUT[pieces on the board]`: 0 for 0–8 pieces, 1 for 9–12, 2 for 13–16, 3 for 17–19, 4 for 20–22, 5 for 23–25, 6 for 26–28, 7 for 29–32.
- **L1, 768 → 16, `i8` weights, `f32` biases.** Integer sums over the nonzero 4-byte chunks of the transformer's output, then `clip(sum as f32 · DEQUANT + bias)` with one fused multiply-add, `DEQUANT = 2^9 / (255 · 255 · 64)` as `f32`. Weights are stored `[bucket][768/4][16][4]`: input `i` to neuron `j` at `(i/4)·64 + j·4 + i%4`.
- **L2, 16 → 32, `f32`.** Start from the biases, then for each input `i`, `out[j] = fma(w[i][j], in[i], out[j])`; clip to [0, 1].
- **L3, 32 → 1, `f32`.** Sixteen running sums, `sums[m] = fma(w[k+m], in[k+m], sums[m])` for k = 0, 16; reduced by halving (8, 4, 2, 1), then plus the bias.
- **Scale:** `eval = (output × 380.0) as i32` (truncated toward zero), from the side to move. That is what `eval` prints (÷ 100, White's point of view).

Clipping is `max(min(x, 1), 0)` as the SIMD instructions do it. All activations are clipped ReLU in [0, 1]; the only nonlinearity before L1 is the pairwise product.

The search does not use `eval` directly: `correct_eval` (in `src/evaluation.rs`) scales it by material and optimism, by the fifty-move counter, and adds the correction history. The trainer's target is the network's output, not that.

### Quantisation

| Tensor                          | Type  | Scale           |
| ------------------------------- | ----- | --------------- |
| piece-square weights, FT biases | `i16` | 255 (1.0 = 255) |
| threat weights                  | `i8`  | 255             |
| L1 weights                      | `i8`  | 64              |
| L1 biases, L2, L3               | `f32` | none            |

The transformer output `(a·b) >> 9` on values scaled by 255 is `a·b·255²/512` units, so the L1 sum is scaled by `255² · 64 / 512`, which `DEQUANT` undoes.

### Where builds of the engine differ

Only in the first layer's integer sums, and only when 16-bit intermediates overflow:

- **AVX2** (the harness's build here): chunks are taken two at a time; per chunk, `maddubs` sums byte pairs with 16-bit saturation (never reached: outputs are at most 127); the two chunks' 16-bit results are added with 16-bit _wrapping_; then widened.
- **AVX-512 VNNI:** exact 32-bit sums.
- **scalar** (no SIMD): four products summed in an `i16`, wrapping in release.

The reference does what AVX2 does and reports whether a wrap happened (`Forward::l1_wrapped`). On 20,000 random positions it never did, and the scalar build matched too, so for this net all three agree with plain 32-bit arithmetic. The float layers are the same everywhere by design (Reckless's comments insist on the FMA and reduction order).

### The net file

The file is the engine's `#[repr(C)] Parameters` struct, transmuted from `include_bytes!`: little-endian, each field starting on a 64-byte boundary, 63,266,880 bytes:

| Field               | Shape          | Type  | Bytes            |
| ------------------- | -------------- | ----- | ---------------- |
| `ft_threat_weights` | 66,864 × 768   | `i8`  | 51,351,552       |
| `ft_piece_weights`  | 7,680 × 768    | `i16` | 11,796,480       |
| `ft_biases`         | 768            | `i16` | 1,536            |
| `l1_weights`        | 8 × (16 · 768) | `i8`  | 98,304           |
| `l1_biases`         | 8 × 16         | `f32` | 512              |
| `l2_weights`        | 8 × 16 × 32    | `f32` | 16,384           |
| `l2_biases`         | 8 × 32         | `f32` | 1,024            |
| `l3_weights`        | 8 × 32         | `f32` | 1,024            |
| `l3_biases`         | 8              | `f32` | 32 (+32 padding) |

There is no header or version: a net of another architecture is a different size, or garbage.

### The net main ships

`main` embeds **`v60-7f587dfb.nnue`** (SHA-256 `7f587dfb1fe5d74d53909328afa6fd51650c8c7f45907602db7fbb1e52948c61`; the name's hex is the start of the hash). Reckless's `build/build.rs` names it (`NETWORK_NAME`) and sets `MODEL` to `EVALFILE` if given, else `networks/<name>`; if neither exists it downloads it with `curl` from `https://github.com/codedeliveryservice/RecklessNetworks/releases/download/networks/v60-7f587dfb.nnue`. `networks/` is git-ignored. `src/nnue.rs` embeds `MODEL` with `include_bytes!`, so changing the net means a new build (and a new `Bench:`).

## What the bullet model needs

- The two input sets with exactly these indices (`features.rs`), both perspectives, side to move first, and the horizontal mirror chosen by the perspective's own king.
- The pairwise activation (clip to [0, 1] in float terms, then the product of the two halves), the 8 output buckets by piece count, and three dense layers with clipped ReLU.
- Quantisation on save: FT weights and biases × 255 (`i16` for piece-square and biases, `i8` for threats, so threat weights must stay within ±127/255 ≈ ±0.5), L1 weights × 64 as `i8` (within ±2), everything else `f32`, written in the layout above with 64-byte alignment. The eval scale is 380.
- The L1 weights' interleaved layout `[bucket][768/4][16][4]`.
- Parity: load the shipped net into the bullet model, quantise it back, and compare with `reference::Network::forward` layer by layer on the same positions.
