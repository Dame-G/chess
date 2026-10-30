# Chess: the design

This is the contract for the app. Every crate, route, event, table and screen follows it. When code and this file disagree, one of them is wrong: fix the code, or change this file in the same commit and say why.

## The idea

**Play, review, analyze.** A chess app for one person, with any UCI engine as an opponent or the analysis engine. Every feature is one of:

- **playing** a game (you against an engine here, you on Lichess, two engines against each other),
- **reading** games (review, the eval graph, the better move),
- **adding games up** (your profile, a comparison, drills from your mistakes, your repertoire, your progress).

So there are three things to understand, **the Board, Games and Players**, and everything else is hidden machinery.

### Principles

1. **Never leave the board.** The app is the board plus a panel. Profiles and settings open beside or over it, never instead of it.
2. **Everything is a game.** Local games, your Lichess games and imported history are one record, one list, one viewer.
3. **Show strength, hide machinery.** Ratings, accuracy, judgements. Engines' paths, node budgets and concurrency exist but are never the first thing you see.
4. **Fair play by construction.** While you play on Lichess, analysis is not on screen to be refused. One policy, enforced in one place on the server.
5. **One set of verbs.** Every action in the UI is a route and a CLI command with the same name. Claude and you drive the same app.

## Words

One vocabulary everywhere: code, routes, the CLI, the UI, docs.

| Word                | Means                                                                                                                    |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| **You**             | The person. Has a linked Lichess account (optional).                                                                     |
| **Engine**          | A UCI engine of the engines list: a built-in (Stockfish, Reckless) or one added by path. Named by an id: `stockfish`.    |
| **Analysis engine** | The engine reviews, live evaluation and drills run on. Stockfish by default.                                             |
| **Level**           | An engine at a node budget (and a MultiPV choice at the lowest), from about 800 to full strength, calibrated per engine. |
| **Game**            | Two seats, moves, clocks, a result. Wherever it was played.                                                              |
| **Player**          | Someone who sits at a board: You, an engine, a person on Lichess.                                                        |
| **Work**            | Anything running in the background: a review, an import, calibrating the levels, setup.                                  |
| **Review**          | An engine reading every move of a finished game.                                                                         |
| **Connection**      | Your Lichess account linked to the app.                                                                                  |

Never: build, match, experiment, test, batch, job, runner, human.

## Shape of the system

```
apps/web  (Vue 3.5 + eos + chessground)  ──REST + SSE──►  chess serve  (127.0.0.1:7979)
                                                          │
crates/                                                    ▼
  core       pure domain: types, chess, review rules, referee rules, levels, engines, Lichess rules, events
  hub        the shared runtime: config, data dir, secrets, event bus, policy (fair play), core ledger
  store      SQLite: schema, migrations, one writer, repositories
  engine     adapter: UCI processes, engine pool, downloads of the built-in engines
  lichess    adapter: HTTP client with Lichess's rate rules, OAuth PKCE, NDJSON streams, the Board API, import, explorer
  play       the Referee runtime: game tasks, seat controllers (engine, local, Lichess board), your connection, levels
  analysis   the evaluation service (warm engine pool, position cache), reviews, profiles, drills, repertoire, progress
  work       the scheduler, the engines list, setup, diagnostics
  server     axum API (utoipa → OpenAPI), SSE, /mcp, wiring, the `chess` binary (serve + CLI)
```

Dependencies only point down this list's layers:

- `core` depends on nothing of ours and does no I/O (no tokio, no filesystem, no network). Everything in it is unit tested.
- `engine` and `lichess` are **adapters**: they know their outside world (UCI, processes, GitHub release downloads; Lichess's HTTP API) and keep their own wire types (a UCI `info` line, a Lichess game JSON). They do not depend on `core`, the store or the bus; `play`, `analysis` and `work` map their types to `core`'s.
- `store` depends on `core` only. `hub` depends on `core` and `store` and owns the bundle every service gets, `Hub { config, store, bus, policy, ledger, secrets, token }`: the store stays a plain library a test can open alone, and the hub is the runtime that wires it to the rest.
- Lichess's business rules (the Board API's speed rules, how a status ends a game) are domain logic: their pure parts live in `core::lichess`, their running parts in `play`.
- `work` depends on `core`, `hub`, `store` and `engine`. It owns the scheduler and **the engines list** (`work::Engines`), which `play` and `analysis` need to turn an engine id into something they can start. So `play` and `analysis` depend on `work`; `work` never depends on them.
- `server` depends on everything and holds no domain logic: routes translate HTTP to service calls, and it wires the services together (registers every executor with the scheduler, starts the connection).

### Interfaces between the services

- **Running work** (`hub::work`): `trait Executor { fn kind() -> WorkKind; async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> }`. `WorkContext` gives the spec (`cx.spec::<T>()`), the last checkpoint (`cx.checkpoint::<T>()`, `cx.save_checkpoint(&t)`), progress (`cx.progress(p)`, throttled; `cx.progress_now(p)`), the log (`cx.log(line)`), cores (`cx.cores(min, max).await -> Option<Grant>`, held only while used), safe points (`cx.safe_point(Some(AssistKind::Review)).await -> bool`: waits while paused or while fair play forbids the assistance; `false` means stopped), and `cx.stopped()` to return when stopped. Executors: `Setup` in `work`; `Review` in `analysis`; `Import` and `Calibrate` in `play`. The server registers them all with `work::Scheduler`.
- **Queueing work**: `work::Scheduler::submit(NewWork) -> Work` (any crate holding the scheduler; the routes do), `stop`, `pause`, `resume`. `Scheduler` is cheap to clone.
- **Engines** (`work::Engines`, cheap to clone): `overview() -> EnginesOverview`, `get(id)`, `add(NewEngineRequest)`, `remove(id)`, `set_analysis(id)`, `install(EngineRecord)` (setup), and the trait `work::EngineSource { async fn ready(id: Option<&str>) -> EngineBinary }` (`None`: the analysis engine; 404 unknown, 409 not installed), which `Engines` implements and `play` and `analysis` take as `Arc<dyn EngineSource>`. `EngineBinary { engine: Engine, path, args, options }`. Tests use `work::FixedEngines`, which points ids at a fake engine.
- **Evaluation** (`analysis::Eval::new(hub, engines)`, cheap to clone): `start_session(EvalRequest { fen, moves, engine, multipv, budget, threads, client }) -> EvalSession { session, engine, fen }` streaming `eval.info` on `eval:<session>` (one session per client; it reserves its cores, and stops when the policy turns off), `stop_session(id)`, `stop_client`, `stop_all`; `evaluate(fen, &EngineBinary, nodes, multipv) -> PositionEval` and `evaluate_positions(&[ToSearch], &EngineBinary, nodes, multipv, engines) -> Evaluated` through the position cache. `play` uses the engine pool directly for its engine seats (it needs clocks and `go wtime btime`), not `Eval`.
- **Reviews and profiles** (`analysis::Reviews::new(eval)`, `analysis::Profiles::new(hub)`): `Reviews` is the `Review` executor and queues reviews (`prepare(ReviewRequest) -> NewWork`, resolved when queued, or `submit(&scheduler, req) -> Work`); `get(game, engine, nodes)` backs `GET /api/games/{id}/review`. `Profiles::profile(key, filter)`, `compare(keys, filter)` and `player(key)` back the player routes.
- **Connections** (`play::Connections::new(hub, games, lichess_config)`, `start()`): owns your Lichess client, shared by everything that talks to Lichess for your account (imports, the explorer, seat controllers), and `anonymous()` for public calls. It keeps `hub.policy` current from your live games. `set_token`, `logout`, `start_login(return_to, redirect_uri)`, `finish_login(code, state, error) -> LoginFinished { return_to, result }`, `explorer`, `tablebase`, `user`, `leaderboard`, `challenge(key, body)`; `you()` (seek, challenges) for the rest.

