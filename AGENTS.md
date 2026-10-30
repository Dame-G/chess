# Chess

**Play, review, analyze.** Chess is an app for one person: you play games (against an engine here, or on Lichess), the app reads them (reviews, the eval graph, the better move) and adds them up (your profile, comparisons, drills from your mistakes, your repertoire, your progress). Any UCI engine can be an opponent or the analysis engine: Stockfish and Reckless are built in, and you can add your own. Every engine plays at full strength or at a level of its own ladder.

`docs/design.md` is the contract for every crate, route, event, table and screen; `docs/roadmap.md` says what comes in what order. This file is how an agent drives the app and works on it. The `chess` skill (`skill/chess/`) is the short version for Claude sessions that drive the app.

## Words

One vocabulary everywhere (code, routes, CLI, UI, docs): **you**, **engine** (a UCI engine of the engines list: `stockfish`, `reckless`, one you added), **analysis engine** (the engine reviews, live evaluation and drills run on; Stockfish by default), **level** (an engine at a node budget, from about 800 to full strength; every engine has its own ladder and ratings), **game**, **player**, **work** (anything running in the background), **review**, **connection** (your Lichess account linked to the app). In the UI, never: build, match, experiment, test, batch, job, runner, human.

## Running it

```bash
cargo build --release -p chess          # target/release/chess; put it on PATH, or use `cargo run -p chess --`
chess serve                             # http://127.0.0.1:7979: the API, the event stream, /mcp and the web UI
chess setup                             # downloads Stockfish 19 and Reckless 0.9.0 (shows the sizes, asks first)
chess status                            # any other subcommand is a client of the running app
```

The web UI is `apps/web/dist` (`corepack pnpm build`), read from disk in debug builds and embedded in release. For UI work, `corepack pnpm dev` serves it with hot reload against a running app.

| Variable                         | Default                                         | What                                                                                                                   |
| -------------------------------- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `CHESS_LISTEN`                   | `127.0.0.1:7979`                                | Address and port.                                                                                                      |
| `CHESS_DATA` (`--data`)          | the platform's local data folder + `/chess`     | `chess.db`, `token`, `secrets/lichess-you`, `tools/` (the built-in engines), `games/`, `logs/`.                        |
| `CHESS_CORES`                    | physical cores − 1                              | Cores the scheduler hands out.                                                                                         |
| `LICHESS_URL`                    | `https://lichess.org`                           | Lichess. `LICHESS_EXPLORER_URL` and `LICHESS_TABLEBASE_URL` point those elsewhere (with a mock: `<mock>/explorer`, …). |
| `CHESS_URL`, `CHESS_TOKEN` (CLI) | `http://127.0.0.1:7979`, the data dir's `token` | Where the CLI finds the app and its token.                                                                             |

The app was called Dameless. On the first start with the default data folder, when `chess` does not exist and the old `dameless` folder does, `chess serve` copies `dameless.db` (with `-wal` and `-shm`, renamed `chess.db`), `token`, `secrets/lichess-you`, `games/` and `logs/` into it, points engines that were in the old `tools/` at the new one, and logs what it copied (`crates/hub/src/legacy.rs`). Nothing else comes over (run `chess setup` again for the built-ins), and the old folder is left as it was, as the backup.

`chess diagnostics` lists the machine's health: the analysis engine, Reckless (installed or not), disk and the database, each with what to do when it fails.

## Driving it: the CLI and MCP

Every action is a route, a CLI command and an MCP tool with the same name. `chess::cli::COMMANDS` maps every operation id to its command, and a test checks it against the OpenAPI document (`crates/server/openapi.json`, `chess openapi`).

