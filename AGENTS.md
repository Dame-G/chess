# Driving the harness

You work through the `chess` CLI, which talks to a running runner (`chess serve`). Every command takes `--json` and prints one JSON document to stdout; errors go to stderr with a non-zero exit code. Every command maps onto one HTTP route, listed in `runner/openapi.json`.

```bash
chess status --json            # machine, runner version, Reckless checkout
chess token                    # the token the hosted UI (or a remote client) needs

chess build <ref> --wait       # build a branch, tag or commit, bench it, print the build
chess branches                 # what can be built or tested: the fork's branches and upstream's, newest first
chess builds                   # every build: commit, bench (and whether it matches the commit's Bench line), nps
chess jobs | job <id> | log <id> | wait <id> | cancel <id>

chess analyse --fen "<fen>" --depth 20 --multipv 3 [--build <id>] [--moves e2e4 e7e5]
```

Builds and other heavy work are **jobs**: queued, run one at a time, each with a log. `chess build` prints the queued job; `--wait` (or `chess wait <id>`) blocks until it finishes and exits non-zero if it failed or was cancelled. A commit that is already built is not rebuilt.

A build's bench must equal the commit's `Bench:` line. When it differs, the build is suspect (non-determinism, or the commit message is stale): say so rather than testing with it.

## Experiments

An experiment asks one question: is `--dev` better than (or no worse than) `--base`? It runs a funnel of stages, and the first stage that fails stops it with its evidence.

```bash
chess experiment new --dev <branch> [--base main] --name "<short name>" --hypothesis "<what and why>" \
  [--kind gainer|nonregression|nonfunctional] [--ltc] [--wait]
chess experiment list [--batch <id>] | show <id> | wait <id> | cancel <id>

chess batch submit <file.json | -> --name "<name>" [--wait]   # a JSON array of experiments, as `experiment new` takes them
chess batch show <id> | wait <id>                              # one results table for the lot

chess test --dev <ref> [--base main] --preset stc [--bounds 0,3 | --pairs 500] [--wait]   # one match, no funnel
```

The funnels mirror Reckless's own OpenBench settings:

- **gainer:** bench must change, then a screen at VSTC 4+0.04 with SPRT [-5, 0] (rejects clear losses in minutes), then STC 8+0.08 [0, 3], then LTC 40+0.4 [0, 3] with `--ltc`.
- **nonregression** (simplifications, refactors that change search): the same, with STC at [-2.75, 0.25].
- **nonfunctional:** bench must be identical, then a paired speed comparison. No games.

Bounds are normalized Elo; the test is a pentanomial GSPRT with alpha 0.05 and beta 0.10. Presets for `chess test`: vstc, stc, ltc, stc-hash, ltc-hash (2 MB / 8 MB hash pressure), smp (6 threads) and nodes (25k per move). Games are in each test's PGN, with eval, depth, nodes and time per move: `GET /api/tests/<id>/games/<index>`.

How to work with them:

- Commit the change on a branch in the fork with an up-to-date `Bench:` line, then queue the experiment. Several independent ideas go in one batch; they run one after another.
- Pick the kind honestly. A change meant to be neutral (a simplification) is nonregression; only a change that cannot alter search is nonfunctional, and the bench stage checks it.
- An accepted STC result is evidence, not proof: a gain under about 2 Elo needs LTC before it is merged.
- A rejected screen means the idea, as written, loses. Read a few games and the nElo estimate before trying a variant.

Live progress is a server-sent event stream: `curl -N http://127.0.0.1:7878/api/events?since=<last id>`. Each event has an `id`, a `kind` and JSON `data`; pass the last id you saw to resume without gaps.

The engine is the Reckless fork in `RECKLESS_DIR` (default `../reckless`). Experiments are branches there. Every engine commit message ends with `Bench: <nodes>`, the node count of `reckless bench`: it is the engine's functional fingerprint, and CI checks it.

## Matches you can watch

