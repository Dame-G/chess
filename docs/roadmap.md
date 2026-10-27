# Dameless: the roadmap

Everything here is built on Windows before the move to Linux. The move happens once the whole app is done. `docs/design.md` is the contract; this file says what comes after the core rebuild and in what order. Each phase ends usable.

## The old phases, judged against the new goal

The goal is now **two players, one board**: you and Dameless both get stronger by playing games and learning from them. The old plan only ever served one of those players.

| Old item                                                             | Verdict            | In the new app                                                                                                                                                                                                  |
| -------------------------------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 4: training (bullet, datagen, data import, net registry)       | **Keep, reframed** | A network is another way Dameless gets stronger, so a trained net is an **idea** (kind `net`). It goes through the same funnel as any change and lands in the logbook. Its loss curves sit on the idea's sheet. |
| First net experiments (pawn-pair inputs, SFNNv18-style king buckets) | **Keep**           | They are the first `net` ideas, once the trainer reproduces Reckless's own network exactly.                                                                                                                     |
| Phase 5: remote runner, burst boxes                                  | **Drop**           | Local compute only.                                                                                                                                                                                             |
| Phase 5: self-hosted OpenBench                                       | **Drop**           | One machine, and the scheduler already does what OpenBench's would.                                                                                                                                             |
| Phase 5: `/mcp`                                                      | **Keep, raised**   | The fifth principle is that Claude and you use the same verbs. MCP is how Claude uses them, from the same routes.                                                                                               |
| Policy nets, MCTS, searchless engines                                | **Still out**      | There is still no evidence they help an alpha-beta engine at this level.                                                                                                                                        |
| Puzzles (left out before)                                            | **Replaced**       | **Drills**: puzzles made from your own mistakes. They are personal, so they serve the goal; generic puzzles do not.                                                                                             |
| Opening explorer (left out before)                                   | **Done**           | Explore mode, outside your live games.                                                                                                                                                                          |
| Open item: false-accept check                                        | **Built in**       | **Check the harness** in Settings, a ready-made custom idea. The machine is marked unchecked until it passes, and trials say so beside their verdict.                                                           |
| Open item: `Bench:` lines                                            | **Built in**       | When an idea's branch has no `Bench:` line, or a stale one, the app offers to fix it when the idea is created: it rewords the tip with the measured bench.                                                      |
| Open item: first tune, ideas batch                                   | **Operational**    | These run on the finished app. The New idea dialog gets a template, "Tune pruning and reductions", with 12 parameters and 2,500 iterations.                                                                     |
| Open item: Lichess account, first real use                           | **Operational**    | A checklist with the user at cutover (below).                                                                                                                                                                   |
| Open item: eos gaps                                                  | **Keep**           | Phase R6 adds them to the eos library and replaces the hand-styled versions.                                                                                                                                    |
| Not built: merging into main from the UI                             | **Done**           | **Promote**.                                                                                                                                                                                                    |
| Not built: a rematch that re-rolls the book opening                  | **Keep**           | **Rematch** on a finished game: same opponent and clock, colours swapped, a new book line for a book start.                                                                                                     |
| Not built: building any tag or commit from the UI                    | **Done**           | `POST /api/versions { ref }`, and the branch picker accepts any ref.                                                                                                                                            |
| Not built: per-build compile logs                                    | **Keep**           | Settings › Machine › Builds, as diagnostics.                                                                                                                                                                    |

The **player side** never had phases at all: the old plan only served the engine. Phase R3 adds them.

## Phases

### R1: the core rebuild (in progress)

The crates in `design.md`, the API and CLI, and the web app on the real backend. It is done when everything the old app did works in the new one, end to end, against the mock Lichess and the real engine.

### R2: trust the harness

- **Check the harness:** `main` against a branch identical to it at VSTC [0, 3], plus an optional symmetry run of a fixed number of pairs (the UI offers 2,000). The result is stored per machine; Settings and every trial verdict show it.
- **Bench line fixer**, when an idea is created.
- **Health** in Settings: fastchess and the book are installed, the engine checkout is found, clang is there (Syzygy builds), and the disk has free space for builds, games and data.

Built in `work` (`work::harness`, `work::health`, `work::ideas::bench`; the pure rules in `core::harness`). What the server exposes:

