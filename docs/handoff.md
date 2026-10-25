# Handoff: moving the lab to Linux (2026-10-25)

The lab moves from Windows to the Linux partition on the same machine (i7-10700K, RTX 3080, 2 TB NVMe), which becomes the daily driver and runs everything. Read this, `AGENTS.md`, `web/DESIGN.md` and `docs/plan.md` first.

## Standing decisions from the user

- **Local compute only.** Never rent machines. Phase 5's remote runner is dropped; only the optional `/mcp` endpoint remains.
- **UI:**
  - The direction is "VLC + Lichess": one window with library, stage and inspector panes and a live status bar.
  - Dense and sharp (radius 3), with no redundant information. eos design principles apply.
  - Every control is an eos component; a missing pattern belongs in the eos library.
  - Redesign work goes to Sonnet subagents, each owning separate files, and is reviewed in the browser by the main session.
- **Order of work:** finish the UI polish, then Phase 3 (SPSA tuning, play against builds, live engine vs engine), then Phase 4 (training with bullet). Overnight batches can wait.
- Before trusting results, run one screen of `main` against a functionally identical copy and confirm it does not falsely accept. It takes about an hour and can run alongside other work.

## State

- **Phases 0–2 are done:**
  - runner: builds, jobs, analysis, experiments, funnels, batches, SSE
  - CLI with `--json`
  - web app
  - CI, and deploy to chess.dame.gg
- **Experiments so far:**
  - **#1**, a comment-only change: passed as nonfunctional.
  - **#2:** cancelled.
  - **#3**, no null-move pruning: rejected at the quick screen after 6,650 games, −6.6 ± 5.2 Elo.
- **Open items:**
  - Check whether the harness writes the `Bench:` line into experiment commits. The test branches had none.
  - The running and queued UI states have not been seen since the redesign.
  - The 5-variant unattended batch from Phase 2 has not been run.
- **Missing eos patterns,** hand-styled here for now:
  - a dense clickable list row
  - a stepper/tab cell with two lines
  - a compact header and footer
  - a virtual list
  - a media timeline

## Setting up on Linux

1. **Clone:** `Dame-G/chess` and `Dame-G/reckless` side by side, e.g. `~/dev/chess` and `~/dev/reckless`. The runner finds the engine through `RECKLESS_DIR`, defaulting to `../reckless`. The fork's `main` tracks upstream. Push `test/*` branches as you make them.
2. **Toolchain:**
   - Rust (rustup) and Node with corepack (pnpm).
   - Clang/LLVM, which allows Syzygy builds. Builds used `make no-syzygy` on Windows.
   - The CUDA Toolkit, with `CUDA_PATH` set, for bullet in Phase 4.
3. **First run:** `cargo build`, then `chess setup` (fastchess v1.8.2, the UHO book), then `chess serve`. Build the web app with `corepack pnpm build`. In debug builds the runner serves `web/dist` from disk.
4. **History (optional):** the runner's history lives in `%LOCALAPPDATA%\chess-harness` on Windows (`runner.db` plus builds and tests). Copy `runner.db` and `tests/` from the NTFS partition into the Linux data directory (`config::default_data_dir`) to keep it. Build paths inside it point at Windows, so rebuild engines rather than reuse binaries. Starting fresh is fine too.
5. **Platform code:**
   - Check anything Windows-specific in the runner: process-wrap job objects vs process groups, `.exe` suffixes, fastchess download names, make vs cargo invocations.
   - Run `cargo test` and one `chess build main --wait` to confirm the bench matches the commit's `Bench:` line.
6. **Training data:** use the large Linux partition. A 50–150 GB slice of a dataset is enough to start fine-tuning the existing network.