The binary is `chess` (crate `server`). `chess serve` runs the app; every other subcommand is a client of a running app over HTTP, one subcommand per route.

### Process and data

- Listens on `127.0.0.1:7979` (`CHESS_LISTEN`). Bearer token for every request except `GET /api/health`; `Origin`/`Host` checks against DNS rebinding; the hosted UI origin `https://chess.dame.gg` is allowed with the token.
- Data directory: `dirs::data_local_dir()/chess` (`CHESS_DATA`): `chess.db`, `token`, `secrets/lichess-you` (0600 on Unix), `tools/` (the built-in engines: `stockfish/`, `reckless/`), `games/` (archived PGN, zstd), `logs/work-<id>.log`. The first start with the default folder copies your files from the old `dameless` folder, if there is one and no `chess` folder yet (`hub::legacy`).
- `LICHESS_URL`, `LICHESS_EXPLORER_URL`, `LICHESS_TABLEBASE_URL` point the adapters elsewhere (a mock in tests).
- In debug builds the server serves `apps/web/dist` from disk; in release it embeds it (rust-embed).

## Core types (`crates/core`)

Rust sketches; serde names are `snake_case`, enums are internally tagged with `type` unless they are plain strings. Every public type that crosses the API derives `utoipa::ToSchema` (utoipa is allowed in core; it is not I/O).

### Identity

```rust
pub struct GameId(i64);  pub struct WorkId(i64);  pub struct ReviewId(i64);

pub enum Side { White, Black }
pub enum Color { White, Black, Random }           // a choice when starting a game

/// Who sits at a board. Serialized as a string key: "you", "engine:<id>", "lichess:<id>".
pub enum PlayerKey { You, Engine(String), Lichess(String) }
pub enum ConnectionKey { You }                    // `you` in routes; the only connection
```

An engine id is a slug: lower-case letters, digits and `-` (`stockfish`, `reckless`, `my-engine-2`), never `you`.

### Scores

All stored and transmitted scores are **from White's point of view**. One rule, no exceptions; the UI flips for display.

```rust
pub enum Score { Cp(i32), Mate(i32) }            // {"cp": 31} | {"mate": -3}; mate 0 = side to move is mated
```

### Time

```rust
pub enum TimeControl {
    Clock { initial_ms: u64, increment_ms: u64 },      // games write "3+2" (minutes+seconds)
    Correspondence { days: u32 },
    Movetime { ms: u64 },
    Nodes { nodes: u64 },
}
pub enum Speed { UltraBullet, Bullet, Blitz, Rapid, Classical, Correspondence }  // Lichess's: initial + 40 × increment
```

### Games

```rust
pub struct Seat {
    pub player: PlayerKey,
    pub name: String,                  // as shown: the Lichess username for You, "Stockfish", "Stockfish 1650" at a level
    pub level: Option<u32>,            // a level of the ladder, or Lichess's AI level
    pub engine: Option<String>,        // what an engine seat's engine called itself: "Stockfish 19"
    pub rating: Option<u32>,
    pub title: Option<String>,
}

pub enum Source {
    Local,                                                                          // played here
    Lichess { connection: ConnectionKey, id: String, rated: bool, speed: Speed },   // played through your connection
    Import  { owner: PlayerKey, id: String, rated: bool, speed: Speed },            // history read from Lichess
}

pub enum GameStatus { Starting, Live, Finished, Aborted }
pub enum Outcome { WhiteWins, BlackWins, Draw }
pub enum Ending {
    Checkmate, Resignation, Timeout, Stalemate, Repetition, FiftyMoves, InsufficientMaterial,
    Agreement, Adjudication, Abandoned, Aborted, EngineFailure, IllegalMove, Other,
}   // Lichess statuses without a word here (noStart with a winner, cheat, variantEnd) are Other; noStart without one is Aborted

pub struct GameSummary {             // what lists, events and the Games rail use
    pub id: GameId,
    pub white: Seat, pub black: Seat,
    pub source: Source,
    pub time: TimeControl,
    pub status: GameStatus,
    pub outcome: Option<Outcome>, pub ending: Option<Ending>,
    pub opening: Option<Opening>,                   // {eco, name, ply}
    pub plies: u32,
    pub started_at: Timestamp, pub ended_at: Option<Timestamp>,
    pub you: Option<Side>,                          // the side You played, if any
    pub accuracy: Option<SidePair<f32>>,            // from the newest review
    pub url: Option<String>,                        // lichess.org link
}

pub struct Game {                    // the full record
    pub summary: GameSummary,
    pub start_fen: Option<String>,                  // None = standard start
    pub moves: Vec<Move>,
    pub clocks: Option<SidePair<u64>>,              // live: the clock each side holds now, ms
    pub offers: Offers,                             // {draw: Option<Side>, takeback: Option<Side>}
    pub actions: Vec<SeatAction>,                   // what You may do now (move, draw, takeback, resign, abort, claim…)
}

pub struct Move {
    pub uci: String, pub san: String,
    pub clock_ms: Option<u64>,       // the mover's clock after the move
    pub think_ms: Option<u64>,
    pub eval: Option<Score>,         // the mover's engine's own score (White POV), engines only
    pub depth: Option<u32>, pub nodes: Option<u64>,
}
```

Games against Lichess's AI are recorded against `engine:stockfish` with the AI's level on the seat. Games recorded before the engines list keep reading: Chess's seats are `engine:reckless` (named "Reckless …", `engine` the commit it played, "Reckless abcdef1"), and the old BOT account's Lichess games are Reckless's imported history (`Import { owner: engine:reckless }`).

### Engines

```rust
pub enum EngineKind { Builtin, Custom }

pub struct Engine {
    pub id: String,                   // the slug seats and player keys use
    pub name: String,                 // "Stockfish", "Reckless"
    pub kind: EngineKind,
    pub path: Option<String>,         // none for a built-in that is not installed
    pub args: Vec<String>,
    pub options: BTreeMap<String, String>,   // UCI options set after the handshake, before the app's own
    pub id_name: Option<String>,      // what it calls itself (UCI `id name`), read when added or installed: "Stockfish 19"
    pub nodes: Option<u64>,           // nodes a move whatever the clock (an engine you added that way): no levels
    pub rating: Option<u32>,          // a rating it plays at by design
    pub multipv: bool,                // it declares UCI's MultiPV (read when added or installed; unknown reads as yes)
    pub installed: bool,              // its binary is there
    pub analysis: bool,               // it is the analysis engine
    pub levels: bool,                 // it has a ladder of levels: installed, and no fixed budget
    pub created_at: Option<Timestamp>,
}
pub struct EnginesOverview { pub analysis: String, pub engines: Vec<Engine> }   // built-ins first, then yours by name
```

