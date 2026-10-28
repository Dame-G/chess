# Dameless

**Two players, one board.** You (the user) and your engine, Dameless, both get stronger by playing games and learning from them. Dameless is a fork of [Reckless](https://github.com/codedeliveryservice/Reckless). The app plays games (you against Dameless, you on Lichess, Dameless on Lichess, two versions of Dameless in a test), reads them (reviews, the eval graph, the better move) and adds them up into verdicts on strength (an idea's trials, your profile, a comparison).

`docs/design.md` is the contract for every crate, route, event, table and screen; `docs/roadmap.md` says what comes in what order. This file is how an agent drives the app and works on it. The `dameless` skill (`skill/dameless/`) is the short version for Claude sessions that drive the app.

## Words

One vocabulary everywhere (code, routes, CLI, UI, docs): **you**, **Dameless**, **version** (a commit that matters: `v14`, `deeper-lmr`), **champion** (the current best version, the tip of the fork's `main`, numbered `v1`, `v2`, … on the champion line), **idea** (a hypothesis and a change; kinds gain, neutral, speed, tune, net; ends kept or dropped), **trial** (one stage of an idea), **game**, **player**, **work** (anything running in the background), **review**, **connection** (a Lichess account linked to the app: `you` or `bot`). In the UI, never: build (except diagnostics), branch (except the idea form), match, experiment, test, batch, job, runner, human.

## Running it

```bash
cargo build --release -p dameless          # target/release/dameless; put it on PATH, or use `cargo run -p dameless --`
dameless serve                             # http://127.0.0.1:7979: the API, the event stream, /mcp and the web UI
dameless status                            # any other subcommand is a client of the running app
```

The web UI is `apps/web/dist` (`corepack pnpm build`), read from disk in debug builds and embedded in release. For UI work, `corepack pnpm dev` serves it with hot reload against a running app.

| Variable                                          | Default                                         | What                                                                                                                      |
| ------------------------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `DAMELESS_LISTEN`                                 | `127.0.0.1:7979`                                | Address and port.                                                                                                         |
| `DAMELESS_DATA` (`--data`)                        | the platform's local data folder + `/dameless`  | `dameless.db`, `token`, `secrets/lichess-you`, `secrets/lichess-bot`, `builds/`, `games/`, `tools/`, `logs/`, nets, data. |
| `RECKLESS_DIR`                                    | `../reckless`                                   | The fork's checkout. Builds happen in git worktrees under the data directory, never in the checkout.                      |
| `DAMELESS_CORES`                                  | physical cores − 1                              | Cores the scheduler hands out.                                                                                            |
| `DAMELESS_GPUS`                                   | 1 when `nvidia-smi -L` lists a GPU, else 0      | GPUs for training.                                                                                                        |
| `DAMELESS_DATA_CAP_GB`                            | 10 on Windows, 200 elsewhere                    | What every dataset together may hold.                                                                                     |
| `DAMELESS_SCALE_CLOCKS`, `DAMELESS_REFERENCE_NPS` | on, 660000                                      | Clock scaling (see Ideas and trials). `off` plays clocks as written; results are then not comparable with upstream's.     |
| `LICHESS_URL`                                     | `https://lichess.org`                           | Lichess. `LICHESS_EXPLORER_URL` and `LICHESS_TABLEBASE_URL` point those elsewhere (with a mock: `<mock>/explorer`, …).    |
| `DAMELESS_TRAINER_DIR`                            | this repository's `trainer/`                    | The trainer's source.                                                                                                     |
| `CUDA_PATH`                                       | unset                                           | bullet is built and run only with it.                                                                                     |
| `DAMELESS_URL`, `DAMELESS_TOKEN` (CLI)            | `http://127.0.0.1:7979`, the data dir's `token` | Where the CLI finds the app and its token.                                                                                |

`dameless setup` downloads fastchess and the opening book the trials use (a few hundred MB; explicit). `dameless diagnostics` lists the machine's health: fastchess, the book, the engine checkout, clang, cargo-pgo and llvm-profdata (PGO builds), CUDA, the GPU, disk, the database, builds and the harness, each with what to do when it fails.

## Driving it: the CLI and MCP

Every action is a route, a CLI command and an MCP tool with the same name. `dameless::cli::COMMANDS` maps every operation id to its command, and a test checks it against the OpenAPI document (`crates/server/openapi.json`, `dameless openapi`).

- **The token.** Every request but `GET /api/health` carries `Authorization: Bearer <token>`. `dameless token` prints it from the data directory. The app's own page needs none; the hosted UI (https://chess.dame.gg) is given it once, by hand. Requests must name a loopback `Host` and come from no other website's `Origin`.
- **The CLI.** Every command takes `--json` (one JSON document on stdout; errors on stderr with a non-zero exit). `--wait` blocks on work and ideas and exits non-zero unless they succeed; `--follow` prints a game's moves as they are played. Lichess tokens are read from stdin, never argv. `dameless <command> --help` lists every flag.
- **MCP.** `POST /mcp` is a Streamable HTTP MCP server (rmcp, stateless: the 2026-07-28 protocol, and older clients' `initialize` too), behind the same token and guard. Connect Claude Code with:

  ```bash
  claude mcp add --transport http dameless http://127.0.0.1:7979/mcp --header "Authorization: Bearer $(dameless token)"
  ```

  There is one tool per OpenAPI operation, named by its operation id and generated at startup from the document (`crates/server/src/mcp/tools.rs`): its description is the route's summary and description plus the CLI command; its input is the path and query parameters under their names and the JSON body under `body`. A call goes through the app's own router, so it does exactly what the route does and answers with the route's JSON (an error is a tool error with the status and the error body). The event stream and `GET /api/token` are left out. `wait_for_work` and `wait_for_idea` wait (looking every two seconds, up to `timeout_seconds`, default 600) and answer `{ finished, work | idea }`. Tools carry annotations: GETs are read-only; tools that act on Lichess or the outside world are open-world and say so in their description; deleting, stopping, resigning, promoting, rewording and pushing are destructive. The server's instructions carry the fair-play and real-account rules below.

## Two rules

**Fair play.** Lichess forbids engines, opening books, tablebases and other help during a game; its explorer is allowed in correspondence only. While you have any game running on Lichess (played here or on lichess.org), the policy (`status.policy`, `policy.changed` events) is `off` (or `explorer_only` when every live game is correspondence), and the server refuses assistance with 409 `fair_play` ("Analysis is off while you're playing on Lichess"): evaluation (`/api/eval`; a running session is stopped), the explorer, the tablebase, reviews (new ones refused, a running one pauses before its next position), the repertoire, and other players' imports. Bot games, trials, local games against Dameless and building are never assistance. The UI hides these controls; the 409 is the backstop. No agent works around it, and no agent analyses a position from a game the user is playing.

**Real accounts.** The app acts on the user's real Lichess accounts and on GitHub. An agent challenges, seeks, accepts, declines, takes the bot online or makes it seek, changes the bot's settings, stores or forgets a token, moves or resigns in a Lichess game, pushes, or downloads datasets only when the user asked for that action. The user logs in and makes tokens themselves. In a Lichess game, a move is the user's own move, never one an engine chose.

## Games

Every game is one record wherever it was played: local, Lichess (yours or the bot's), imported history, trial games.

```bash
dameless games [--player you --source local,lichess,import,trial --status --speed --rated --trial <id> --limit 20]
dameless game <id> [--pgn | --review [--version --nodes] | --follow]
dameless play --white you --black dameless@5 --tc 5+3 | --nodes N | --movetime MS [--fen … --moves e2e4 e7e5 | --book] [--hash --threads --adjudicate] [--follow]
dameless move <id> e4 [--no-wait]          # UCI or SAN; waits for the reply
dameless draw <id> [--accept|--decline] | takeback <id> [--accept|--decline] | resign <id> | abort <id> | claim <id> [--draw]
dameless stop <id>                         # a game you sit in is resigned (aborted before each side moved); an engine game aborted
dameless import --pgn game.pgn [--you white]
```

- Seats are `you`, `dameless` (the champion at full strength on the clock), `dameless:<version>`, and `@<level>` after either Dameless form (`dameless@5`). Clocks in games are written minutes+seconds (`5+3`); trial presets seconds+seconds (`8+0.08`).
- Local games are refereed by the app: clocks charged with measured think time on a monotonic clock, the increment added after each move, 250 ms grace; endings by checkmate, stalemate, repetition, fifty moves, insufficient material, time, engine failure or an illegal move; with `adjudicate`, the trials' adjudication. Engines get Hash 16, Threads 1 and MoveOverhead 0 unless asked otherwise.
- Seat actions are one set for every game: `move`, `offer_draw`, `accept_draw`, `decline_draw`, `takeback`, `accept_takeback`, `decline_takeback`, `resign`, `abort`, `claim_victory`, `claim_draw` (`POST /api/games/{id}/actions/{action}`). `Game.actions` lists what you may do now; anything else is 409, an illegal move 400. An engine accepts a draw from ply 20 when its own last score is at most +0.10.
- Games left live when the app stops: local ones are recorded aborted at the next start; Lichess ones are taken up again by their connection.

## Reviews, players and comparison

```bash
dameless review <game>… | --player you [--limit 20 --speeds blitz,rapid --rated true --version champion --nodes 200000 --concurrency N] [--wait]
dameless import you | lichess:<name> [--max 50 --since <ms> --speeds --rated] [--wait]
dameless player <key> | profile <key> [--speeds --rated --since --limit] | compare you lichess:<name> dameless | players top [blitz] [--nb 10]
```

- A review is work (priority before ideas). It searches every position with a version (default: the champion, 200,000 nodes, MultiPV 2) through a position cache keyed by FEN without counters, commit, nodes and MultiPV. Reviewing again with the same version and budget skips the game.
- The judge is the old harness's, ported as is (`crates/core/src/review/judge.rs` holds every formula and threshold, with lila's test values): Lichess's win % (`50 + 50 × (2 / (1 + e^(−0.00368208 × cp)) − 1)`, capped at ±1000 cp), move accuracy, game accuracy (volatility-weighted and harmonic means), ACPL, judgements (a drop of 5, 10, 15 win % points is an inaccuracy, mistake, blunder; mates by lila's rules), classifications (`brilliant`, `great`, `best`, `excellent`, `good`, `book`, `inaccuracy`, `mistake`, `blunder`, `miss`), phases (lila's Divider) and time-trouble blunders (a blunder with under a tenth of the starting clock). Scores are White's point of view; win % and accuracy are the mover's.
- Player keys are `you`, `dameless`, `lichess:<name>`, `stockfish:<n>`. A profile adds up a player's games: results by colour, accuracy overall, per phase and by speed, ACPL, classifications per game, openings, time, the trend, moments, and for you **weak spots** (positions before your mistakes, recurring themes first). Results count every finished game; engine figures count reviewed games only (`pending_review` says how many wait).
- `compare` returns the profiles side by side with `leaders` per row (ties lead together). Other players' imports use your token when you are logged in, else none; never the bot's.

## The player side: levels, drills, repertoire, progress

```bash
dameless levels | levels plan [--count 3] | levels calibrate [--count --games-per-pair --version] [--wait]
dameless drills | drills today [--new-per-day --limit] | drills make [--limit 20] | drills answer <id> <move> | drills delete <id> | drills trend
dameless repertoire --side white [--max-plies 20 --min-games 2 --limit] | progress [--window 10]
```

- **Levels**: Dameless from about 800 to full strength. Strength comes from a node budget (300 to 400,000 nodes a move), with a MultiPV softmax at a temperature for levels 1 to 6, so the moves are the engine's own, never random blunders. Level 12 is full strength on the clock. Ratings are calibrated by the app: levels play each other in the background at the lowest priority, and a Bradley–Terry fit under priors is anchored on your Lichess rating and your games against levels. The ladder and the reasons for its numbers are in `docs/roadmap.md` (R3).
- **Drills**: your mistakes as exercises, on SM-2 (a miss returns in a day; passes in 1, 6, then interval × ease days). Every line within 5 win % of the best counts as right. `drills today` hides the answers; `drills answer` returns them. Reviews of your games make drills.
- **Repertoire**: your lines as White or Black from your games, how you score, and where you leave theory. It is assistance: refused while you play on Lichess.
- **Progress**: your rating, accuracy (rolling 10 games) and drill hit rate, beside Dameless's cumulative Elo over `v1`.

## Ideas and trials

An idea asks one question: is its branch better than (or no worse than) its base, the champion by default? It runs a funnel of trials, and the first that fails stops it with its evidence.

```bash
dameless ideas [--status --tag --limit] | idea show <id> | idea trial <id> <trial> | idea stop <id> | idea resume <id>
dameless idea new --kind gain --branch <branch> --title "…" --hypothesis "…" [--base v14 --author claude --tags a,b --notes "…" --long --fix-bench] [--wait]
dameless idea batch ideas.json --tag <tag>     # a JSON array of `idea new` bodies, or `-` for stdin
dameless idea check-branch <branch> | idea fix-bench <branch> [--commit <sha>]
dameless test --dev <branch> [--base main] --preset vstc [--bounds 0,3 | --pairs 2000] [--wait]   # a custom idea: one trial
```

The funnels copy Reckless's OpenBench (`crates/core/src/ideas.rs`):

| Kind    | Stages                                                                                                                     |
| ------- | -------------------------------------------------------------------------------------------------------------------------- |
| gain    | fingerprint (bench must change) → screen VSTC 4+0.04 [-5, 0] → short STC 8+0.08 [0, 3] → long LTC 40+0.4 [0, 3] (`--long`) |
| neutral | fingerprint (bench must change) → screen VSTC [-5, 0] → short STC [-2.75, 0.25] → long LTC [-2.75, 0.25] (`--long`)        |
| speed   | fingerprint (bench must be identical) → speed (5 paired bench runs; fails when measurably slower)                          |
| tune    | tune (SPSA writes the change) → fingerprint → screen → confirm STC [0, 4] → long LTC [0, 4] (`--long`)                     |
| net     | train (a run makes the net) → fingerprint → screen → short STC [0, 3] → long LTC [0, 3] (`--long`)                         |

- **Statistics** are the old harness's, ported as is: a pentanomial GSPRT with alpha 0.05 and beta 0.10 on normalized Elo bounds, nElo, and logistic Elo with a t-interval. Trials play in rounds (about 12 × cores game pairs each, with the cores granted at the round's start) and check the SPRT after each round and on the running counts. A round is **wound down** early, keeping every pair whose two games finished, as soon as the SPRT decides, a pause is asked (a live game, or you), work of higher priority waits for cores, or the trial is stopped: long rounds keep the slots busy (each round restarts fastchess, and short rounds' tails wasted about 5 % of slot time) without making anyone wait them out. Rounds are checkpoints: a restart resumes from the last finished one.
- **Adjudication** is OpenBench's with one deviation: resign after 3 moves at 300 cp or worse (upstream uses 400). Replaying 17,944 trial games, that cut 6 % of the plies and changed 0.25 % of results. Draw adjudication is upstream's (from move 30, 6 moves within 10 cp).
- **Presets**: `vstc` 4+0.04, `stc` 8+0.08 (16 MB), `ltc` 40+0.4 (64 MB), `stc_hash` (2 MB hash), `ltc_hash` (8 MB), `smp` (40+0.4, 6 threads, 512 MB), `nodes` (25,000 nodes a move).
- **Clocks are scaled to this machine, as OpenBench scales them**, so "8+0.08" means what it means upstream. At each trial's start the app runs `reckless bench` on base and on dev with as many copies at once as the threads the games will use, takes the mean nps, and multiplies the clock's base time and increment by 660,000 / base's nps, rounded to hundredths (on a machine at 480k nps, STC is played as 11+0.11). Every copy must give the build's node count, or the trial fails as non-deterministic. The trial records `time` (as written), `played` and `scaling`. Node limits and the speed stage are never scaled.
- **`Bench:` lines.** Every engine commit message ends with `Bench: <nodes>`, the node count of `reckless bench`: the engine's functional fingerprint, which Reckless's CI checks. The fingerprint stage fails when either build's bench differs from its commit's line (a stale line, or non-determinism). An idea whose branch has a missing or stale line is refused with 409 unless created with `fix_bench`, which builds the tip and rewords it locally with the measured bench. `check-branch` never builds; `fix-bench` refuses `main`, upstream branches and a branch that moved since its check.
- **Reading verdicts.** A passed screen is not evidence of a gain: at [-5, 0] a change that does nothing passes about 90 % of the time; the screen only throws out clear losses, and the short trial decides. A kept STC result is evidence, not proof: a gain under about 2 Elo needs the long trial before it is merged. A dropped screen means the idea, as written, loses: read a few of its games (`dameless games --trial <id>`) and the nElo before trying a variant.
- **Authors and notes.** `author: claude` and markdown `notes` (the reasoning) are kept, so the logbook says who tried what and why. Batches are tags.
- How to work: commit each idea on its own branch of the fork with an up-to-date `Bench:` line, and queue independent ideas together as a batch. Pick the kind honestly: a change meant to be neutral is `neutral`; only a change that cannot alter search is `speed`, and the fingerprint checks it.

## Checking the harness

Before trusting results on a new machine, or after changing fastchess, the book or how trials run: `dameless harness check --wait` (a few hours). It plays the champion against `harness/identical` (its tip plus a comment, so the same bench) in two parts, and passes only when both pass:

1. **Determinism** (`--determinism-pairs`, default 1,000): pairs at 25,000 nodes, one thread, 16 MB hash, the normal book with colours swapped. At fixed nodes on one thread Reckless is deterministic (fastchess sends `ucinewgame` before every game, and Reckless clears its hash, histories and correction tables on it), so both games of every pair must be the same game move for move and every pair must score exactly 1. Any pair that does not fails the check, and the openings (FENs) that differed are reported. When this part fails, the second is skipped.
2. **Symmetry** (`--symmetry-pairs`, default 4,000): a fixed run of timed VSTC pairs, not an SPRT. It passes when the identical engine's Elo interval holds 0 and the pentanomial is symmetric within 3 standard errors (LL ≈ WW, LD ≈ WD).

The result belongs to the machine (CPU, cores, OS, fastchess, book): `dameless harness` says whether it is `current` and `trusted`, and `last` has both parts (`determinism`: pairs, nodes, `uneven`, `openings`; `symmetry`: z scores; `elo`), and while it is not trusted, every trial verdict says so. The check catches nondeterminism (hash carried over, a time limit sneaking in, a build that is not what it claims) and gross bias (colour or order, dev and base mixed up, a broken book). A failed check means no result on this machine is trusted until it is investigated. (It replaced an SPRT at VSTC [0, 3], which between identical engines averages about 53,000 games.)

## Tuning

A tune moves search constants by SPSA, as OpenBench does. The tunables are declared in the fork's `src/parameters.rs` with `define!` (`i32 name: default, min, max, c_end;`); the branch `spsa/tunables` declares them. Without the `spsa` cargo feature each is a `const fn`, so the bench does not change; with it, each is a UCI option and `reckless spsa` prints them in OpenBench's format. A tune builds its branch with the feature (cached as a build with `spsa` in its features).

```bash
dameless tunables spsa/tunables                       # 409 (and the spsa build queued) while there is none
dameless tune --branch spsa/tunables [--params a,b] [--iterations 1000 --pairs 8] [--tc 8+0.08 | --nodes 25000] [--hash 16] [--concurrency N] [--long] [--wait]
dameless idea tune <id>                               # its parameters and every SPSA step
```

- OpenBench's defaults: alpha 0.602, gamma 0.101, A = 0.1 × iterations, `r_end` 0.002; θ± rounded stochastically and `c_k` never under 0.5. It runs as workers of two games, about `concurrency / 2` iterations in flight, each applied to θ as it stands when its batch ends. Its clock is scaled by the tuned build's own speed when it starts or resumes. A stopped tune keeps its steps and resumes from the last.
- When SPSA ends it writes the rounded values into `src/parameters.rs` on `tune/<idea id>`, commits with its `Bench:` line, and the confirm trial (STC [0, 4], the bounds Reckless confirms tunes with) decides.
- The default, 1,000 iterations of 8 pairs, is 16,000 games: enough to see a tune works, short for many parameters. Reckless's own sessions run 95,000 to 200,000 games; use 5,000 iterations or more, or fewer parameters. The schedule is fixed by the iteration count. Trust the confirmation, not the trajectory, and tune at the time control you care about (`--nodes` only checks that everything works).

## Versions, the champion, promoting

```bash
dameless dameless                      # the logbook: champion, the champion line, ideas running, queued and recent, the bot, strength
dameless versions [--line champion|idea|upstream|other] | version make <ref> [--wait] | builds <version>
dameless idea promote <id>             # a kept idea becomes the next champion
dameless upstream [sync [--wait] | auto on|off]   # taking upstream Reckless as the next champion
dameless push                          # the fork's main to origin: explicit, only when the user asks
dameless build delete <id> | build prune [--older-than-days N] [--all] [--dry-run]
```

- Versions in requests are `champion` (or `main`), `v14`, `#12` (an id), or a branch, tag or commit (a version is made on first use). A build is a cache keyed by (commit, features, net), shown only as diagnostics. **PGO**: the bot and your games against Dameless at full strength play a profile-guided build (`pgo` among its features: cargo-pgo, `bench` as the profile), made in the background after the first game that wants it, used once its bench equals the plain build's, and only when `cargo install cargo-pgo` and `rustup component add llvm-tools-preview` are done (else plain builds, and diagnostics says so). Trials, tunes, the harness, levels and engine-against-engine games never use PGO: two builds' layouts would bias a comparison.
- **Promoting** merges a kept idea's branch into the fork's `main` in a scratch worktree (fast-forward, or a merge commit with the `Bench:` line) and makes champion `v<n+1>`. The user's checkout of `main` is fast-forwarded when clean; with uncommitted changes the promotion is refused (409) and nothing changes. A net idea's promotion records the champion's code with the new net. **Pushing** is separate and never automatic.
- **Taking upstream.** The fork's remote `upstream` (codedeliveryservice/Reckless) merges a handful of tested gainers a month. An `Upstream` work item fetches it every six hours while the setting is on (default on; `dameless upstream auto off`, `PUT /api/upstream`) and on demand (`dameless upstream sync`, `POST /api/upstream/sync`, MCP `sync_upstream`). When `upstream/main` has commits the champion lacks, they are merged into the fork's `main` in the scratch worktree the way a promotion merges, with no trials (upstream tested them on its fleet), but built and benched: a fast-forward must bench what upstream's commit says, and a merge commit (the fork has its own commits) gets its `Bench:` line from the measured bench. The result is champion `v<n+1>` with `upstream` (the base, the upstream commit, the commits brought): the logbook reads "v3 · from upstream · 4 commits". A conflict, a failed build or bench, a `main` that moved, or uncommitted changes in your checkout of `main` changes nothing; the failure is in the work's log, `GET /api/upstream` (`last`), the logbook and the `upstream.synced` event. Syncs share the promotions' lane, so never run during a promotion. Never pushes.
- Deleting a build removes its binary and keeps its record (409 while in use); pruning keeps each version's newest build unless `--all`.

## Networks, data and training

A trained net is an idea of kind `net`. Nets are build inputs (Reckless embeds its net at compile time); the trainer (`trainer/`, its own Cargo workspace, so the app never needs CUDA to compile) is built on demand.

```bash
dameless nets [--id N] | data
dameless data datagen --positions 20M [--name dfrc --threads 8 --soft-nodes 5000 --opening dfrc|chess960|startpos --version] [--wait]
dameless data import <url> --bytes N --name <dataset> [--sha256 H] [--yes]   # asks "Download …? [y/N]" without --yes
dameless data delete <id|name> --yes | data from-trials <idea> [--name trials-12]
dameless train [--dataset 3 … --parent <net> --from-scratch --superbatches 20 --lr … --wdl …] [--wait] | train show <idea> | train steps <work>
dameless idea new --kind net --title "…" --hypothesis "…" [--superbatches 20 …] [--long]
```

- **Disk.** Datasets share `DAMELESS_DATA_CAP_GB`, and nothing that writes much starts with less than 15 GB free (`dameless data` shows the cap, what is used and the room left). An import needs `confirm: true` (`--yes`), given only after the user saw its size; it is checked against the cap at about 3.15 × the download at peak. Datagen resumes on the same name; a training run checks the disk for its checkpoints.
- **The GPU** is granted whole to one training run at a time (`DAMELESS_GPUS`), with two to four cores for its loader, so trials keep playing beside it. Without a GPU, training is a 409.
- Details (executors, file layout, promoting and publishing a net) are in `docs/roadmap.md` (R4) and `trainer/README.md`.

## Lichess

Two connections, independent: **you** (your own account, through the Board API) and **bot** (Dameless's BOT account, through the Bot API). Tokens are files in `secrets/` (0600 on Unix), never rows, and no route returns them. Nothing here creates an account or logs in by itself.

```bash
dameless lichess                                   # both connections' status
dameless lichess you [login | token < token.txt | logout]
dameless lichess you seek --tc 10+5 [--rated --color white --range 1500-1800] | seek --stop
dameless lichess you challenge <user> | --ai 1-8 | --bot  --tc 5+3 [--rated --color black]
dameless lichess you accept <id> | decline <id> [--reason later] | cancel <id>
dameless lichess bot [token < token.txt | logout | online | offline | seek once|always|off | bots]
dameless lichess bot settings --set version=champion --set hash=64    # any field of the bot's settings
dameless lichess bot challenge <user> --tc 3+2 [--rated --color random]
```

**You.** `login` prints Lichess's OAuth link (PKCE, client id `dameless`, scopes `board:play challenge:read challenge:write`); Lichess sends the browser back to `/api/connections/you/callback` (outside the guard and the OpenAPI document: the single-use `state` is its proof). A BOT account, or the bot's, is refused. Your games are played on the board (offers, takebacks, abort, claims once the opponent left), and every game of yours, here or on lichess.org, turns fair play on. What the Board API allows (`crates/core/src/lichess/board.rs`): rapid, classical and correspondence; blitz too for direct challenges and games against the AI; never bullet. Lichess's speed is initial + 40 × increment, so a seek needs at least 8 minutes estimated, a challenge at least 3; a challenge's clock is 0, 15, 30, 45 or 90 seconds or whole minutes up to 180, with an increment of at most 180 seconds (60 against the AI). A challenge the Board API cannot play (`playable: false`) is played on lichess.org.

**The bot.** Its settings (`crates/core/src/lichess/bot.rs`): the version that plays (`champion` means the champion at each game's start; nothing plays until one is chosen), hash 64, threads 1, move overhead 300 ms, ponder on (it thinks on the opponent's time when its engine declares UCI's `Ponder` option; no other game ever ponders), rated and casual, speeds bullet, blitz and rapid (never UltraBullet or correspondence), initial 60–1800 s, increment 0–30 s, standard chess (and `from_position` if allowed), people and bots, 1 game at once. Anything else is declined with Lichess's reason (`variant`, `timeControl`, `tooFast`, `tooSlow`, `casual`, `rated`, `noBot`, `onlyBot`, `later`, `generic`). Each game has its own engine process with Lichess's clocks; after each move it ponders the reply it expects (`go ponder` with the clocks as they stand), sends `ponderhit` when the opponent plays it and otherwise stops the ponder and searches afresh; draw offers are ignored; a dead engine or an illegal move resigns.

- **Seeking** (a BOT cannot use the lobby, so it challenges other bots): while seeking is on and a slot is free, it picks a clock from `seek_clocks` (default 3+0, 3+2, 5+3) and challenges a random online bot rated within `seek_range` (300) of its own rating at that speed (provisional or none counts as 1500), else within twice that, else anyone, leaving out bots that declined or ignored it in the last hour. A decline, or 25 s without an answer, moves on after 7 s; a 429 waits 61 s; nobody to challenge, 60 s. `once` stops when a game starts; `always` carries on after every game and across restarts. Never more than one challenge of ours is pending.
- **The daily limit**: Lichess lets a BOT play at most 100 games against other bots in any rolling 24 hours (lila's `bot.vsBot.day`); games against people are unlimited. At 100 the bot stops challenging bots and declines theirs as `later` until the oldest game ages out, while still taking people's. Lichess's own refusal sets the reset time.

Errors from Lichess come back as 400 (it refused), 429 (rate-limited, with `Retry-After` when known) or 502 (unreachable). Lichess allows one request at a time per token and a minute's pause after a 429; the adapter keeps to it.

## Work

Builds, trials, tunes, reviews, imports, promotions, upstream syncs, calibration, datagen, training and dataset imports are **work**: queued, logged, stoppable, pausable, and resumed from their last checkpoint after a restart.

```bash
dameless work [--recent 20] | work show <id> | work log <id> | work wait <id> | work stop|pause|resume <id>
```

- Live play comes first: a game with a Dameless seat reserves its threads, and live evaluation one core. When they oversubscribe the cores, the lowest-priority running work pauses (`paused_for: "A live game"`) at its next safe point and resumes when cores free up.
- Priorities, lower first: live, interactive (builds), review, idea. Lanes run one item at a time each (builds; trials, tunes and calibration; reviews; promotions and upstream syncs; datagen; training on the GPU). Network work (imports) needs no cores and runs at once.

## Events

One server-sent event stream: `GET /api/events?since=<id>&topics=<t,…>` (`Last-Event-ID` resumes; `?token=` for `EventSource`), or `dameless events --topics games,work`. Each event is `{ id, topic, kind, data }`. Topics: `game:<id>`, `games`, `idea:<id>`, `ideas`, `work`, `player:<key>`, `connection:you`, `connection:bot`, `versions`, `policy`, `eval:<session>`, `training`; no `topics` means all but `eval:*` and game detail. Kinds: `game.upserted`, `game.moved`, `game.undone`, `game.changed`, `game.thinking`, `idea.upserted`, `trial.upserted`, `trial.progress`, `tune.progress`, `work.upserted`, `work.log`, `version.upserted`, `review.upserted`, `connection.upserted`, `challenge`, `policy.changed`, `eval.info`, `drill.upserted`, `drill.answered`, `drill.deleted`, `levels.updated`, `train.progress`, `dataset.upserted`, `net.upserted`, `upstream.synced` (topic `versions`); payloads are typed in the OpenAPI document. When events after `since` are no longer kept, a `reload` event tells the client to refetch. Scores everywhere are White's point of view.

## Working on Dameless itself

```
apps/web   Vue 3.5 + eos + chessground (REST + SSE)
crates/
  core       pure domain: types, chess, stats, review rules, referee rules, SPSA, scaling, Lichess rules, events (no I/O)
  hub        the shared runtime: config, data dir, secrets, event bus, fair-play policy, resource ledger
  store      SQLite: schema, numbered migrations, one writer, repositories
  engine     adapter: UCI processes, engine pool, git and cargo builds, bench, fastchess
  lichess    adapter: HTTP with Lichess's rate rules, OAuth PKCE, NDJSON streams, Bot/Board API, import, explorer; MockLichess
  play       the Referee: game tasks, seats (engine, local, Lichess), connections, the bot, imports, levels
  analysis   evaluation (warm engine pool, position cache), reviews, profiles, drills, repertoire, progress
  work       the scheduler, versions and builds, ideas and trials, tunes, promoting, harness, nets, data, training
  server     axum API (utoipa → OpenAPI), SSE, /mcp, the wiring, the `dameless` binary (serve + CLI)
trainer/   its own Cargo workspace: the CPU reference of Reckless's net, the bullet model, the data tools
skill/     the Claude Code skill for driving the app
```

- **Layering.** Dependencies point down the list. `core` does no I/O and is unit tested. `engine` and `lichess` are adapters with their own wire types and no dependency on `core`; `play`, `analysis` and `work` map them. `server` holds no domain logic: routes translate HTTP to service calls. `docs/design.md` has the seams between services.
- **Gates** before committing: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <crate>` for what you changed (the whole workspace before a merge); for the web, `corepack pnpm lint`, `format:check`, `typecheck`, `test`, `knip`.
- **The API.** Every route has a `utoipa` path. After changing a route or a type it returns, run `corepack pnpm api` and commit `crates/server/openapi.json` and `apps/web/src/api/schema.d.ts` (`corepack pnpm api:check` fails on stale files). Add the route's CLI command to `cli::COMMANDS`. Its MCP tool appears by itself; if the route acts on Lichess or the outside world, or destroys something, add it to `REACH` or `DESTRUCTIVE` in `crates/server/src/mcp/tools.rs` (the tests check those tables against the document).
- **Shared checkout.** Several agents work in this repository at once. Each owns its paths and commits only them: `git commit -m "…" -- <paths>`, never `git add -A`, never leave files staged. Build with `CARGO_TARGET_DIR=target-shared` (one cache for every agent; cargo locks it) or your own `target-<you>` (both ignored), and use your own port and data directory for a hand-run app.
- **Data directories live outside the repository** (for example `C:\dev\dameless-<you>-data`): engine builds make git worktrees inside the data directory, which break inside this Cargo workspace (only `.data` and `.data-web` are excluded from it). Delete yours when done.
- **Never touch real accounts.** No agent logs in to Lichess, stores a real token or talks to lichess.org. Every Lichess path is tested against `dameless_lichess::testing::MockLichess` (the crate's `mock` feature); a hand-run app gets `LICHESS_URL` set to the mock or an unreachable address (`http://127.0.0.1:9`).
- **Walking the Lichess flows by hand.** `cargo run -p dameless-lichess --features mock --example mock_server [-- <port>]` starts the mock on `127.0.0.1:9797` (or `<port>`) and a control API on the next port. Run the app with `LICHESS_URL=http://127.0.0.1:9797`. The mock knows you as `Dame` (fake token `lip_mock_you`) and the bot as `DamelessBot` (`lip_mock_bot`); its OAuth page grants every login as `Dame` at once, so Log in with Lichess works end to end, and the bot's token goes in with `echo lip_mock_bot | dameless lichess bot token`. Opponents in games from challenges and seeks answer with their first legal move and take offers; bots Alpha, Beta, Gamma and Delta are online and accept challenges; `Dame` has six games to import. The control API scripts the rest with JSON posts: `/games` (start a game, seats `you`, `bot`, `user:<name>:<rating>`, `ai:<level>`), `/games/{id}/move {uci}`, `offer-draw`, `offer-takeback`, `accept-draw`, `accept-takeback`, `gone`, `back`, `end {status, winner}`, `/seeks/{id}/take`, `/challenges {from, to, clock: [300, 3]}` and `/challenges/{id}/accept|decline|cancel`, `/answers {user, answer}`, `/reply {reply, accepts_offers}` and `/bot-daily-limit {ours, seconds}`; `GET /state` lists games, challenges and seeks. The file's header (`crates/lichess/examples/mock_server.rs`) has the details.
- Doc comments say what a thing is and why, in plain sentences. No commented-out code, no TODOs without an owner.
