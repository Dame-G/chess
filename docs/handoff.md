# Handoff: moving the lab to Linux (2026-10-26)

The lab moves from Windows to the Linux partition on the same machine (i7-10700K, RTX 3080, 2 TB NVMe). Linux becomes the daily driver and runs everything. The Claude session that built all this was on Windows and does not carry over: this file, `AGENTS.md` (how to drive the harness), `web/DESIGN.md` (the UI brief) and `docs/plan.md` (the original plan) are the whole memory. Read them first.

## Standing decisions from the user

- **Local compute only.** Never rent machines or propose cloud boxes. Phase 5's remote runner is dropped; only an optional `/mcp` endpoint remains.
- **The goal** is the strongest personal engine: a Reckless fork improved by tested changes, tuning and, later, network training.
- **UI direction:** "VLC + Lichess smashed into something beautiful":
  - one window with library / stage / inspector panes and a live status bar
  - dense and sharp (radius 3), simple, intuitive for a layman, no redundant information
  - eos design principles; the site eos.dame.gg is the reference
- **Every control is an eos component**, imported by its own path (`@eos-ui/vue/components/EButton`), never a native `<button>`/`<input>`/`<select>`. A missing pattern belongs in the eos library, not hand-rolled here.
- **How UI work is done:** redesigns go to Sonnet subagents, one per area with separate files; the main session reviews every result in the browser (desktop, ~1100px, phone, light and dark) and fixes what's off before reporting.
- **Layout rules learned the hard way:**
  - The stack ("mobile") breakpoint is **75rem** app-wide, so tablets in either orientation get the stacked layout.
  - The board must meet its frame exactly: its side is rounded down to chessground's 8-device-pixel step (`--side` / `--cg-step` in `BoardView.vue`). Measure `.cg-wrap` against `cg-container` after any layout change.
  - eos buttons have a fixed control height: a multi-line row built from `EButton` needs `block-size: auto`, or its text spills over the next row.
- **The user wants things done, not asked about**, but confirm anything that acts on their real accounts (Lichess) or deletes things.

## What exists now

The app is **Dameless** (repo `Dame-G/dameless`, deployed to chess.dame.gg on push to main; the user is putting it behind Cloudflare Access). The engine is still **Reckless** (fork `Dame-G/reckless`). The CLI is `chess`, the data folder `chess-harness`.

- **Nav: Board · Improve · Versions · Lichess.**
- **Board** (`/board`, `/matches/:id`, `/games/:test/:index`): one board plus a right panel with Moves and Games tabs.
  - Modes: Analyse (live multipv), Play (you against a build, with clocks, takeback, draw offer; eval hidden until "Analyse this game"), Engines (two builds live).
  - New menu: start position, load a PGN/FEN (paste anywhere, drop a file), latest test games, recent positions.
- **Improve** (`/improve`): experiments and tunes in one list. One New button: "Test a change" (a branch against main through the funnel) or "Tune numbers" (SPSA). Applying a tune makes a branch and queues its [0, 4] test.
- **Versions** (`/versions`): one row per branch with its verdict against main, tune state and whether the Lichess bot plays it.
  - Quick actions: Test it, Tune it, Watch it play, Play it, Put it on Lichess, Build now.
  - Builds are a detail with Delete; branches can be hidden or deleted (typed confirmation); "Free disk space" prunes old builds after a dry run.
- **Lichess** (`/lichess`): the BOT account **Dameless** is connected (token stored in `<data dir>/lichess-token`).
  - It accepts challenges by its rules (currently bullet only; the user may widen that) and plays them with the chosen branch.
  - Find a game (one random challenge to a bot near its rating) and Keep playing (continuous matchmaking).
  - **Lichess caps bot-vs-bot games at 100 per rolling day** (humans unlimited). The runner pauses matchmaking at the cap and says when it resumes.
