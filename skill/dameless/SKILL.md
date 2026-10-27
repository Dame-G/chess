---
name: dameless
description: Drive Dameless, the user's chess app (their games, their engine Dameless, a fork of Reckless) through its MCP tools or the `dameless` CLI. Use when improving the engine (ideas, trials, tunes, nets, promoting a champion), reviewing or coaching the user's games (reviews, profiles, compare, drills, repertoire), or acting on their Lichess accounts.
---

# Driving Dameless

Dameless is **two players, one board**: the user and their engine both get stronger by playing games and learning from them. You drive the same app the user does: every MCP tool is one API route, named by its operation id, and every route is one `dameless` CLI command (each tool's description names it). Results are the route's JSON.

## Connect

The app runs as `dameless serve` on `http://127.0.0.1:7979`. Check with the `health` tool or `dameless health`.

- **MCP** (preferred): the endpoint is `/mcp`, behind the app's bearer token. The user adds it once:

  ```bash
  claude mcp add --transport http dameless http://127.0.0.1:7979/mcp --header "Authorization: Bearer $(dameless token)"
  ```

  `dameless token` prints the token from the app's data directory. Never paste the token into chat, files or commits.

- **CLI**: `dameless <command> --json` prints one JSON document; errors go to stderr with a non-zero exit. `--wait` blocks on work and ideas.

Start every session with `get_dameless` (champion, logbook, the bot) and `status` (machine, cores, `policy`).

## The rules

These hold in every branch of the work.

- **Fair play.** While the user plays a game on Lichess, `status.policy.assist` is not `allowed` and the app refuses analysis (evaluation, explorer, tablebase, reviews, repertoire, other players' imports) with 409 `fair_play`. Treat a 409 `fair_play` as the answer: wait until the game ends. Never analyse, evaluate or suggest moves for a position from a game the user is playing, by any route, tool or engine of your own. In a Lichess game, `act_in_game` sends only the move or action the user chose.
- **Real accounts.** Tools whose description says "acts on the user's real Lichess account" (challenges, seeks, accepting or declining, the bot's settings, online and seek, tokens, logging out, moves and resignations in Lichess games) and tools that act on the outside world (`push`, `setup`, `import_dataset`) run only when the user asked for that action in this conversation. A token is something the user stores themselves.
- **The fork is the user's.** Commit experiments on new branches of the fork (`RECKLESS_DIR`, usually `../reckless`). `main` changes only through `promote_idea`; `origin` changes only through `push`, when the user asks for that push.

## The improvement loop

The engine gets stronger through **ideas**: a hypothesis and a branch, run through a funnel of trials, ending **kept** or **dropped**. Read [improving.md](improving.md) before writing or queueing an idea; it has the steps, the funnels, the statistics and how to read a verdict.

In short: read the logbook (`list_ideas`, `get_idea`) so you do not repeat a dropped idea; write each idea as its own branch with an up-to-date `Bench:` line; queue them as a batch (`create_batch`) with a hypothesis, notes and `author: "claude"`; wait (`wait_for_idea`); report verdicts honestly; promote only kept ideas the user wants promoted.

**Trust comes first.** On a new machine, or after the harness's tools changed, `harness_status` says whether the harness is trusted. If it is not, run `check_harness` and wait for it before reading any verdict.

## Coaching the user

Reviews, profiles, comparisons, drills and the repertoire are in [coaching.md](coaching.md). Read it before reviewing games or advising the user on their play.
