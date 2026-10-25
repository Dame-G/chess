# chess.dame.gg — Reckless dev harness (research-revised)

## Context

You want the strongest personal engine you can get by forking Reckless and iterating fast, with Claude running experiments mostly unattended. That needs one tool covering six jobs:

- build engine variants
- run a testing funnel that matches how Reckless itself tests
- watch and analyse games
- play against any build
- tune parameters
- train networks

A person uses it through a web UI. An AI uses the same capabilities through a CLI and an HTTP API. It lives at chess.dame.gg, is styled with eos (`@eos-ui/vue` 0.17.1 on npm), and copies the eos/eos-site CI and deploy setup.

Everything below was checked against the state of the art as of 2026-10-25. Where something is new or experimental, that's because it has evidence behind it, not because it's novel.

**Your machine:**

- **CPU:** i7-10700K, 8 cores / 16 threads, AVX2. Default concurrency 14.
- **RAM:** 32 GB.
- **GPU:** RTX 3080 10 GB, but no CUDA toolkit installed yet.
- **Rust:** 1.99.1.
- **Clang:** none, so engine builds use `make no-syzygy` until LLVM is installed.

**Decisions:**

- Board: chessground, so the harness is GPL-3. Easiest is to make the repo public, which also satisfies GPL for the hosted JavaScript.
- Data: local-first runner, no cloud backend.
- Runner language: Rust.

## What the research changed (key facts)

- **Reckless's own process is the template.** It runs OpenBench on fastchess at recklesschess.space with these settings:
  - **Tests:** STC 8+0.08 with 16 MB hash, LTC 40+0.4 with 64 MB, VSTC 4+0.04 as a pre-screen, plus "hash pressure" variants (2 MB / 8 MB) and SMP 40+0.4 at 6 threads.
  - **SPRT:** nElo pentanomial GSPRT. Bounds [0, 3] for gainers, [-2.75, 0.25] for non-regression, [0, 4] to confirm SPSA results. alpha 0.05, beta 0.10.
  - **Book and options:** book `UHO_Lichess_4852_v1`, and `Minimal=true MoveOverhead=0`.
  - **Adjudication:** win at 3 moves ≥ 400; draw from move 30 after 6 moves within ±10.

  Copying these exactly keeps your results directly comparable with upstream's.

- **Bench is a functional fingerprint.** Every Reckless commit carries a `Bench: N` line, and its CI checks it on 5 ISAs using Intel SDE. If the bench is unchanged, the change is functionally identical, so only a speed check is needed and no games. This is the strongest free test tier.
- **fastchess v1.8.2** already has most of what we need:
  - built-in pentanomial SPRT
  - `nodes=` limits
  - `-pgnout` with nodes, seldepth, nps and pv per move
  - `-strict`, `-recover`, `-use-affinity`
  - autosave state with `-config outname=`

  It has no streaming JSON, so the runner parses stdout and computes stats itself.

- **Reckless's network:**
  - **Inputs:** 66,864 threat features (i8 weights) plus 10-bucket mirrored king-PSQ inputs.
  - **Layers:** a 768 pairwise layer, then 768 → 16 → 32 → 1, with 8 output buckets.
  - **Loading:** the net is embedded at compile time via `EVALFILE`, so a net is a build input.
  - **No public training config.** There are tools on the `nnue-tools-rsn-3` branch (PGN → rbinpack, rescore with MSE/MAE).
  - **No datagen on main.** The `datagen` branch adds a `UseSoftNodes` option for this.