- **The token.** Every request but `GET /api/health` carries `Authorization: Bearer <token>`. `chess token` prints it from the data directory. The app's own page needs none; the hosted UI (https://chess.dame.gg) is given it once, by hand. Requests must name a loopback `Host` and come from no other website's `Origin`.
- **The CLI.** Every command takes `--json` (one JSON document on stdout; errors on stderr with a non-zero exit). `--wait` blocks on work and exits non-zero unless it succeeds; `--follow` prints a game's moves as they are played. Lichess tokens are read from stdin, never argv. `chess <command> --help` lists every flag.
- **MCP.** `POST /mcp` is a Streamable HTTP MCP server (rmcp, stateless: the 2026-07-28 protocol, and older clients' `initialize` too), behind the same token and guard. Connect Claude Code with:

  ```bash
  claude mcp add --transport http chess http://127.0.0.1:7979/mcp --header "Authorization: Bearer $(chess token)"
  ```

  There is one tool per OpenAPI operation, named by its operation id and generated at startup from the document (`crates/server/src/mcp/tools.rs`): its description is the route's summary and description plus the CLI command; its input is the path and query parameters under their names and the JSON body under `body`. A call goes through the app's own router, so it does exactly what the route does and answers with the route's JSON (an error is a tool error with the status and the error body). The event stream and `GET /api/token` are left out. `wait_for_work` waits (looking every two seconds, up to `timeout_seconds`, default 600) and answers `{ finished, work }`. Tools carry annotations: GETs are read-only; tools that act on Lichess or the outside world are open-world and say so in their description; deleting, removing, stopping, resigning and logging out are destructive. The server's instructions carry the fair-play and real-account rules below.

## Two rules

**Fair play.** Lichess forbids engines, opening books, tablebases and other help during a game; its explorer is allowed in correspondence only. While you have any game running on Lichess (played here or on lichess.org), the policy (`status.policy`, `policy.changed` events) is `off` (or `explorer_only` when every live game is correspondence), and the server refuses assistance with 409 `fair_play` ("Analysis is off while you're playing on Lichess"): evaluation (`/api/eval`; a running session is stopped), the explorer, the tablebase, reviews (new ones refused, a running one pauses before its next position), the repertoire, and other players' imports. Local games against an engine and installing engines are never assistance. The UI hides these controls; the 409 is the backstop. No agent works around it, and no agent analyses a position from a game the user is playing.

**Real accounts.** The app acts on the user's real Lichess account. An agent challenges, seeks, accepts, declines, stores or forgets a token, moves or resigns in a Lichess game, or downloads engines only when the user asked for that action. The user logs in and makes tokens themselves. In a Lichess game, a move is the user's own move, never one an engine chose.

## Engines

```bash
chess engines                                     # the list: built-ins (installed or not) and yours, and the analysis engine
chess setup [--only stockfish,reckless] [--yes] [--wait]   # downloads the built-ins: shows each file's size and asks first
chess engines add --name "Berserk" --path ./berserk [--option Threads=2 --arg … --nodes N --rating R --id x]
chess engines remove <id>
chess engines analysis <id>                       # the analysis engine
```

- **The list.** An engine has an `id` (a slug), a `name`, a `path` with `args`, UCI `options` (set after the handshake, before the app's own `Threads`, `Hash`, `MultiPV`; options an engine does not declare are not sent), and `id_name`: what it calls itself (UCI `id name`, read when it is added or installed, such as `Stockfish 19`), and `multipv`: whether it declares UCI's `MultiPV` (read at the same time). An engine added with `nodes` plays that many nodes a move whatever the clock and has no levels; `rating` is a rating it plays at by design. Every other engine has `levels: true` once installed.
- **Built-ins.** `stockfish`: Stockfish 19, the release build for this platform (picked from the release's asset list; on Windows `stockfish-windows-x86-64-universal.zip`), in `tools/stockfish/`. `reckless`: Reckless 0.9.0 from codedeliveryservice/Reckless's releases, in `tools/reckless/`. Its assets are bare executables, one per CPU family (`reckless-windows-avx2.exe`, `-avx512`, `-generic`, the same for Linux, and `reckless-macos`); setup takes the AVX2 build when the CPU has AVX2, else the generic one (`work::setup::pick_reckless`). It plays at full strength on the clock, has levels and can analyse (it has MultiPV). Old games against `engine:reckless` (from before the engines list) are this engine's games. Each built-in is its own package (`stockfish`, `reckless`; about 75 and 65 MB): `GET /api/setup` lists the downloads with the sizes GitHub gives; `POST /api/setup` queues the `setup` work (network work: no cores). A built-in that is not installed is listed with `installed: false`; starting a game or a review with it is a 409 that says to run setup.
- **The analysis engine** (`PUT /api/engines/analysis`, default `stockfish`): reviews, live evaluation and drills run on it, and it is the engine `chess levels` means when none is named. One with a fixed budget cannot be it. Removing it from the list makes Stockfish the analysis engine again.
- **Maia** was a built-in (lc0 with nine nets) until migration 8, which removed its engines from the list. Games against `engine:maia-<n>` keep their seats and names.

## Games

Every game is one record wherever it was played: local, your Lichess games, imported history.

```bash
chess games [--player you --source local,lichess,import --status --speed --rated --limit 20]
chess game <id> [--pgn | --review [--engine --nodes] | --follow]
chess play --white you --black stockfish@5 --tc 5+3 | --nodes N | --movetime MS [--fen … --moves e2e4 e7e5] [--hash --threads --adjudicate] [--follow]
chess move <id> e4 [--no-wait]          # UCI or SAN; waits for the reply
chess draw <id> [--accept|--decline] | takeback <id> [--accept|--decline] | resign <id> | abort <id> | claim <id> [--draw]
chess stop <id>                         # a game you sit in is resigned (aborted before each side moved); an engine game aborted
chess import --pgn game.pgn [--you white]
```

- Seats are `you`, an engine's id (`stockfish`, `reckless`, one you added) at full strength (or its fixed budget), and `<engine id>@<level>` for a level of that engine's ladder (`stockfish@5`, `reckless@3`; an engine with a fixed budget has none: 400). Clocks are written minutes+seconds (`5+3`).
- Local games are refereed by the app: clocks charged with measured think time on a monotonic clock, the increment added after each move, 250 ms grace; endings by checkmate, stalemate, repetition, fifty moves, insufficient material, time, engine failure or an illegal move; with `adjudicate`, engine games end early when decided. Engines get Hash 16, Threads 1 and MoveOverhead 0 unless asked otherwise.
- Seat actions are one set for every game: `move`, `offer_draw`, `accept_draw`, `decline_draw`, `takeback`, `accept_takeback`, `decline_takeback`, `resign`, `abort`, `claim_victory`, `claim_draw` (`POST /api/games/{id}/actions/{action}`). `Game.actions` lists what you may do now; anything else is 409, an illegal move 400. An engine accepts a draw from ply 20 when its own last score is at most +0.10.
- Games left live when the app stops: local ones are recorded aborted at the next start; Lichess ones are taken up again by the connection.

## Reviews, players and comparison

```bash
chess review <game>… | --player you [--limit 20 | --all] [--background --speeds blitz,rapid --rated true --engine stockfish --nodes 200000 --concurrency N] [--wait]
chess import you | lichess:<name> [--max 50 --since <ms> --speeds --rated] [--wait]   # you, with no flags: your whole history, then only new games
chess player <key> | profile <key> [--speeds --rated --since --limit] | compare you lichess:<name> engine:stockfish | players top [blitz] [--nb 10]
```

- A review is work. `--all` (`background: true`, the profile's "Review all N") reviews every game no engine has reviewed yet, newest first, at the lowest priority and in a lane of its own: a review asked for meanwhile runs at once, and profiles, drills and progress fill in as each game is done. A game reviewed by any engine counts as reviewed (changing the analysis engine re-reviews nothing). It searches every position with the analysis engine (or the one named; 200,000 nodes, MultiPV 2) through a position cache keyed by FEN without counters, the engine (its id and what it calls itself), nodes and MultiPV. Reviewing a game again with the same engine and budget skips it. Each game's positions are searched in the order played on one engine that keeps its hash table (a clean one for each run), idle engines split what another has left, and the next games are read ahead, so every granted core stays busy. A background review takes every free core by default (others: half) and gives them back within one search when it is paused, when you start playing on Lichess, or when work of higher priority asks for cores.
- Every review names what made it (`engine`: `Stockfish 19`; reviews made before the engines list read `Reckless <commit>`).
- The judge (`crates/core/src/review/judge.rs` holds every formula and threshold, with lila's test values): Lichess's win % (`50 + 50 × (2 / (1 + e^(−0.00368208 × cp)) − 1)`, capped at ±1000 cp), move accuracy, game accuracy (volatility-weighted and harmonic means), ACPL, judgements (a drop of 5, 10, 15 win % points is an inaccuracy, mistake, blunder; mates by lila's rules), classifications (`brilliant`, `great`, `best`, `excellent`, `good`, `book`, `inaccuracy`, `mistake`, `blunder`, `miss`), phases (lila's Divider) and time-trouble blunders (a blunder with under a tenth of the starting clock). Scores are White's point of view; win % and accuracy are the mover's.
- Player keys are `you`, `engine:<id>`, `lichess:<name>`. A profile adds up a player's games: results by colour, accuracy overall, per phase and by speed, ACPL, classifications per game, openings, time, the trend, moments, and for you **weak spots** (positions before your mistakes, recurring themes first). Results count every finished game; engine figures count reviewed games only (`pending_review` says how many wait).
- `compare` returns the profiles side by side with `leaders` per row (ties lead together). Other players' imports use your token when you are logged in, else none.

## The player side: levels, drills, repertoire, progress

```bash
chess levels [--engine reckless] | levels plan [--engine --count 3] | levels calibrate [--engine --count --games-per-pair] [--wait]
chess drills | drills today [--new-per-day --limit --theme hanging_piece --games 4,9 --max-ply 24] | drills hint <id> | drills make [--limit 20] | drills answer <id> <move> | drills delete <id> | drills trend
chess repertoire --side white [--max-plies 20 --min-games 2 --limit] | prep --side white | progress [--window 10]
```

- **Levels**: every engine that plays on the clock (Stockfish, Reckless, one you added) from about 800 to full strength, on the same ladder. Strength comes from a node budget (300 to 400,000 nodes a move), with a MultiPV softmax at a temperature for levels 1 to 6, so the moves are the engine's own, never random blunders; an engine without `MultiPV` plays each level's node budget alone (its best move at that budget), and Settings says so. Level 12 is full strength on the clock. Ratings are per engine (`level_ratings` and `level_games` are keyed by engine; `GET /api/levels?engine=reckless`, default the analysis engine), each starting from the nominal estimates. Calibration is per engine too (`levels calibrate --engine`, default the analysis engine): that engine's levels play each other in the background at the lowest priority (from four random plies), and a Bradley–Terry fit under priors is anchored on your Lichess rating and your games against that engine's levels. A game counts for an engine's ladder when every seat is you or one of its levels (a game between two engines' levels counts for neither). Until calibration has played, a level shows its nominal rating as an estimate.
- **Drills**: your mistakes as exercises, on SM-2 (a miss returns in a day; passes in 1, 6, then interval × ease days). Every line within 5 win % of the best counts as right. `drills today` hides the answers; `drills answer` returns them. Reviews of your games make drills. Each has a theme (`crates/core/src/themes.rs`: time trouble, king safety, hanging piece, missed tactic, opening error, endgame technique, else other), found from the position and the engine's line; profiles group weak spots by theme with cost per game. `drills hint` gives the kind of move, then the piece; a right answer after a hint is half a miss (back tomorrow, half credit). `--theme` or `--games` makes a session on a theme or opening line.
- **Repertoire**: your lines as White or Black from your games, how you score, and where you leave theory. It is assistance: refused while you play on Lichess. `prep` lists the named opening lines you play with score and early mistakes, flagged when you score badly or go wrong in the first twelve moves; same refusal.
- **Progress**: your rating, accuracy (rolling 10 games) and drill hit rate.

## Lichess

Your account, through the Board API. The token is a file in `secrets/` (0600 on Unix), never a row, and no route returns it. Nothing here creates an account or logs in by itself.

```bash
chess lichess                                   # your connection's status
chess lichess you [login | token < token.txt | logout]
chess lichess you seek --tc 10+5 [--rated --color white --range 1500-1800] | seek --stop
chess lichess you challenge <user> | --ai 1-8  --tc 5+3 [--rated --color black]
chess lichess you accept <id> | decline <id> [--reason later] | cancel <id>
```

`login` prints Lichess's OAuth link (PKCE, client id `chess`, scopes `board:play challenge:read challenge:write`); Lichess sends the browser back to `/api/connections/you/callback` (outside the guard and the OpenAPI document: the single-use `state` is its proof). A BOT account is refused. Your games are played on the board (offers, takebacks, abort, claims once the opponent left), and every game of yours, here or on lichess.org, turns fair play on. What the Board API allows (`crates/core/src/lichess/board.rs`): rapid, classical and correspondence; blitz too for direct challenges and games against the AI; never bullet. Lichess's speed is initial + 40 × increment, so a seek needs at least 8 minutes estimated, a challenge at least 3; a challenge's clock is 0, 15, 30, 45 or 90 seconds or whole minutes up to 180, with an increment of at most 180 seconds (60 against the AI). A challenge the Board API cannot play (`playable: false`) is played on lichess.org. Games against Lichess's AI are recorded against `engine:stockfish` at that level.

Errors from Lichess come back as 400 (it refused), 429 (rate-limited, with `Retry-After` when known) or 502 (unreachable). Lichess allows one request at a time per token and a minute's pause after a 429; the adapter keeps to it.

## Work

Reviews, imports, calibration and setup are **work**: queued, logged, stoppable, pausable, and resumed from their last checkpoint after a restart.

```bash
chess work [--recent 20] | work show <id> | work log <id> | work wait <id> | work stop|pause|resume <id>
```

- Live play comes first: a game with an engine seat reserves its threads, and live evaluation one core. When they oversubscribe the cores, the lowest-priority running work pauses (`paused_for: "A live game"`) at its next safe point and resumes when cores free up.
- Priorities, lower first: live, review (reviews asked for now, imports, setup), background (reviewing all of a player's games, calibration). Lanes run one item at a time each (calibration; reviews; background reviews). Network work (imports, setup) needs no cores and runs at once.

## Events

One server-sent event stream: `GET /api/events?since=<id>&topics=<t,…>` (`Last-Event-ID` resumes; `?token=` for `EventSource`), or `chess events --topics games,work`. Each event is `{ id, topic, kind, data }`. Topics: `game:<id>`, `games`, `work`, `player:<key>`, `connection:you`, `engines`, `policy`, `eval:<session>`; no `topics` means all but `eval:*` and game detail. Kinds: `game.upserted`, `game.moved`, `game.undone`, `game.changed`, `game.thinking`, `work.upserted`, `work.log`, `review.upserted`, `connection.upserted`, `challenge`, `policy.changed`, `eval.info`, `drill.upserted`, `drill.answered`, `drill.deleted`, `levels.updated` (naming its engine), `engines.changed` (both on topic `engines`); payloads are typed in the OpenAPI document. When events after `since` are no longer kept, a `reload` event tells the client to refetch. Scores everywhere are White's point of view.

## Working on Chess itself

```
apps/web   Vue 3.5 + eos + chessground (REST + SSE)
crates/
  core       pure domain: types, chess, review rules, referee rules, levels, engines, Lichess rules, events (no I/O)
  hub        the shared runtime: config, data dir, secrets, event bus, fair-play policy, core ledger
  store      SQLite: schema, numbered migrations, one writer, repositories
  engine     adapter: UCI processes, the engine pool, downloads of the built-in engines
  lichess    adapter: HTTP with Lichess's rate rules, OAuth PKCE, NDJSON streams, the Board API, import, explorer; MockLichess
  play       the Referee: game tasks, seats (engine, local, Lichess), your connection, imports, levels
  analysis   evaluation (warm engine pool, position cache), reviews, profiles, drills, repertoire, progress
  work       the scheduler, the engines list, setup, diagnostics
  server     axum API (utoipa → OpenAPI), SSE, /mcp, the wiring, the `chess` binary (serve + CLI)
skill/     the Claude Code skill for driving the app
```

- **Layering.** Dependencies point down the list. `core` does no I/O and is unit tested. `engine` and `lichess` are adapters with their own wire types and no dependency on `core`; `play`, `analysis` and `work` map them. `server` holds no domain logic: routes translate HTTP to service calls. `docs/design.md` has the seams between services.
- **Gates** before committing: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <crate>` for what you changed (the whole workspace before a merge); for the web, `corepack pnpm lint`, `format:check`, `typecheck`, `test`, `knip`.
- **The API.** Every route has a `utoipa` path. After changing a route or a type it returns, run `corepack pnpm api` and commit `crates/server/openapi.json` and `apps/web/src/api/schema.d.ts` (`corepack pnpm api:check` fails on stale files). Add the route's CLI command to `cli::COMMANDS`. Its MCP tool appears by itself; if the route acts on Lichess or the outside world, or destroys something, add it to `REACH` or `DESTRUCTIVE` in `crates/server/src/mcp/tools.rs` (the tests check those tables against the document).
- **Shared checkout.** Several agents work in this repository at once. Each owns its paths and commits only them: `git commit -m "…" -- <paths>`, never `git add -A`, never leave files staged. Build with `CARGO_TARGET_DIR=target-shared` (one cache for every agent; cargo locks it) or your own `target-<you>` (both ignored), and use your own port and data directory for a hand-run app.
- **Data directories live outside the repository** (for example `C:\dev\chess-<you>-data`). Delete yours when done. Tests use the fake UCI engines in the repository (`crates/engine/tests/support/fake_uci.rs`, `crates/analysis/tests/support/fake_eval.rs`, `crates/play/tests/support/fake_engine.rs`), never a download; for a hand run, add any UCI binary with `chess engines add`.
- **Never touch real accounts.** No agent logs in to Lichess, stores a real token or talks to lichess.org. Every Lichess path is tested against `chess_lichess::testing::MockLichess` (the crate's `mock` feature); a hand-run app gets `LICHESS_URL` set to the mock or an unreachable address (`http://127.0.0.1:9`).
- **Walking the Lichess flows by hand.** `cargo run -p chess-lichess --features mock --example mock_server [-- <port>]` starts the mock on `127.0.0.1:9797` (or `<port>`) and a control API on the next port. Run the app with `LICHESS_URL=http://127.0.0.1:9797`. The mock knows you as `Dame` (fake token `lip_mock_you`); its OAuth page grants every login as `Dame` at once, so Log in with Lichess works end to end. Opponents in games from challenges and seeks answer with their first legal move and take offers; bots Alpha, Beta, Gamma and Delta accept challenges; `Dame` has six games to import. The control API scripts the rest with JSON posts: `/games` (start a game, seats `you`, `user:<name>:<rating>`, `ai:<level>`), `/games/{id}/move {uci}`, `offer-draw`, `offer-takeback`, `accept-draw`, `accept-takeback`, `gone`, `back`, `end {status, winner}`, `/seeks/{id}/take`, `/challenges {from, to, clock: [300, 3]}` and `/challenges/{id}/accept|decline|cancel`, `/answers {user, answer}` and `/reply {reply, accepts_offers}`; `GET /state` lists games, challenges and seeks. The file's header (`crates/lichess/examples/mock_server.rs`) has the details.
- Doc comments say what a thing is and why, in plain sentences. No commented-out code, no TODOs without an owner.