| Route                                 | Service call                                                    | Answers                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------- | --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /api/harness/check`             | `Harness::start_check(HarnessCheckRequest { pairs? })`          | 201, the `Idea` (custom, tag `harness`, branch `harness/identical`). 409 while a check runs or without fastchess and the book; 400 for `pairs: 0`.                                                                                                                                                                               |
| `GET /api/harness`                    | `Harness::status()`                                             | `HarnessStatus { checked, current, passed, at, idea, trusted, running, summary, last: HarnessCheck }`                                                                                                                                                                                                                            |
| `GET /api/ideas/check-branch?branch=` | `Ideas::check_branch(branch)`                                   | `BranchCheck { branch, commit, bench_line, measured, status: ok\|missing\|stale\|unbuilt, detail }`. Never builds. 404 for an unknown branch.                                                                                                                                                                                    |
| `POST /api/ideas/fix-bench`           | `Ideas::fix_bench_line(branch, commit?)` with `FixBenchRequest` | The new tip's `BranchCheck`. Builds the tip if needed, rewords it locally (never pushed). 409 when the branch moved (`commit` is the tip it was checked at); 400 for `main` or upstream.                                                                                                                                         |
| `POST /api/ideas` with `fix_bench`    | `Ideas::create(NewIdeaRequest { fix_bench, .. })`               | Without it, a branch whose tip has no `Bench:` line or a stale one is a 409 `conflict` naming the problem (for ideas with a fingerprint stage; tunes and custom tests are not held up). With it, the line is fixed first. Batches take it per idea.                                                                              |
| `GET /api/diagnostics`                | `Diagnostics::health()`                                         | `Health { ok, items: [{ key, ok, level: ok\|info\|warn\|fail, title, detail, action?: { label, command } }] }`, keys `fastchess`, `book`, `engine`, `clang`, `cuda`, `gpu`, `disk`, `database`, `builds`, `harness`. Behind the token, since it names machine paths; `GET /api/health` stays the unauthenticated liveness check. |

- The check's idea ends **kept** when the harness passed (the SPRT rejected the identical engine, its Elo interval holds 0, and the symmetry run, if any, was symmetric within 3 standard errors) and **dropped** when it failed, with the check's sentence as its verdict ("Harness check failed: An identical engine was accepted …: do not trust results until this is investigated"). It cannot be promoted. Its SPRT trial reads "rejected" as any trial does.
- The result is `meta` `harness.check`: `{ machine: {cpu, cores, os}, at, idea, passed, elo, symmetry?, engine_commit, fastchess, book, summary }`. It is `current` while the CPU, core count, OS, fastchess and book are the same as now.
- There is no harness event: clients refetch `GET /api/harness` on `idea.upserted` for an idea tagged `harness`. Trial verdicts show `summary` beside them while `trusted` is false; `Trial` keeps its shape.
- The 409 for a stale or missing `Bench:` line carries the problem in its message only (errors have no detail field); the dialog calls `check-branch` to show it and offers "Fix the Bench: line".
- Health's slow probes (clang, nvcc, nvidia-smi) are kept for a minute; disk warns under 20 GB free and fails under 5 GB.

### R3: the player side

You get stronger at the same board.

- **Levels.** Dameless at a strength you choose, from about 800 to full. The engine's own play is kept, not handicapped by random blunders: strength comes from a node budget, plus a small MultiPV softmax with a temperature at the lowest levels. Each level's rating is **calibrated by the app**: it plays levels against each other and against full strength at fixed nodes in the background, at low priority, and fits an Elo per level, anchored to your own results against them. The New game dialog then offers "Dameless at 1600", and Play this out starts at your level by default.
- **Drills.** Weak spots become exercises: find the move you missed. They repeat on a spaced schedule (SM-2: a miss comes back tomorrow, a pass in growing intervals). Each drill opens on the board in a new **Drill** mode: no engine, one try, then the answer and the line. The You sheet shows today's drills and how your hit rate moves over time. Drills are local practice, so fair play does not apply, but they are hidden while a Lichess game of yours is live, like everything else.
- **Your openings.** From your games: the lines you play as White and as Black, where you leave theory, and how you score in each. It is built from your games, with the explorer, and is shown on the You sheet and in Explore as "your repertoire". It is off while you play on Lichess.
- **Progress.** One line on the You sheet: rating, accuracy and drill hit rate over time. You can see the two players' lines side by side: you and Dameless.

#### The R3 contract

The pure logic is in `core` and the tables in `store` (both built); `play`, `analysis`, `work`, the server and the web app build on these shapes.

**Core modules** (each pure and unit tested):

- `core::levels`
  - `Level { id, name, nodes: Option<u64>, multipv, temperature, nominal }`: `id` 1 is the weakest; `nodes: None` is full strength, which plays on the clock (calibration gives it `FULL_CALIBRATION_NODES`, 1,000,000). `ladder()` is the default ladder.
  - `weights(candidates, temperature)` and `choose_move(candidates: &[(uci, Score)], temperature, u) -> Option<uci>`. Candidates are the engine's MultiPV lines with scores **from the side to move**, as the engine reports them. It is a softmax on win%, `exp(−(best − win%) / T)`, and the same `u` always gives the same move. The engine seat draws `u` from the game's own seeded RNG.
  - Calibration:
    - `Entrant { Level(id), You }`, written `level:<id>` / `you`.
    - `LevelGame { white, black, outcome }` and `pair_results(games) -> PairResult[]`.
    - `priors(levels, you: Option<(rating, sd)>) -> Prior[]`: each level at its nominal rating with sd 350, You at your Lichess rating with sd 120 or Lichess's own deviation.
    - `fit(priors, results) -> EntrantRating { entrant, estimate: {elo, low, high}, games }[]`: the MAP estimate of the Bradley–Terry (logistic Elo) model, a draw counting half, under Gaussian priors, found by Newton's method. The 95 % intervals come from the inverse curvature. Your results against levels and your Lichess rating are what put the ladder on the human scale. Without them it stays at the nominal ratings, which matter less and less as the games come in.
  - `plan(levels, ratings, count) -> PlannedMatch { a, b, games, priority }[]`: neighbouring levels only, ranked by (width a + width b) × p(1 − p), so the widest and most even pairs come first.
  - `nearest_level(ratings, target)` gives the level for your rating.
- `core::drills`
  - `Drill { id: DrillId, game, ply, fen, key, side, best_moves, best_san, line, played, played_san, swing, theme: {opening, phase}, due, interval_days, ease, reps, lapses, created_at, answered_at }`.
  - Making drills:
    - `new_drill(&WeakSpot, lines: &[EvalLine]) -> NewDrill`: every line within 5 win% of the best counts as correct, except the move played. The lines come from the position cache (MultiPV ≥ 2) or from `lines_of(&ReviewedMove)`, which gives the best and second best. Without lines, only the weak spot's best move counts.
    - `dedupe(existing_keys, candidates)`: one drill per normalized FEN key.
  - Answering:
    - `check(drill, answer)` takes UCI or SAN and returns `Pass | Fail`.
    - `grade(drill, outcome, now)` applies SM-2 with two answers (a pass is q 5, a miss q 2). A miss resets the run and comes back in 1 day. Passes come back in 1, then 6, then interval × ease days. The ease moves +0.10 on a pass and −0.32 on a miss, with a floor of 1.3.
  - Today's set: `today(drills, now, new_per_day, started_today, limit)` lists due tried drills first, the most overdue first, then new ones by swing, capped at `NEW_PER_DAY` (10) a day. `day_start(now)` uses UTC days.
  - The trend: `trend(answers, window) -> HitRatePoint { date, answers, passes, rate, rolling }[]`.
- `core::repertoire`
  - `RepertoireGame::from_game(&Game, &PlayerKey)` reads finished games from the standard start.
  - `build(side, games, RepertoireOptions { max_plies: 20, min_games: 2 }, theory: Option<&dyn Fn(&key) -> bool>) -> Repertoire { side, games, options, nodes, departures }`.
    - Nodes are keyed by FEN without counters, so transpositions meet: `{ key, fen, ply, games, your_turn, theory, moves: [{ uci, san, games, share, wins, draws, losses, score, next, leaves_theory }] }`.
    - Departures are each game's first move out of theory, added up: `{ key, fen, ply, uci, san, by_you, games, score, example }`.
    - `Repertoire::main_line()` and `your_deviations()`.
- `core::progress`
  - `Progress { series: Series[] }`, where `Series { player, metric: rating | accuracy | drill_hit_rate | elo, points: [{ at, value, low, high, game, version, label }] }`.
  - `your_series(trend, drill_trend, window)` gives accuracy as a rolling mean of 10 games.
  - `dameless_series(versions, anchor)` gives the champion line's cumulative Elo over `v1`, with the errors added in quadrature. With `anchor` set to full strength's calibrated rating, the line is shifted onto the human scale.
  - `progress(...)` builds both.

**The ladder** (nominal rating, nodes per move, MultiPV, temperature in win% points):

| Level | Nominal | Nodes   | MultiPV | T   |
| ----- | ------- | ------- | ------- | --- |
| 1     | 800     | 300     | 8       | 15  |
| 2     | 1000    | 300     | 6       | 10  |
| 3     | 1200    | 500     | 5       | 6   |
| 4     | 1400    | 800     | 4       | 3.5 |
| 5     | 1600    | 1,500   | 3       | 2   |
| 6     | 1800    | 3,000   | 2       | 1   |
| 7     | 2000    | 6,000   | 1       | 0   |
| 8     | 2200    | 15,000  | 1       | 0   |
| 9     | 2400    | 40,000  | 1       | 0   |
| 10    | 2600    | 120,000 | 1       | 0   |
| 11    | 2800    | 400,000 | 1       | 0   |
| 12    | full    | clock   | 1       | 0   |

Why these numbers:

- **Few nodes are still strong.** An NNUE engine at a few hundred nodes already completes depth 3 to 5 with quiescence, and that beats most club players. So nodes alone cannot reach 800 without searches too short to finish depth 1 for every root move.
- **Below 1800, choice does the weakening.** Nodes stay at 300 or more, so every MultiPV line is a real search, and the weakening comes from choosing among the engine's own candidates. At T = 15, a move 10 win% worse is played half as often as the best. That is a beginner's inconsistency, with no move outside the engine's top 8, and the move is never a random blunder.
- **Above 1800, only nodes change.** From 6,000 to 400,000 nodes is six doublings. At roughly +130 Elo per doubling at low budgets, falling to about +60 higher up, that is the 2000 to 2800 span.

These are guesses that calibration replaces. The UI shows a level's calibrated rating once it has one (games > 0), else "about <nominal>".

**Storage** (migration `0002_player.sql`; repositories `store::drills`, `store::levels`):

- `drills`: one row per position key (UNIQUE), the schedule in plain columns, indexed on `due`.
- `drill_answers (drill, at, answer, outcome, first)`: `first` marks a drill's first answer, for the daily cap of new drills.
- `level_games (game PRIMARY KEY, white, black, calibration)`: the entrant at each seat of a game played at a level. `play` writes it when a local game with a level starts, and for calibration games.
- `level_ratings (entrant PRIMARY KEY, elo, low, high, games, updated_at)`: the last fit.
- Methods:
  - `insert_drills`, `drill`, `drills`, `drill_keys`, `record_drill_answer(graded, answer)` (one transaction), `drill_answers(since)`, `drills_started_since`, `delete_drill`
  - `record_level_game`, `level_seats`, `level_games` (finished only), `put_level_ratings` (replaces the fit), `level_ratings`

**Routes** (the server's; every one with a CLI command of the same name):

```
GET    /api/levels                        { levels: [{ level: Level, rating: Estimate | null, games }], you: EntrantRating | null,
                                            suggested: level id | null (nearest your rating), updated_at }
