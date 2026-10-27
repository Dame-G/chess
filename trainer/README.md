# trainer

Training Reckless's networks. This is its own Cargo workspace, excluded from the app's, so the app never needs CUDA to compile. Build it with its own target directory:

```sh
cd trainer
CARGO_TARGET_DIR=../target-trainer cargo test
```

Crates:

- `reference`: a CPU reference of Reckless's network in plain Rust, bit for bit with the engine. The bullet model is checked against it.
- `data`: datasets (see its own docs).
- `bullet`: the network as a [bullet](https://github.com/jw1912/bullet) model, Reckless's net file in and out, parity with the reference, and the training runner `bullet-train`. It needs the CUDA Toolkit, so it is not a default member: a plain `cargo test` here leaves it out (see [The bullet model](#the-bullet-model)).

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

## The bullet model

`bullet` (crate in `bullet/`, binary `bullet-train`) is the architecture above in bullet, pinned to commit **`579b78b37a694cacd6e9ede4706386c15c34837c`** (2026-10-20, "Allow tracking of gradient norms"; `bullet_lib`, `bullet-trainer`, `bullet-compiler` and `bullet-gpu` from git at that rev, `trainer/Cargo.toml`). bullet has no CPU backend any more, so the crate builds and runs only with CUDA.

### Building

bullet links `cuda`, `cudart`, `nvrtc` and `cublas` from `CUDA_PATH` (`lib/x64`) and compiles its kernels at run time with NVRTC; nothing else is needed (no cuDNN). Verified with the CUDA Toolkit 13.4 (driver 616.92, RTX 3080) on native Windows with MSVC. The DLLs are in `bin/x64` in 13.x, which must be on `PATH` to run.

```sh
cd trainer
export CUDA_PATH="C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.4"
export PATH="/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v13.4/bin/x64:$PATH"
CARGO_TARGET_DIR=../target-bullet cargo build --release -p bullet     # target-bullet/release/bullet-train(.exe)
CARGO_TARGET_DIR=../target-bullet cargo test -p bullet                # includes a GPU parity test (needs the net)
```

### The model (`src/model.rs`, `src/inputs.rs`)

- **Inputs:** threats and piece-squares per perspective, as four sparse inputs (66,864 × up to 256 slots, 7,680 × 32), plus the output bucket and the target. They are computed by `reference::features` itself, so the indices are the reference's (and the engine's). bullet's loaders give positions as bulletformat `ChessBoard`s, from the side to move (flipped and colour-swapped for Black); since Reckless's features are colour-symmetric, the side to move's inputs are White's view of that board and the other side's are Black's (tested against the real board in `inputs.rs`).
- **Feature transformer:** `threat_w` (768 × 66,864, no bias) and `psq_w` (768 × 7,680) plus `psq_b`; each half of 384 is clipped to [0, 1], halves multiplied, the side to move's 384 first.
- **Layers:** 768 → 8·16 → select bucket → clipped ReLU → 8·32 → select → clipped ReLU → 8 → select. The output × 380 is the eval.
- **Quantisation-aware** (on by default, `--no-qat` turns it off): the pairwise output is truncated to the engine's `(a·b) >> 9` grid (steps of 512/255²) and the L1 weights rounded to 1/64, with the gradient passed straight through. Without it the float model is up to 131 eval units off the engine on the shipped net (see parity), because the engine truncates every product.
- **Loss:** `|sigmoid(output · 380 / eval_scale) − target|^power` (power 2), target `wdl · result + (1 − wdl) · sigmoid(score / eval_scale)`, both from the side to move; `--eval-scale 380` makes the prediction `sigmoid(output)`. Scores should be in Reckless's internal units (`data datagen --units reckless-internal`), which is what the output × 380 is.
- **Clipping** (AdamW's weight bounds): threats to [−128/255, 127/255] (`i8` × 255), L1 weights to [−2, 127/64] (`i8` × 64), piece-squares and biases to ±1.98, L2 and L3 weights to ±0.99, float biases ±1.98. The shipped net sits exactly on several of these (threats ±0.498, piece-squares 1.98, L2/L3 ±0.99, L1 ±0.984), which says it was trained with the same bounds (L1 probably ±0.99).

### Net files (`src/net.rs`)

`FloatNet::from_network` dequantises a file (FT ÷ 255, L1 ÷ 64, floats as they are) into bullet's column-major layout; `FloatNet::to_bytes` rounds back and writes the engine's `#[repr(C)]` struct: every field on a 64-byte boundary with zero padding, L1 interleaved `[bucket][192][16][4]`, L2 `[bucket][16][32]`, L3 `[bucket][32]`. A value outside its integer type is an error, never a silent wrap.

### Parity (`bullet-train parity`)

```sh
bullet-train parity --net v60-7f587dfb.nnue --positions 4000 --eval-positions 10000 --both --resave resaved.nnue
```

It loads the net into the model on the GPU and compares every layer with `reference::Network::forward` on the reference's random positions (seed 1), then copies the weights to the GPU and back, quantises them, and compares the file and the reference's evals of both files (seed 2). Result on 2026-10-26 (v60-7f587dfb, RTX 3080, CUDA 13.4):

| Check                                     | QAT on (default)                 | QAT off                      |
| ----------------------------------------- | -------------------------------- | ---------------------------- |
| accumulators, 4,000 positions × 1,536     | 0 mismatches (max 0.00055 units) | same                         |
| pairwise outputs, × 768                   | 0 mismatches (max 3.8e-6 units)  | 278,692 differ (by < 1 unit) |
| L1 / L2 activations, max difference       | 3.0e-7 / 5.2e-7                  | 0.048 / 0.080                |
| float output, max difference              | 2.1e-6                           | 0.34                         |
| evals equal to the reference (and engine) | **4,000 of 4,000**               | 365 of 4,000 (max 131 off)   |

- **Re-saved file: byte-identical** to the original (63,266,880 bytes, 0 differing, SHA-256 `7f587dfb…`), so the reference's evals of both files agree on 10,000 of 10,000 positions.
- **Engine:** Reckless `main` 69aea90 built with the re-saved net (`EVALFILE`, AVX2, no syzygy): `bench` = **2837905**, its `Bench:`.
- The most threat inputs one perspective had on these positions was 60 (256 slots are reserved).
- `tests/parity.rs` repeats this on 300 positions with every `cargo test -p bullet` (it passes with a note when the net is absent).

### Training (`bullet-train train`)

```sh
bullet-train train --data a.vf b.vf --out runs/smoke --name smoke \
  [--parent v60-7f587dfb.nnue | --resume runs/smoke/checkpoints/smoke-20] [--format viriformat|binpack|bulletformat] \
  --superbatches 100 --batches-per-superbatch 6104 --batch-size 16384 \
  --lr 0.001 --final-lr 0.00001 --lr-schedule cosine|linear|step|constant [--lr-step 30 --lr-gamma 0.3] [--warmup-batches N] \
  --wdl 0.3 [--final-wdl 0.6] --eval-scale 380 [--loss-power 2] [--no-qat] [--weight-decay 0.01] \
  --save-rate 10 --keep-checkpoints 2 --log-rate 32 [--read-threads 4 --map-threads 4 --buffer-mb 1024] [--no-grad-norms]
```

- **Data:** viriformat through bullet's `ViriBinpackLoader` (Reckless's filter at load time: no capture or promotion, not in check, |score| < 4096), Stockfish binpacks through `SfBinpackLoader` (same filter), or bulletformat in file order. The loaders cycle through the files until the schedule ends and shuffle within a buffer of `--buffer-mb`.
- **From scratch or a fine-tune** (`--parent`: a Reckless net file, dequantised into the model). The LR and WDL schedules are functions of the absolute superbatch, so a resumed run continues them.
- **Checkpoints:** `<out>/checkpoints/<name>-<sb>/` with bullet's `optimiser_state/` (weights, Adam moments: about 0.7 GB), the quantised net `<name>-<sb>.nnue` and `state.json` (superbatch, net, sha256, config); every `--save-rate` superbatches and at the end, keeping the newest `--keep-checkpoints`. At the end the last net is copied to `<out>/<name>.nnue`. `<out>/run.json` holds the bullet rev and the config.
- **Resume:** `--resume <checkpoint dir>` with the run's other arguments unchanged (and no `--parent`): weights and optimiser state are loaded and training starts at the next superbatch. bullet's viriformat loader does not skip data, so a resumed run reads its files from the start again, in a new shuffle.
- **Output: JSON lines on stdout**, one object each (diagnostics on stderr):
  - `{"type":"started","device","start_superbatch","superbatches","batch_size","batches_per_superbatch","parent","resumed_from","bullet"}`
  - `{"type":"progress","superbatch","batch","loss","lr","wdl","positions_per_sec","grad_norm","grad_norms":{weight: norm},"positions","elapsed_seconds"}` every `--log-rate` batches and at each superbatch's last: `loss` is the mean batch loss since the previous line, `batch` counts from 1, `grad_norm` the norm of the whole gradient (from bullet's per-weight norms; null with `--no-grad-norms`), `positions` since this process started.
  - `{"type":"superbatch","superbatch","loss","positions_per_sec","elapsed_seconds"}`
  - `{"type":"checkpoint","superbatch","path","net","sha256"}`
  - `{"type":"done","net","sha256","superbatches","positions","elapsed_seconds","dropped_inputs"}` (`dropped_inputs`: threat inputs beyond the 256 slots, expected 0)
  - `{"type":"error","message"}` and exit code 1; a failed checkpoint is `{"type":"warning",…}` and fails the run at the end.

