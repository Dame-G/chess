# Chess: the roadmap

`docs/design.md` is the contract; this file says what is built and what comes next, in order. Each step ends usable.

## Where it stands

The app plays, reviews and analyzes, with any UCI engine:

- **Play:** local games against an engine at full strength, at a level of the analysis engine (about 800 to full, calibrated by the app), or against Maia at 1100 to 1900; your Lichess games played from the same board through the Board API.
- **Review and analyze:** reviews on the analysis engine with Lichess's judgements, profiles and comparisons, weak spots and drills (SM-2, themes, hints), your repertoire and opening prep, your progress, free analysis with the explorer and the tablebase, imports of your history and anyone's.
- **Engines:** the engines list (Stockfish 19 and Maia on lc0 as built-ins that `chess setup` downloads, and any engine added by path), the analysis engine as a setting, reviews and the position cache keyed by the engine and what it calls itself.
- **Claude in the loop:** `/mcp` serves every route as a tool, generated from the OpenAPI document, with the fair-play and real-account rules in its instructions; the `chess` skill documents driving the app.

## Next

1. **The name.** The app becomes "Chess": the crates, the binary, the environment variables and the data directory are renamed in one change, with the old data directory picked up once.
2. **The design pass on the web app.** The opponent picker as an engines list (levels, Maia by rating, your engines), Settings' Engines section and setup with its sizes, and the screens that lost the engine's sheet, polished to the brief (`docs/ui-brief.md`) and the kit.
3. **Calibrate the Stockfish ladder.** The node budgets were chosen for Reckless; calibrate on Stockfish (`chess levels calibrate`) and, if the spread is off, retune the budgets and temperatures so the ladder spans 800 to full strength evenly.
4. **Maia everywhere.** lc0 publishes Windows builds only: on Linux and macOS, setup uses an `lc0` on the PATH. Build or fetch lc0 for those platforms, or document it in `chess diagnostics`.
5. **eos gains the patterns the app hand-styles** (a non-modal side sheet, a popover, a selectable list row and a tree, a compact stat link, a chart with series highlighting, a virtual list), released and adopted; then a last audit of words, keyboard paths, phone and tablet, both schemes, reduced motion and contrast.

## The player side in detail

The pure logic is in `core` and the tables in `store`; `play`, `analysis`, the server and the web app build on these shapes.

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
  - `Progress { series: Series[] }`, where `Series { player, metric: rating | accuracy | drill_hit_rate, points: [{ at, value, game }] }`.
  - `your_series(trend, drill_trend, window)` gives accuracy as a rolling mean of 10 games; `progress(...)` builds the three.

**The ladder** (on the analysis engine, Stockfish by default; nominal rating, nodes per move, MultiPV, temperature in win% points):

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

These were chosen for Reckless and carried over to Stockfish, which is stronger per node, so the nominal ratings likely read low until calibration replaces them. The UI shows a level's calibrated rating once it has one (games > 0), else "about <nominal>".

**Storage** (migrations `0002_player.sql`, `0006_drill_themes.sql`; repositories `store::drills`, `store::levels`):

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
POST   /api/levels/calibrate              { matches?: PlannedMatch[], count?, games_per_pair? } → Work (default: plan(…, 3));
                                            refits after each match