- **Built-ins** (`core::engines::BUILTINS`): `stockfish` (Stockfish 19) and `reckless` (Reckless 0.9.0). They are always listed; one that is not installed has `installed: false` and no path. Each is its own setup package (`SetupPackage::Stockfish`, `Reckless`). `chess setup` downloads them into `tools/` (`work::setup`): Stockfish's release build for this platform, picked from the release's asset list by OS and architecture (universal, else AVX2, else plain x86-64; the Windows fallback is `stockfish-windows-x86-64-universal.zip`), and Reckless's, a bare executable picked from codedeliveryservice/Reckless's v0.9.0 assets (`reckless-<os>-avx2` when the CPU has AVX2, else `reckless-<os>-generic`, `.exe` on Windows; `reckless-macos` on macOS; when GitHub does not answer, the name is built and the size is unknown). Each installed engine is started once to read its `id name` and whether it declares `MultiPV`. The old player key `engine:reckless` (Chess's seats from before the engines list) is the built-in's, so those games and new ones are one player.
- **Maia** (lc0 with nine nets, `maia-1100` … `maia-1900`) was a built-in until migration 8 removed it: its rows leave the engines table, and games against it keep their seats and names (a player no longer on the list is named from its id).
- **Engines you add** (`kind: custom`): a name, a path, arguments, options, and optionally `nodes` and `rating`. The app starts it once to check it answers UCI's handshake (400 when it does not), keeps what it calls itself, and makes its id from the name unless given one (409 when taken by a built-in or another engine).
- **The analysis engine** is `meta` `engines.analysis` (default `stockfish`); an engine with a fixed budget cannot be it (400). Removing it from the list makes Stockfish the analysis engine again.
- **What results are keyed by:** an engine's `cache_key()` is `<id>|<id_name>` (`stockfish|Stockfish 19`): reviews and the position cache use it, so a new release of an engine never reads an old one's results as its own, and its `label()` (`id_name`, else the name) is what reviews and seats show.
- **Seats** (`core::engines::SeatSpec`): `you`, `<engine id>` (full strength on the clock, or its fixed budget), `<engine id>@<level>` (a level of that engine's ladder: any engine whose `levels` is true; 400 for one with a fixed budget). `engine:<id>` reads too.

### Reviews

The judge (`review/judge.rs`): Lichess's win%, move accuracy, game accuracy (volatility-weighted and harmonic), ACPL, judgements, our classifications (brilliant, great, best, excellent, good, book, inaccuracy, mistake, blunder, miss), phases (lila's Divider), time-trouble blunders, with lila's test values.

```rust
pub struct Review { pub id: ReviewId, pub game: GameId, pub engine: String, pub nodes: u64,
                    pub summary: ReviewSummary, pub plies: Vec<ReviewedMove>, pub created_at: Timestamp }
```

`engine` is the label of the engine that made it (`Stockfish 19`; reviews made before the engines list read `Reckless <commit>`), and the summary names it too. `ReviewSummary` and `ReviewedMove` have every score White POV. The searched positions they are judged from (`PositionEval { lines: [{uci, score, pv}], depth, nodes }`) are White POV too: `analysis` flips the engine's side-to-move scores before caching.

### Players and profiles

```rust
pub struct Player {
    pub key: PlayerKey, pub name: String, pub title: Option<String>,
    pub ratings: Ratings,                       // Lichess's per speed, when known
    pub lichess: Option<String>,                // username
}
pub struct Profile {                            // GET /api/players/{key}/profile
    pub player: Player,
    pub filter: ProfileFilter,                  // speeds, rated, since, limit
    pub games: u32, pub reviewed: u32,
    pub results: Results,                       // overall, as White, as Black
    pub accuracy: PhaseStats,                   // overall, opening, middlegame, endgame, by speed
    pub acpl: Option<f32>,
    pub counts: ClassCounts, pub counts_per_game: ClassRates,   // totals, and per reviewed game
    pub openings: Vec<OpeningStats>,
    pub time: TimeStats,
    pub trend: Vec<TrendPoint>,                 // accuracy and rating over time, one point per game
    pub moments: Moments,                       // best moves, worst blunders, most/least accurate game
    pub weak_spots: Vec<WeakSpot>,              // You only: positions from your mistakes, to play out
    // and: pending_review, unreviewed_total, first_game, last_game, rating, judgements,
    // opponents {average_rating, performance}, longest_win_streak
}
```

An engine's player name comes from the engines list, else the built-in's name, else its id capitalised (`engine:reckless` is "Reckless"). Comparison is profiles side by side: `GET /api/compare?players=you,engine:stockfish,lichess:cutemouse83&…` returns `Comparison { profiles: Vec<Profile>, leaders: Vec<Leader { row, players: [index] }> }` (ties lead together; a row only one player has figures for leads nobody).

A **weak spot** is a position before one of your mistakes or blunders in a reviewed game (`{game, ply, fen, played, best, swing, opening, phase, recurring}`), recurring themes first (same opening family and phase). "Play this out" starts a local game from that FEN against the analysis engine at your level.

### Work

```rust
pub enum WorkKind { Review, Import, Calibrate, Setup }
pub enum WorkStatus { Queued, Running, Paused, Done, Failed, Stopped }
pub enum Priority { Live = 0, Review = 2, Background = 4 }   // lower runs first; the numbers are what the store keeps

pub struct Work {
    pub id: WorkId, pub kind: WorkKind, pub status: WorkStatus, pub priority: Priority,
    pub title: String,                // "Reviewing 20 of your games"
    pub subject: Subject,             // Game { id } | Player { key } | Engines
    pub cores: u32,                   // granted now
    pub progress: Option<Progress>,   // {done, total, unit, eta_seconds, text}
    pub paused_for: Option<String>,   // "A live game" | "Paused by you"
    pub error: Option<String>,
    pub created_at: Timestamp, pub started_at: Option<Timestamp>, pub ended_at: Option<Timestamp>,
}
```

### Events

One stream, `GET /api/events?since=<id>&topics=<t,…>` (SSE, `Last-Event-ID` resumes). Each event:

```rust
pub struct Event { pub id: u64, pub topic: String, pub kind: EventKind, pub data: serde_json::Value }
```

Topics: `game:<id>`, `games` (summaries of every game), `work`, `player:<key>`, `connection:you`, `engines` (the engines list and the levels of every engine), `policy`, `eval:<session>`. No `topics` means all except `eval:*` and `game:<id>` detail.

Kinds (a closed enum in `core::events`, each with a typed payload in the OpenAPI document):

| Kind                  | Payload                                                                              | Kept for replay |
| --------------------- | ------------------------------------------------------------------------------------ | --------------- |
| `game.upserted`       | `GameSummary`                                                                        | yes             |
| `game.moved`          | `{ id, ply, move: Move, clocks, fen }`                                               | yes             |
| `game.undone`         | `{ id, plies }` (takebacks)                                                          | yes             |
| `game.changed`        | `{ id, offers, actions, clocks, opponent_gone: Option<{claim_in_seconds}> }`         | yes             |
| `game.thinking`       | `{ id, ply, side, depth, score, nodes, nps, pv }`, at most 5/s                       | no              |
| `work.upserted`       | `Work`                                                                               | yes             |
| `work.log`            | `{ id, line }`                                                                       | no              |
| `review.upserted`     | `{ game, accuracy: SidePair<f32> }` (fetch the review for detail)                    | yes             |
| `connection.upserted` | `ConnectionStatus`                                                                   | yes             |
| `challenge`           | `Challenge` with `event: created \| accepted \| declined \| canceled`                | yes             |
| `policy.changed`      | `Policy`                                                                             | yes             |
| `eval.info`           | `{ session, depth, lines: [{ score, pv }] , nodes, nps }`                            | no              |
| `drill.upserted`      | `Drill` (topic `player:you`)                                                         | yes             |
| `drill.answered`      | `{ drill, outcome, at }` (topic `player:you`)                                        | yes             |
| `drill.deleted`       | `{ id }` (topic `player:you`)                                                        | yes             |
| `levels.updated`      | `{ engine, ratings: EntrantRating[], updated_at }` (topic `engines`)                 | yes             |
| `engines.changed`     | `EnginesOverview`: an engine added, installed or removed, or the analysis engine set | yes             |

The web client keeps one normalised store per resource and applies these: an `*.upserted` replaces the object, `game.moved`/`game.undone` patch the open game, `engines.changed` replaces the list.

## Policy (`crates/hub`): fair play

```rust
pub struct Policy { pub assist: Assist, pub reason: Option<String> }  // reason: "Analysis is off while you're playing on Lichess"
pub enum Assist { Allowed, ExplorerOnly /* every live game is correspondence */, Off }
```

- Computed from your connection: any live Lichess game of yours (played here or on lichess.org) turns it `Off` (or `ExplorerOnly`).
- Every service that gives assistance asks `hub.policy().allows(AssistKind)` and returns `Error::FairPlay` (HTTP 409, the reason above) when not: evaluation (`/api/eval`), the explorer, the tablebase, reviews (new and running: a running review pauses before its next position), imports of other players, the repertoire and prep.
- Never assistance: local games against an engine, installing engines.
- The web client hides those controls while `policy.assist != Allowed`, so the 409 is a backstop, not a UX.

## Scheduler and resources (`crates/work`, ledger in `crates/hub`)

- **Cores:** `CHESS_CORES`, default physical cores − 1. A ledger in `hub` hands out cores.
- **Live first.** A local game with an engine seat reserves its threads (1 per engine by default) for as long as it runs. Live evaluation on the board reserves 1 core (more if asked). These are reservations, not work items.
- **Work** takes what is left, by priority (`Review` before `Background`: reviewing all of a player's games and calibrating the levels), oldest first within a priority. The ledger keeps that order: a request it cannot grant waits in line (`Ledger::request_cores`; `cx.cores` passes the item's priority), keyed by priority and then by when it began waiting, and nothing behind a waiter is granted, even when it would fit, so background work that gives its cores back between units and asks again at once never starves a review waiting beside it. A waiter that is stopped leaves the line (the request is a guard); one asking for more cores than the machine has holds nobody up. Reservations never wait.
- **Lanes:** the scheduler runs one item at a time per lane — calibration; reviews; background reviews (so a review asked for now never waits days behind one) — and starts an item only while a core is free or lower-priority work holds some (it hands them back at its next unit, and the ledger serves the new item first).
- **Preemption:** when live play's reservations oversubscribe the ledger, the lowest-priority running CPU work is paused (`paused_for: "A live game"`) at its next safe point, and resumed by the scheduler once at least two cores (or what it held, if less) are free; work a person paused stays paused. After a restart, work paused for live play is queued again.
- **Network work** (imports, setup) needs no cores and runs at once. Lichess allows one request at a time per token: the `lichess` client holds the token's slot for each plain request and while a stream opens, not for a stream's life, so a long export never holds up the moves of a live game.
- **Pausing:** a review pauses on the fair-play policy; any CPU work can be paused and resumed (`POST /api/work/{id}/pause|resume`). A review pauses between positions (each engine finishes the search it is in), calibration between batches of games.
- **Restart:** work that was running when the server stopped comes back `Queued` and resumes from its last checkpoint (reviews keep their finished games, calibration its games).

## The Referee (`crates/play`)

One runtime plays every game that is played (not imported).

```rust
#[async_trait]
pub trait SeatController: Send {
    /// Ask for this seat's next move in `position`. Engine seats search; the local seat waits for the board;
    /// Lichess seats wait for the stream (the opponent) or forward yours.
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove>;
    /// A seat action reaching this seat's side: draw offer, takeback request, resign, abort, claim…
    async fn action(&mut self, action: SeatAction) -> Result<()>;
    fn kind(&self) -> SeatKind;            // Engine | Local | LichessYou | LichessOpponent
}
```

- **Local games** (`POST /api/games`): the Referee owns the rules and the clocks (`core::referee`: a pure state machine — apply a move, charge a clock with measured think time on a monotonic clock, add the increment, 250 ms grace, decide endings, adjudication rules, the draw-offer acceptance rule for engines).
- **Engine seats** start the engine from the engines list with its own options and then the app's (`Hash`, `Threads`, `MoveOverhead`, `Minimal`, each only when the engine declares it). An engine with a fixed budget (`nodes`) searches that budget whatever the clock; a level searches the level's budget with its MultiPV and plays `core::levels::choose_move`'s pick from the seat's seeded RNG.
- **Lichess games:** Lichess is the referee. The runtime mirrors Lichess's game stream into the record (moves, clocks, offers, the end), and seat controllers translate: `LichessYou` (your moves from the board go to the Board API), `LichessOpponent` (moves arrive from the stream). A task per game follows the stream (reconnecting) into a mirror the game's task and the seats watch; a Lichess seat's move counts once the stream shows it, so the record only ever holds what Lichess has.
- **The service** (`play::Games`, cheap to clone): `start(NewGameRequest)` (seats are strings: `you`, `<engine id>`, `<engine id>@<level>`; engines are started before the game is stored), `act(id, action, uci)` (404 no such game, 409 over or not in `Game.actions`, 400 an illegal move; a move answers once it is on the board), `stop(id)`, `game(id)` (live clocks, offers and actions laid over the stored record), `live()`, and `recover()` at startup: local games left live are recorded aborted; Lichess games left live are taken up by the connection when Lichess still has them (the record continues as far as it agrees with Lichess) and closed with no result (`ending: other`) when it does not.
- **Seat actions** are one set for every game: `move`, `offer_draw`, `accept_draw`, `decline_draw`, `takeback`, `accept_takeback`, `decline_takeback`, `resign`, `abort`, `claim_victory`, `claim_draw`. `Game.actions` lists what You may do now; anything else is 409.
- **You on Lichess:** seek, challenge (a player, Stockfish 1–8), accept/decline/cancel challenges, the Board API's rules (rapid+ for seeks, blitz+ for challenges, never bullet) live in `play::you`.

## The evaluation service (`crates/analysis`)

- `eval(fen, moves, engine, budget: Depth | Nodes | Infinite, multipv) -> stream of EvalInfo` for interactive sessions (`POST /api/eval` starts a session on the analysis engine or the one named, events on `eval:<session>`, `DELETE` stops it; one session per client tab). An engine with a fixed budget cannot analyse (400).
- `evaluate(position_key, engine, nodes, multipv) -> EvalResult` for reviews and profiles, through the **position cache** (`positions` table, keyed by FEN without move counters, the engine's cache key, nodes, multipv).
- A pool of warm engine processes per engine and options, sized by the cores the ledger grants.
- Reviews are work: `POST /api/reviews {games | player, limit, speeds, rated, engine, nodes}`. Defaults: the analysis engine, 200,000 nodes, MultiPV 2. A player's reviews take the games no engine has reviewed yet (changing the analysis engine re-reviews nothing); a game named by id is skipped only when it has a review by the same engine and budget.
- A review searches in **runs** (`analysis::runs`): each game's uncached positions in the order played, on one engine that keeps its hash table between them (`ucinewgame` at a run's start), so each search starts from what the one before found. Engines share one queue; an idle engine takes the next run or splits the longest one left, and games are read a few ahead, so no engine waits for the end of a game. Batches last at most 8 s (cache writes, judging, a safe point and a new grant between them), and within one each engine stops before its next position when the review is stopped or paused, the policy forbids reviews, or higher-priority work waits for cores. Concurrency defaults to half the cores, all of them in the background.

## Storage (`crates/store`)

SQLite in WAL mode with foreign keys on: one writer thread owning the only write connection (each write a transaction, fed by a channel of closures), readers on a small pool of query-only connections. Migrations are numbered SQL files (`crates/store/migrations/`) embedded in the binary and tracked by `PRAGMA user_version`, with a test that applies them from empty and one that migrates an old database. They run with foreign keys off (so a table can be rebuilt), and the foreign keys are checked when they are done. Tables are `STRICT`. Timestamps are ISO-8601 UTC strings. JSON columns hold structured values the database never queries; what it filters or sorts on is a plain column beside them.

```sql
players     (key TEXT PRIMARY KEY, name, title, lichess, ratings JSON, updated_at)
engines     (id TEXT PRIMARY KEY, name, kind, path, args JSON, options JSON, id_name, nodes, rating, created_at)
             -- engines you added, and built-ins once installed; meta `engines.analysis` names the analysis engine
games       (id INTEGER PRIMARY KEY, source_kind, source JSON, source_owner, white JSON, black JSON, white_key, black_key,
             time JSON, speed, rated, status, live, outcome, ending, opening JSON, start_fen, plies,
             moves BLOB,             -- zstd JSON of Vec<Move>; one row per game, never a row per move
             you_side, lichess_id, url, started_at, ended_at)
             -- source_owner: the connection (lichess) or the player key (import): a Lichess game is stored once per owner
             -- indexes: (live, started_at, id), (white_key, started_at), (black_key, started_at),
             --          (source_kind, started_at), (lichess_id, source_kind, source_owner)
reviews     (id INTEGER PRIMARY KEY, game, engine, nodes, white_accuracy, black_accuracy, summary JSON, plies BLOB,
             created_at, UNIQUE (game, engine, nodes))
             -- engine: the engine's cache key; the accuracies are copied out of the summary for game lists
positions   (key TEXT, engine TEXT, nodes INTEGER, multipv INTEGER, result JSON, PRIMARY KEY (key, engine, nodes, multipv))
work        (id INTEGER PRIMARY KEY, kind, status, priority, title, subject JSON, spec JSON, checkpoint JSON,
             progress JSON, cores, paused_for, error, created_at, started_at, ended_at)
connections (key TEXT PRIMARY KEY, account JSON, settings JSON, state JSON, updated_at)   -- tokens are files, never rows
drills, drill_answers, level_games, level_ratings     -- the player side: docs/roadmap.md
meta        (key TEXT PRIMARY KEY, value JSON)
```

A game's offers, clocks and actions are not columns: they belong to the game being played, which `play` holds in memory and overlays on the stored record.

**Migration 7** (`0007_engines.sql`) turned a database of the engine-development app into this one: it deletes trial games and calibration games (with their reviews and drills), the levels' ratings and seats (the ladder was Reckless's), and the work, meta and the BOT connection only engine development used; it rewrites Chess's seats onto `engine:reckless`, Lichess's AI onto `engine:stockfish`, the BOT account's games into Reckless's imported history, and each review's key and label from its commit (`reckless|Reckless abcdef1`); and it drops the versions, builds, ideas, trials, tune steps, nets, datasets and training tables, and the old position cache.