### Smoke run (2026-10-26)

The pipeline end to end on this machine (i7-10700K, RTX 3080 10 GB, Windows, CUDA 13.4), proving it works, not a stronger net:

1. **Data:** `data datagen --engine <Reckless origin/datagen 966be9a, v60 embedded> --threads 8 --soft-nodes 1000 --units reckless-internal --seed 1` (DFRC + 8 random plies), stopped after about 25 minutes (while other builds shared the CPU): **392,130 positions, 4,116 games, 1.7 MB** of viriformat.
2. **Fine-tune of v60:** `bullet-train train --data part-*.vf --parent v60-7f587dfb.nnue --superbatches 20 --batches-per-superbatch 200 --batch-size 16384 --lr 0.0001 --final-lr 0.000001 --wdl 0.3 --save-rate 10 --buffer-mb 256`: 65.5 M positions (the small set cycled ~170 times, so it overfits: loss 1.2e-3 in the first superbatch, 1.2e-5 at the end) in 134 s.
   - **Throughput: ~760,000 positions/s** in steady superbatches (QAT on, grad norms on, 4 map threads), 360–550 k/s while the CPU was busy elsewhere or right after a checkpoint; GPU utilisation 77–95 %.
   - **VRAM: ~1.45 GB** for the trainer (3.79 GB used in all against 2.34 GB before), batch 16,384. A checkpoint is 0.72 GB on disk (weights and Adam moments).
   - `dropped_inputs` 0. Net SHA-256 `37c9acdf…4d674d16`.
