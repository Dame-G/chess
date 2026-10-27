# Handoff: moving Dameless to Linux

The rebuilt app runs on Windows today, live from `C:\dev\dameless-live` (a release build of `main`) on http://127.0.0.1:7979. Its data is in `%LOCALAPPDATA%\dameless`. Linux becomes the daily driver once this move is done. Read `AGENTS.md` (how to drive and work on the app), `docs/design.md` (the contract) and `docs/roadmap.md` (what is built and what comes next) first: together they are the whole memory of this project.

## Standing decisions from the user

- **Local compute only.** Never rent machines or propose cloud boxes.
- **The goal:** two players, one board. You and Dameless (a Reckless fork) both get stronger by playing games and learning from them.
- **UI:** VLC + Lichess, dense and sharp, every control an eos component imported by path. Missing patterns go into eos.
- **Do rather than ask, except for these.** Confirm anything that acts on the user's real Lichess accounts, deletes things, or publishes. Never enter or move tokens or passwords: the user logs in to Lichess themselves.
- **Fair play is a hard rule.** Never analyse a position from a game the user is playing; the app refuses it, so don't work around it.

## Before moving

1. **Stop the Windows app.** Never run the bot from two machines at once.
2. **Data:** starting fresh on Linux is fine. Otherwise copy `%LOCALAPPDATA%\dameless\dameless.db` and `nets\` to `~/.local/share/dameless/`. Builds hold Windows paths, so let them rebuild. The user reconnects Lichess in Settings either way.

## Setting up on Linux

1. **Clone** `Dame-G/dameless` and `Dame-G/reckless` side by side, for example `~/dev/dameless` and `~/dev/reckless`. `RECKLESS_DIR` defaults to `../reckless`.
2. **Toolchain:** Rust (rustup), Node 24 with corepack (pnpm), clang (Syzygy builds), and the CUDA Toolkit with `CUDA_PATH` set (training).
3. **Build:** `corepack pnpm install`, `corepack pnpm build`, `cargo build --release -p dameless`.
4. **Run:** `dameless serve` (http://127.0.0.1:7979), then `dameless setup` (fastchess v1.8.2 and the UHO book). Keep the data directory outside the repo; engine builds fail inside it.
5. **Check:**
   - `cargo test --workspace` (CI already runs on Linux and passes).
   - `dameless diagnostics`.
   - The first engine build's bench must equal the commit's `Bench:` line (2837905 for `main` at 69aea90).
6. **Check the harness on this machine:** Settings › Check the harness, or `dameless harness check --wait`. Trust no result before it passes; the result is per machine.
7. **Reconnect Lichess.** The user logs in with their account and pastes the bot's token. Then do the first real use with the user: one game of theirs played from the app (moves, offers, abort, the fair-play lock lifting, an import) and one bot game.

## Open items, in order

1. **Training at scale:**
   - The pipeline is proven end to end: the trainer matches the engine exactly, and a smoke fine-tune ran.
   - Next is real data on the big partition: datagen with all cores (about 4–5 M positions an hour on the i7-10700K), or relabelled binpacks imported shard by shard.
   - Then a real fine-tune of v60 as a `net` idea.
   - First architecture experiments: pawn-pair inputs, then queen-conditioned king buckets.
   - Still to build: offline net metrics (validation loss), and training on Stockfish binpacks (they need their own eval scale).
2. **First real tune:** `spsa/tunables`, with the dozen pruning and reduction tunables, at 2,500 iterations; then let the [0, 4] confirmation decide.
3. **Ideas batch:** Claude proposes and runs ideas through MCP (`skill/dameless`).
4. **Levels:** calibrate them once fastchess and the book are installed (`dameless levels calibrate`).
5. **eos:** once the release with the app patterns (ESheet, EPopover, EList/EListRow, ETree, EVirtualList, EStatLink) is out, switch the app's hand-styled sheet frame, status popover, rail and move rows, and header stat links to them. The eos PR's report lists each swap.

## Working on the repo

See `AGENTS.md` › Working on Dameless itself for gates, regenerating API types, the shared-checkout rules and the mock Lichess.

- **Run the live app from its own worktree**, so half-finished code can't break it.
- **Give each agent its own `CARGO_TARGET_DIR`,** or share one `target-shared` (cargo locks it).
- **Keep an eye on disk:** one debug target is 3–7 GB. On Windows, many parallel agents' targets once filled the drive.