Two builds play each other live, move by move; the board shows it at `/matches/<id>`, and finished games replay like any other. A match runs outside the job queue (so a long experiment never holds it up), one at a time, on two search threads (one per engine). It is stored in the database and survives a restart; one that was running when the runner stopped is kept as it stood, aborted.

```bash
chess match --white <build id|ref> --black <build id|ref> --tc 10+0.1 | --nodes N | --movetime MS \
  [--fen "<fen>" --moves e2e4 e7e5 | --book] [--hash 16] [--threads 1] [--adjudicate] [--minimal] [--follow]
chess matches [--limit 20]   # recent first
chess match show <id>        # headers, every move with eval, depth, time and clocks, the result
chess match stop <id>        # recorded as aborted
```

A ref means the newest build of it. `--follow` prints each move as it is played and then the result in words ("Black won on time"); with `--json` the moves go to stderr and stdout is the final match. `--book` starts from a random line of the opening book the tests use.

The routes: `POST /api/matches` (409 while another match runs), `GET /api/matches`, `GET /api/matches/{id}`, `DELETE /api/matches/{id}` (stops it) and `GET /api/matches/{id}/pgn` (a comment on every move like the test PGNs: `{+0.31/9 0.063s, n=9351, clk=9.85s}`). Request: `{ white, black, time: {type: "clock", seconds, increment} | {type: "nodes", nodes} | {type: "movetime", ms}, opening?: {type: "start"} | {type: "position", fen, moves} | {type: "book"}, hash?, threads?, adjudicate?, minimal? }`.

Events on `/api/events`:

- `match.started`: `{ id, match }`.
- `match.move`: `{ id, ply, side, uci, san, fen_after, clock_white_ms, clock_black_ms, eval, depth, nodes, time_ms }`. `eval` is the mover's own score.
- `match.info`: the thinking engine's progress, at most five a second: `{ id, ply, side, depth, seldepth, score, nodes, nps, pv }`. `ply` is the move being chosen, so a client replaying old events can ignore stale ones. Scores are from the thinking side.
- `match.ended`: `{ id, result, reason, status }`.

How games are run and ended:

- Clocks are charged with the measured think time on a monotonic timer, and the increment is added after each move. A move that takes longer than the clock holds plus 250 ms (as the tests allow, `timemargin=250`) loses on time.
- Engines get `Hash 16`, `Threads 1` and `MoveOverhead 0` as in the tests. `Minimal` is off by default, because with it on Reckless reports nothing while it thinks and there is nothing to watch; pass `minimal` to match the tests exactly. It does not change how the engine plays.
- Games end by checkmate, stalemate, threefold repetition, the fifty-move rule, insufficient material, time forfeit, an engine crash or an illegal move. With `adjudicate`, the tests' adjudication also applies: a side whose last three moves each scored -4 or worse loses, and a game within 0.10 for six moves each from move 30 is drawn.
- A match is stopped with `DELETE`; its result is `*` with reason `aborted`.

The game record (`runner/src/matches/mod.rs`) is a plain `Match` filled in by a player task. A later human-versus-build game can fill the same record by taking one side's moves from the board instead of an engine.

## The Lichess bot

One of our builds plays as the user's Lichess BOT account through the Bot API. The user makes the account and a personal token with the `bot:play` scope on lichess.org; nothing here creates either. The token is kept in the data directory as `lichess-token` (mode 0600 on Unix) and no route returns it; the settings and whether the bot was online are rows of the `meta` table, so a restarted runner comes back online by itself. `LICHESS_URL` (default https://lichess.org) points the runner at another server, such as the mock the tests use.

```bash
chess lichess                     # status: account, state, games being played, settings
chess lichess token < token.txt   # store the token, read from stdin (never argv); Lichess checks it
chess lichess logout              # forget it and go offline
chess lichess settings --build main --hash 64 --threads 1 --move-overhead 300 \
  --rated true --casual true --speeds bullet,blitz,rapid --initial 60-1800 --increment 0-30 \
  --from-position false --humans true --bots true --max-games 1   # each flag changes only itself
chess lichess start | stop        # online across restarts until stopped; stopping lets running games finish
chess lichess challenge <user> --tc 3+2 [--rated] [--color white|black|random]   # minutes+seconds, as Lichess writes it
chess lichess bots                # bots online now, with ratings
chess lichess upgrade --yes-irreversible   # make the account a BOT account: permanent, only for an account with no games
```

