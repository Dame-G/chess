# trainer/data

Training data for Dameless's nets: the formats bullet and Reckless use, conversion, filters, stats, shuffling,
rescoring, self-play datagen and the `manifest.json` the app's data registry reads. A library (`data`) and a CLI
(`data`), in the `trainer/` workspace, so the app never compiles it (or CUDA).

```bash
cd trainer
cargo build --release -p data          # target/release/data(.exe)
cargo test -p data                     # everything but the real-engine datagen test
```

## Formats

Every format reads into and writes from one `Game`: a start position, the moves with the score of the position each
was played from (side to move's view, in memory), and the result.

| Format       | Ext         | Read           | Write | What it is                                                                                                                                                                                                                                                                                                                                           |
| ------------ | ----------- | -------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| viriformat   | `.vf`       | yes            | yes   | Viridithas's game chains, what bullet loads (`ViriBinpackLoader`). A 32-byte marlinformat header (board, side, ep, clocks, result), then `(u16 move, i16 eval)` per move, ended by 4 zero bytes. Evals are **White's**. Castling rooks are marked in the header, so Chess960/DFRC works. Checked against the `viriformat` crate's own reader.        |
| rbinpack     | `.rbinpack` | yes            | yes   | Reckless's records (`src/tools/binpack.rs` on `nnue-tools-rsn-3`): `u32` length, a 72-byte `#[repr(C)]` position (six piece and two colour bitboards, side, ep, castling flags, clocks, result), then Reckless's `Move` (u16) and the score (i16, **side to move's**) per move. Castling rooks are found as Reckless finds them, from the corner in. |
| binpack      | `.binpack`  | yes            | yes   | Stockfish's binpack, through the `sfbinpack` crate (Disservin's port). Each chain of continuation entries is a game; a chain that does not convert is skipped and counted. Writing is standard chess only.                                                                                                                                           |
| bulletformat | `.bullet`   | stats, shuffle | yes   | bullet's 32-byte `ChessBoard` per position, from the side to move. The format per-position filters write into.                                                                                                                                                                                                                                       |
| PGN          | `.pgn`      | yes            | no    | fastchess/cutechess PGN with `{+0.31/9 0.063s, ...}` comments (what our trials write), parsed as Reckless's converter does after its fastchess fix: the score is what comes before `/`, times 100; the first mate score ends the game; unscored moves before the first scored one (book) move the start forward; result `*` is skipped.              |

Results are stored as White's (0 Black won, 1 draw, 2 White won) in every format but bulletformat.

**Score units.** Scores are what the engine reports over UCI (`cp`), as Reckless's own PGN converter keeps them.
`--units reckless-internal` turns UCI centipawns back into Reckless's internal eval with the inverse of its
`normalize_to_cp` (material-dependent, ≈3.2× at full material), which is what its `rescore` tool writes. The manifest
records the units.

## Commands

Every command takes `--json` and then prints one JSON document to stdout.

```bash
data convert games.pgn -o games.vf                  # PGN → viriformat (also .rbinpack, .binpack, .bullet)
data convert shard.binpack -o shard.vf              # Stockfish binpack → viriformat
data convert a.vf b.vf -o trimmed.vf --skip-plies 8 --cut-tail-at 3000 --min-positions 10
data filter games.vf -o quiet.bullet                # alias of convert; bulletformat applies the position filter
data stats games.vf [--min-ply 16 --max-score 3000 --keep-check ...]
data shuffle a.vf b.vf -o mixed.vf [--memory 2G] [--seed 1]
data interleave a.vf b.vf c.vf -o mixed.vf
data rescore games.vf -o rescored.vf --engine reckless.exe --nodes 5000 --threads 8
data datagen --engine reckless-datagen.exe --out data/dfrc-1 --threads 14 --positions 50M --max-size 10G
data manifest data/dfrc-1 [--name dfrc-1 --source datagen]
```

**Filters.** Two kinds (`src/filter.rs`):

- _Position filters_ judge one position: before a ply (`--min-ply`), too few pieces (`--min-pieces`), score too large
  (`--max-score`), the move captures or promotes, the side to move is in check. Default: Reckless's
  (|score| < 4096, no capture or promotion, not in check; `--keep-tactical`, `--keep-check`, `--no-filter`). Chain
  formats keep every position of a game (dropping one breaks the chain), so this filter applies when writing
  bulletformat, in `stats` (how many pass), and at load time in the trainer (bullet filters viriformat as it loads).
- _Game transforms_ work for every output: `--skip-plies N` starts games N plies later, `--cut-tail-at S` cuts a game
  at its first |score| ≥ S (adjudicated or mating play), `--min-positions N` drops short games.

**Shuffle and interleave** move whole records (games, or bulletformat positions) without decoding them, so inputs
and output share one format (binpack must be converted first). A shuffle bigger than `--memory` scatters records into
temporary buckets beside the output (`<out>.shuffle-tmp/`, removed after), then shuffles each bucket in memory: a
uniform shuffle in two passes. Interleave streams, picking each next record from an input with probability
proportional to its bytes left.

**Rescore** searches every position from its FEN (no history) at `--nodes` or `--depth` with `--threads` engine
processes, writes the new scores, and reports MSE, MAE and the old and new scores' moments over the positions
Reckless's filter keeps, like Reckless's tool. The scorer is a trait (`rescore::Scorer`); a net's static eval (the
trainer's CPU reference) plugs in the same way.

## Datagen

`data datagen` (library: `datagen::run(&DatagenConfig, dir, &stop, &progress)`) plays an engine against itself the
way Reckless's `datagen` branch is meant for:

- **The engine** is a binary built from that branch (or any UCI engine). Each worker starts one process with
  `Threads 1`, `Hash 16`, `UCI_Chess960 true`, `Minimal true` and `UseSoftNodes true`: `go nodes N` then stops where
  the time manager would, at the end of an iteration, never in the middle of one, so N is a soft limit (default
  5,000). Without `UseSoftNodes`, N is a hard limit.
- **Openings:** DFRC by default (White and Black each get a random Chess960 back rank), or `chess960`, `startpos`, or
  `--book file.epd`; then `--random-plies` (8) random legal moves. The opening is searched once and thrown away when
  its |score| is over `--max-opening-score` (1000 cp). The random plies are not recorded.
- **Every move** is the engine's best move; its score (mate as ±32000) is recorded for the position it was played
  from. Games end by mate, stalemate, insufficient material, the fifty-move rule, threefold repetition, or
  adjudication: a win once the score is ≥ 1000 cp for one side 4 plies in a row, a draw once |score| ≤ 10 for 12
  plies in a row after ply 60, a draw at 400 plies. All adjustable (`--win-score`, `--draw-plies`, ...).
- **Parallel:** `--threads` workers, each with its own engine and output file (`part-000.vf`, ...), viriformat.
  Openings are seeded from `--seed`, the worker and its game count.
- **Stops** at `--positions` (overshooting by at most one game per worker), at `--max-size` (the parts never pass
  it; default 10 GB), or when `stop` is set.
- **Resumable:** `datagen.json` records, per part, the bytes, positions and games written as whole, flushed and
  synced games; it is rewritten every 2 s and at the end. Running again on the same directory cuts each part back to
  its recorded length (dropping a game half written when the process was killed) and appends, so killing it is
  always safe. A resumed run may change the target, cap and thread count; each run is recorded in `sessions`.
- At the end `manifest.json` is rebuilt, with the run's config as `generator`.

**Measured** on the i7-10700K (8 cores, 16 threads, Windows), Reckless `origin/datagen` (`966be9a`, bench 802k nps),
5,000 soft nodes, defaults otherwise, with other builds loading the machine at 50–90% at the time:

- A soft-node search stops at the end of the iteration that passes 5,000 nodes: about 5,300 nodes, depth 6, 8–12 ms
  cold. Datagen made **about 60 positions per second per worker** (1 worker: 58/s; 4: 241/s; 6: 362/s), so expect
  roughly 1,000–1,500/s with 14–16 workers on an idle machine: 4–5 M positions an hour.
- DFRC games averaged 120 positions (results 8–3–4 of 15), standard-start games 113.
- **4.3 bytes per position** in viriformat (4.32 measured on 18,000 positions).

Engine failures (a timeout, a crash, an illegal move) cost the game, restart the engine and are counted; five in a
row stop the worker.

## Sizes

Measured on the same 18,000 positions of Reckless self-play (standard starts, games of about 113 positions):

| Format       | Layout                   | Bytes per position | Per million positions |
| ------------ | ------------------------ | ------------------ | --------------------- |
| viriformat   | 4 per move + 36 per game | 4.32               | 4.3 MB                |
| rbinpack     | 4 per move + 76 per game | 4.67               | 4.7 MB                |
| binpack      | compressed chains        | 2.01               | 2.0 MB                |
| bulletformat | 32 per position          | 32                 | 32 MB                 |

**On the Windows partition** (52 GB free at the start of R4), keep the datagen cap at the default **10 GB**: about
2.3 billion viriformat positions, far more than datagen makes here in any reasonable time (≈ 5 M an hour), so the
cap is a safety net. The smoke run's "about 1 GB" is ≈ 230 M positions, two days of datagen; a smoke run only proves
the pipeline, so 20–50 M positions (100–200 MB, a few hours) is plenty. Avoid bulletformat for whole sets (7× the
size); shuffle with `--memory` at most a quarter of free RAM, and remember that a bucketed shuffle needs the set's
size again in free disk while it runs. Big sets (the relabelled binpacks, tens to hundreds of GB) wait for Linux.

## For the app

- The data registry reads `<dataset>/manifest.json` (`manifest::Manifest`): name, format, positions, games, size,
  source, score units, generator, created/updated, and every file with its sha256. `data manifest <dir>` (or
  `manifest::build_and_write`) rebuilds it; counts are reused for unchanged files.
- A datagen executor runs `data datagen --json --engine <datagen build> --out <dataset dir> --threads <cores>
--positions <n> --max-size <setting>` (progress on stderr: with `--json`, one JSON object per line, `{"positions", "games", "bytes", "target_positions", "max_bytes", "session_positions", "positions_per_second", "elapsed_seconds"}`; the report as JSON on stdout), or calls
  `datagen::run` with a stop flag and a progress callback. Stopping is killing the process or setting the flag;
  resuming is running it again on the same directory.
- An import executor downloads binpack shards (with the user's go-ahead), then `data convert <shard>.binpack -o
<shard>.vf` and `data manifest`.
- The `Train` executor points bullet at the viriformat files listed in the manifest, with the position filter as the
  loader's filter.
