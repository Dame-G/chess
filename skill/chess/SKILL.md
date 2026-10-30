---
name: chess
description: Drive Chess, the user's chess app (their games, reviews, profiles, drills, repertoire, engines, their Lichess account) through its MCP tools or the `chess` CLI. Use when reviewing or coaching the user's games, starting games against an engine, managing the engines list, or acting on their Lichess account.
---

# Driving Chess

Chess is the user's app to **play, review and analyze**, with any UCI engine as an opponent or the analysis engine. You drive the same app the user does: every MCP tool is one API route, named by its operation id, and every route is one `chess` CLI command (each tool's description names it). Results are the route's JSON.

## Connect

The app runs as `chess serve` on `http://127.0.0.1:7979`. Check with the `health` tool or `chess health`.

- **MCP** (preferred): the endpoint is `/mcp`, behind the app's bearer token. The user adds it once:

  ```bash
  claude mcp add --transport http chess http://127.0.0.1:7979/mcp --header "Authorization: Bearer $(chess token)"
  ```

  `chess token` prints the token from the app's data directory. Never paste the token into chat, files or commits.

- **CLI**: `chess <command> --json` prints one JSON document; errors go to stderr with a non-zero exit. `--wait` blocks on work.

Start every session with `status` (machine, cores, `policy`) and `list_engines` (the engines and the analysis engine).

## The rules

These hold in every branch of the work.

- **Fair play.** While the user plays a game on Lichess, `status.policy.assist` is not `allowed` and the app refuses analysis (evaluation, explorer, tablebase, reviews, repertoire, other players' imports) with 409 `fair_play`. Treat a 409 `fair_play` as the answer: wait until the game ends. Never analyse, evaluate or suggest moves for a position from a game the user is playing, by any route, tool or engine of your own. In a Lichess game, `act_in_game` sends only the move or action the user chose.
- **Real accounts and downloads.** Tools whose description says "acts on the user's real Lichess account" (challenges, seeks, accepting or declining, tokens, logging out, moves and resignations in Lichess games) and `setup` (downloads) run only when the user asked for that action in this conversation; for `setup`, show `setup_plan`'s sizes first. A token is something the user stores themselves.

## Engines

- `list_engines`: the built-ins (`stockfish`, `reckless`, installed or not) and the engines the user added, each with what it calls itself (`id_name`), whether it has levels and MultiPV, and which one is the analysis engine.
- A fresh app has no engines installed: `setup_plan` lists the downloads and their sizes, one package per built-in; `setup` (when the user agrees; `{ packages: ["reckless"] }` for one) queues them as work.
- `add_engine` (`{ name, path, options?, args?, nodes?, rating? }`) adds an engine the user names by path; `set_analysis_engine` chooses the analysis engine (reviews, evaluation and drills run on it); `remove_engine` takes one off the list.
- Games: `start_game` with seats `you`, an engine's id (`stockfish`, `reckless`), or `<engine id>@<level>` for a level of that engine (`stockfish@5`, `reckless@3`): every engine on the clock has its own ladder and ratings.

## Coaching the user

Reviews, profiles, comparisons, drills, the repertoire and levels are in [coaching.md](coaching.md). Read it before reviewing games or advising the user on their play.
