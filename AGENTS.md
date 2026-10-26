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

Bounds are normalized Elo; the test is a pentanomial GSPRT with alpha 0.05 and beta 0.10. The bench stage also fails when either build's bench differs from its commit's `Bench:` line.

**A passed screen is not evidence of a gain.** At [-5, 0] a change that does nothing passes about 90% of the time; the screen only throws out clear losses. The STC stage decides.

**Clocks are scaled to this machine, as OpenBench scales them**, so "8+0.08" means what it means upstream. Before each test's games (every stage measures afresh), the runner runs `reckless bench` on base and on dev with as many copies at once as the threads the games will use (games at once × threads per game), takes the mean nodes per second, and multiplies the clock's base time and increment by `reference / base's nps`, rounded to hundredths. The reference is Reckless's OpenBench value, 660,000 nps (its tests read "Scaling BASE 660000 NPS"). On a machine at 480k nps with every slot busy, STC is played as 11+0.11. Node limits (the `nodes` preset) and the nonfunctional speed stage are never scaled. The test records it as `scaling`: `{ copies, dev_nps, base_nps, measured_nps, reference_nps, scale, played }`, null when not scaled; the stage's detail ends "at 11+0.11 (×1.38)" and `chess experiment show` prints a `clock` line per test. Every bench copy must give the build's node count, or the test fails as non-deterministic. `CHESS_REFERENCE_NPS=<n>` in `chess serve`'s environment changes the reference; `CHESS_SCALE_CLOCKS=off` plays clocks as written (then results are not comparable with upstream's).

Presets for `chess test`: vstc, stc, ltc, stc-hash, ltc-hash (2 MB / 8 MB hash pressure), smp (6 threads) and nodes (25k per move). Games are in each test's PGN, with eval, depth, nodes and time per move: `GET /api/tests/<id>/games/<index>`.

How to work with them:

- Commit the change on a branch in the fork with an up-to-date `Bench:` line, then queue the experiment. Several independent ideas go in one batch; they run one after another.
- Pick the kind honestly. A change meant to be neutral (a simplification) is nonregression; only a change that cannot alter search is nonfunctional, and the bench stage checks it.
- An accepted STC result is evidence, not proof: a gain under about 2 Elo needs LTC before it is merged.
- A rejected screen means the idea, as written, loses. Read a few games and the nElo estimate before trying a variant.
- **Check the harness for false accepts** on a new machine, or after changing the runner, fastchess or the book. Play `main` against a branch with the same bench (a comment-only change, such as `test/comment`) at the gainer's bounds, which a change that does nothing passes at most 5% of the time:

  ```bash
  chess test --dev test/comment --base main --preset vstc --bounds 0,3 --wait   # expect rejected, Elo within its ± of 0
  chess test --dev test/comment --base main --preset vstc --pairs 2000 --wait   # optional: pentanomial roughly symmetric (LL≈WW, LD≈WD)
  ```

  Not an experiment: the bench stage would stop it, as the bench does not change. Not the screen either, which passes such a change most of the time. One run catches a gross bug (colour or order bias, a mixed-up dev and base); it cannot measure a 5% rate. An accept is a reason to stop and investigate before trusting any result.

Live progress is a server-sent event stream: `curl -N http://127.0.0.1:7878/api/events?since=<last id>`. Each event has an `id`, a `kind` and JSON `data`; pass the last id you saw to resume without gaps.

The engine is the Reckless fork in `RECKLESS_DIR` (default `../reckless`). Experiments are branches there. Every engine commit message ends with `Bench: <nodes>`, the node count of `reckless bench`: it is the engine's functional fingerprint, and CI checks it.

## Tuning

A tune moves a build's search constants by SPSA, as OpenBench does. The tunables are declared in the fork's `src/parameters.rs` with `define!` (`i32 name: default, min, max, c_end;`). Without the `spsa` cargo feature each one is a `const fn`, so the bench does not change. With it, each is a UCI spin option, and `reckless spsa` prints them in OpenBench's format. The branch `spsa/tunables` declares 38 of them: aspiration, RFP, NMP, ProbCut, SE, LMP, futility, LMR and history bonuses. A tune builds its ref with the feature; that build is cached separately, with `+spsa` in its `features`.

```bash
chess tune new --build spsa/tunables [--params rfp_quad,lmr_base] [--iterations 1000] [--pairs 8] \
  [--preset stc | --nodes 25000] [--hash 16] [--concurrency N] [--name "<name>"] [--wait]
chess tune list | show <id> | wait <id> | cancel <id> | resume <id>
chess tune tunables <build id>        # what an spsa build declares
chess tune apply <id> [--branch tune/<id>] [--base main] [--no-experiment] [--ltc] [--push] [--wait]
```

- **How it runs.** A tune is a job in the queue, so it waits behind tests. Each iteration plays `--pairs` game pairs of θ+ against θ−, with the tests' book and adjudication, and takes a step. To keep every core busy it runs like OpenBench's fleet of workers: the `--concurrency` slots are split into workers of two games (one pair at a time), each playing its own iteration's batch, drawn against θ as it stands when the worker frees up and applied to θ as it stands when the batch ends. So about `concurrency / 2` iterations are in flight at once, each a few updates behind, and `history` still has one entry per update, numbered in the order they were applied. The defaults are OpenBench's: alpha 0.602, gamma 0.101, A = 0.1 × iterations, `r_end` 0.002. `c_k` shrinks to `c_end` and `a_k / c_k²` shrinks to `r_end` at the last iteration. With no `--params`, every declared tunable is tuned. In the API, each entry of `params` can override `start`, `min`, `max`, `c_end` or `r_end`.
- **Its clock is scaled** like a test's, each time it starts or resumes: `--concurrency` × threads copies of the tuned build's bench, the clock times 660,000 / its nps (OpenBench scales tunes by dev's speed; here dev and base are one build). The tune's `scaling` holds it, with the clock its games are played at; `chess tune show` prints it. `--nodes` is not scaled.
- **What is kept.** Every iteration is stored: the values after it and its game pairs. `GET /api/tunes/{id}` returns `{ tune, history }`, and `history[i].values` follow the order of `tune.params`. A cancelled tune keeps its iterations, and so does one that was running when the runner stopped (that one is marked failed). `resume` carries on from the last iteration.
- **Applying a tune.** `apply` writes the current values, rounded, into `src/parameters.rs` as the new defaults, on a new branch made from the tune's commit. It builds that branch and commits with its `Bench:` line. Unless you pass `--no-experiment`, it also queues a gainer experiment against `--base`: bench, the screen, then STC at **[0, 4]** (and LTC at [0, 4] with `--ltc`), the bounds Reckless confirms its tunes with. Apply is a job. Its result is `{ tune, branch, commit, bench, experiment }`.
- **Events.** `tune.updated` sends the whole tune on each change of status. `tune.progress` sends `{ id, iteration, values, result, penta }` after each iteration, where `result` is θ+'s games won minus games lost. `tune.finished` sends `{ id, status, summary, iteration }`.
- **Rounding.** As OpenBench does, θ± are rounded stochastically (`floor(θ ± c_k + u)`, one `u` for both sides) and `c_k` never drops under 0.5, so the two sides always differ and average exactly θ ± c_k.
- **How long.** The default, 1,000 iterations of 8 pairs, is 16,000 games: enough to see that a tune works, short for 38 parameters. Reckless's own SPSA sessions run 95,000 to 200,000 games (its commits #956, #988, #1072). For a real tune of every tunable, use 5,000 iterations or more (`--iterations`), or tune fewer parameters. The schedule is fixed by the iteration count, so choose it at the start: a tune cannot be lengthened later.
- **What to trust.** A tune is only as good as its games. Even a long tune can drift on noise, so trust the [0, 4] confirmation, not the trajectory. Tune at the time control you care about, and use `--nodes` only to check that everything works.

## Matches you can watch

Two builds play each other live, move by move; the board shows it at `/matches/<id>`, and finished games replay like any other. A match runs outside the job queue (so a long experiment never holds it up), one at a time, on two search threads (one per engine). It is stored in the database and survives a restart; one that was running when the runner stopped is kept as it stood, aborted.

```bash
chess match --white <build id|ref|human> --black <build id|ref|human> --tc 10+0.1 | --nodes N | --movetime MS \
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

- Clocks are charged with the measured think time on a monotonic timer, and the increment is added after each move. A move that takes longer than the clock holds plus 250 ms (as the tests allow, `timemargin=250`) loses on time, or draws if the other side could never mate.
- Engines get `Hash 16`, `Threads 1` and `MoveOverhead 0` as in the tests. `Minimal` is off by default, because with it on Reckless reports nothing while it thinks and there is nothing to watch; pass `minimal` to match the tests exactly. It does not change how the engine plays.
- Games end by checkmate, stalemate, threefold repetition, the fifty-move rule, insufficient material, time forfeit, an engine crash or an illegal move. With `adjudicate`, the tests' adjudication also applies: a side whose last three moves each scored -4 or worse loses, and a game within 0.10 for six moves each from move 30 is drawn.
- A match is stopped with `DELETE`; its result is `*` with reason `aborted`. In a game you play it is a resignation once each side has moved.

You can take a seat yourself: `white` or `black` of `POST /api/matches` is a build id or `"human"` (at least one side must be a build; that player is `{name: "You", human: true}`). The board sends your moves, and the clock runs on you exactly as on an engine. From the CLI:

```bash
chess match --white human --black main --movetime 1000 [--follow]   # human works wherever a build or ref does
chess move <id> <move>       # UCI (e2e4, e7e8q) or SAN (e4, Nf3, O-O, exd8=Q); waits for the reply unless --no-wait
chess takeback <id>          # your last move and the reply, in a game without a clock
chess draw <id>              # offer a draw: "Draw by agreement" or "Draw declined"
chess match stop <id>        # resigns, once each side has moved
```

`chess move` converts SAN against the match's current position and prints your move and the engine's reply (with `--json`, the match after the reply). `--follow` on a game you play prints both sides' moves and takebacks as they happen, so it can run in one shell while you move from another.

The routes:

- `POST /api/matches/{id}/moves` `{ uci }`: your move, answered with the match once it is on the board. 400 for an illegal move or a match without a human, 409 when it is not your turn or the game is over.
- `POST /api/matches/{id}/takeback`: takes back your last move and the reply, on your turn, in a game without a clock (`movetime` or `nodes`). The `match.takeback` event carries `{ id, plies }`.
- `POST /api/matches/{id}/draw`: offers a draw on your turn; the engine takes it from ply 20 when its own last score is at most +0.10 (`{ accepted }`), and the game ends as `agreement`.

The game record (`runner/src/matches/mod.rs`) is a plain `Match` filled in by a player task, from an engine or, for a human seat, from the moves the board sends. The Lichess bot's games fill the same record through a `Recorder`.

## The Lichess bot

One of our builds plays as the user's Lichess BOT account through the Bot API. The user makes the account and a personal token with the `bot:play` scope on lichess.org; nothing here creates either. The token is kept in the data directory as `lichess-token` (mode 0600 on Unix) and no route returns it; the settings and whether the bot was online are rows of the `meta` table, so a restarted runner comes back online by itself. `LICHESS_URL` (default https://lichess.org) points the runner at another server, such as the mock the tests use.

```bash
chess lichess                     # status: account, state, games being played, bot games today, settings
chess lichess token < token.txt   # store the token, read from stdin (never argv); Lichess checks it
chess lichess logout              # forget it and go offline
chess lichess settings --build main --hash 64 --threads 1 --move-overhead 300 \
  --rated true --casual true --speeds bullet,blitz,rapid --initial 60-1800 --increment 0-30 \
  --from-position false --humans true --bots true --max-games 1 \
  --seek-clocks 3+0,3+2,5+3 --seek-rated true --seek-range 300   # each flag changes only itself
chess lichess start | stop        # online across restarts until stopped; stopping lets running games finish
chess lichess challenge <user> --tc 3+2 [--rated] [--color white|black|random]   # minutes+seconds, as Lichess writes it
chess lichess bots                # bots online now, with ratings
chess lichess seek [--always]     # find a game: challenge random online bots until one accepts (--always: after every game too)
chess lichess seek stop           # stop looking and withdraw the challenge waiting for an answer
chess lichess upgrade --yes-irreversible   # make the account a BOT account: permanent, only for an account with no games
```

The routes: `GET /api/lichess` (`{ configured, account: {id, username, title, bot}, online, state: off|connecting|online|error, error, playing: [match ids], settings, seek, bot_games_today, bot_limit_until }`), `PUT /api/lichess/token` `{ token }` and `DELETE` (both answer the status), `GET`/`PUT /api/lichess/settings`, `POST /api/lichess/start`, `/stop`, `/upgrade` (body `{ "confirm": true }`, else 400), `POST /api/lichess/challenge` `{ username, clock_seconds, increment, rated, color }` (201, `{ id, url, username, rated, clock_seconds, increment, color }`; 409 while offline) and `GET /api/lichess/bots` (`[{ name, title, ratings: {bullet, blitz, rapid, classical} }]`), and `POST /api/lichess/seek` `{ mode: once|always|off }` and `DELETE /api/lichess/seek` (both answer the status; 409 while offline or without a token). Errors from Lichess come back as 400 (it refused), 429 (rate-limited) or 502 (unreachable).

How it plays:

- The settings (`runner/src/lichess/rules.rs`) pick the build (an id, or a ref meaning its newest build when each game starts), hash (64), threads (1), `MoveOverhead` (300 ms, for the network) and the challenges taken: rated and casual, speeds (bullet, blitz and rapid by default; never correspondence), the initial clock and increment ranges, standard chess (and `fromPosition` if allowed), people and bots, and games at once (1). Anything else is declined with Lichess's reason: `variant`, `timeControl`, `tooFast`, `tooSlow`, `casual` (only rated wanted), `rated` (only casual wanted), `noBot`, `onlyBot`, `later` (busy), `generic` (no build).
- Each game has its own task and engine process, up from the first move to the last. The engine is given Lichess's clocks (`go wtime btime winc binc`); draw offers are ignored; when the opponent leaves, the win is claimed once Lichess allows; if the engine dies or answers an illegal move, the game is resigned (aborted before both sides have moved). Streams reconnect with backoff, and a 429 waits a minute.
- Every game is a `Match` with a `lichess` block `{ id, url, rated, speed, our_side }`, so `/matches/<id>`, the events and `/pgn` work as for local matches. The opponent's `Player` has no `build` but has `name`, `rating`, `title` and `lichess`; its moves carry no eval. Clocks are Lichess's. Lichess games run beside the one local match and never get or cause a 409; `GET /api/matches?source=lichess|local` lists one kind. `DELETE /api/matches/{id}` on a Lichess game resigns it, and the record ends when Lichess confirms. A game the bot comes back to (after a restart) continues in the same record.
- Looking for games (`runner/src/lichess/seek.rs`): a BOT cannot use the lobby or pools, so the bot challenges other bots. While `seek.mode` is not `off`, the bot is online, fewer than `max_games` games run and no challenge of ours is pending, it picks a clock at random from `seek_clocks` (default 3+0, 3+2, 5+3; each a valid Lichess challenge clock, no UltraBullet), takes our rating at that speed from `GET /api/account` (provisional or none counts as 1500; refreshed after each game), lists `GET /api/bot/online`, leaves out ourselves, bots without a rating at that speed and bots that declined or ignored us in the last hour, and challenges one at random within `seek_range` (300) of us, else within twice that, else anyone (`seek_rated`, default rated; colour random). A decline (`challengeDeclined`) or 25 s without an answer (then withdrawn with `POST /api/challenge/{id}/cancel`) moves on to another bot after 7 s. When the game starts, `once` turns off and `always` carries on 7 s after it ends. A 429 waits 61 s; nobody to challenge, 60 s. There is never more than one challenge of ours pending, and an incoming challenge that fills the last slot withdraws it. `always` is stored in `meta` and resumes with `chess serve`; stopping the bot, or forgetting its token, turns seeking off.
- The status's `seek` is `{ mode: off|once|always, pending: { id, url, opponent: {name, title, rating}, clock: {initial, increment}, rated } | null, note }`, where `note` is a short sentence on the last outcome for people ("Alpha declined (too fast). Trying another bot.").
- The daily bot-game limit (`runner/src/lichess/quota.rs`): Lichess lets a BOT play at most 100 games against other bots in any rolling 24 hours (lila's `BotLimit`, key `bot.vsBot.day`); games against people are unlimited. The bot counts the bot games it starts (`bot_games_today`, kept in `meta` across restarts) and at 100 stops challenging bots and declines their challenges as `later` (the `lichess.challenge` event's `detail` says why), while still taking people's; `bot_limit_until` (ISO, UTC) says when that ends, and the seek note says "Played 100 games against bots today; looking again at 14:32." Seeking stays on and resumes by itself. Lichess is the judge: a refusal with `bot.vsBot.day` (429 or 400) sets the pause to the reset it gives, and a bot over its own limit is left alone until its reset while another is tried. Other 429s still wait a minute.
- Lichess endings add reasons: `resigned`, `agreement`, `abandoned` (Lichess's `timeout`: left and claimed), `no_start`, `cheat`, `variant_end` and `other`; `aborted`, `time`, `checkmate`, `stalemate` and the draw rules map to the reasons above.

Events on `/api/events`:

- `lichess.status`: the whole status (as `GET /api/lichess`) whenever the connection, the account, the settings, the games being played or the search for games change.
- `lichess.challenge`: `{ id, direction: incoming|outgoing, from: {name, title, rating}, to, rated, speed, variant, clock: {initial, increment} | null, decision: accepted|declined|canceled, reason, detail }`. `reason` is the Lichess decline key, ours for incoming challenges and theirs for outgoing ones; `detail` is a few words more, or null.

## Your own Lichess account

Beside the bot, the user can log in with their own (human) Lichess account, and the runner becomes their Lichess client: the games they play are matches on the board, played through the Board API, and their history can be imported. The account is independent of the bot (both can be connected at once) and can never be the bot's: a BOT account, or the bot's own, is refused. The token is kept in the data directory as `lichess-me-token` (0600 on Unix), apart from the bot's, and no route returns it. Nothing here logs in by itself: the user does, in the browser.

```bash
chess lichess me                    # status: account and ratings, connection, games, challenges, seek, fair-play lock
chess lichess me login              # prints the Lichess login link (PKCE); the runner stores the token when Lichess answers
chess lichess me token < token.txt  # or a personal token (board:play, challenge:read, challenge:write), from stdin
chess lichess me logout             # revokes the token at Lichess and forgets it
chess lichess me seek --tc 10+5 [--rated] [--color white] [--range 1500-1800]   # rapid or slower; `seek stop` cancels
chess lichess me challenge <user> | --bot | --ai 1-8  --tc 5+3 [--rated] [--color black]   # blitz or slower
chess lichess me accept <id> | decline <id> [--reason later] | cancel <id>
chess lichess me import [--max N] [--since MS] [--wait]   # finished games as matches; later runs fetch only newer ones
chess move <match id> e4            # your move in a Lichess game, like any game you play here
```

Logging in (`runner/src/lichess/me/oauth.rs`): `POST /api/lichess/me/login {return_to?}` answers `{ url, expires_in_seconds }`. The URL is Lichess's `/oauth` with `client_id=dameless`, an S256 code challenge, the scopes `board:play challenge:read challenge:write` and a random `state`; the verifier and the state stay in the runner for ten minutes, single-use. Lichess sends the browser to `http://<the runner's loopback host>/api/lichess/me/callback`, which exchanges the code at `POST /api/token`, checks and stores the token, and redirects (303) to `return_to` with `?lichess=connected` or `?lichess=error&message=...`. `return_to` must be a page of the runner on a loopback address or of https://chess.dame.gg (default `/lichess` on the runner). The callback is outside the API's guard and the OpenAPI document: a browser arriving from lichess.org is cross-site with no token, and the state is the proof. A login lasts a year (Lichess's token lifetime); `expires_at` says when.

The routes:

- `GET /api/lichess/me`: `{ configured, account: {id, username, title, ratings: {bullet, blitz, rapid, classical}}, scopes, missing_scopes, expires_at, state: off|connecting|online|error, error, playing: [match ids], elsewhere: [{id, url, speed}], challenges: {incoming, outgoing}, seek: {seeking: {clock_seconds, increment, rated, color, rating_range} | null, note}, fair_play_lock }`. A challenge is `{ id, url, direction, opponent: {name, title, rating}, rated, speed, variant, clock: {initial, increment} | null, days, color (the colour you would play), playable }`.
- `PUT /api/lichess/me/token {token}` (400 if Lichess refuses it, it lacks `board:play`, or it is a BOT or the bot's account), `DELETE /api/lichess/me` (revokes at Lichess, then forgets).
- `POST /api/lichess/me/seek {clock_seconds, increment, rated, color, rating_range?}` and `DELETE` to cancel. The seek is a connection Lichess holds open until someone takes it; the game then starts.
- `POST /api/lichess/me/challenge {username | ai_level: 1-8 | my_bot: true, clock_seconds, increment, rated, color}` (201, the bot's `ChallengeSent` shape). AI games are casual and start at once; `my_bot` challenges the bot's account, which answers by its own settings.
- `POST /api/lichess/me/challenges/{id}/accept`, `POST .../decline {reason?}` (Lichess's keys: `generic`, `later`, `tooFast`, ...), `DELETE /api/lichess/me/challenges/{id}` (withdraw yours). All answer the status.
- `POST /api/lichess/me/import {max?, since?}` (202, a job). Errors from Lichess come back as 400, 429 or 502, as for the bot.

What Lichess allows through the Board API (its documentation and lila's `isBoardCompatible`): rapid, classical and correspondence; blitz too for direct challenges and AI games, never bullet. A game's speed is Lichess's estimate, initial + 40 × increment: under 3 minutes is bullet, under 8 blitz. So a seek must be at least 8 minutes (400 otherwise, pointing to a challenge for blitz), a challenge at least 3, and a challenge's clock is 0, 15, 30, 45 or 90 seconds or whole minutes up to 180, the increment at most 180 seconds (60 against the AI). Accepting a challenge the Board API cannot play (`playable: false`: bullet, a variant) is a 400: that one is played on lichess.org. Lichess asks for one request at a time and a minute's pause after a 429.

Playing (`runner/src/lichess/me/game.rs`):

- The runner follows the account's event stream while a token is stored, and comes back after a restart. Each game that starts and that the Board API plays (`compat.board`) is a `Match` with `lichess: { id, url, rated, speed, our_side, account: "me", opening, draw_offer, takeback_offer }`; your seat is a human `Player` (`human: true`, name = username, rating), the opponent has name, rating and title (Lichess's AI is "Stockfish level N"). Clocks are Lichess's; moves carry no eval. A correspondence game's `time` is `movetime` of its days per turn. Games that cannot be played here (started on lichess.org at bullet, variants) are listed in `elsewhere` and still lock analysis.
- The match routes act on it: `POST /api/matches/{id}/moves {uci}` plays the move through the Board API and answers once Lichess has it (409 when it is not your turn, 400 when illegal); `POST .../draw` accepts the opponent's offer (`{accepted: true}`) or offers one (`{accepted: false, offered: true}`), on either turn; `POST .../takeback` accepts the opponent's proposal or proposes one, clock or not; `DELETE /api/matches/{id}` resigns (aborts before both sides have moved). Only a Lichess game has `POST /api/matches/{id}/abort`, `/claim-victory`, `/claim-draw` (after the opponent left), `/decline-draw` and `/decline-takeback` (409 without an offer).
- A game between our two accounts (you against your bot) has two records, one per account; `GET /api/matches?account=me|bot` (with or without `source=lichess`) lists one account's games, played here or imported.

History (`runner/src/lichess/me/import.rs`): a job that starts at once beside the queue (it uses the network, not the CPU), listed, logged and cancelled like any job. It reads `GET /api/games/user/{username}` as NDJSON with clocks, evals and the opening, and stores each finished standard game as a finished match (`account: "me"`, `opening` such as "C20 King's Pawn Game", Lichess's evaluations where the game was analysed, as the mover's score). A Lichess game is stored once per account; the newest game's time is kept in `meta` (`lichess.me.import.since`), so a run without `since` fetches only newer games. The result is `{ imported, skipped, other, newest }`.

Analysis helpers (`runner/src/lichess/me/explorer.rs`):

- `GET /api/lichess/explorer?db=lichess|masters|player&fen&play&speeds&ratings&modes&since&until&moves&top_games&recent_games&player&color`: Lichess's opening explorer answer, as is. explorer.lichess.org now answers 401 without a login, so it is asked with your token: 409 when you are not logged in. The player explorer streams while it indexes; the last document within 8 seconds is returned.
- `GET /api/lichess/tablebase?fen`: tablebase.lichess.org's answer for at most 7 pieces (no login needed).
- Both are cached in memory (explorer ten minutes, tablebase a day). `LICHESS_EXPLORER_URL` and `LICHESS_TABLEBASE_URL` point them elsewhere; with `LICHESS_URL` set to a mock they default to `<mock>/explorer` and `<mock>/tablebase`.

Fair play, a hard rule: Lichess's terms forbid engines, opening books, tablebases and other help during a game; the explorer is allowed in correspondence only. While you have any game running on Lichess, `fair_play_lock` is true and the runner refuses, with 409 "Analysis is off while you're playing on Lichess": `POST /api/analyse`, `POST /api/analysis` (a live search running when a game starts is stopped), the tablebase, and the explorer (unless every running game is correspondence). Bot games, tests and local matches are unaffected. Do not work around it: no agent should analyse a position from a game the user is playing.

Events on `/api/events`:

- `lichess.me.status`: the whole status whenever the connection, the account, the games, the challenges or the seek change.
- `lichess.me.challenge`: `{ challenge, event: created|accepted|declined|canceled, reason }`.
- `lichess.me.fair_play`: `{ locked }` when the lock turns on or off.
- `match.offer`: `{ id, draw_offer, takeback_offer }` (each `white`, `black` or null) when an offer in your game changes; the record's `lichess.draw_offer` and `takeback_offer` hold the same.
- `match.opponent_gone`: `{ id, gone, claim_win_in_seconds }`: the opponent left (or came back), and when you may claim.
- `match.started`, `match.move`, `match.takeback` and `match.ended` as for any match.

## Game review and comparison

A review searches every position of a stored game (yours, the bot's, another player's, a local match) with one of our builds and judges each move. `compare` puts players side by side over their reviewed games. The code is `runner/src/review/` (`judge.rs` holds every formula and threshold below, as plain functions with tests) and `runner/src/lichess/players.rs`.

```bash
chess review <match id>... [--wait]               # with one id and --wait, prints the review
chess review --account me | --player <name> [--limit 20] [--speeds blitz,rapid] [--rated] [--build main] [--nodes 200000] [--concurrency N] [--wait]
chess review show <match id>                      # accuracy and counts per side, then every move
chess compare me <name> [bot] [--speeds blitz] [--rated] [--since 2026-01-01] [--limit 200]
chess lichess players import <name> [--max 50] [--speeds blitz,rapid] [--rated] [--wait]
chess lichess players top [blitz] [--nb 10]       # Lichess's leaderboard
chess lichess players show <name>                 # profile, and how many of their games are here
```

How a review runs:

- **A job in the queue** (it loads the CPU), behind tests and tunes. Default build: the newest build of `main`; default budget: **200,000 nodes per position**, `MultiPV 2`, Hash 16, one thread per engine, on half the default test concurrency (3 engines on the i7-10700K). Engines are started once per job and reused across positions and games; positions go to whichever engine is free.
- **Cost:** a 63-ply blitz game (64 positions) took 7.2 s on 3 engines here, about 0.35 s per position per core (depth 12 to 16), with the runner's other work going on. Fifty games is about six minutes.
- **Cache:** every search is kept in `position_evals` by FEN without the move counters, build, nodes and MultiPV, so shared positions (openings, mostly) are searched once. The engine gets the bare FEN, without the game's history.
- **Stored** in `reviews`, one row per game, build and node budget; reviewing again with the same build and budget skips the game. Lists (`GET /api/matches`) carry each game's newest review as `review` (`{ id, build, nodes, white: {accuracy, acpl, counts}, black }`).
- **Fair play:** a review is analysis. While you play on Lichess (`fair_play_lock`) `POST /api/reviews` and player imports answer 409, and a running review waits before its next position until the game ends (its log says "paused"). It never looks at the game being played: a running match is skipped.

Lichess's definitions (lila `AccuracyPercent.scala`, `AccuracyCP.scala`, `Advice.scala`; scalachess `WinPercent`; scalalib `Maths`), followed exactly:

- **Win%** of the side to move: `50 + 50 × (2 / (1 + e^(−0.00368208 × cp)) − 1)`, centipawns capped at ±1000, a mate counting as the cap.
- **Move accuracy** from the mover's win% before (the engine's best) and after: 100 if it did not drop, else `103.1668 × e^(−0.04354 × drop) − 3.1669 + 1`, within 0 and 100.
- **Game accuracy** per side: the mean of a volatility-weighted mean and the harmonic mean (values under 1 count as 1) of its move accuracies. Each move is weighted by the population standard deviation of the win% in a window around it (clamped 0.5 to 12); the window is a tenth of the game's positions, 2 to 8. lila's own test values pass (`review::tests::game_accuracy_matches_lilas_tests`). Lichess scores the start +0.15; we search it.
- **ACPL:** the mean of `max(0, before − after)` per move, each score capped at ±1000.
- **Judgements**, only for a move that is not the engine's best: a drop in winning chances (the −1..1 scale, so half the win% drop) of 0.1 is an inaccuracy, 0.2 a mistake, 0.3 a blunder (5, 10, 15 points of win%). Walking into a forced mate: inaccuracy if already 10 pawns down, mistake if 7, else blunder. Losing one's own forced mate: inaccuracy if still 10 pawns up, mistake if 7, else blunder. A slower mate is no judgement.

Our classifications (one per move, first match wins; chess.com's published descriptions of brilliant, great and miss as the model, made deterministic):

| Classification | Rule |
|---|---|
| `miss` | Lichess judges it a mistake or blunder, the opponent's previous move was a mistake or blunder, win% before ≥ 60 and after ≥ 30: the chance was let go, the game not lost. |
| `blunder`, `mistake`, `inaccuracy` | Lichess's judgement. |
| `book` | Within the named opening: ply ≤ Lichess's `opening.ply` for the game (`lichess.opening_ply`, kept by imports; games imported before it came need `chess lichess me import --since 0` to get it). |
| `brilliant` | The engine's best or within 2 win% of it; gives up ≥ 2 pawns of material (N = B = 3, R = 5, Q = 9: an exchange, or a piece for a pawn) along the engine's best reply: the mover's balance before, less the balance after the move and six more plies of the engine's line from the new position (longer while it keeps capturing, at most ten); win% before < 85 (not already winning); win% after ≥ 50; more than one legal move. |
| `great` | The engine's best or within 2 win% of it; the second-best line is ≥ 15 win% worse than the best (every other move a blunder); win% after ≥ 40; not a recapture on the square just captured on; more than one legal move. |
| `best` | The engine's best move. |
| `excellent` | Within 2 win% of the best. |
| `good` | Anything else that is not an inaccuracy. |

`judgements` in a summary are Lichess's counts (a miss counts as its mistake or blunder), comparable with Lichess's own analysis.

**Phases**, per position, never going back: **endgame** once at most 6 queens, rooks, bishops and knights are on the board (lila's `Divider` rule); **middlegame** once at most 10, or a side's back rank holds fewer than 4 of its own pieces, or 30 plies are played; **opening** before. A move belongs to the phase of the position it was played from. A phase's accuracy is the game accuracy of its positions, from the one before its first move (lila's `phaseAccuracies`).

**Time:** think time per move comes from the clocks (Lichess records none for each side's first move). A **time-trouble blunder** is a blunder (judgement) played with less than a tenth of the starting clock.

The routes:

- `POST /api/reviews {matches: [ids] | account: "me"|"bot" | player: name, limit? (20), speeds?, rated?, build? (id or ref), nodes?, concurrency?}` → 202, a job (`kind: "review"`, `{matches, build, nodes, concurrency}` resolved when queued). Result `{ reviewed, skipped, failed, positions_searched, positions_cached, build, nodes }`. 404 for an unknown match, 409 for no build or the fair-play lock.
- `GET /api/matches/{id}/review?build=&nodes=` → the newest review (or the one asked): `{ id, match_id, build, nodes, created_at, summary, plies }`. 404 without one.
  - `summary`: `{ build, nodes, white, black, middlegame_ply, endgame_ply }`; a side is `{ moves, accuracy, acpl, counts: {brilliant, great, best, excellent, good, book, inaccuracy, mistake, blunder, miss}, judgements: {inaccuracy, mistake, blunder}, phases: {opening, middlegame, endgame: {moves, accuracy, acpl, timed_moves, time_ms}}, time_trouble_blunders }`.
  - each ply: `{ ply, side, uci, san, eval_before, eval_after, best_uci, best_san, best_pv, second_uci, eval_second, win_before, win_after, accuracy, cp_loss, classification, judgement, phase, sacrifice, clock_before_ms, depth }`. Scores are White's `{cp}` or `{mate}`; `mate: 0` after the last move means the side to move is checkmated. Win% and accuracy are the mover's.
- `GET /api/compare?players=me,<name>,bot&speeds=blitz,rapid&rated=true&since=2026-01-01&limit=200` → `{ players: [...], speeds, rated, since, limit }`. Each player: `player, username, title, rating` (from the newest game), `games, reviewed, pending_review, first_game, last_game`, `results: {overall, white, black: {games, wins, draws, losses, score}}`, `accuracy: {overall, opening, middlegame, endgame}` (mean of game accuracies), `acpl`, `by_speed: [{speed, games, reviewed, results, accuracy, acpl}]`, `counts` and `per_game` (each classification per reviewed game), `judgements`, `opponents: {average_rating, performance}` (performance = opponents' average + 400 × (wins − losses) / games), `openings` (top 8 families by games, ECO code and variation dropped, with results), `time: {games, average_ms, opening_ms, middlegame_ms, endgame_ms, time_trouble_blunders, time_trouble_blunders_per_game}`, `longest_win_streak`, `notable: {brilliant: [moment], blunders: [moment], most_accurate_game, least_accurate_game}`. A moment is `{ match_id, ply, san, side, classification, win_before, win_after, sacrifice, opponent, played_at, url }`: open `/matches/<match_id>` at `ply`. Results count every finished game; everything from the engine counts reviewed games only.
- `POST /api/lichess/players/{username}/import {max? (50, at most 1000), speeds?, rated?}` → 202, a job beside the queue (network only): `GET /api/games/user/{username}` with clocks, evals and the opening, sent with your token when you are logged in, else anonymously (never the bot's). Games are matches with `lichess.account: "player"`, `lichess.player: <id>` and `our_side` = that player's side; no seat is `human`. Each Lichess game is stored once per player. Result `{ player, imported, skipped, other }`. These games stay out of `GET /api/matches` unless asked for with `?player=<name>` (or `account=player`), so the board's lists are not flooded.
- `GET /api/lichess/players/top?perf=blitz&nb=10` → `[{ id, username, title, rating, progress }]`, Lichess's leaderboard (`/api/player/top/{nb}/{perf}`), cached ten minutes.
- `GET /api/lichess/players/{username}` → `{ id, username, title, url, ratings: {bullet, blitz, rapid, classical: {rating, games, provisional}}, counts: {all, rated, wins, draws, losses}, flag, created_at, seen_at, disabled, imported, reviewed }`, cached ten minutes; 404 when Lichess has no such player.

Events: `review.progress` `{ job, done, total, match, review, paused }` after each game (`review` is the brief, or null when skipped or failed) and when the review pauses for or resumes after a game of yours; `job.updated` and `job.log` as for any job.

What a UI should show: in game lists, each side's accuracy (and a ?? count); on the board, the classification glyph on each move, the eval graph from `eval_after`, the best move as an arrow (`best_uci`, `best_pv`) where the move was not the best, and the summary per side and phase; on the compare screen, the players as columns with accuracy, classifications per game, results by colour, openings, time and the notable moments as links into the board, plus "N games not reviewed yet — Review them" (a `POST /api/reviews` with `player` or `account`).

## Versions and tidying

A branch is a version of the engine. `GET /api/versions` gives one entry per branch (the fork's and upstream's, as `chess branches`), newest commit first: its builds (each with `on_disk`), `latest_build_id`, the `verdict` of its latest experiment as dev (status, summary, the last match's Elo estimate) and the base it was `tested_against`, its latest `tune`, `tunables`, `on_lichess`, `is_main` and `hidden`. A build belongs to a branch when it was built from that name or is of its tip commit.

```bash
chess versions [--all]                              # one line per version; --all includes hidden ones
chess version hide <name> | show <name>             # hidden versions stay buildable and testable
chess version delete <name> [--remote] [--builds] --yes
chess build delete <id>                             # delete a build's binary; the record stays
chess build prune [--older-than-days N] [--all] [--dry-run]   # binaries nothing uses; keeps each branch's newest unless --all
```

- Deleting a build removes its binary and sets `deleted_at`; experiments, tests, tunes and matches still show it, and the next build of its commit is a new build. A deleted build cannot play, analyse or be the Lichess build. 409 while a running experiment, test, tune or match uses it, or the Lichess bot plays it.
- Deleting a version deletes the fork branch locally (`--remote`: on `origin` too, first) and needs `{ "confirm": "<name>" }` (`--yes`). It refuses `main`, upstream branches and a branch a queued or running experiment or tune uses (400), and the Lichess bot's branch or the checked-out one (409). Records that used it stay. Only delete a branch the user asked to delete.
- Routes: `GET /api/versions`, `PUT /api/versions/{name}/hidden {hidden}`, `DELETE /api/versions/{name}?remote=&builds=` with the confirm body, `DELETE /api/builds/{id}`, `POST /api/builds/prune {older_than_days?, keep_latest_per_branch = true, dry_run}`. A name with a `/` goes in a path as `%2F`.
- Event: `versions.changed` `{ reason: hidden|deleted|build.deleted, ... }` after these. New builds, verdicts and tunes come as `job.updated`, `experiment.updated` and `tune.*`.

## Working on the harness itself

- `runner/` is Rust (axum, rusqlite, utoipa). After changing a route or a type it returns, run `corepack pnpm api` and commit the regenerated `runner/openapi.json` and `web/src/api/schema.d.ts`.
- `web/` is Vue with [eos](https://eos.dame.gg) components, imported by their own paths (`@eos-ui/vue/components/EButton`), never the barrel. The theme ("Saddle": colours, fonts) is `web/src/theme.ts`, built by eos's `defineTheme`. Build every control from eos; when a pattern is missing, it belongs in the eos library, not in a hand-rolled element here.
- Before pushing: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `corepack pnpm lint`, `format:check`, `typecheck`, `knip`, `test`.