The routes: `GET /api/lichess` (`{ configured, account: {id, username, title, bot}, online, state: off|connecting|online|error, error, playing: [match ids], settings }`), `PUT /api/lichess/token` `{ token }` and `DELETE` (both answer the status), `GET`/`PUT /api/lichess/settings`, `POST /api/lichess/start`, `/stop`, `/upgrade` (body `{ "confirm": true }`, else 400), `POST /api/lichess/challenge` `{ username, clock_seconds, increment, rated, color }` (201, `{ id, url, username, rated, clock_seconds, increment, color }`; 409 while offline) and `GET /api/lichess/bots` (`[{ name, title, ratings: {bullet, blitz, rapid, classical} }]`). Errors from Lichess come back as 400 (it refused), 429 (rate-limited) or 502 (unreachable).

How it plays:

- The settings (`runner/src/lichess/rules.rs`) pick the build (an id, or a ref meaning its newest build when each game starts), hash (64), threads (1), `MoveOverhead` (300 ms, for the network) and the challenges taken: rated and casual, speeds (bullet, blitz and rapid by default; never correspondence), the initial clock and increment ranges, standard chess (and `fromPosition` if allowed), people and bots, and games at once (1). Anything else is declined with Lichess's reason: `variant`, `timeControl`, `tooFast`, `tooSlow`, `casual` (only rated wanted), `rated` (only casual wanted), `noBot`, `onlyBot`, `later` (busy), `generic` (no build).
- Each game has its own task and engine process, up from the first move to the last. The engine is given Lichess's clocks (`go wtime btime winc binc`); draw offers are ignored; when the opponent leaves, the win is claimed once Lichess allows; if the engine dies or answers an illegal move, the game is resigned (aborted before both sides have moved). Streams reconnect with backoff, and a 429 waits a minute.
- Every game is a `Match` with a `lichess` block `{ id, url, rated, speed, our_side }`, so `/matches/<id>`, the events and `/pgn` work as for local matches. The opponent's `Player` has no `build` but has `name`, `rating`, `title` and `lichess`; its moves carry no eval. Clocks are Lichess's. Lichess games run beside the one local match and never get or cause a 409; `GET /api/matches?source=lichess|local` lists one kind. `DELETE /api/matches/{id}` on a Lichess game resigns it, and the record ends when Lichess confirms. A game the bot comes back to (after a restart) continues in the same record.
- Lichess endings add reasons: `resigned`, `agreement`, `abandoned` (Lichess's `timeout`: left and claimed), `no_start`, `cheat`, `variant_end` and `other`; `aborted`, `time`, `checkmate`, `stalemate` and the draw rules map to the reasons above.

Events on `/api/events`:

- `lichess.status`: the whole status (as `GET /api/lichess`) whenever the connection, the account, the settings or the games being played change.
- `lichess.challenge`: `{ id, direction: incoming|outgoing, from: {name, title, rating}, to, rated, speed, variant, clock: {initial, increment} | null, decision: accepted|declined|canceled, reason }`. `reason` is the Lichess decline key, ours for incoming challenges and theirs for outgoing ones.

## Working on the harness itself

- `runner/` is Rust (axum, rusqlite, utoipa). After changing a route or a type it returns, run `corepack pnpm api` and commit the regenerated `runner/openapi.json` and `web/src/api/schema.d.ts`.
- `web/` is Vue with [eos](https://eos.dame.gg) components, imported by their own paths (`@eos-ui/vue/components/EButton`), never the barrel. The theme ("Saddle": colours, fonts) is `web/src/theme.ts`, built by eos's `defineTheme`. Build every control from eos; when a pattern is missing, it belongs in the eos library, not in a hand-rolled element here.
- Before pushing: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `corepack pnpm lint`, `format:check`, `typecheck`, `knip`, `test`.
