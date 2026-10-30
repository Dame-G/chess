# Dameless

**Two players, one board.** You and your engine, Dameless (a fork of [Reckless](https://github.com/codedeliveryservice/Reckless)), both get stronger by playing games and learning from them.

- **You** play Dameless at any strength, or on Lichess from the same board. Your games are reviewed move by move; your mistakes come back as drills; your openings, accuracy and rating add up into a profile.
- **Dameless** gets stronger through ideas: a hypothesis and a change, tested in a funnel of trials adapted from Reckless's OpenBench to one machine (with a regression check after every fifth promotion), then promoted to champion when kept. Tunes (SPSA) and trained networks are ideas too. It plays on Lichess as a BOT.
- **Claude** drives the same app through its MCP endpoint or CLI: every action in the UI is a route and a command with the same name.

Everything runs on your machine. The app serves its own UI; https://chess.dame.gg is the same UI, paired with your local app by its token.

## Quick start

The fork's checkout sits beside this repository (`../reckless`; set `RECKLESS_DIR` to use another).

```bash
corepack pnpm install && corepack pnpm build    # the web UI, into apps/web/dist
cargo build --release -p dameless
target/release/dameless setup                   # fastchess and the opening book (a few hundred MB)
target/release/dameless serve                   # http://127.0.0.1:7979
```

Then open http://127.0.0.1:7979. To let Claude Code drive it:

```bash
claude mcp add --transport http dameless http://127.0.0.1:7979/mcp --header "Authorization: Bearer $(dameless token)"
```

## More

- [`AGENTS.md`](AGENTS.md): running the app, the CLI and MCP, every area, and working on the code.
- [`skill/dameless/`](skill/dameless/SKILL.md): the Claude Code skill for driving the app.
- [`docs/design.md`](docs/design.md): the contract for every crate, route, event, table and screen.
- [`docs/roadmap.md`](docs/roadmap.md): what is built and what comes next.

## License

[GPL-3.0-or-later](LICENSE). The board (chessground) and chess logic (chessops, shakmaty) are GPL, and Reckless is AGPL.