**Migration 8** (`0008_engine_levels.sql`) gives every engine its own ladder and drops Maia: it deletes the built-in `maia-*` rows from `engines` (games against them stay), adds `engines.multipv`, adds `level_games.engine` (filled from the seat that played at a level, else `stockfish`), and rebuilds `level_ratings` keyed by `(engine, entrant)` with the one fit there was as Stockfish's.

## API (`crates/server`)

Resource routes; actions are `POST` sub-routes. Every route has a `utoipa` path, so `crates/server/openapi.json` (`chess openapi`) is complete and typed, and `scripts/api-types.mjs` (`corepack pnpm api`; `api:check` fails on stale files) regenerates `apps/web/src/api/schema.d.ts` from it. Errors are `{ error: { code, message } }` with codes `bad_request` 400, `unauthorized` 401, `forbidden` 403 (a foreign `Host` or `Origin`), `not_found` 404, `conflict` 409, `fair_play` 409, `rate_limited` 429 (with `Retry-After` when known), `upstream` 502, and `internal` 500 for anything unexpected (core::error::Error carries one of these codes, so every crate shares the vocabulary). A body, query or path that does not read is a `bad_request` in the same shape.

**Who may call** (`api::guard`): every request but `GET /api/health` and the login callback carries `Authorization: Bearer <token>` (the event stream also takes `?token=`, for `EventSource`). Bound to loopback, every request (the UI's pages too) must name a loopback `Host` (DNS rebinding), and a request with an `Origin` must come from the app itself (`http://<host>`) or the hosted UI `https://chess.dame.gg`, which gets CORS (with `Authorization`, `Last-Event-ID` and private-network access). The UI the app serves needs no token: bound to loopback, a request the browser marks `Sec-Fetch-Site: same-origin` whose `Origin`, if any, is `http://<its Host>` is let in (no page can forge that header, and a program on the machine could read the token anyway); the page also carries the token, `<meta name="chess-token" content="…">`, for browsers that send no `Sec-Fetch-Site`. Every answer carries `X-Frame-Options: DENY`, `nosniff` and `no-referrer`. The web UI is `apps/web/dist`, read from disk in debug builds and embedded (rust-embed) in release, with paths that are not files answered by `index.html`.

Seats of a local game are `you`, `<engine id>`, `<engine id>@<level>`. Player keys are `you`, `engine:<id>`, `lichess:<name>`. Engines in requests are ids (`stockfish`); none means the analysis engine.

```
GET    /api/health                                   { ok, version }; no token
GET    /api/status                                   { version, machine: { os, cpu, physical_cores, logical_cores, memory_bytes, data_dir },
                                                       cores, ledger, policy, lichess_url, next_event }
GET    /api/token                                    { token } (Settings: pairing the hosted UI)
GET    /api/diagnostics                              Health { ok, items: [{ key: analysis|reckless|disk|database, ok, level, title, detail, action? }] }

GET    /api/engines                                  EnginesOverview
POST   /api/engines                                  NewEngineRequest { name, path, id?, args?, options?, nodes?, rating? } → 201 Engine
                                                     (400 not a UCI engine or not a file; 409 the id is taken)
DELETE /api/engines/{id}                             EnginesOverview (404 for none)
PUT    /api/engines/analysis                         { engine } → EnginesOverview (400 a fixed budget; 409 not installed)
GET    /api/setup                                    SetupPlan { downloads: [{ package: stockfish|reckless, name, url, bytes? }], bytes,
                                                     installed, notes } (sizes from GitHub; downloads nothing)
POST   /api/setup                                    { packages? } → 201 Work (kind setup; explicit)

GET    /api/games?player=&source=&status=&speed=&rated=&since=&before=&limit=&calibration=
                                                     GameSummary[], live first then newest; lists are comma-separated;
                                                     calibration games are left out unless calibration=true|all
POST   /api/games                                    NewGameRequest { white, black, time, from?: {type: position, fen, moves},
                                                     hash?, threads?, adjudicate?, seed? } → 201 Game
POST   /api/games/import                             { pgn, you?: white|black } → 201 GameSummary[] (finished local games)
GET    /api/games/{id}                               Game
GET    /api/games/{id}/pgn                           application/x-chess-pgn
POST   /api/games/{id}/actions/{action}              move { uci } (UCI or SAN) | offer_draw | accept_draw | … → Game (see the Referee)
POST   /api/games/{id}/stop                          stop a live game: a game you sit in is resigned (aborted before each
                                                     side has moved), an engine game aborted
GET    /api/games/{id}/review?engine=&nodes=         Review (404 without one)

POST   /api/eval                                     { fen?, moves, engine?, multipv, budget: {type: depth|nodes|infinite, …}, threads?, client }
                                                     → EvalSession { session, engine, fen }; `eval.info` on `eval:<session>`
GET    /api/eval                                     the sessions running ([{ session, client }]); one that reached its budget is gone
DELETE /api/eval/{session}                           { stopped }
GET    /api/explorer?db=lichess|masters|player&fen=&play=&speeds=&ratings=&modes=&since=&until=&moves=&top_games=&recent_games=&player=&color=
                                                     Lichess's answer, as is (needs your connection: 409 without it)
GET    /api/tablebase?fen=                           Lichess's answer, as is

POST   /api/reviews                                  { games | player, limit?, background?, speeds?, rated?, engine?, nodes?, concurrency? } → 202 Work
                                                     (background: priority Background in its own lane; with player and no limit, every game no
                                                     engine has reviewed, newest first)
GET    /api/players/top?speed=blitz&nb=10            [{ key, username, title, rating, progress }] (Lichess's leaderboard)
GET    /api/players/{key}                            Player
GET    /api/players/{key}/profile?speeds=&rated=&since=&limit=   Profile
GET    /api/compare?players=…&speeds=&rated=&since=&limit=       Comparison
POST   /api/players/{key}/import                     { max?, since?, speeds?, rated? } → 202 Work (lichess:<name>: their history,
                                                     refused while you play; you: your history, whole when nothing is capped or filtered:
                                                     newer games, then older ones until meta lichess.you.import.complete)
GET    /api/players/you/repertoire?side=&max_plies=&min_games=&limit=   Repertoire (409 fair_play while the explorer is not allowed)
GET    /api/players/you/prep?side=&limit=            Prep (409 fair_play likewise)
GET    /api/progress?window=10                       Progress

GET    /api/levels?engine=                           LevelsOverview { engine, levels: [{ level, rating, games }], you, suggested, updated_at } (default the analysis engine; 400 a fixed budget)
GET    /api/levels/plan?engine=&count=3              PlannedMatch[]
POST   /api/levels/calibrate                         { engine?, matches?, count?, games_per_pair? } → 202 Work (that engine's levels; default the analysis engine)
GET    /api/drills                                   Drill[] (soonest due first)
POST   /api/drills                                   { limit? } → 201 Drill[] made from your weak spots
GET    /api/drills/today?new_per_day=&limit=&theme=&games=&max_ply=   DrillsToday { drills, due, new, started_today }
POST   /api/drills/{id}/hint                         DrillHint
POST   /api/drills/{id}/answer                       { move } → DrillAnswerResult
DELETE /api/drills/{id}                              204
GET    /api/drills/trend?window=50                   HitRatePoint[]

GET    /api/work?recent=20                           { running, queued, recent }
GET    /api/work/{id}                                Work
POST   /api/work/{id}/stop | /pause | /resume        Work
GET    /api/work/{id}/log                            text/plain

GET    /api/connections                              { you: ConnectionStatus }
POST   /api/connections/you/login                    { return_to? } → { url, expires_in_seconds } (OAuth PKCE; the redirect is
                                                     http://<the request's Host>/api/connections/you/callback)
GET    /api/connections/you/callback                 outside the guard and the OpenAPI document: 303 to return_to
PUT    /api/connections/{key}/token                  { token } → ConnectionStatus
DELETE /api/connections/{key}                        log out (revokes at Lichess) → ConnectionStatus
POST   /api/connections/you/seek                     { time, rated, color, rating_range? } → ConnectionStatus;  DELETE to cancel
POST   /api/connections/{key}/challenge              { username | ai_level, time, rated, color } → 201 ChallengeSent
POST   /api/connections/{key}/challenges/{id}/accept | /decline { reason? } ;  DELETE …/challenges/{id}

GET    /api/events?since=&topics=&token=             SSE
```

**The event stream**: each event is `id: <id>`, `event: <kind>`, `data: { id, topic, kind, data }` (the `Event` schema: a union of every kind with its typed payload). `since` or `Last-Event-ID` replays the kept events after it, then live ones follow. When events after that id are no longer kept, or a slow client fell behind and events were dropped, a `reload` event (no SSE id; envelope id 0, topic `stream`, `data: { reason }`) tells the client to fetch what it shows again. A comment every 15 seconds keeps idle connections open; streams end when the app stops.

**The CLI** (`crates/server/src/cli`) mirrors the routes; `chess::cli::COMMANDS` maps every operation id to its command, and a test checks it against the document. Every command takes `--json` (one JSON document on stdout; errors on stderr, non-zero exit); `--wait` (work) and `--follow` (games) where something runs. It finds the app at `CHESS_URL` (default http://127.0.0.1:7979), with `CHESS_TOKEN` or the token in the data directory (`CHESS_DATA`). Tokens for Lichess are read from stdin, never from the command line.

```
chess serve | openapi | health | status | diagnostics | token | events [--topics] [--since]
chess setup [--only stockfish,reckless] [--yes] [--wait]      (shows each download's size and asks first)
chess engines | engines add --name … --path … [--option K=V --arg … --nodes --rating --id] | engines remove <id> | engines analysis <id>
chess games [--player --source --status --speed --rated --calibration --limit]
chess game <id> [--pgn | --review [--engine --nodes] | --follow]
chess play --white you --black stockfish@5 --tc 5+3 | --nodes N | --movetime MS [--fen --moves …] [--follow]
chess move <game> e4 [--no-wait] | draw <game> [--accept|--decline] | takeback <game> [--accept|--decline]
chess resign|abort|stop <game> | claim <game> [--draw]
chess import you | lichess:<name> [--max --since --speeds --rated --wait] | import --pgn <file|-> [--you white]
chess review <game>… | --player you [--limit --all --background --engine --nodes --wait]
chess player <key> | profile <key> | compare <key>… [--speeds --rated --since --limit] | players top [speed] [--nb]
chess repertoire --side white | prep --side white | progress | levels [--engine] [plan | calibrate] | drills [today | hint | make | answer <id> <move> | delete <id> | trend]
chess eval [--fen --moves --engine --depth|--nodes --multipv] (streams until done) | eval --list | eval --stop <session>
chess explorer [--db --fen --play …] | tablebase --fen <fen, _ for spaces>
chess work [show|stop|pause|resume|log|wait <id>]
chess lichess | lichess you [login | token | logout | seek --tc 10+5 | seek --stop | challenge … | accept|decline|cancel <id>]
```

**MCP** (`crates/server/src/mcp`): `POST /mcp` serves one tool per operation of the document plus `wait_for_work`, through the app's own router, with annotations from three tables checked by tests (`REACH`: real-account and outside-world operations, `DESTRUCTIVE`, `FAIR_PLAY`) and the fair-play and real-account rules in the server's instructions.

## The web app (`apps/web`)

### Layout

```
┌ header: Chess · [command bar ⌘K] · New game · You 1,840 · ● work · ⚙ ─────────────────────────────────┐
├ Games rail ─────────┬────────────── Board ──────────────────────┬ Panel (or a sheet) ───────────────────┤
│ filter: All · You · │ opponent strip (name, rating, clock)       │ the game card: who plays whom, the     │
│ Lichess             │                                            │ result or "vs", one muted line         │
│ live first, then    │        board (chessground) + eval bar      │ the mode's content and actions         │
│ newest, by month;   │                                            │                                        │
│ paged from the      │ your strip (name, rating, clock)           │ moves list (always, under the content) │
│ server              │ eval graph (a baseline until there is one) │                                        │
│                     │ transport                                  │                                        │
└─────────────────────┴────────────────────────────────────────────┴────────────────────────────────────────┘
```

- **Window sizes.** From 75rem the rail, board and panel sit side by side. From 48rem to 75rem the board is on the left with the panel beside it, the rail is a drawer from a button in the header, and a sheet covers the window under the header. Under 48rem the page is one column: the board edge to edge with its strips and transport, the panel under it (the moves are one row under the board, and Moves opens the whole list in a bottom sheet), and a tab bar at the foot moves between Board, Games and You; the header keeps the search, New game and the icons.
- **The board is the same size and place in every mode** of a window (Play, Watch, Review, Explore, Drill), because the strips, the eval graph slot and the transport keep their heights whether or not they have anything to show (`--strip`, `--graph`, `--transport` in `base.css`). A sheet narrows the stage, so the board is smaller beside a sheet than beside the panel, and a wide sheet (You, Compare) takes more than a plain one.
- **Routes** (the address is the board's state): `/` (the board; `?game=<id>&ply=<n>` or `?fen=…`), `/you`, `/players/<key>`, `/compare?players=…`, `/settings`. Profiles and settings are **sheets** that take the panel's place beside the board on wide screens (the board shrinks to fit and stays visible and live; a sheet opened directly shows the same game `/` would), full screens below 75rem; closing returns to the board as it was. A sheet with several parts is **tabbed** and keeps its tab in the address (`?tab=openings`). `?drill=<id>` puts a drill on the board. A Lichess login comes back to the page it started from with `?lichess=connected` (or `?lichess=error&message=…`), which the app says in a toast and takes off the address.
- **What opens first** at `/` with nothing chosen: a live game (yours first, then a local game between engines), else your last finished game, else the start position in Explore.
- **Modes** (what the panel shows, only those that apply to what is on the board; a finished game's panel has Review and Explore as one quiet switch):
  - **Play:** you are a seat in a live game. Clocks, offers (draw, takeback, abort/resign), the opponent leaving and claims. Its first line is a polite live region ("Bob played Nf6. Your move."). On Lichess, nothing else is on screen (no eval bar, no engine, no graph); the work indicator says why.
  - **Watch:** a live game you are not in: the thinking engine's line and score, both clocks, the eval graph growing.
  - **Review:** a finished game. The result and how it ended in words ("Black resigned", "Draw by repetition"), accuracy per side, class counts (each jumps to the next such move), the selected move's judgement, the better move as an arrow and "Show line", **Play this out** (a local game from here against the analysis engine at the level nearest your rating), the eval graph with judged dots, and, for a finished local game against an engine, the **end card**: how it went ("You beat Stockfish 1600"), the rating context, the one filled button **Rematch** (same opponent and clock, colours swapped; `R`), the level one step easier and one step harder as outline buttons, and "Review this game" until a review has been asked for. Not reviewed yet, for any other game: one button, "Review this game". The review names the engine that made it.
  - **Explore:** free analysis from any position: engine lines (MultiPV) from the analysis engine, the explorer (Lichess, Masters, your games), the tablebase at 7 pieces or fewer, and where the position stands in your repertoire. Play moves freely into a tree of variations, or paste a FEN or PGN; "Play from here" starts a game.
  - **Drill:** one of your drills takes the board and the panel (the game under it waits): your side to move, the opening and phase it came from, no engine, one try. Then the answer, the line and when it comes back; Next goes on through today's set. `D` (or Start on the You sheet's Drills) begins today's set. Hidden while you play on Lichess.
- **New game** (a quiet outline button in the header, and `N`; the one filled button is the view's own): the opponent, from the engines list: in two steps, from the engines list: the engine (the analysis engine first, then Reckless and each engine you added), then its strength (that engine's levels, each with that engine's rating for it, the one nearest yours marked "About your strength", a rating still being calibrated written "≈ 2,441 ± 488", one that has not played "≈ 2,800", and full strength last); an engine that plays as it is (a fixed budget) has no second step, and one that is not installed is shown with the way to install it (Settings › Engines). The button says what it starts: "Play 10+5 · Stockfish 6". The last choice is remembered; the clock defaults to 10+5, and "No clock" is the only game that allows a takeback. Lichess: find a game, challenge someone or Lichess's AI. Time control, colour, rated. One dialog, the choices that apply to the opponent.
- **Games rail:** one list of `GameSummary`, filtered All · You · Lichess, live games pinned on top with a pulse, then newest first under month headings, the filter and the paging done by the server. Each row: players (you are "You"), result, accuracy if reviewed, time control, when; a screen reader hears it as one sentence. Calibration games are left out. Nothing yet is one sentence and one button (New game).
- **You** (`/you`, a wide sheet): your Lichess ratings across the top (Log in with Lichess first when not linked), then tabs. **Overview**: the filters (speed, rated, how many games), the figures (games, score, accuracy, average loss, performance, win streak), the review notice with Review all and Import, Progress (your rating, accuracy and drill hit rate over time), results, accuracy by phase and speed, moves per game by class, time per move, best moments and worst blunders as links onto the board. **Openings**: the openings you play, then the lines you play as White and as Black, where you leave theory, how you score. **Train**: today's drills and Start, the hit rate over time, themes, Make drills from your mistakes, the weak spots with Play this out, and the **Levels** (one engine's ladder at a time, the engines with levels to choose from, the analysis engine first; the ladder in level order, each rung an interval and a dot on one shared scale; calibrate with what it costs). **Challenges** when there are any. Compare opens the comparison.
- **Settings** (`/settings`, a tabbed sheet): **Accounts** (your Lichess login), **Engines** (the list with what each calls itself, Use for analysis, Remove for yours, Add engine by path, and setup: the downloads with their sizes, each package with its own Download and Download all), **This machine** (cores, the data directory), **Health** (the analysis engine, Reckless, disk, the database, each with what fixes it), and **Appearance** (theme, sounds and their volume, and the token for the hosted app).
- **Command bar** (`⌘K` / `Ctrl K`): places, games, players, and verbs in the CLI's words (`play 5+3 stockfish`, `review last 20`, `compare cutemouse83`, `drills`), and the common ones as entries (New game, Drills, Review your last 20 games, Flip the board, the theme). What fair play forbids is not offered.
- **Work indicator** (in the header): what background work is doing in one short sentence, with the dot alone below 75rem; click for the queue (a popover: running, queued, recent, with stop and pause), and "Analysis is off while you're playing on Lichess" when the policy says so. It is the only place background work appears.
- **Toasts:** a game of yours starting elsewhere ("Your game against X started" — Open), an incoming challenge (Accept · Decline).

### Look and rules

VLC + Lichess; dense and sharp (one radius, flat controls, 4 px spacing grid, snappy motion); hairline dividers, flat panes; Hanken Grotesk for reading, JetBrains Mono for figures, Fraunces only for the wordmark. The rules, in short (the brief and the kit are `docs/ui-brief.md` and `/?fixtures&kit`):

- **One way to do each thing.** The homebrew kit in `src/ui/` (`UiSection`, `UiStats`, `UiTable`, `UiKeyValue`, `UiEmpty`, `UiToolbar`, `UiPlayer`, `UiScore`, `UiResult`, `UiBadge`, `UiSeg`, `UiSelect`, `UiField`, `UiDialog`, `UiConfirm`, `UiSheetPage`, `UiPanelHead`, `UiLive`) is the only place a section header, a figure, a table, an empty state, a badge, a segmented control, a dropdown or a dialog frame is drawn; screens use the kit and eos components imported by path, never a native control.
- **Tokens, not values.** Spacing `--sp-1..6`, type `--t-*`, colour roles `--c-*`, heights `--h-*`, icons `--i-*`, layout `--w-*` and the stage chrome (`--strip`, `--graph`, `--transport`), all in `base.css`; no bare px, rem or hex in a component except 1 px hairlines and chessground's own offsets. Figures are mono and tabular, labels sans and sentence case; a missing number is an en dash.
- **Colour has roles.** Cognac is the one filled button of a view, the selected marker, the live dot and focus. Green, red, amber and teal are outcomes and judgements. Links are ink with an underline.
- **One focus per screen.** Exactly one filled button per view, named with a verb (the header's New game is quiet so a view's own primary is the one); a fact appears once per screen (a sheet's subtitle never repeats its hero); every state is designed (a skeleton of the final shape after 150 ms, an empty state that teaches the next action in one sentence and one button, an error that says what failed with Try again, in the same frame).
- **Plain words first**, with `Term` toggletips for jargon; the vocabulary above (never build, match, experiment, test, batch, job, runner or human in the UI).
- **Touch and motion.** Controls are 32 px with a fine pointer and 44 px with a coarse one; `prefers-reduced-motion` turns every transition and animation off; text is at least 4.5:1 and graphics 3:1 in both schemes (the kit page measures them). Logical properties; keyboard first.

The stack breakpoint is 75rem (`src/media.ts` has the list). The board's side is rounded to chessground's 8-device-pixel step and must meet its frame exactly.

Sounds: a soft knock for a move, a lower one for a capture, a ping for check, a chime when a game you watch ends, two ticks when your own clock first drops under 20 seconds. They are made with WebAudio (nothing bundled, so no licence), play only for live games, are on and quiet by default, start off when the system asks for reduced motion, and are switched off or turned up in Settings › Appearance. A clock under 20 seconds turns red and, under ten, its bar breathes (not with reduced motion); your own is announced to a screen reader once.

Keyboard: `←/→` moves, `Home/End`, `F` flip, `Space` engine on/off (Explore), `X`/`Shift X` next/previous mistake (Review), `T` or `⌘Z` takes back your move (a local game with no clock), `R` plays a finished game again (Review), `N` new game, `D` today's drills, `⌘K` command bar, `G Y` / `G S` go to You / Settings, `Esc` closes what is open (the topmost dialog, then the sheet), `?` the shortcuts. Keys work whenever you are not typing. A sheet takes focus at its heading when it opens and gives it back to what opened it when it closes.

### Client architecture

- `api/client.ts`: openapi-fetch typed by `schema.d.ts`; the base URL and token (same origin, or the paired local app for the hosted site).
- `api/stream.ts`: one SSE connection, typed events, resumes with the last id, dispatches into stores.
- `stores/` (Pinia): `games` (summaries by id, the open game in detail), `work`, `connections`, `policy`, `engines`, `levels`, `players`, `machine`. Each loads with one GET and stays current from events. No component fetches on its own.
- `board/`: `Board.vue` (chessground wrapper), `BoardStage` (the strips, eval bar, graph and transport around it), `EvalBar`, `EvalGraph` (uPlot), `MoveList`, `Strip` (a seat: name, rating, clock), `Transport`. `position.ts` (chessops) holds the game tree and the cursor; `address.ts` keeps the address and the board in step.
- `panel/`: `PlayPanel`, `WatchPanel`, `ReviewPanel`, `ExplorePanel`, `DrillPanel`.
- `ui/`: the kit (see Look and rules), `format.ts` the one set of figure formatters, `media.ts` the breakpoints.
- `rail/`, `sheets/` (`You`, `Player`, `Compare`, `Settings`, each a `UiSheetPage`), `parts/` (what the sheets are made of), `shell/` (header, tab bar, command bar, work indicator, toasts, new-game dialog).

## Working on the app

- `C:\dev\chess` is the development checkout of `main`; the live app runs from a worktree of its own, so half-finished code cannot break it.
- **Never touch real accounts.** No agent logs in to Lichess, stores a real token, or talks to lichess.org; every Lichess path is tested against the mock server (`chess_lichess::testing::MockLichess`, behind the crate's `mock` feature so `play` and `server` test against the same mock).
- **Never download engines in tests.** Tests use the fake UCI engines in the repository; `work`'s engine tests start `work-fake-uci`, built from the engine crate's.
- Each agent owns its crate or directory and commits only its own paths: `git commit -m "…" -- <paths>`, never `git add -A`, never leave files staged. Build into the shared `CARGO_TARGET_DIR` `target-shared` (cargo locks it) or your own `target-<you>`; `target*/` is ignored.
- Gates per crate before committing: `cargo fmt`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Web: `corepack pnpm lint`, `format:check`, `typecheck`, `test`, `knip`.
- Doc comments say what a thing is and why, in plain sentences. No commented-out code, no TODOs without an owner.
