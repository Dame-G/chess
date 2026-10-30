# Handoff: moving Chess to Linux

The app runs on Windows today, live from `C:\dev\chess-live` (a release build of `main`) on http://127.0.0.1:7979. Its data is in `%LOCALAPPDATA%\chess`. Linux becomes the daily driver once this move is done. Read `AGENTS.md` (how to drive and work on the app), `docs/design.md` (the contract) and `docs/roadmap.md` (what is built and what comes next) first: together they are the whole memory of this project.

## Standing decisions from the user

- **Local compute only.** Never rent machines or propose cloud boxes.
- **The goal:** play, review, analyze, with any UCI engine.
- **UI:** VLC + Lichess, dense and sharp, every control an eos component imported by path. Missing patterns go into eos.
- **Do rather than ask, except for these.** Confirm anything that acts on the user's real Lichess account, deletes things, downloads, or publishes. Never enter or move tokens or passwords: the user logs in to Lichess themselves.
- **Fair play is a hard rule.** Never analyse a position from a game the user is playing; the app refuses it, so don't work around it.

## Before moving

1. **Stop the Windows app.**
2. **Data:** starting fresh on Linux is fine. Otherwise copy `%LOCALAPPDATA%\chess\chess.db` to `~/.local/share/chess/`; the engines are downloaded again by setup (the engines list keeps Windows paths for engines added by hand: add those again). The user reconnects Lichess in Settings either way.

## Setting up on Linux

1. **Clone** `Dame-G/chess`.
2. **Toolchain:** Rust (rustup) and Node 24 with corepack (pnpm).
3. **Build:** `corepack pnpm install`, `corepack pnpm build`, `cargo build --release -p chess`.
4. **Run:** `chess serve` (http://127.0.0.1:7979), then `chess setup` (Stockfish 19 and Reckless 0.9.0, each its own package). Keep the data directory outside the repo.
5. **Check:** `cargo test --workspace` (CI already runs on Linux and passes), then `chess diagnostics`.
6. **Reconnect Lichess.** The user logs in with their account. Then do the first real use with the user: one game of theirs played from the app (moves, offers, abort, the fair-play lock lifting) and an import.

## Open items, in order

See `docs/roadmap.md` › Next: the rename to "Chess", the web design pass, calibrating the ladders (Stockfish's and Reckless's), and the eos patterns.

## Working on the repo

See `AGENTS.md` › Working on Chess itself for gates, regenerating API types, the shared-checkout rules and the mock Lichess.

- **`C:\dev\chess` is the dev checkout** of `main`. Builds go to `CARGO_TARGET_DIR=C:\dev\chess\target-shared`, one cache every agent shares (cargo locks it); a hand-run dev app is copied to `C:\dev\chess\run\chess.exe` (ignored), port 7978, data in `C:\dev\chess-dev-data`.
- **Run the live app from its own worktree** (`C:\dev\chess-live`), so half-finished code can't break it.
- **Keep an eye on disk:** one debug target is 3–7 GB. On Windows, many parallel agents' targets once filled the drive.