3. **In the engine:** Reckless `main` 69aea90 built with it (`EVALFILE`): `bench` = 3039071 (v60: 2837905), `go depth 10` gives legal moves on four positions (e2e4 from the start), and `reference parity` against that build matches 2,000 of 2,000 positions, all 16,000 bucket outputs: a trained net's file is exactly what the engine reads.

The data, checkpoints and builds were deleted afterwards.

### What the app's `Train` executor does

- Builds `bullet-train` with `CUDA_PATH` set (`cargo build --release -p bullet` in `trainer/`, its own target directory) and runs it with the CUDA `bin/x64` on `PATH`.
- Passes `train --data <the dataset's viriformat files from its manifest> --out <run dir> --name <net name>`, `--parent <net file>` for a fine-tune, the schedule (`--superbatches`, `--batches-per-superbatch`, `--batch-size`, `--lr`, `--final-lr`, `--lr-schedule`, `--wdl`, `--final-wdl`), `--eval-scale 380`, and `--save-rate`/`--keep-checkpoints` sized to the disk (0.72 GB a checkpoint). It should hold `gpu: 1` and about four cores (`--read-threads`, `--map-threads`).
- Reads stdout line by line: `progress` lines become `train.progress` events (loss, lr, grad norm, positions/s), `checkpoint` lines record resumable points, `done` gives the net's path and SHA-256 to register, `error` (exit 1) fails the run.
- Stops by killing the process; resumes with the same arguments plus `--resume <the last checkpoint's path>` (without `--parent`).

Not done here: offline metrics (validation loss on held-out data), a factoriser for from-scratch training, and data skipping on resume. The datasets' score units must be Reckless's internal eval (`--units reckless-internal`); `cp` data would need `--eval-scale` adjusted.