GET    /api/levels/plan?count=3           PlannedMatch[]
POST   /api/levels/calibrate              { matches?: PlannedMatch[], count?, games_per_pair?, version? } → Work (default: plan(…, 3));
                                            refits after each match
POST   /api/games                         a Dameless seat at a level: "dameless@5" (or { player: "dameless", level: 5 }), optional `seed`;
                                            a bare "dameless" is full strength, as today; the game's seats go to level_games
GET    /api/drills                        Drill[] (all, soonest due first)
POST   /api/drills                        { limit? } → Drill[] made from your profile's weak spots (new ones only)
GET    /api/drills/today?new_per_day=&limit=   { drills: Drill[], due, new, started_today }
POST   /api/drills/{id}/answer            { move } (UCI or SAN) → { outcome, drill: Drill (rescheduled), best_moves, best_san, line, played_san }
DELETE /api/drills/{id}
GET    /api/drills/trend?window=50        HitRatePoint[]
GET    /api/players/you/repertoire?side=white|black&max_plies=&min_games=&limit=   Repertoire (409 fair_play while the explorer is not allowed)
GET    /api/progress?window=10            Progress
```

- **Seeing a drill** (`GET /api/drills/today`) shows `fen`, `side` and `theme`. The web client must not show `best_moves`, `best_san` or `line` before the answer; the answer route returns them.
- **Fair play.** Drills are not refused by the server. The web client hides them while `policy.assist ≠ allowed`. The repertoire is refused with 409 while you play (it is assistance, like the explorer: allowed when every live game is correspondence).
- **Theory for the repertoire.** By default a position is theory when it was reached within the named opening's plies (`opening.ply`) in any of your Lichess games: free, and Lichess's own book. The explorer (masters) can refine it later, outside live games.
- **Calibration** is background work at `Priority::Idea`, the lowest priority. Every level plays at fixed nodes with the tests' book, and level 1 to 6 seats use `choose_move` (fastchess cannot), so all calibration games are played by `play`'s engine seats.
- **When the fit runs.** The server refits (`fit` over `level_games` with `priors(ladder, your Lichess rating)`) after each calibration match and after each finished game of yours against a level, then stores it.

**Services and wiring** (built: `play::levels`, `play::calibrate`, `analysis::drills`, `analysis::repertoire`, `analysis::progress`). Each route calls one function.

- **Construction** (in the server's app):
  - `Games::new(hub, binaries, timing)` makes its own `Levels` (the default ladder): `games.levels()`. Set the anchor once the connections exist: `games.levels().set_anchor(Arc::new(move || play::your_anchor(&connections.status(ConnectionKey::You))))`. It uses your rapid rating, else blitz, classical, bullet, an established rating before a provisional one.
  - `scheduler.register(Arc::new(play::Calibrate::new(games.clone())))` for `WorkKind::Calibrate` (the trials' lane, priority Idea).
  - `let drills = analysis::Drills::new(hub.clone())`, and the review executor registered as `Reviews::new(eval).with_drills(drills.clone())`: each review of one of your games then makes drills from it.
  - Repertoire and progress are methods of the existing `analysis::Profiles`.
- **Routes → calls:**

```
GET    /api/levels                 games.levels().overview()                    → play::LevelsOverview
GET    /api/levels/plan?count=3    games.levels().plan(count)                   → PlannedMatch[]
POST   /api/levels/calibrate       body play::CalibrateRequest { matches?, count?, games_per_pair?, version? }
                                   scheduler.submit(games.levels().prepare_calibration(&*binaries, req).await?) → Work
