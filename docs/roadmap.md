# Dameless: the roadmap

Everything here is built on Windows before the move to Linux. The move happens once the whole app is done. `docs/design.md` is the contract; this file says what comes after the core rebuild and in what order. Each phase ends usable.

## The old phases, judged against the new goal

The goal is now **two players, one board**: you and Dameless both get stronger by playing games and learning from them. The old plan only ever served one of those players.

| Old item                                                             | Verdict            | In the new app                                                                                                                                                                                                  |
| -------------------------------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 4: training (bullet, datagen, data import, net registry)       | **Keep, reframed** | A network is another way Dameless gets stronger, so a trained net is an **idea** (kind `net`). It goes through the same funnel as any change and lands in the logbook. Its loss curves sit on the idea's sheet. |
| First net experiments (pawn-pair inputs, SFNNv18-style king buckets) | **Keep**           | They are the first `net` ideas, once the trainer reproduces Reckless's own network exactly.                                                                                                                     |
| Phase 5: remote runner, burst boxes                                  | **Drop**           | Local compute only.                                                                                                                                                                                             |
| Phase 5: self-hosted OpenBench                                       | **Drop**           | One machine, and the scheduler already does what OpenBench's would.                                                                                                                                             |
| Phase 5: `/mcp`                                                      | **Keep, raised**   | The fifth principle is that Claude and you use the same verbs. MCP is how Claude uses them, from the same routes.                                                                                               |
| Policy nets, MCTS, searchless engines                                | **Still out**      | There is still no evidence they help an alpha-beta engine at this level.                                                                                                                                        |
| Puzzles (left out before)                                            | **Replaced**       | **Drills**: puzzles made from your own mistakes. They are personal, so they serve the goal; generic puzzles do not.                                                                                             |
| Opening explorer (left out before)                                   | **Done**           | Explore mode, outside your live games.                                                                                                                                                                          |
| Open item: false-accept check                                        | **Built in**       | **Check the harness** in Settings, a ready-made custom idea. The machine is marked unchecked until it passes, and trials say so beside their verdict.                                                           |
| Open item: `Bench:` lines                                            | **Built in**       | When an idea's branch has no `Bench:` line, or a stale one, the app offers to fix it when the idea is created: it rewords the tip with the measured bench.                                                      |
| Open item: first tune, ideas batch                                   | **Operational**    | These run on the finished app. The New idea dialog gets a template, "Tune pruning and reductions", with 12 parameters and 2,500 iterations.                                                                     |
| Open item: Lichess account, first real use                           | **Operational**    | A checklist with the user at cutover (below).                                                                                                                                                                   |
| Open item: eos gaps                                                  | **Keep**           | Phase R6 adds them to the eos library and replaces the hand-styled versions.                                                                                                                                    |
| Not built: merging into main from the UI                             | **Done**           | **Promote**.                                                                                                                                                                                                    |
| Not built: a rematch that re-rolls the book opening                  | **Keep**           | **Rematch** on a finished game: same opponent and clock, colours swapped, a new book line for a book start.                                                                                                     |
| Not built: building any tag or commit from the UI                    | **Done**           | `POST /api/versions { ref }`, and the branch picker accepts any ref.                                                                                                                                            |
| Not built: per-build compile logs                                    | **Keep**           | Settings › Machine › Builds, as diagnostics.                                                                                                                                                                    |

The **player side** never had phases at all: the old plan only served the engine. Phase R3 adds them.

## Phases

### R1: the core rebuild (in progress)

The crates in `design.md`, the API and CLI, and the web app on the real backend. It is done when everything the old app did works in the new one, end to end, against the mock Lichess and the real engine.

### R2: trust the harness

- **Check the harness:** `main` against a branch identical to it (`test/comment`) at VSTC [0, 3], plus an optional 2,000-pair symmetry run. The result is stored per machine; Settings and every trial verdict show it.
- **Bench line fixer**, when an idea is created.
- **Health** in Settings: fastchess and the book are installed, the engine checkout is found, clang is there (Syzygy builds), and the disk has free space for builds, games and data.

### R3: the player side

You get stronger at the same board.

- **Levels.** Dameless at a strength you choose, from about 800 to full. The engine's own play is kept, not handicapped by random blunders: strength comes from a node budget, plus a small MultiPV softmax with a temperature at the lowest levels. Each level's rating is **calibrated by the app**: it plays levels against each other and against full strength at fixed nodes in the background, at low priority, and fits an Elo per level, anchored to your own results against them. The New game dialog then offers "Dameless at 1600", and Play this out starts at your level by default.
- **Drills.** Weak spots become exercises: find the move you missed. They repeat on a spaced schedule (SM-2: a miss comes back tomorrow, a pass in growing intervals). Each drill opens on the board in a new **Drill** mode: no engine, one try, then the answer and the line. The You sheet shows today's drills and how your hit rate moves over time. Drills are local practice, so fair play does not apply, but they are hidden while a Lichess game of yours is live, like everything else.
- **Your openings.** From your games: the lines you play as White and as Black, where you leave theory, and how you score in each. It is built from your games, with the explorer, and is shown on the You sheet and in Explore as "your repertoire". It is off while you play on Lichess.
- **Progress.** One line on the You sheet: rating, accuracy and drill hit rate over time. You can see the two players' lines side by side: you and Dameless.