POST   /api/games                         a seat at a level: "stockfish@5" (or { player: "stockfish", level: 5 }), optional `seed`;
                                            a bare engine id is full strength; the game's seats go to level_games
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
- **Calibration** is background work at `Priority::Background`, the lowest priority (beside reviews of all of a player's games). Every level plays at fixed nodes from four random plies, and level 1 to 6 seats use `choose_move`, so all calibration games are played by `play`'s engine seats.
- **When the fit runs.** The server refits (`fit` over `level_games` with `priors(ladder, your Lichess rating)`) after each calibration match and after each finished game of yours against a level, then stores it.

**Services and wiring** (built: `play::levels`, `play::calibrate`, `analysis::drills`, `analysis::repertoire`, `analysis::progress`). Each route calls one function.

- **Construction** (in the server's app):
  - `Games::new(hub, engines, timing)` makes its own `Levels` (the default ladder): `games.levels()`. Set the anchor once the connections exist: `games.levels().set_anchor(Arc::new(move || play::your_anchor(&connections.status(ConnectionKey::You))))`. It uses your rapid rating, else blitz, classical, bullet, an established rating before a provisional one.
  - `scheduler.register(Arc::new(play::Calibrate::new(games.clone())))` for `WorkKind::Calibrate` (its own lane, priority Background).
  - `let drills = analysis::Drills::new(hub.clone())`, and the review executor registered as `Reviews::new(eval).with_drills(drills.clone())`: each review of one of your games then makes drills from it.
  - Repertoire and progress are methods of the existing `analysis::Profiles`.
- **Routes → calls:**

```
GET    /api/levels                 games.levels().overview()                    → play::LevelsOverview
GET    /api/levels/plan?count=3    games.levels().plan(count)                   → PlannedMatch[]
POST   /api/levels/calibrate       body play::CalibrateRequest { matches?, count?, games_per_pair? }
                                   scheduler.submit(games.levels().prepare_calibration(&engines, req).await?) → Work
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

- **Level seats** (`play::SeatRequest`): `<engine id>@<level>`, or the object `{ "player": "stockfish", "level"?: 5 }`, for the analysis engine only (`Engine.levels`; another engine at a level is 400). A bare engine id is full strength on the clock (or its fixed budget). An unknown level is 400.
  - A seat at a level searches the level's nodes per move, whatever the game's clock (full strength, `nodes: None`, plays the clock), with its MultiPV. It plays `choose_move`'s pick, drawn from a SplitMix64 seeded by `NewGameRequest.seed` (random when none; the seed is not stored).
  - Its name is "<engine name> <rating>" ("Stockfish 1650": calibrated once the level has games, else nominal), with `rating` set.
  - When every seat is a level or you, the game goes to `level_games` (not calibration), and the ladder is refitted when it ends.
- **Calibration work:** the spec is `play::CalibrateSpec { matches, engine }`, resolved when queued: the plan when no matches are given, `games_per_pair` applied, the analysis engine's id.
  - It plays each match's games, as many at once as `cx.cores(2, all)` grants (two cores a game). Its engines start without ledger reservations, so they never pause other work as live play does.
  - Colours alternate. Games start from four random plies. Each level plays at `calibration_nodes()` (full strength 1,000,000), with adjudication on.
  - Games are stored as finished local games whose seats are named "<engine name> <rating>" with their `level` set, and `level_games.calibration` set. They are not announced one by one (no `game.upserted`); game lists should leave calibration games out once they have a filter for it.
  - It checkpoints after each batch (`CalibrateCheckpoint { done per match, failed }`), stops at a safe point before each batch, refits after each match (`levels.updated`), and fails when every game of a batch fails. Its result is `{ games, failed, matches }`.
- **The fit:** `Levels::refit()` runs `fit(priors(ladder, anchor), pair_results(level_games))`, stores it and emits `levels.updated`. Without your Lichess rating, You has no prior, so your games are left out of the fit and the ladder keeps its nominal scale.
- **Drills** are made from your reviewed games' mistakes and blunders: the weak spots of each game, with the reviewed move's best and second lines, deduplicated by key, biggest swing first. `make_from_weak_spots` reads your newest 500 games. Every change is announced on `player:you`.
- **Progress:** your trend over your newest 1,000 games and the drill hit rate (window 50).

**Events:**

| Kind             | Payload                                    | Topic        | Kept for replay |
| ---------------- | ------------------------------------------ | ------------ | --------------- |
| `drill.upserted` | `Drill`                                    | `player:you` | yes             |
| `drill.answered` | `{ drill, outcome, at }`                   | `player:you` | yes             |
| `drill.deleted`  | `{ id }`                                   | `player:you` | yes             |
| `levels.updated` | `{ ratings: EntrantRating[], updated_at }` | `engines`    | yes             |

These kinds are in `core::events::EventKind` (payloads `Drill`, `DrillAnswered`, `DrillDeleted`, `LevelsUpdated`), with `WorkKind::Calibrate`.