POST   /api/games                  games.start(NewGameRequest)  (level seats below; optional `seed`)
GET    /api/drills                 drills.list()                                → Drill[]
POST   /api/drills                 body analysis::MakeDrillsRequest { limit? }  drills.make_from_weak_spots(limit) → Drill[]
GET    /api/drills/today           ?new_per_day=&limit=  drills.today(&now(), new_per_day, limit) → analysis::DrillsToday
POST   /api/drills/{id}/answer     body analysis::AnswerRequest { move }  drills.answer_now(id, &move) → analysis::DrillAnswerResult
DELETE /api/drills/{id}            drills.delete(id)   (204; 404 for none)
GET    /api/drills/trend?window=50 drills.trend(window)                         → HitRatePoint[]
GET    /api/players/you/repertoire ?side=&max_plies=&min_games=&limit= (analysis::RepertoireRequest, IntoParams)
                                   profiles.repertoire(req) → Repertoire (409 fair_play while the explorer is not allowed)
GET    /api/progress?window=10     profiles.progress(window)                    → core::progress::Progress
```

- **Level seats** (`play::SeatRequest`): `dameless@<level>` and `dameless:<version>@<level>` (a trailing `@` and digits; a version name with another `@` stays a name), or the object `{ "player": "dameless", "version"?: "v14", "level"?: 5 }`. A bare `dameless` is full strength on the clock, as before. An unknown level is 400.
  - A seat at a level searches the level's nodes per move, whatever the game's clock (full strength, `nodes: None`, plays the clock), with its MultiPV. It plays `choose_move`'s pick, drawn from a SplitMix64 seeded by `NewGameRequest.seed` (random when none; the seed is not stored).
  - Its name is "Dameless <rating>" (calibrated once the level has games, else nominal), with `rating` set.
  - When every seat is a level or you, the game goes to `level_games` (not calibration), and the ladder is refitted when it ends.
- **Calibration work:** the spec is `play::CalibrateSpec { matches, version }`, resolved when queued: the plan when no matches are given, `games_per_pair` applied, the version's name.
  - It plays each match's games, as many at once as `cx.cores(2, all)` grants (two cores a game). Its engines start without ledger reservations, so they never pause other work as live play does.
  - Colours alternate. Games start from a random line of the trials' book, or four random plies without one. Each level plays at `calibration_nodes()` (full strength 1,000,000), with adjudication on.
  - Levels 7 to 12 are played the same way as 1 to 6 (engine seats, not fastchess): simpler, and exact.
  - Games are stored as finished local games named "Dameless <rating> (level N)", with `level_games.calibration` set. They are not announced one by one (no `game.upserted`); game lists should leave calibration games out once they have a filter for it.
  - It checkpoints after each batch (`CalibrateCheckpoint { done per match, failed }`), stops at a safe point before each batch, refits after each match (`levels.updated`), and fails when every game of a batch fails. Its result is `{ games, failed, matches }`.
- **The fit:** `Levels::refit()` runs `fit(priors(ladder, anchor), pair_results(level_games))`, stores it and emits `levels.updated`. Without your Lichess rating, You has no prior, so your games are left out of the fit and the ladder keeps its nominal scale.
- **Drills** are made from your reviewed games' mistakes and blunders: the weak spots of each game, with the reviewed move's best and second lines, deduplicated by key, biggest swing first. `make_from_weak_spots` reads your newest 500 games. Every change is announced on `player:you`.
- **Progress:** your trend over your newest 1,000 games, the drill hit rate (window 50), and every champion version in the store, anchored at full strength's calibrated rating once it has games.

**Events:**

| Kind             | Payload                                    | Topic             | Kept for replay |
| ---------------- | ------------------------------------------ | ----------------- | --------------- |
| `drill.upserted` | `Drill`                                    | `player:you`      | yes             |
| `drill.answered` | `{ drill, outcome, at }`                   | `player:you`      | yes             |
| `drill.deleted`  | `{ id }`                                   | `player:you`      | yes             |
| `levels.updated` | `{ ratings: EntrantRating[], updated_at }` | `player:dameless` | yes             |

These kinds are in `core::events::EventKind` (payloads `Drill`, `DrillAnswered`, `DrillDeleted`, `LevelsUpdated`), with `WorkKind::Calibrate`.

### R4: training, the foundation

This is everything except training on big datasets, sized for 52 GB free on the Windows partition. The work happens once data and the GPU are ready.

- **Nets are build inputs.** A `nets` table holds each net: hash, file, architecture, parent net, the data it was trained on, its training run, offline metrics. A build's cache key becomes (commit, features, net). Reckless embeds its net at compile time (`EVALFILE`); `engine::build` already takes a networks directory. Versions show their net.
- **The GPU is a resource.** The ledger gains `gpu: 1`, beside cores. Training takes the GPU, never cores beyond one feeder thread, so a trial can run while a net trains.
- **Data.** A data registry of datasets: name, format (viriformat, Stockfish binpack), positions, size, source, path.
  - **Datagen** is work. Self-play with Reckless's `datagen` branch (`UseSoftNodes`) from DFRC or random openings, written as viriformat. Its size is capped by a setting (default 10 GB on Windows).
  - **Imports** of relabelled binpacks from HuggingFace (vondele/\*) shard by shard. Each download states its size and needs the user's go-ahead in the UI. On Linux this is where the big sets go.
  - Rescoring and filtering use the tools ported from Reckless's `nnue-tools-rsn-3` branch.
- **The trainer** (`trainer/`, its own Cargo workspace, so the app never needs CUDA to compile). It is built on demand, like an engine, by a `Train` executor.
  1. **A CPU reference of Reckless's network in plain Rust:** threat inputs, king buckets, the pairwise layer and 768 → 16 → 32 → 1 with output buckets, with Reckless's quantisation. **Parity test:** it loads the shipped net and matches the engine's evals on 10,000 positions. No GPU or CUDA is needed, so this is done fully on Windows now. **Done:** `trainer/reference`; on Reckless `main` (69aea90, net v60-7f587dfb) it matches the engine's `eval` on 10,000 of 10,000 random positions, through all eight output buckets, for AVX2 and scalar builds (`trainer/README.md`).
  2. **The bullet model** of the same architecture (bullet from git, pinned). Compiled only when the CUDA Toolkit is present (`CUDA_PATH`). Parity: weights loaded into it give the CPU reference's outputs. **Done:** `trainer/bullet` (bullet at 579b78b); with v60 loaded, its quantisation-aware forward pass matches the reference on 4,000 of 4,000 positions layer by layer (evals exact), the weights saved back from the GPU give a byte-identical file, and Reckless built with it benches 2837905 (`trainer/README.md`).
  3. **Training runs** as work: from scratch or a fine-tune of a parent net, with checkpoints, loss and gradient norms (bullet's event API) streamed as `train.progress` events. At the end, quantise, compute offline metrics, register the net. **Done (the runner):** `bullet-train train`: from scratch or `--parent`, viriformat/binpack/bulletformat, LR and WDL schedules, checkpoints and `--resume`, progress as JSON lines on stdout (loss, lr, grad norms, positions/s) for the `Train` executor to stream; a smoke fine-tune of v60 ran at ~760k positions/s on the RTX 3080 (1.45 GB VRAM) and the result plays in Reckless. The executor is built (see the R4 contract below); offline metrics remain.
- **Net ideas.** An idea of kind `net`: a run, which produces a net, then a version (the champion's code plus the new net), then fingerprint (bench must change), screen, short and long. Its sheet shows the loss curves next to the evidence.
- **On Windows now:** everything above, plus a **smoke run**: a tiny fine-tune of the shipped net on a small datagen set (about 1 GB). It proves the pipeline, not a stronger net. It needs the CUDA Toolkit, which the user installs (about 3 GB). The real runs and the first experiments (pawn-pair inputs, then queen-conditioned king buckets) wait for Linux and its disk.

#### The R4 contract

Built in `work` (`work::nets`, `work::data`, `work::train`, `work::trainer`; the pure rules and types in `core::training`; the GPU in `hub::Ledger`; tables in `store` migration 0003). **Done (the app side):** the registry of nets and datasets, the GPU in the ledger, the trainer's tools built on demand, the `Datagen`, `DatasetImport` and `Train` executors, builds keyed by (commit, features, net), net ideas through their funnel and their promotion. Tested with fakes (a fake `data`/`bullet-train` program, the fake toolchain, a local HTTP server); nothing in tests downloads or trains for real. Offline metrics (validation loss) are still not computed: `Net.metrics` is always null.

**What the server wires** (in this order, after `Ideas::register`):

```text
let probes: Arc<dyn Probes> = Arc::new(SystemProbes::default());          // shared with Diagnostics
let trainer = Arc::new(TrainerBuild::new(TrainerBuild::default_source(), hub.config.trainer_tools_dir()));
let nets = Nets::new(hub.clone());
let datasets = Datasets::new(versions.clone(), trainer, probes.clone());
datasets.register();                                // Datagen, DatasetImport executors
let training = Training::new(ideas.clone(), datasets.clone(), nets.clone(), probes);
training.register();                                // Train executor
scheduler.start().await?;
nets.ensure_shipped(&versions).await.ok();          // the champion's shipped net, when on disk
datasets.scan().await.ok();                         // dataset directories already in data/
```

`Ideas::register` already hooks `WorkKind::Train` into the funnel. `TrainerBuild::default_source()` is `DAMELESS_TRAINER_DIR`, else this repository's `trainer/`.

**Configuration:** `DAMELESS_GPUS` (default: 1 when `nvidia-smi -L` lists a GPU, else 0; read only by `Config::from_env`), `DAMELESS_DATA_CAP_GB` (default 10 on Windows, 200 elsewhere), `CUDA_PATH` (bullet is built and run only with it; its `bin\x64` and `bin` go on `PATH`). Data directory: `nets/<sha256>.nnue`, `data/<dataset>/` (each with `manifest.json`), `runs/idea-<id>/` or `runs/work-<id>/` (bullet's checkpoints while a run lasts), `downloads/` (imports in progress), `tools/trainer/<tree>/` (the built `data` and `bullet-train`) and `tools/trainer/target/`.

| Route                                                  | Service call                                                                                        | Answers                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GET /api/nets`                                        | `Nets::list()`                                                                                      | `[Net { id, name, sha256, path, arch, size_bytes, parent, dataset, run: { idea, work, superbatches, positions, final_loss }?, metrics, created_at }]`, newest first.                                                                                                                                   |
| `GET /api/nets/{id}`                                   | `Nets::get(id)`                                                                                     | `Net`; 404.                                                                                                                                                                                                                                                                                            |
| `GET /api/datasets`                                    | `Datasets::status()`                                                                                | `DataStatus { cap_bytes, used_bytes, disk_free_bytes, keep_free_bytes, datasets: [Dataset { id, name, format, positions, games, size_bytes, source: {type: datagen, settings} \| {type: import, url} \| {type: trials, idea} \| {type: other, source}, score_units, path, created_at, updated_at }] }` |
| `POST /api/datasets/datagen`                           | `Datasets::datagen(DatagenRequest { name?, positions, threads?, soft_nodes?, opening?, version? })` | 201, the `Work` (`kind: datagen`). Same name again = more of that dataset (resumes). 400 for a bad name or 0 positions; 409 when the cap or the disk's 15 GB floor leaves no room, or the name is not a datagen dataset.                                                                               |
| `POST /api/datasets/import`                            | `Datasets::import(ImportRequest { url, expected_bytes, sha256?, name, confirm })`                   | 201, the `Work` (`kind: dataset_import`). **400 without `confirm: true`**: the UI shows the size and asks first. 409 when the download plus its conversion (≈ 3.15× the download at peak, 2.15× kept) would pass the cap or leave under 15 GB free.                                                    |
| `DELETE /api/datasets/{id}` with `{ "confirm": true }` | `Datasets::delete(id, confirm)`                                                                     | The deleted `Dataset`. 400 without confirm; 409 while datagen, an import or a training run uses it; 404.                                                                                                                                                                                               |
| `POST /api/ideas/{id}/dataset` (optional)              | `Datasets::from_trials(idea, name?)`                                                                | The `Dataset` made from the idea's trial PGNs (`data convert`, `trials-<id>` by default). 404 without trial games.                                                                                                                                                                                     |
| `POST /api/ideas` with `kind: "net"` and `train`       | `Ideas::create(NewIdeaRequest { kind: Net, train: Some(TrainSpec), branch: "" .. })`                | 201, the `Idea`, funnel `train → fingerprint → screen → short [0, 3] (→ long with `long`)`. `branch` may be empty (the base's code). 400 for a bad schedule; 409 without a GPU.                                                                                                                        |
| `GET /api/ideas/{id}/train`                            | `Training::idea(id)`                                                                                | `TrainView { spec: TrainSpec, steps: [TrainStep { seq, superbatch, batch, loss, lr, grad_norm, positions_per_sec, elapsed_seconds }], net: Net? }`: the loss chart from its first load (every run of the idea, resumes included). 404 for an idea that is not a net.                                   |
| `POST /api/train` (a run outside any idea)             | `Training::submit(TrainSpec)`                                                                       | 201, the `Work` (`kind: train`). 400 bad schedule; 409 no GPU; 404 unknown parent net or dataset.                                                                                                                                                                                                      |
| `GET /api/work/{id}/train`                             | `Training::steps(work)`                                                                             | `[TrainStep]` of one run.                                                                                                                                                                                                                                                                              |

`TrainSpec { datasets: [id] (empty: every dataset in Reckless's internal units), parent: net id? (default: the base's net, i.e. the champion's), from_scratch, name?, schedule: { superbatches, batches_per_superbatch, batch_size, lr, final_lr, lr_schedule: cosine|linear|step|constant, wdl, final_wdl? } (default: the smoke fine-tune, 20 × 200 × 16,384 at lr 1e-4 → 1e-6, wdl 0.3), save_rate (10), keep_checkpoints (2) }`. Versions carry `net` (null: the commit's own); builds too. `GET /api/health` already reports CUDA and the GPU; `LedgerSnapshot` gains `gpus` and `gpu_grants`.

**Events:** `train.progress` `{ work, idea?, superbatch, superbatches, batch, batches_per_superbatch, loss, lr, positions_per_sec, grad_norm, elapsed_seconds }` at most once a second, on `idea:<id>` for a net idea's run, else on `work` (kept for replay); `net.upserted` `Net` and `dataset.upserted` `Dataset` on the new topic `training`. Deleting a dataset sends nothing: the client drops it from its list on the 200. Work items send `work.upserted`/`work.log` as any work (datagen's progress unit is `positions`, an import's `bytes`, a run's `batches`).

**CLI:**

```bash
dameless nets [--json]                                   # GET /api/nets
dameless data [--json]                                   # GET /api/datasets: each dataset, the cap and the room left
dameless data datagen --positions 20M [--name dfrc] [--threads 8] [--soft-nodes 5000] [--opening dfrc]
dameless data import <url> --bytes N [--sha256 H] --name <dataset> [--yes]   # asks "Download 1.2 GB? [y/N]" without --yes
dameless data delete <id|name> --yes
dameless data from-trials <idea id> [--name trials-12]
dameless train [--dataset 3 ...] [--parent <net id>] [--from-scratch] [--name n] [--superbatches 20] ...   # POST /api/train
dameless idea new --kind net --title "…" --hypothesis "…" [--superbatches 20 ...] [--long]
dameless train show <idea id>                            # GET /api/ideas/{id}/train: the loss curve as text
```

**How the executors behave:**

- **Datagen** (lane of its own, priority Idea): resolves the engine (`origin/datagen` by default) as a version and builds it like any version, embedding the champion's net when the champion is code plus a trained net. It asks for cores (default half the machine's), runs `data datagen --json --engine … --out data/<name> --threads <granted> --positions N --max-size <what the cap and the disk leave> --units reckless-internal`, and reads the tool's progress JSON lines from stderr (a small change in `trainer/data`: with `--json` it now writes them there). Stopping or a pause for live play kills the process; the manifest is rebuilt (`data manifest`) and the dataset updated; the next run on the same directory resumes it.
- **Import** (no lane, no cores: network work): checked again against the cap and the disk when it starts; streams the download to `downloads/` with progress; a server size other than the confirmed one, more bytes than confirmed, or another SHA-256 fails it and deletes the partial file. Then `data convert <shard> -o data/<name>/<shard>.vf` (a `.vf` is copied as is), `data manifest`, register, delete the download. Only PGN scores are converted to Reckless's internal units (`convert --units` reads PGN); a Stockfish binpack keeps Stockfish's scores, so its manifest has no units and default training leaves it out. Training on relabelled binpacks needs an eval scale for their units, not done yet.
- **Train** (the GPU's lane, priority Idea): 409-style failure without a GPU, with the message bullet's build gives without CUDA. It holds the GPU for the whole run (released while paused) and two to four cores for bullet's loader (`--read-threads`, `--map-threads`); checks that the disk keeps 15 GB after its checkpoints (`(keep_checkpoints + 1) × 0.72 GB`); then `bullet-train train` as `trainer/README.md` describes, `--parent <net file>` or `--resume <last checkpoint>`. Checkpoints beyond `keep_checkpoints` are deleted (bullet does it within a run; the executor also after a resume). On `done` the net is copied to `nets/` and registered, and the run's directory is removed. A stopped net idea resumed later starts a new `Train` work item that finds the checkpoint in `runs/idea-<id>/` and passes `--resume`.

**Promoting a net idea.** Reckless keeps its nets outside its repository (`networks/` is git-ignored; nets are releases of codedeliveryservice/RecklessNetworks, fetched by the build script), so nothing is committed or merged. Promotion records a new champion `v<n+1>` whose commit is the current champion's code and whose `net` is the trained net (`Versions::crown_net`); every build of the champion embeds it from then on (`EVALFILE`), a later code promotion keeps it, and ideas are tested with it (an idea's version inherits its base's net; a tune tunes with it). If the code moved since the net was tested, the net goes onto the current code and the promotion's log says so. Versions with a trained net have no `Bench:` line of their own (the commit's is measured with the shipped net), so the fingerprint compares measured benches only. **Publishing a net upstream** is manual and outside the app: the file is `nets/<sha256>.nnue`; name it `<name>-<first 8 hex of its sha256>.nnue`, attach it to a release of a networks repository, and change `NETWORK_NAME` in Reckless's `build/build.rs` in a commit with its new `Bench:` line. That commit is then an ordinary code idea.

### R5: Claude in the loop (done)

- **`/mcp`.** The API's routes as MCP tools, from the same handlers (rmcp, the stateless spec), with the same token. A Claude Code skill (`skill/`) documents the loop: read the logbook, propose ideas with a hypothesis, write the branch, queue a batch, read verdicts, promote.
- **Ideas carry their author and reasoning.** `author: claude`, notes in markdown. The logbook says who tried what and why.
- **Not built:** the app never runs Claude by itself. You start a Claude session; it drives the app.

What was built:

- **`POST /mcp`** (`crates/server/src/mcp/`): a Streamable HTTP MCP server on rmcp 3.8, stateless (the 2026-07-28 protocol, and an older client's `initialize` too), answering JSON, behind the API's own `host` and `guard` layers (bearer token, loopback `Host`, known `Origin`).
- **Tools are generated from the OpenAPI document** at startup, one per operation (85 today, plus the two below), named by operation id. The description is the route's summary and description plus its CLI command from `cli::COMMANDS`; the input schema is the path and query parameters plus `body`, with the component schemas it uses copied into `$defs`. A call is turned into a request and sent through the app's router, so a tool cannot drift from its route; a route's error is a tool error with its status and body. Left out: the event stream and `GET /api/token` (the OAuth callback is not in the document).
- **Annotations and warnings** from three tables in `mcp/tools.rs`, checked against the document by tests: `REACH` (open-world: Lichess actions on the user's real accounts say "acts on the user's real Lichess account: only when the user asked"; `push`, `setup` and dataset imports act on the outside world; explorer, tablebase, leaderboard, player lookups and imports read from Lichess), `DESTRUCTIVE` (deleting, stopping, resigning, promoting, rewording, pushing, logging out) and `FAIR_PLAY` (tools the policy refuses, which say so). GETs are read-only, PUT and DELETE idempotent.
- **Two convenience tools**, `wait_for_work` and `wait_for_idea`, poll the same routes every two seconds up to a timeout (default 600 s, at most an hour).
- **Server instructions** carry the loop in brief, fair play (never analyse a position from a game the user is playing; the API refuses with 409 `fair_play`; do not work around it) and the real-account rule.
- **The skill** (`skill/dameless/`): `SKILL.md` (connecting, the rules, the loop in short), `improving.md` (ideas step by step, a batch file, the funnels, reading verdicts, promoting, tuning, nets, versions) and `coaching.md` (imports, reviews, profiles, compare, drills without spoiling answers, repertoire, levels).
- **`AGENTS.md`** rewritten for the new app (R7's first item), and the README.
- Tests: unit tests for the generation (every operation but the excluded ones is a tool; schemas stand alone; the tables name real operations; real-account tools say so) and for turning calls into requests; integration tests (`crates/server/tests/mcp.rs`) for the guard, `tools/list` against the document, read and write calls through the routes, a route's error, `wait_for_work`, and an older client's `initialize`.

### R6: one library, one look

- **eos gains the patterns the app hand-styles:**
  - a non-modal side sheet
  - a popover
  - a selectable multi-line list row and a tree
  - a compact stat link
  - a chart with series highlighting
  - a virtual list
  - a fix for `EAppShell`'s grid letting content widen the page

  These go in the eos repository, are released, and the app switches to them.

- **A last audit:** the words table, one fact once, keyboard paths, phone and tablet, light and dark, `prefers-reduced-motion`, contrast.

### R7: cutover

1. **AGENTS.md and the skill are rewritten** for the new app; README.
2. **First real use with the user**, who logs in themselves:
   - your Lichess account: one game from the app, with moves, premoves, offers and abort, the lock lifting, and an import
   - the bot online, with one game.
3. **The old runner is stopped** (port 7878, `C:\dev\chess-live`). `next` is merged into `main`, which deploys chess.dame.gg.
4. **A Linux handoff doc:** clone, toolchain (Rust, Node, clang, CUDA), `dameless setup`, check the harness, then data and the first real training runs on the big partition.

## Order and what can run at once

R1 first. Then R2, R3 and R4 at the same time: their files don't overlap, and R4's trainer is its own workspace. R5 after R1's API is final. R6 after R3 and R4 settle the UI. R7 last.