### R4: training, the foundation

This is everything except training on big datasets, sized for 52 GB free on the Windows partition. The work happens once data and the GPU are ready.

- **Nets are build inputs.** A `nets` table holds each net: hash, file, architecture, parent net, the data it was trained on, its training run, offline metrics. A build's cache key becomes (commit, features, net). Reckless embeds its net at compile time (`EVALFILE`); `engine::build` already takes a networks directory. Versions show their net.
- **The GPU is a resource.** The ledger gains `gpu: 1`, beside cores. Training takes the GPU, never cores beyond one feeder thread, so a trial can run while a net trains.
- **Data.** A data registry of datasets: name, format (viriformat, Stockfish binpack), positions, size, source, path.
  - **Datagen** is work. Self-play with Reckless's `datagen` branch (`UseSoftNodes`) from DFRC or random openings, written as viriformat. Its size is capped by a setting (default 10 GB on Windows).
  - **Imports** of relabelled binpacks from HuggingFace (vondele/\*) shard by shard. Each download states its size and needs the user's go-ahead in the UI. On Linux this is where the big sets go.
  - Rescoring and filtering use the tools ported from Reckless's `nnue-tools-rsn-3` branch.
- **The trainer** (`trainer/`, its own Cargo workspace, so the app never needs CUDA to compile). It is built on demand, like an engine, by a `Train` executor.
  1. **A CPU reference of Reckless's network in plain Rust:** threat inputs, king buckets, the pairwise layer and 768 → 16 → 32 → 1 with output buckets, with Reckless's quantisation. **Parity test:** it loads the shipped net and matches the engine's evals on 10,000 positions. No GPU or CUDA is needed, so this is done fully on Windows now.
  2. **The bullet model** of the same architecture (bullet from git, pinned). Compiled only when the CUDA Toolkit is present (`CUDA_PATH`). Parity: weights loaded into it give the CPU reference's outputs.
  3. **Training runs** as work: from scratch or a fine-tune of a parent net, with checkpoints, loss and gradient norms (bullet's event API) streamed as `train.progress` events. At the end, quantise, compute offline metrics, register the net.
- **Net ideas.** An idea of kind `net`: a run, which produces a net, then a version (the champion's code plus the new net), then fingerprint (bench must change), screen, short and long. Its sheet shows the loss curves next to the evidence.
- **On Windows now:** everything above, plus a **smoke run**: a tiny fine-tune of the shipped net on a small datagen set (about 1 GB). It proves the pipeline, not a stronger net. It needs the CUDA Toolkit, which the user installs (about 3 GB). The real runs and the first experiments (pawn-pair inputs, then queen-conditioned king buckets) wait for Linux and its disk.

### R5: Claude in the loop

- **`/mcp`.** The API's routes as MCP tools, from the same handlers (rmcp, the stateless spec), with the same token. A Claude Code skill (`skill/`) documents the loop: read the logbook, propose ideas with a hypothesis, write the branch, queue a batch, read verdicts, promote.
- **Ideas carry their author and reasoning.** `author: claude`, notes in markdown. The logbook says who tried what and why.
- **Not built:** the app never runs Claude by itself. You start a Claude session; it drives the app.

### R6: one library, one look

- **eos gains the patterns the app hand-styles:**
  - a non-modal side sheet
  - a popover
  - a selectable multi-line list row and a tree
  - a compact stat link
  - a chart with series highlighting
  - a virtual list
  - a fix for `EAppShell`'s grid letting content widen the page

  These go in the eos repository, are released, and the app switches to them.

- **A last audit:** the words table, one fact once, keyboard paths, phone and tablet, light and dark, `prefers-reduced-motion`, contrast.

### R7: cutover

1. **AGENTS.md and the skill are rewritten** for the new app; README.
2. **First real use with the user**, who logs in themselves:
   - your Lichess account: one game from the app, with moves, premoves, offers and abort, the lock lifting, and an import
   - the bot online, with one game.
3. **The old runner is stopped** (port 7878, `C:\dev\chess-live`). `next` is merged into `main`, which deploys chess.dame.gg.
4. **A Linux handoff doc:** clone, toolchain (Rust, Node, clang, CUDA), `dameless setup`, check the harness, then data and the first real training runs on the big partition.

## Order and what can run at once

R1 first. Then R2, R3 and R4 at the same time: their files don't overlap, and R4's trainer is its own workspace. R5 after R1's API is final. R6 after R3 and R4 settle the UI. R7 last.
