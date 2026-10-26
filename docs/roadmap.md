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

- **Check the harness:** `main` against a branch identical to it (`test/comment`) at VSTC [0, 3], plus an optional 2,000-pair symmetry run. The result is stored per machine; Settings and every trial verdict show it.
- **Bench line fixer**, when an idea is created.
- **Health** in Settings: fastchess and the book are installed, the engine checkout is found, clang is there (Syzygy builds), and the disk has free space for builds, games and data.

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
- `level_games (game PRIMARY KEY, white, black, calibration)`: the entrant at each seat of a game played at a level. `play` writes it when a local game with a level starts, and `work` writes it for calibration games.
- `level_ratings (entrant PRIMARY KEY, elo, low, high, games, updated_at)`: the last fit.
- Methods:
  - `insert_drills`, `drill`, `drills`, `drill_keys`, `record_drill_answer(graded, answer)` (one transaction), `drill_answers(since)`, `drills_started_since`, `delete_drill`
  - `record_level_game`, `level_seats`, `level_games` (finished only), `put_level_ratings` (replaces the fit), `level_ratings`

**Routes** (the server's; every one with a CLI command of the same name):

```
GET    /api/levels                        { levels: [{ level: Level, rating: Estimate | null, games }], you: EntrantRating | null,
                                            suggested: level id | null (nearest your rating), updated_at }
GET    /api/levels/plan?count=3           PlannedMatch[]
POST   /api/levels/calibrate              { matches?: PlannedMatch[] } → Work (default: plan(…, 3)); refits when done
POST   /api/games                         gains `level` on a Dameless seat: { white: "you", black: { player: "dameless", level: 5 }, time, … }
                                            (a bare "dameless" is full strength, as today); the game's seats go to level_games
GET    /api/drills                        Drill[] (all, soonest due first)
POST   /api/drills                        { limit? } → Drill[] made from your profile's weak spots (new ones only)
GET    /api/drills/today?new_per_day=&limit=   { drills: Drill[], due, new, started_today }
POST   /api/drills/{id}/answer            { move } (UCI or SAN) → { outcome, drill: Drill (rescheduled), best_moves, best_san, line, played_san }
DELETE /api/drills/{id}
GET    /api/drills/trend?window=50        HitRatePoint[]
GET    /api/players/you/repertoire?side=white|black&max_plies=&min_games=   Repertoire (409 fair_play while assist ≠ allowed)
GET    /api/progress?window=10            Progress
```

- **Seeing a drill** (`GET /api/drills/today`) shows `fen`, `side` and `theme`. The web client must not show `best_moves`, `best_san` or `line` before the answer; the answer route returns them.
- **Fair play.** Drills are not refused by the server. The web client hides them while `policy.assist ≠ allowed`. The repertoire is refused with 409 while you play.
- **Theory for the repertoire.** By default a position is theory when it was reached within the named opening's plies (`opening.ply`) in any of your Lichess games: free, and Lichess's own book. The explorer (masters) can refine it later, outside live games.
- **Calibration** is background work at `Priority::Idea`, the lowest priority. It plays fastchess at fixed nodes per level with the tests' book, and level 1 to 6 seats use `choose_move`. It needs a level-aware engine wrapper (fastchess cannot apply a softmax), so the first version can calibrate levels 7 to 12 (MultiPV 1) with fastchess and levels 1 to 6 with the Referee's engine seats.
- **When the fit runs.** The server refits (`fit` over `level_games` with `priors(ladder, your Lichess rating)`) after each calibration match and after each finished game of yours against a level, then stores it.

**Events:**

| Kind             | Payload                                    | Topic             | Kept for replay |
| ---------------- | ------------------------------------------ | ----------------- | --------------- |
| `drill.upserted` | `Drill`                                    | `player:you`      | yes             |
| `drill.answered` | `{ drill, outcome, at }`                   | `player:you`      | yes             |
| `drill.deleted`  | `{ id }`                                   | `player:you`      | yes             |
| `levels.updated` | `{ ratings: EntrantRating[], updated_at }` | `player:dameless` | yes             |

These kinds are not yet in `core::events::EventKind`, a closed enum the server owns. Adding them is the server's first R3 change, together with a `WorkKind::Calibrate`.

### R4: training, the foundation

This is everything except training on big datasets, sized for 52 GB free on the Windows partition. The work happens once data and the GPU are ready.

- **Nets are build inputs.** A `nets` table holds each net: hash, file, architecture, parent net, the data it was trained on, its training run, offline metrics. A build's cache key becomes (commit, features, net). Reckless embeds its net at compile time (`EVALFILE`); `engine::build` already takes a networks directory. Versions show their net.
- **The GPU is a resource.** The ledger gains `gpu: 1`, beside cores. Training takes the GPU, never cores beyond one feeder thread, so a trial can run while a net trains.
- **Data.** A data registry of datasets: name, format (viriformat, Stockfish binpack), positions, size, source, path.
  - **Datagen** is work. Self-play with Reckless's `datagen` branch (`UseSoftNodes`) from DFRC or random openings, written as viriformat. Its size is capped by a setting (default 10 GB on Windows).
  - **Imports** of relabelled binpacks from HuggingFace (vondele/\*) shard by shard. Each download states its size and needs the user's go-ahead in the UI. On Linux this is where the big sets go.
  - Rescoring and filtering use the tools ported from Reckless's `nnue-tools-rsn-3` branch.
- **The trainer** (`trainer/`, its own Cargo workspace, so the app never needs CUDA to compile). It is built on demand, like an engine, by a `Train` executor.
  1. **A CPU reference of Reckless's network in plain Rust:** threat inputs, king buckets, the pairwise layer and 768 → 16 → 32 → 1 with output buckets, with Reckless's quantisation. **Parity test:** it loads the shipped net and matches the engine's evals on 10,000 positions. No GPU or CUDA is needed, so this is done fully on Windows now.
  2. **The bullet model** of the same architecture (bullet from git, pinned). Compiled only when the CUDA Toolkit is present (`CUDA_PATH`). Parity: weights loaded into it give the CPU reference's outputs.
  3. **Training runs** as work: from scratch or a fine-tune of a parent net, with checkpoints, loss and gradient norms (bullet's event API) streamed as `train.progress` events. At the end, quantise, compute offline metrics, register the net.
- **Net ideas.** An idea of kind `net`: a run, which produces a net, then a version (the champion's code plus the new net), then fingerprint (bench must change), screen, short and long. Its sheet shows the loss curves next to the evidence.
- **On Windows now:** everything above, plus a **smoke run**: a tiny fine-tune of the shipped net on a small datagen set (about 1 GB). It proves the pipeline, not a stronger net. It needs the CUDA Toolkit, which the user installs (about 3 GB). The real runs and the first experiments (pawn-pair inputs, then queen-conditioned king buckets) wait for Linux and its disk.

### R5: Claude in the loop

- **`/mcp`.** The API's routes as MCP tools, from the same handlers (rmcp, the stateless spec), with the same token. A Claude Code skill (`skill/`) documents the loop: read the logbook, propose ideas with a hypothesis, write the branch, queue a batch, read verdicts, promote.
- **Ideas carry their author and reasoning.** `author: claude`, notes in markdown. The logbook says who tried what and why.
- **Not built:** the app never runs Claude by itself. You start a Claude session; it drives the app.

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
