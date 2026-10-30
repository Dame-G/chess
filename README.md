# Chess

**Play, review, analyze.** A chess app for one person, with any UCI engine as an opponent or the analysis engine.

- **Play** an engine at any strength (Stockfish, Reckless or any engine you add, each with levels from about 800 to full strength), or play on Lichess from the same board.
- **Review** every game move by move: accuracy, the mistakes and the better moves; your mistakes come back as drills.
- **Analyze**: your openings, accuracy and rating add up into a profile you can compare with anyone's; free analysis, the Lichess explorer and the tablebase on any position.
- **Claude** drives the same app through its MCP endpoint or CLI: every action in the UI is a route and a command with the same name.

Everything runs on your machine. The app serves its own UI; https://chess.dame.gg is the same UI, paired with your local app by its token.

## Quick start

```bash
corepack pnpm install && corepack pnpm build    # the web UI, into apps/web/dist
cargo build --release -p chess
target/release/chess serve                   # http://127.0.0.1:7979
target/release/chess setup                   # Stockfish 19 and Reckless 0.9.0 (shows the sizes, asks first)
```

Then open http://127.0.0.1:7979. To let Claude Code drive it:

```bash
claude mcp add --transport http chess http://127.0.0.1:7979/mcp --header "Authorization: Bearer $(chess token)"
```

## More

- [`AGENTS.md`](AGENTS.md): running the app, the CLI and MCP, every area, and working on the code.
- [`skill/chess/`](skill/chess/SKILL.md): the Claude Code skill for driving the app.
- [`docs/design.md`](docs/design.md): the contract for every crate, route, event, table and screen.
- [`docs/roadmap.md`](docs/roadmap.md): what is built and what comes next.

## License

[GPL-3.0-or-later](LICENSE). The board (chessground) and chess logic (chessops, shakmaty) are GPL; Stockfish and Reckless are GPL, downloaded as their own binaries.