- **Your own Lichess account** (`/lichess`, "You"): log in with Lichess (OAuth PKCE, no app registration; token in `<data dir>/lichess-me-token`), then seek (rapid and slower only: Lichess's Board API rule), challenge (blitz and slower) a friend, Lichess's Stockfish or your bot, play on our board, import your history.
  - **Fair play is enforced by the runner:** while any game of yours is live on Lichess (even one on lichess.org), analysis, the explorer and the tablebase answer 409 and the board hides them.
  - **Analyse** has an Explorer tab (Lichess, Masters, your games; needs the login) and a tablebase pane for 7 pieces or fewer.
- **Tuning:** the fork's `spsa/tunables` branch declares 38 search tunables (same bench as main without the feature: 2837905). Tunes run OpenBench's SPSA as concurrent batches across the machine.
- **Clock scaling:** tests and tunes scale their clocks to 660,000 nps of `bench` (Reckless's OpenBench reference), measured with every game slot busy before each test, so STC here searches about what upstream's STC does. On the i7-10700K with 7 slots busy, `bench` read about 210–320k nps (other work running), so clocks play 2–3× longer than written. Re-check on Linux: `chess experiment show` prints each test's `clock` line.
- **Experiments so far:** #1 comment-only change passed as nonfunctional; #2 cancelled; #3 no null-move pruning rejected at the quick screen after 6,650 games, −6.6 ± 5.2 Elo. No tune has run for real yet.

## Open items, in order

1. **False-accept check (do this first on Linux).** `chess test --dev test/comment --base main --preset vstc --bounds 0,3 --wait`: `main` against a functionally identical copy at the gainer's bounds, and confirm it is rejected (see "Check the harness for false accepts" in `AGENTS.md`). Not the [-5, 0] screen, which passes a do-nothing change about 90% of the time by design, and not an experiment, whose bench stage stops it. About an hour. Nothing should be trusted before it passes.
2. **`Bench:` lines:** check that the harness writes the `Bench:` line into experiment commits; the early test branches had none.
3. **Your Lichess account, first real use:** log in, play one game from the app, and check moves, premoves, draw/takeback offers, abort, the fair-play lock lifting at the end, and an import. Built against a mock; never tried against Lichess.
4. **First real tune:** `spsa/tunables` with the dozen pruning/reduction tunables, 2,000–3,000 iterations (about a night with scaled clocks), then Apply and let the [0, 4] test decide. All 38 need 5,000+ iterations (days).
5. **Ideas batch:** the user asks for engine ideas (or brings their own); each becomes a branch and they run as a batch.
6. **Phase 4, training:** CUDA, bullet from git, a 50–150 GB slice of a relabelled dataset on the Linux partition. First targets: pawn-pair inputs (a proven gap), then SFNNv18-style queen-conditioned king buckets. See `docs/plan.md`.
7. **Missing eos patterns,** hand-styled here for now: a dense selectable multi-line list row, a multi-select chip group, a two-line tab cell, a compact header/footer, a virtual list, a media timeline, a chart with series highlighting, a spinner with a status line.
8. **Not built:** merging a passed branch into main from the UI, a rematch re-rolling the book opening, building an arbitrary tag/commit from the UI (the CLI and tests still do), per-build compile logs in the UI.

## Working on this repo

- **Gates before pushing:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `corepack pnpm api:check`, `corepack pnpm lint` (2 tolerated `vue/one-component-per-file` warnings in `BoardView.vue`), `corepack pnpm format:check`, `corepack pnpm exec knip`, `corepack pnpm -r typecheck`, `corepack pnpm -r test`. After changing a route or a type it returns, run `node scripts/api-types.mjs` and commit `runner/openapi.json` and `web/src/api/schema.d.ts`.
- **Several agents in one checkout:** each commits only with an explicit pathspec, `git commit -m "…" -- <its files>`, and never leaves files staged. A plain `git commit` takes the whole shared index and twice swept another agent's staged changes into a commit (once deleting a file a page still imported, which broke the deployed site). Never run `pnpm install` from a worktree whose `node_modules` is a junction into the main checkout: it rewrote the shared links to point into the worktree, and removing the worktree broke every build. Each agent uses its own `CARGO_TARGET_DIR`. When the shared tree is mid-edit, verify and commit from a clean worktree of HEAD plus your files.
- **Keep the live runner apart from agents' edits.** On Windows the live runner was built from a separate worktree so half-finished code in the main checkout could not break it; do the same on Linux if agents work while the bot is live. In debug builds the runner serves `web/dist` from the checkout it was compiled in.
- **Never run the bot from two machines at once.** Stop the Windows runner before starting the Linux one; both would answer the same challenges.

## Setting up on Linux

1. **Clone** `Dame-G/dameless` and `Dame-G/reckless` side by side, e.g. `~/dev/dameless` and `~/dev/reckless`. The runner finds the engine through `RECKLESS_DIR`, defaulting to `../reckless`. The fork's `main` tracks upstream; keep `spsa/tunables` and push `test/*` branches as you make them.
2. **Toolchain:** Rust (rustup), Node with corepack (pnpm), Clang/LLVM (allows Syzygy builds; Windows used `make no-syzygy`), and for Phase 4 the CUDA Toolkit with `CUDA_PATH` set.
3. **First run:** `cargo build`, `chess setup` (fastchess v1.8.2, the UHO book), `corepack pnpm build`, then `chess serve` (http://127.0.0.1:7878).
4. **Platform code:** check the runner's Windows specifics: process-wrap job objects vs process groups, `.exe` suffixes, fastchess download names, make vs cargo invocations. Then `cargo test` and `chess build main --wait`, and confirm the bench equals the commit's `Bench:` line (2837905 at the time of writing).
5. **Data (optional):** the data folder is `dirs::data_local_dir()/chess-harness`: `%LOCALAPPDATA%\chess-harness` on Windows, `~/.local/share/chess-harness` on Linux. Copying `runner.db` keeps history and the Lichess bot's settings; `lichess-token` keeps the connection (or paste the token again on the Lichess page). Build paths in the database point at Windows, so rebuild engines rather than reuse binaries. Starting fresh is fine too.
6. **Training data:** keep it on the large Linux partition.