- **The clear architecture gap: pawn-pair inputs.** Stockfish (SFNNv16), Viridithas 20, Stormphrax 8 and PlentyChess 8 have them. Reckless doesn't.
- **The genuinely new frontier: SFNNv18 (2026-10-13).** It adds K32Q2 king buckets conditioned on whether the opponent has a queen, 32 layer stacks, and int8 FT weights. Nobody outside Stockfish has adopted it yet.
- **Training:**
  - bullet is the standard. It's used from git (no releases) and runs natively on Windows with the **CUDA Toolkit and `CUDA_PATH` set; WSL isn't needed**.
  - Its CPU training backend was removed in 2026, so the GPU is mandatory.
  - It supports quantisation-aware training, custom inputs and the viriformat, Stockfish binpack and Monty binpack formats.
  - Training data: relabelled SF/Lc0 binpacks are on HuggingFace (vondele/*, 130–565 GB each). Downloading them is far cheaper than relabelling with Lc0 on a 3080.
- **Policy nets inside alpha-beta:** no evidence of gains in top engines, so low priority. Searchless and transformer approaches are research value only on one GPU.
- **Tuning:** SPSA is still the standard, with OpenBench defaults α 0.602, γ 0.101, A 0.1. Reckless has an `spsa` feature and a `define!` macro, but no parameters are declared yet.
- **Stack:**
  - `@lichess-org/chessground` 10.5. The old `chessground` package is deprecated, and vue3-chessboard pins it, so it's skipped.
  - chessops 0.15, uPlot 1.6.
  - axum 0.8, rusqlite 0.41, utoipa 6.
  - process-wrap, which uses Windows Job Objects to kill fastchess and its engine processes as one tree.
  - rust-embed, shakmaty 0.30, pgn-reader.
- **Browser access to localhost:** Chrome 142+ and Firefox 153+ prompt for Local Network Access. So the **primary UI is served by the runner itself on 127.0.0.1**: same origin, no prompt. The hosted chess.dame.gg connects with one prompt plus a pairing token.
- **Agents:** a CLI with `--json` plus a Claude Code skill is the leanest and most debuggable. rmcp 3.7 implements the stateless MCP spec of 2026-07-28, so `/mcp` can be added later from the same handlers.

## Architecture

```
Browser ── http://127.0.0.1:7878 (UI embedded in runner; primary)
        └─ https://chess.dame.gg (same SPA on Cloudflare; LNA prompt + pairing token; or remote runner via Tailscale/Cloudflare Tunnel+Access)
                   │ REST (POST commands) + SSE /events (monotonic ids, Last-Event-ID replay)
`chess` runner (Rust, one binary: serve | CLI)
   ├─ utoipa OpenAPI = single schema → CLI --json, web client types (openapi-typescript), later /mcp (rmcp)
   ├─ SQLite (WAL, one writer thread) + files: builds/, nets/, games/ (zstd PGN), data/
   └─ drives (process-wrap): cargo/make for the Reckless fork, UCI engines, fastchess, bullet
```

Security: binds to 127.0.0.1 by default, requires a bearer token for any state-changing request, and checks `Origin` and `Host` to block DNS rebinding.

## Repos and layout

- **`Dame-G/reckless`:** fork of upstream, cloned at `C:\dev\reckless`. `main` tracks upstream; each experiment is a branch.
  - Located via the `RECKLESS_DIR` env var, defaulting to `../reckless`, the same pattern eos-site uses with `EOS_DIR`.
  - The harness writes the `Bench: N` line into experiment commits.
  - Reckless's own CI stays as it is.
- **`Dame-G/chess`** at `C:\dev\chess`:

```
runner/src/  main.rs (clap) · api/ (axum, SSE, auth, utoipa) · store/ (rusqlite + migrations)
             engine/ (build cache, UCI wrapper) · jobs/ (queue + executors) · stats/ (nElo, pentanomial GSPRT, Elo CI)
             tools/ (pinned fastchess, books, bullet checkout; SHA-256 verified) · funnel/ (stage rules)
web/src/     api/ (generated client) · board/ (ChessBoard.vue thin chessground wrapper, MoveList, EvalGraph)
             charts/ (one uPlot wrapper reading eos CSS vars) · views/ · stores/ (Pinia 4 + Pinia Colada; SSE → cache)
trainer/     Rust crate on bullet (git pin): Reckless arch reproduction + experimental arches  [Phase 4]
skill/       Claude Code skill (plugin) documenting the CLI workflow; AGENTS.md (+ CLAUDE.md → AGENTS.md)
.github/     actions/setup (from eos + Rust toolchain/cache) · workflows ci.yml, deploy.yml, release.yml
wrangler.jsonc  from eos-site, but not_found_handling "single-page-application", custom domain chess.dame.gg
```

**Web stack:** match eos-site's versions (Vue 3.5.44, Vite 8.3, Vitest 5 in browser mode, TS 6, pnpm 12), plus vue-router 5.4 and `@eos-ui/vue`/`@eos-ui/tokens` from npm.

Everything is built from eos components (AppShell, DataGrid, Tabs, Metric, Progress, Form, Select, CommandPalette, Toast). The only custom pieces are the board and the chart wrapper. If the chart wrapper turns out generic, it moves into eos later.

## Core domain

- **Build.** Key: `(commit, net hash, cargo profile/features, ISA)` → binary. Each build stores its bench node count, NPS (from `speedtest`) and the compile log. Builds are cached and never rebuilt.
- **Net.** Hash, file, parent net, architecture, the training run that produced it, and offline metrics (validation loss, MSE/MAE vs a deep-search label set).
- **Test spec.** Time control (st/tc/nodes, optionally NPS-scaled to a 660k-NPS base so different machines give comparable results), threads, hash, book, adjudication, and SPRT bounds or a fixed game count. Presets copy Reckless exactly: VSTC, STC, LTC, hash-pressure STC/LTC, SMP, and fixed nodes.
- **Experiment.** Hypothesis, base build, dev build, a kind (gainer, non-regression, or non-functional), and a funnel. Stages run in order; a pass moves to the next stage automatically and a fail stops the experiment with its evidence:
  - **T0 fingerprint (seconds):**
    1. Build both engines and run bench.
    2. A "non-functional" change must keep the bench identical; it then gets only a speedtest NPS comparison and is done.
    3. A functional change must not crash and must pass a `-strict` smoke match of 8 games, like Reckless's `games.yml`.
  - **T1 screen (minutes):** fixed nodes (about 25k per move) or VSTC, UHO book, game pairs, wide nElo bounds [0, 10]. Rejects bad ideas fast.
  - **T2 confirm (about 1 hour locally, minutes on a burst box):** STC with [0, 3], or [-2.75, 0.25] for non-regression.
  - **T3, optional:** LTC 40+0.4.
- **Tune job.** SPSA with OpenBench's algorithm and defaults, over tunables declared through Reckless's `define!` macro. The result becomes a dev build that gets confirmed at [0, 4].
- **Datagen job.** fastchess self-play using the `UseSoftNodes` soft node limit and DFRC/random openings, then PGN, then rbinpack/viriformat (porting the tools from `nnue-tools-rsn-3`). Imported HuggingFace binpacks also count as data.
- **Train job.** A bullet run that produces a net. It records loss curves and gradient norms (bullet's event API) and promotes the net to a build input.
- **Game.** PGN plus per-move eval, depth, seldepth, nodes and time. Stored compressed and indexed.
- **Batch.** A group of experiments: the unit of the AI loop. `chess batch submit`, then `chess wait --batch`, then a results table.
- **Stats.** nElo, pentanomial GSPRT LLR and logistic Elo with a t-distribution confidence interval, as OpenBench does it. Unit-tested against published Fishtest and OpenBench runs.

## Phases (each ends usable)

**Phase 0: Foundation**

- Both repos.
- CI:
  - Rust: fmt, `clippy -D warnings`, test
  - Web: eslint, prettier, stylelint, `vue-tsc`, vitest, knip
  - a check that generated API types are current
  - a `ci-ok` aggregator job
- `deploy.yml` adapted from eos-site, so pushing to main deploys to chess.dame.gg, with a warning when secrets are missing. A placeholder page goes live.

**Phase 1: Runner core**

- `chess setup` pins fastchess v1.8.2 and the UHO book.
- `chess build`, `chess bench`, `chess speedtest`, and `chess analyse <fen>` (live multipv over SSE).
- SQLite store, OpenAPI, pairing and the embedded UI.
- Engines view, and an analysis board using chessground and chessops.

**Phase 2: Testing funnel and AI loop**

- fastchess executor and stats.
- Experiments, funnels and batches; automatic `Bench:` commits.
- Experiment view: stage timeline, live LLR chart, pentanomial counts, Elo ±.
- Game viewer with eval graph.
- CLI `--json`, `wait`, `show`; the skill and `AGENTS.md`. **From here on, Claude can run experiments unattended.**

**Phase 3: Play and tuning**

- Human vs any build, with clocks.
- Engine-vs-engine live view; load a PGN or FEN.
- SPSA jobs, with tunables declared in the fork; parameter trajectory charts.

**Phase 4: Training**

- You install the CUDA Toolkit.
- The trainer crate rebuilds Reckless's architecture in bullet with custom threat inputs. Exit criterion: v60 weights loaded into it give the same evals as the engine.
- Datagen and binpack import; Training view (loss and gradient-norm charts, net registry).
- First experiments, each fine-tuned from v60 and run through the funnel:
  1. Pawn-pair inputs, a proven gap.
  2. SFNNv18-style queen-conditioned king buckets plus more layer stacks, the new frontier.

**Phase 5: Scale**

- Remote runner: the same binary on a burst box, such as an AWS c8a.16xlarge spot (64 vCPU, about $1/hr), reached over Tailscale or a Tunnel.
- If one box isn't enough, self-host OpenBench (it handles workers, SPSA and datagen) and let the runner submit to it, rather than building our own distribution.
- Optional `/mcp` endpoint via rmcp.

Things deliberately left out: accounts, cloud database, a WASM engine, puzzles, an opening explorer, social features, policy-net and MCTS work (no evidence it pays off at this level).

## What you'll need to do (I'll prompt at each step)

1. Fork Reckless to `Dame-G/reckless` and create `Dame-G/chess` (public, because of GPL). I can run both with `gh` if you want.
2. Add the `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` secrets to `Dame-G/chess`. Reuse eos-site's token if it covers Workers deploys and custom domains on dame.gg.
3. Phase 4: install the CUDA Toolkit and set `CUDA_PATH`. Optional: LLVM for Syzygy, which also unlocks tablebase adjudication.
4. Approve Chrome's local-network prompt once, the first time the hosted site connects.

## Verification

- **CI:** all gates above. The stats module is checked against reference values from finished Fishtest and OpenBench runs.
- **Phase 1:** `chess build main` then `chess bench --json` gives the same node count as the `Bench:` line in upstream's latest commit (2837905 as of 2026-10-23). `chess analyse startpos` streams depths. The board shows live analysis both on 127.0.0.1 and on chess.dame.gg.
- **Phase 2:**
  - A whitespace-only change passes T0 as non-functional.
  - `main` vs a functionally identical copy finishes T1 with Elo ≈ 0 and no false accept.
  - A deliberately weakened branch (null-move pruning removed) is rejected in T1 within minutes.
  - A 5-variant batch runs unattended through the CLI and shows up in the UI.
- **Phase 4:** the parity test between bullet's reproduction and the engine's eval passes on 10k positions before any experimental architecture is trained.
- **Deploy:** a push to main updates chess.dame.gg; a page check confirms it renders in light and dark mode.
