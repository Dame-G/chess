# Dameless: the design

This is the contract for the rebuild. Every crate, route, event, table and screen follows it. When code and this file disagree, one of them is wrong: fix the code, or change this file in the same commit and say why.

## The idea

**Two players, one board.** You and your engine, Dameless, both get stronger by playing games and learning from them. Every feature is one of:

- **playing** a game (you against Dameless, you on Lichess, Dameless on Lichess, two versions of Dameless in a test),
- **reading** games (review, the eval graph, the better move),
- **adding games up** into a verdict on strength (an idea's trials, your profile, a comparison).

So there are three things to understand, **the Board, Games and Players**, and everything else is hidden machinery.

### Principles

1. **Never leave the board.** The app is the board plus a panel. Profiles and settings open beside or over it, never instead of it.
2. **Everything is a game.** Local games, Lichess games (yours and the bot's), imported history and test games are one record, one list, one viewer.
3. **Show strength, hide machinery.** Elo, accuracy, verdicts. Builds, bench, nps, scaling and concurrency exist but are never the first thing you see.
4. **Fair play by construction.** While you play on Lichess, analysis is not on screen to be refused. One policy, enforced in one place on the server.
5. **One set of verbs.** Every action in the UI is a route and a CLI command with the same name. Claude and you drive the same app.

## Words

One vocabulary everywhere: code, routes, the CLI, the UI, docs.

| Word           | Means                                                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| **You**        | The person. Has a linked Lichess account (optional).                                                                            |
| **Dameless**   | The engine, a fork of Reckless. Also the name of its Lichess BOT account when connected.                                        |
| **Version**    | One commit of Dameless that matters: the champion line, an idea's change, an upstream tip. Shown by name (`v14`, `deeper-lmr`). |
| **Champion**   | The current best version: the tip of the fork's `main`. Versions on the champion line are numbered `v1`, `v2`, …                |
| **Idea**       | A hypothesis and a change to try. Kinds: **gain**, **neutral**, **speed**, **tune**. Ends **kept** or **dropped**.              |
| **Trial**      | One stage of an idea: fingerprint, screen, short, long, speed, tune, confirm. A set of games and a verdict.                     |
| **Game**       | Two seats, moves, clocks, a result. Wherever it was played.                                                                     |
| **Player**     | Someone who sits at a board: You, Dameless (at some version), a person on Lichess, Stockfish level N.                           |
| **Work**       | Anything running in the background: a build, a trial, a tune, a review, an import.                                              |
| **Review**     | The engine reading every move of a finished game.                                                                               |
| **Connection** | A Lichess account linked to the app: **you** (your account) or **bot** (Dameless's BOT account).                                |

Never: build (except in Settings/diagnostics), branch (except the idea's form), match, experiment, test, batch, job, runner, human.

## Shape of the system

```
apps/web  (Vue 3.5 + eos + chessground)  ──REST + SSE──►  dameless serve  (127.0.0.1:7979)
                                                          │
crates/                                                    ▼
  core       pure domain: types, chess, stats, review rules, referee rules, SPSA, scaling maths, events
  hub        the shared runtime: config, data dir, secrets, event bus, policy (fair play), resource ledger
  store      SQLite: schema, migrations, one writer, repositories
  engine     adapter: UCI processes, engine pool, git + cargo builds, bench, fastchess
  lichess    adapter: HTTP client with Lichess's rate rules, OAuth PKCE, NDJSON streams, Bot/Board API, import, explorer
  play       the Referee runtime: game tasks, seat controllers (engine, local, Lichess board, Lichess bot)
  analysis   the evaluation service (warm engine pool, position cache), reviews, profiles and comparisons
  work       the scheduler, versions and builds, ideas and trials (funnel), tunes (SPSA), promoting
  server     axum API (utoipa → OpenAPI), SSE, wiring, the `dameless` binary (serve + CLI)
```

Dependencies only point down this list's layers:

- `core` depends on nothing of ours and does no I/O (no tokio, no filesystem, no network). Everything in it is unit tested.
- `engine` and `lichess` are **adapters**: they know their outside world (UCI, git, cargo, fastchess; Lichess's HTTP API) and keep their own wire types (a UCI `info` line, a Lichess game JSON). They do not depend on `core`, the store or the bus; `play`, `analysis` and `work` map their types to `core`'s.
- `store` depends on `core` only. `hub` depends on `core` and `store` and owns the bundle every service gets, `Hub { config, store, bus, policy, ledger, secrets, token }`: the store stays a plain library a test can open alone, and the hub is the runtime that wires it to the rest.
- Lichess's business rules (which challenges the bot takes, how it seeks, the daily bot limit, the Board API's speed rules) are domain logic: their pure parts live in `core::lichess`, their running parts in `play`.
- `work` depends on `core`, `hub`, `store` and `engine`. It owns the scheduler and **versions and builds** (`work::versions::Versions`), which `play` and `analysis` need to turn a version into a binary. So `play` and `analysis` depend on `work` (and on `core`, `hub`, `store` and their adapters); `work` never depends on them.
- `server` depends on everything and holds no domain logic: routes translate HTTP to service calls, and it wires the services together (registers every executor with the scheduler, starts the connections).

### Interfaces between the services

These are the seams `play`, `analysis` and `work` share; they are built at the same time against these shapes.

- **Running work** (`hub::work`, written): `trait Executor { fn kind() -> WorkKind; async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> }`. `WorkContext` gives the spec (`cx.spec::<T>()`), the last checkpoint (`cx.checkpoint::<T>()`, `cx.save_checkpoint(&t)`), progress (`cx.progress(p)`, throttled; `cx.progress_now(p)`), the log (`cx.log(line)`), cores (`cx.cores(min, max).await -> Option<Grant>`, held only while used), safe points (`cx.safe_point(Some(AssistKind::Review)).await -> bool`: waits while paused or while fair play forbids the assistance; `false` means stopped), and `cx.stopped()` to return when stopped. Executors: `Build`, `Trial`, `Tune`, `Promote` in `work`; `Review` in `analysis`; `Import` (your history, another player's) in `play`. The server registers them all with `work::Scheduler`.
- **Queueing work**: `work::Scheduler::submit(NewWork) -> Work` (any crate holding the scheduler; the routes do), `stop`, `pause`, `resume`. `Scheduler` is cheap to clone.
- **Versions and builds** (`work::versions::Versions`, cheap to clone): `resolve(&VersionRef) -> Version` where `VersionRef` is `Champion | Id(VersionId) | Name(String)` (a name may be a branch, tag or commit, making a version of it on first use); `binary(&VersionRef, features: &[&str]) -> (Build, PathBuf)` returns a ready binary, building it first (as `Build` work, awaited) when there is none; `champion() -> Version`; `list(...)`.
- **Evaluation** (`analysis::Eval::new(hub, binaries)`, cheap to clone): `start_session(EvalRequest { fen, moves, version, multipv, budget, threads, client }) -> EvalSession { session, version, fen }` streaming `eval.info` on `eval:<session>` (one session per client; it reserves its cores, and stops when the policy turns off), `stop_session(id)`, `stop_client`, `stop_all`; `evaluate(fen, &Version, nodes, multipv) -> PositionEval` and `evaluate_positions(&[ToSearch], &Version, nodes, multipv, engines) -> Evaluated` through the position cache. A version becomes a binary through `analysis::Binaries` (`resolve`, `binary`), which `work::Versions` implements, so tests run analysis against a fake engine without the scheduler. `play` uses the engine pool directly for its engine seats (it needs clocks and `go wtime btime`), not `Eval`.
- **Reviews and profiles** (`analysis::Reviews::new(eval)`, `analysis::Profiles::new(hub)`): `Reviews` is the `Review` executor and queues reviews (`prepare(ReviewRequest) -> NewWork`, resolved when queued, or `submit(&scheduler, req) -> Work`); `get(game, version, nodes)` backs `GET /api/games/{id}/review`. `Profiles::profile(key, filter)`, `compare(keys, filter)` and `player(key)` back the player routes.
- **Connections** (`play::Connections::new(hub, games, lichess_config)`, `start()`): owns the two Lichess clients (`you`, `bot`), one each, shared by everything that talks to Lichess for that account (imports, the explorer, seat controllers), and `anonymous()` for public calls. It keeps `hub.policy` current from your live games. `set_token`, `logout`, `start_login(return_to, redirect_uri)`, `finish_login(code, state, error) -> LoginFinished { return_to, result }`, `explorer`, `tablebase`, `user`, `leaderboard`, `challenge(key, body)`; `you()` (seek, challenges) and `bot()` (settings, online, seek, online bots) for the rest. Engines become binaries through `play::Binaries` (`resolve`, `binary`, by the version text `work::VersionRef::parse` reads), implemented for `work::Versions`.

The binary is `dameless` (crate `server`). `dameless serve` runs the app; every other subcommand is a client of a running app over HTTP, one subcommand per route.

### Process and data

- Listens on `127.0.0.1:7979` (`DAMELESS_LISTEN`). Bearer token for every request except `GET /api/health`; `Origin`/`Host` checks against DNS rebinding; the hosted UI origin `https://chess.dame.gg` is allowed with the token (as today).
- Data directory: `dirs::data_local_dir()/dameless` (`DAMELESS_DATA`): `dameless.db`, `token`, `secrets/lichess-you`, `secrets/lichess-bot` (0600 on Unix), `builds/`, `games/` (archived PGN, zstd), `tools/` (fastchess, books), `logs/work-<id>.log`.
- The engine checkout: `RECKLESS_DIR` (default `../reckless`). Builds happen in a git worktree under the data directory, never in the user's checkout.
- `LICHESS_URL`, `LICHESS_EXPLORER_URL`, `LICHESS_TABLEBASE_URL` point the adapters elsewhere (a mock in tests).
- In debug builds the server serves `apps/web/dist` from disk; in release it embeds it (rust-embed).

## Core types (`crates/core`)

Rust sketches; serde names are `snake_case`, enums are internally tagged with `type` unless they are plain strings. Every public type that crosses the API derives `utoipa::ToSchema` (utoipa is allowed in core; it is not I/O).

### Identity

```rust
pub struct GameId(i64);  pub struct IdeaId(i64);  pub struct TrialId(i64);
pub struct WorkId(i64);  pub struct VersionId(i64); pub struct BuildId(i64);
pub struct ReviewId(i64);

pub enum Side { White, Black }
pub enum Color { White, Black, Random }           // a choice when starting a game

/// Who sits at a board. Serialized as a string key: "you", "dameless", "lichess:<id>", "stockfish:<1-8>".
pub enum PlayerKey { You, Dameless, Lichess(String), Stockfish(u8) }
```

### Scores

All stored and transmitted scores are **from White's point of view**. One rule, no exceptions; the UI flips for display.

```rust
pub enum Score { Cp(i32), Mate(i32) }            // {"cp": 31} | {"mate": -3}; mate 0 = side to move is mated
```

### Time

```rust
pub enum TimeControl {
    Clock { initial_ms: u64, increment_ms: u64 },      // games write "3+2" (minutes+seconds); test presets "8+0.08" (seconds+seconds)
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
    pub name: String,                  // "You" is shown as the Lichess username when linked
    pub version: Option<VersionId>,    // Dameless seats
    pub build: Option<BuildId>,        // Dameless seats: the exact binary
    pub rating: Option<u32>,
    pub title: Option<String>,
}

pub enum Source {
    Local,                                         // played here
    Lichess { connection: ConnectionKey, id: String, rated: bool, speed: Speed },  // played through a connection
    Import  { owner: PlayerKey, id: String, rated: bool, speed: Speed },            // history read from Lichess
    Trial   { trial: TrialId, index: u32 },        // a test game
}
pub enum ConnectionKey { You, Bot }

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

### Versions and builds

```rust
pub struct Version {
    pub id: VersionId,
    pub name: String,                 // "v14" on the champion line, else the branch name
    pub commit: String,               // full sha
    pub branch: Option<String>,
    pub line: Line,                   // Champion { number } | Idea { idea: IdeaId } | Upstream | Other
    pub parent: Option<VersionId>,    // the champion it was built on
    pub bench: Option<u64>,           // the commit's Bench: line (none for a version with a trained net)
    pub net: Option<NetId>,           // the net its builds embed; none = the commit's own (R4: "v14 + net-3")
    pub elo: Option<Estimate>,        // against its parent, from the deciding trial: {elo, low, high}
    pub created_at: Timestamp,
}
pub struct Build {                    // diagnostics only; a cache keyed by (commit, features, net)
    pub id: BuildId, pub version: VersionId, pub features: Vec<String>,
    pub bench: Option<u64>, pub nps: Option<u64>, pub path: String, pub status: BuildStatus,
}
```

### Ideas, trials, tunes

```rust
pub enum IdeaKind { Gain, Neutral, Speed, Tune, Net }
pub enum IdeaStatus { Queued, Running, Kept, Dropped, Stopped, Failed }
pub enum Author { You, Claude }

pub struct Idea {
    pub id: IdeaId,
    pub title: String,                // "Deeper LMR at high depth"
    pub hypothesis: String,
    pub kind: IdeaKind,
    pub author: Author,
    pub branch: String,               // the change (for a tune: the branch with the tunables)
    pub base: VersionId,              // usually the champion when queued
    pub version: Option<VersionId>,   // the version under test (a tune's: once SPSA has written it)
    pub status: IdeaStatus,
    pub verdict: Option<String>,      // one sentence: "Kept: +3.1 ± 2.0 Elo at short games"
    pub trials: Vec<TrialSummary>,
    pub tags: Vec<String>,            // batches are tags
    pub notes: Option<String>,        // Claude's or your reasoning (markdown)
    pub created_at: Timestamp, pub ended_at: Option<Timestamp>,
}

pub enum Stage { Fingerprint, Screen, Short, Long, Speed, Tune, Confirm, Custom }
pub enum TrialStatus { Waiting, Running, Passed, Failed, Stopped, Skipped }

pub struct Trial {
    pub id: TrialId, pub idea: IdeaId, pub stage: Stage, pub status: TrialStatus,
    pub preset: Option<Preset>,       // vstc, stc, ltc, stc_hash, ltc_hash, smp, nodes
    pub time: Option<TimeControl>,    // as written (8+0.08)
    pub played: Option<TimeControl>,  // after scaling (11+0.11)
    pub scaling: Option<Scaling>,     // {copies, dev_nps, base_nps, measured_nps, reference_nps, scale}
    pub bounds: Option<[f64; 2]>,     // nElo, [lower, upper]
    pub pairs: Option<u32>,           // a fixed number of pairs instead of an SPRT (custom trials)
    pub penta: Pentanomial,           // [LL, LD, DD+WL, WD, WW] pairs (u64 counts, as stats uses)
    pub wdl: [u32; 3],
    pub llr: Option<Llr>,             // {value, lower, upper}
    pub elo: Option<Estimate>,        // logistic Elo ± 95 %
    pub nelo: Option<Estimate>,
    pub detail: Option<String>,       // one sentence for people
    pub started_at: Option<Timestamp>, pub ended_at: Option<Timestamp>,
}
```

The funnels copy Reckless's OpenBench (unchanged from the old harness, see `AGENTS.md`):

| Kind    | Stages                                                                                                                    |
| ------- | ------------------------------------------------------------------------------------------------------------------------- |
| gain    | fingerprint (bench must change) → screen VSTC 4+0.04 [-5, 0] → short STC 8+0.08 [0, 3] → long LTC 40+0.4 [0, 3] (opt-in)  |
| neutral | fingerprint (bench must change) → screen → short STC [-2.75, 0.25] → long (opt-in)                                        |
| speed   | fingerprint (bench must be identical) → speed (paired nps comparison, no games)                                           |
| tune    | tune (SPSA writes the change) → fingerprint → screen → confirm STC [0, 4] → long [0, 4] (opt-in)                          |
| net     | train (a run makes the net; the version is the base's code with it) → fingerprint → screen → short [0, 3] → long (opt-in) |

An idea can also be a single **custom** trial (any preset and bounds or a fixed number of pairs): what `chess test` was, including the false-accept check.

Statistics are the old harness's `stats.rs` ported as is: pentanomial GSPRT (alpha 0.05, beta 0.10), nElo, logistic Elo with a t-interval, checked against Fishtest/OpenBench reference runs. Clock scaling is the old `engine/scale.rs`: 660,000 nps reference, measured with as many bench copies as the trial's threads, `CHESS_SCALE_CLOCKS` becomes `DAMELESS_SCALE_CLOCKS`, `CHESS_REFERENCE_NPS` becomes `DAMELESS_REFERENCE_NPS`.

A tune (`kind: tune`) carries `TuneSpec { params: Vec<TunableOverride>, iterations, pairs, time | nodes, hash }` and stores its SPSA history (one row per update: values, result, penta). SPSA follows OpenBench exactly as the old `tune.rs` does (alpha 0.602, gamma 0.101, A = 0.1 × iterations, `r_end` 0.002, stochastic rounding, `c_k ≥ 0.5`, workers of two games). When SPSA ends, the tune writes the rounded values into `src/parameters.rs` on a new branch `tune/<idea id>`, commits with its `Bench:` line, records that as the idea's version, and the remaining stages run on it.

**Promoting:** a kept idea can be promoted: its branch is merged into the fork's `main` locally (fast-forward or a merge commit with the `Bench:` line), which makes a new champion `v<n+1>`. The merge is made in a scratch worktree under the data directory; since `main` is usually checked out in the user's checkout, that checkout is fast-forwarded to the result when it has no uncommitted changes, and the promotion is refused (changing nothing) when it has (`engine::git::CheckedOut`). Pushing the fork is a separate, explicit action (`POST /api/dameless/push`), never automatic.

### Reviews

The old `review/judge.rs` is ported as is: Lichess's win%, move accuracy, game accuracy (volatility-weighted and harmonic), ACPL, judgements, our classifications (brilliant, great, best, excellent, good, book, inaccuracy, mistake, blunder, miss), phases (lila's Divider), time-trouble blunders. `AGENTS.md` of the old harness has every formula and threshold; its tests come along.

```rust
pub struct Review { pub id: ReviewId, pub game: GameId, pub version: VersionId, pub nodes: u64,
                    pub summary: ReviewSummary, pub plies: Vec<ReviewedMove>, pub created_at: Timestamp }
```

`ReviewSummary` and `ReviewedMove` keep the old shapes (`PlyReview` is now `ReviewedMove`), with every score White POV and the summary naming its `version` where the old one named a build. The searched positions they are judged from (`PositionEval { lines: [{uci, score, pv}], depth, nodes }`) are White POV too: `analysis` flips the engine's side-to-move scores before caching.

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
    // and, from the old compare: pending_review, first_game, last_game, rating, judgements,
    // opponents {average_rating, performance}, longest_win_streak
}
```

Comparison is profiles side by side: `GET /api/compare?players=you,dameless,lichess:cutemouse83&…` returns `Comparison { profiles: Vec<Profile>, leaders: Vec<Leader { row, players: [index] }> }` (ties lead together; a row only one player has figures for leads nobody). The old `review/compare.rs` is the starting point.

A **weak spot** is a position before one of your mistakes or blunders in a reviewed game (`{game, ply, fen, played, best, swing, opening, phase, recurring}`), recurring themes first (same opening family and phase). "Play this out" starts a local game from that FEN against Dameless.

**Dameless's profile** adds the logbook: `GET /api/dameless` → `{ champion: Version, line: Vec<Version>, ideas: { running, queued, recent }, connection: Option<BotStatus>, strength: Vec<StrengthPoint> }`, where a strength point is a champion version and its cumulative Elo over `v1` (summing each promotion's deciding estimate).

### Work

```rust
pub enum WorkKind { Build, Trial, Tune, Review, Import, Promote, Calibrate, Datagen, Train, DatasetImport }
pub enum WorkStatus { Queued, Running, Paused, Done, Failed, Stopped }
pub enum Priority { Live = 0, Interactive = 1, Review = 2, Idea = 3 }   // lower runs first

pub struct Work {
    pub id: WorkId, pub kind: WorkKind, pub status: WorkStatus, pub priority: Priority,
    pub title: String,                // "Testing deeper-lmr · short games"
    pub subject: Subject,             // Idea(id) | Trial(id) | Game(id) | Player(key) | Version(id)
    pub cores: u32,                   // granted now
    pub progress: Option<Progress>,   // {done, total, unit, eta_seconds, text}
    pub paused_for: Option<String>,   // "Your Lichess game" | "Live analysis"
    pub error: Option<String>,
    pub created_at: Timestamp, pub started_at: Option<Timestamp>, pub ended_at: Option<Timestamp>,
}
```

### Events

One stream, `GET /api/events?since=<id>&topics=<t,…>` (SSE, `Last-Event-ID` resumes). Each event:

```rust
pub struct Event { pub id: u64, pub topic: String, pub kind: EventKind, pub data: serde_json::Value }
```

Topics: `game:<id>`, `games` (summaries of every game), `idea:<id>`, `ideas`, `work`, `player:<key>`, `connection:you`, `connection:bot`, `versions`, `policy`, `eval:<session>`, `training` (nets and datasets). No `topics` means all except `eval:*` and `game:<id>` detail.

Kinds (a closed enum in `core::events`, each with a typed payload in the OpenAPI document):

| Kind                  | Payload                                                                                                                    | Kept for replay |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------- | --------------- |
| `game.upserted`       | `GameSummary`                                                                                                              | yes             |
| `game.moved`          | `{ id, ply, move: Move, clocks, fen }`                                                                                     | yes             |
| `game.undone`         | `{ id, plies }` (takebacks)                                                                                                | yes             |
| `game.changed`        | `{ id, offers, actions, clocks, opponent_gone: Option<{claim_in_seconds}> }`                                               | yes             |
| `game.thinking`       | `{ id, ply, side, depth, score, nodes, nps, pv }`, at most 5/s                                                             | no              |
| `idea.upserted`       | `Idea`                                                                                                                     | yes             |
| `trial.upserted`      | `Trial`                                                                                                                    | yes             |
| `trial.progress`      | `{ id, idea, penta, wdl, llr, elo }` (throttled 1/s, topic `idea:<id>`)                                                    | yes             |
| `tune.progress`       | `{ idea, iteration, values, result }`                                                                                      | yes             |
| `work.upserted`       | `Work`                                                                                                                     | yes             |
| `work.log`            | `{ id, line }`                                                                                                             | no              |
| `version.upserted`    | `Version`                                                                                                                  | yes             |
| `review.upserted`     | `{ game, accuracy: SidePair<f32> }` (fetch the review for detail)                                                          | yes             |
| `connection.upserted` | `ConnectionStatus`                                                                                                         | yes             |
| `challenge`           | `Challenge` with `event: created                                                                                           | accepted        | declined | canceled` | yes |
| `policy.changed`      | `Policy`                                                                                                                   | yes             |
| `eval.info`           | `{ session, depth, lines: [{ score, pv }] , nodes, nps }`                                                                  | no              |
| `train.progress`      | `{ work, idea?, superbatch, superbatches, batch, loss, lr, positions_per_sec, grad_norm, … }` (1/s; `idea:<id>` or `work`) | yes             |
| `net.upserted`        | `Net` (topic `training`)                                                                                                   | yes             |
| `dataset.upserted`    | `Dataset` (topic `training`)                                                                                               | yes             |

The web client keeps one normalised store per resource and applies these: an `*.upserted` replaces the object, `game.moved`/`game.undone` patch the open game.

## Policy (`crates/hub`): fair play

```rust
pub struct Policy { pub assist: Assist, pub reason: Option<String> }  // reason: "Analysis is off while you're playing on Lichess"
pub enum Assist { Allowed, ExplorerOnly /* every live game is correspondence */, Off }
```

- Computed from the **you** connection: any live Lichess game of yours (played here or on lichess.org) turns it `Off` (or `ExplorerOnly`).
- Every service that gives assistance asks `hub.policy().allows(AssistKind)` and returns `Error::FairPlay` (HTTP 409, the reason above) when not: evaluation (`/api/eval`), the explorer, the tablebase, reviews (new and running: a running review pauses before its next position), imports of other players.
- Never assistance: bot games, trials, local games against Dameless, building.
- The web client hides those controls while `policy.assist != Allowed`, so the 409 is a backstop, not a UX.

## Scheduler and resources (`crates/work`, ledger in `crates/hub`)

- **Cores:** `DAMELESS_CORES`, default physical cores − 1. A ledger in `hub` hands out cores.
- **The GPU** (R4): `DAMELESS_GPUS`, default 1 when nvidia-smi finds one. The ledger grants it whole (`grant_gpu`, `cx.gpu()`) to one training run at a time, which also takes 2–4 cores for its data loader, so trials keep playing beside it. Datagen and training each have a lane; a dataset import is network work (no lane, no cores). Datasets share a disk cap (`DAMELESS_DATA_CAP_GB`), and nothing that writes much starts with less than 15 GB free. See the roadmap's R4 contract.
- **Live first.** A local or Lichess game with a Dameless seat reserves its threads (1 per engine by default) for as long as it runs. Live evaluation on the board reserves 1 core (more if asked). These are reservations, not work items.
- **Work** takes what is left, by priority (`Review` before `Idea`), oldest first within a priority. A trial asks for `concurrency` cores (default: all that are free, at least 2) and plays in **rounds**: each round runs fastchess with the cores granted at the start of that round for about `4 × cores` game pairs, then the trial updates its statistics and checks SPRT. Between rounds the scheduler may grant more or fewer cores, or pause the trial. Scaling is measured once at the trial's start with the planned concurrency and is part of the trial's record.
- **Lanes:** the scheduler runs one item at a time per lane — builds; trials and tunes together; reviews; promotions — and starts an item only while a core is free; cores themselves come from the ledger when the item asks (`cx.cores`). So a trial waiting for its build never blocks the build (builds are queued at `Interactive`), and an idea's trials run one after another.
- **Preemption:** when live play's reservations oversubscribe the ledger, the lowest-priority running CPU work is paused (`paused_for: "A live game"`) at its next safe point, and resumed by the scheduler once at least two cores (or what it held, if less) are free; work a person paused stays paused. After a restart, work paused for live play is queued again.
- **Network work** (imports) needs no cores and runs at once. Lichess allows one request at a time per token: the `lichess` client holds the token's slot for each plain request and while a stream opens, not for a stream's life, so a long export never holds up the moves of a live game.
- **Pausing:** a review pauses on the fair-play policy; any CPU work can be paused and resumed (`POST /api/work/{id}/pause|resume`). A trial pauses between rounds; a tune between iterations; a review between positions.
- **Restart:** work that was running when the server stopped comes back `Queued` and resumes from its last checkpoint (trials keep their rounds, tunes their iterations, reviews their finished games).

## The Referee (`crates/play`)

One runtime plays every game that is played (not imported, not trial games, which fastchess plays).

```rust
#[async_trait]
pub trait SeatController: Send {
    /// Ask for this seat's next move in `position`. Engine seats search; the local seat waits for the board;
    /// Lichess seats wait for the stream (the opponent) or forward ours (you).
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove>;
    /// A seat action reaching this seat's side: draw offer, takeback request, resign, abort, claim…
    async fn action(&mut self, action: SeatAction) -> Result<()>;
    fn kind(&self) -> SeatKind;            // Engine | Local | LichessYou | LichessBot | LichessOpponent
}
```

- **Local games** (`POST /api/games`): the Referee owns the rules and the clocks (`core::referee`: a pure state machine — apply a move, charge a clock with measured think time on a monotonic clock, add the increment, 250 ms grace, decide endings, adjudication rules from the tests, draw-offer acceptance rule for engines).
- **Lichess games:** Lichess is the referee. The runtime mirrors Lichess's game stream into the record (moves, clocks, offers, the end), and seat controllers translate: `LichessBot` (Dameless plays through the Bot API, the engine searches with Lichess's clocks), `LichessYou` (your moves from the board go to the Board API), `LichessOpponent` (moves arrive from the stream). A task per game follows the stream (reconnecting) into a mirror the game's task and the seats watch; a Lichess seat's move counts once the stream shows it, so the record only ever holds what Lichess has. A dropped engine search for the same position is picked up again rather than restarted (an offer or the opponent leaving wakes the game's task while the engine thinks).
- **The service** (`play::Games`, cheap to clone): `start(NewGameRequest)` (seats are strings: `you`, `dameless`, `dameless:<version>`; engines are started, and their versions built, before the game is stored), `act(id, action, uci)` (404 no such game, 409 over or not in `Game.actions`, 400 an illegal move; a move answers once it is on the board), `stop(id)`, `game(id)` (live clocks, offers and actions laid over the stored record), `live()`, and `recover()` at startup: local games left live are recorded aborted; Lichess games left live are taken up by their connection when Lichess still has them (the record continues as far as it agrees with Lichess) and closed with no result (`ending: other`) when it does not.
- **Seat actions** are one set for every game: `move`, `offer_draw`, `accept_draw`, `decline_draw`, `takeback`, `accept_takeback`, `decline_takeback`, `resign`, `abort`, `claim_victory`, `claim_draw`. `Game.actions` lists what You may do now; anything else is 409.
- **Dameless on Lichess** (the bot): accepting challenges by its rules, seeking other bots, the daily bot-game limit — ported from the old `lichess/{rules,seek,quota}.rs` — live in `play::bot`, on top of the `lichess` adapter's bot connection.
- **You on Lichess:** seek, challenge (a friend, Stockfish 1–8, your bot), accept/decline/cancel challenges, the Board API's rules (rapid+ for seeks, blitz+ for challenges, never bullet) — ported from the old `lichess/me/*` — live in `play::you`.

## The evaluation service (`crates/analysis`)

- `eval(fen, moves, version, budget: Depth | Nodes | Infinite, multipv) -> stream of EvalInfo` for interactive sessions (`POST /api/eval` starts a session, events on `eval:<session>`, `DELETE` stops it; one session per client tab).
- `evaluate(position_key, version, nodes, multipv) -> EvalResult` for reviews and profiles, through the **position cache** (`positions` table, keyed by FEN without move counters, version's commit, nodes, multipv).
- A pool of warm engine processes per build, sized by the cores the ledger grants.
- Reviews are work: `POST /api/reviews {games | player, limit, speeds, rated, version, nodes}`. Defaults: the champion, 200,000 nodes, MultiPV 2.

## Storage (`crates/store`)

SQLite in WAL mode with foreign keys on: one writer thread owning the only write connection (each write a transaction, fed by a channel of closures), readers on a small pool of query-only connections. Migrations are numbered SQL files (`crates/store/migrations/`) embedded in the binary and tracked by `PRAGMA user_version`, with a test that applies them from empty. Tables are `STRICT`. Timestamps are ISO-8601 UTC strings. JSON columns hold structured values the database never queries; what it filters or sorts on is a plain column beside them (`line_kind`, `champion`, `live`, the accuracies).

```sql
players     (key TEXT PRIMARY KEY, name, title, lichess, ratings JSON, updated_at)
versions    (id INTEGER PRIMARY KEY, name UNIQUE, commit_sha, branch, line JSON, line_kind, champion, parent, bench,
             elo JSON, created_at)
             -- line_kind (champion|idea|upstream|other) and champion (the number, unique) are copied out of `line`
builds      (id INTEGER PRIMARY KEY, version, commit_sha, features, bench, nps, path, status, log_path, created_at, deleted_at)
             -- features: sorted, comma-joined ("" or "spsa"); the cache key is (commit_sha, features)
games       (id INTEGER PRIMARY KEY, source_kind, source JSON, source_owner, white JSON, black JSON, white_key, black_key,
             time JSON, speed, rated, status, live, outcome, ending, opening JSON, start_fen, plies,
             moves BLOB,             -- zstd JSON of Vec<Move>; one row per game, never a row per move
             you_side, lichess_id, trial, url, started_at, ended_at)
             -- source_owner: the connection (lichess) or the player key (import): a Lichess game is stored once per owner
             -- live: status is starting or live, for "live first" lists; url: the lichess.org link, made by the caller
             -- indexes: (live, started_at, id), (white_key, started_at), (black_key, started_at),
             --          (source_kind, started_at), (trial), (lichess_id, source_kind, source_owner)
reviews     (id INTEGER PRIMARY KEY, game, version, nodes, white_accuracy, black_accuracy, summary JSON, plies BLOB,
             created_at, UNIQUE (game, version, nodes))
             -- the accuracies are copied out of the summary so game lists show them without reading it
positions   (key TEXT, commit_sha TEXT, nodes INTEGER, multipv INTEGER, result JSON, PRIMARY KEY (key, commit_sha, nodes, multipv))
ideas       (id INTEGER PRIMARY KEY, title, hypothesis, kind, author, branch, base, version, status, verdict, tags JSON,
             notes, spec JSON, created_at, ended_at)
trials      (id INTEGER PRIMARY KEY, idea, stage, ord, status, preset, time JSON, played JSON, scaling JSON, bounds JSON,
             pairs, penta JSON, wdl JSON, llr JSON, elo JSON, nelo JSON, detail, rounds INTEGER, pgn_path, started_at,
             ended_at, UNIQUE (idea, ord))
tune_steps  (idea, iteration, theta JSON, result, penta JSON, PRIMARY KEY (idea, iteration))
             -- theta: the values after the step (`values` is an SQL keyword)
work        (id INTEGER PRIMARY KEY, kind, status, priority, title, subject JSON, spec JSON, checkpoint JSON,
             progress JSON, cores, paused_for, error, created_at, started_at, ended_at)
connections (key TEXT PRIMARY KEY, account JSON, settings JSON, state JSON, updated_at)   -- tokens are files, never rows
meta        (key TEXT PRIMARY KEY, value JSON)
datasets    (id, name UNIQUE, format, positions, games, size_bytes, source JSON, source_kind, score_units, path, created_at,
             updated_at)                                         -- R4; read from each dataset's manifest.json
nets        (id, name UNIQUE, sha256 UNIQUE, path, arch, size_bytes, parent, dataset, run JSON, metrics JSON, created_at)
train_steps (work, seq, idea, superbatch, batch, loss, lr, grad_norm, positions_per_sec, elapsed_seconds,
             PRIMARY KEY (work, seq))                            -- a run's loss chart
             -- versions.net and builds.net (NULL: the commit's own net); the build cache is (commit_sha, features, net)
```

A game's offers, clocks and actions are not columns: they belong to the game being played, which `play` holds in memory and overlays on the stored record.

Trial games are read from fastchess's PGN after each round into `games` (`source_kind = 'trial'`), so any test game opens on the board. Tune games are not stored as games (tens of thousands of near-identical pairs); a tune keeps its steps.

## API (`crates/server`)

Resource routes; actions are `POST` sub-routes. Every route has a `utoipa` path, so `crates/server/openapi.json` (`dameless openapi`) is complete and typed, and `scripts/api-types.mjs` (`corepack pnpm api`; `api:check` fails on stale files) regenerates `apps/web/src/api/schema.d.ts` from it. Errors are `{ error: { code, message } }` with codes `bad_request` 400, `unauthorized` 401, `forbidden` 403 (a foreign `Host` or `Origin`), `not_found` 404, `conflict` 409, `fair_play` 409, `rate_limited` 429 (with `Retry-After` when known), `upstream` 502, and `internal` 500 for anything unexpected (core::error::Error carries one of these codes, so every crate shares the vocabulary). A body, query or path that does not read is a `bad_request` in the same shape.

**Who may call** (`api::guard`): every request but `GET /api/health` and the login callback carries `Authorization: Bearer <token>` (the event stream also takes `?token=`, for `EventSource`). Bound to loopback, every request (the UI's pages too) must name a loopback `Host` (DNS rebinding), and a request with an `Origin` must come from the app itself (`http://<host>`) or the hosted UI `https://chess.dame.gg`, which gets CORS (with `Authorization`, `Last-Event-ID` and private-network access). The UI the app serves needs no token: bound to loopback, a request the browser marks `Sec-Fetch-Site: same-origin` whose `Origin`, if any, is `http://<its Host>` is let in (no page can forge that header, and a program on the machine could read the token anyway); the page also carries the token, `<meta name="dameless-token" content="…">`, for browsers that send no `Sec-Fetch-Site`. Every answer carries `X-Frame-Options: DENY`, `nosniff` and `no-referrer`. The web UI is `apps/web/dist`, read from disk in debug builds and embedded (rust-embed) in release, with paths that are not files answered by `index.html`.

Versions in requests and queries are plain strings read by `VersionRef::parse`: `champion` (or `main`), `v14`, `#12` (an id), a branch, tag or commit. Seats of a local game are `you`, `dameless`, `dameless:<version>`, `dameless@<level>`, `dameless:<version>@<level>`. Player keys are `you`, `dameless`, `lichess:<name>`, `stockfish:<n>`.

```
GET    /api/health                                   { ok, version }; no token
GET    /api/status                                   { version, machine: { os, cpu, physical_cores, logical_cores, memory_bytes, data_dir,
                                                       engine: { dir, found, branch, commit, dirty }, builds: { count, bytes } },
                                                       cores, ledger, policy, lichess_url, next_event }
GET    /api/token                                    { token } (Settings: pairing the hosted UI)
POST   /api/setup                                    downloads fastchess and the book (explicit) → { log }
GET    /api/diagnostics                              work's Health (Settings › Machine); behind the token, since it names paths

GET    /api/games?player=&source=&status=&speed=&rated=&trial=&since=&before=&limit=&calibration=
                                                     GameSummary[], live first then newest; lists are comma-separated;
                                                     calibration games are left out unless calibration=true|all
POST   /api/games                                    NewGameRequest { white, black, time, from?: {type: position, fen, moves} | {type: book},
                                                     hash?, threads?, adjudicate?, seed? } → 201 Game
POST   /api/games/import                             { pgn, you?: white|black } → 201 GameSummary[] (finished local games)
GET    /api/games/{id}                               Game
GET    /api/games/{id}/pgn                           application/x-chess-pgn
POST   /api/games/{id}/actions/{action}              move { uci } (UCI or SAN) | offer_draw | accept_draw | … → Game (see the Referee)
POST   /api/games/{id}/stop                          stop a live game: a game you sit in is resigned (aborted before each
                                                     side has moved), an engine game aborted, the bot's resigned on Lichess
GET    /api/games/{id}/review?version=&nodes=        Review (404 without one)

POST   /api/eval                                     { fen?, moves, version?, multipv, budget: {type: depth|nodes|infinite, …}, threads?, client }
                                                     → EvalSession { session, version, fen }; `eval.info` on `eval:<session>`
GET    /api/eval                                     the sessions running ([{ session, client }]); one that reached its budget is gone
DELETE /api/eval/{session}                           { stopped }
GET    /api/explorer?db=lichess|masters|player&fen=&play=&speeds=&ratings=&modes=&since=&until=&moves=&top_games=&recent_games=&player=&color=
                                                     Lichess's answer, as is (needs the you connection: 409 without it)
GET    /api/tablebase?fen=                           Lichess's answer, as is

POST   /api/reviews                                  { games | player, limit?, speeds?, rated?, version?, nodes?, concurrency? } → 202 Work
GET    /api/players/top?speed=blitz&nb=10            [{ key, username, title, rating, progress }] (Lichess's leaderboard)
GET    /api/players/{key}                            Player
GET    /api/players/{key}/profile?speeds=&rated=&since=&limit=   Profile
GET    /api/compare?players=…&speeds=&rated=&since=&limit=       Comparison
POST   /api/players/{key}/import                     { max?, since?, speeds?, rated? } → 202 Work (lichess:<name>: their history,
                                                     refused while you play; you: your history)
GET    /api/players/you/repertoire?side=&max_plies=&min_games=&limit=   Repertoire (409 fair_play while the explorer is not allowed)
GET    /api/progress?window=10                       Progress

GET    /api/levels                                   LevelsOverview { levels: [{ level, rating, games }], you, suggested, updated_at }
GET    /api/levels/plan?count=3                      PlannedMatch[]
POST   /api/levels/calibrate                         { matches?, count?, games_per_pair?, version? } → 202 Work
GET    /api/drills                                   Drill[] (soonest due first)
POST   /api/drills                                   { limit? } → 201 Drill[] made from your weak spots
GET    /api/drills/today?new_per_day=&limit=         DrillsToday { drills, due, new, started_today }
POST   /api/drills/{id}/answer                       { move } → DrillAnswerResult
DELETE /api/drills/{id}                              204
GET    /api/drills/trend?window=50                   HitRatePoint[]

GET    /api/dameless                                 { champion, line, ideas: { running, queued, recent }, connection: BotStatus?,
                                                       strength: ProgressPoint[] (cumulative Elo over v1, oldest first) }
GET    /api/versions?line=                           Version[]
POST   /api/versions                                 { ref } → 201 Version; its build is queued (see /api/work)
GET    /api/versions/{id}/builds                     Build[] (diagnostics)
DELETE /api/builds/{id}                              Build (409 while in use)
POST   /api/builds/prune                             { older_than_days?, keep_latest? (true), dry_run } → { builds, bytes, in_use, dry_run }
GET    /api/tunables?branch=                         Tunable[] from the branch's spsa build; 409 (and the build queued) while there is none
POST   /api/dameless/push                            push the fork's main to origin (explicit, confirmed in the UI) → { remote, head }

GET    /api/ideas?status=&tag=&limit=                Idea[]
POST   /api/ideas                                    NewIdeaRequest { title, hypothesis, kind, branch, base?, author?, tags?, notes?, long?,
                                                     tune?, custom?, fix_bench? } → 201 Idea
POST   /api/ideas/batch                              { ideas: [...], tag } → 201 Idea[]
GET    /api/ideas/check-branch?branch=               BranchCheck (never builds)
POST   /api/ideas/fix-bench                          { branch, commit? } → BranchCheck
GET    /api/ideas/{id}                               Idea with trials
GET    /api/ideas/{id}/trials/{trial}                Trial
GET    /api/ideas/{id}/tune                          { spec, params: [{ name, start, min, max }], steps }
POST   /api/ideas/{id}/stop | /resume                Idea
POST   /api/ideas/{id}/promote                       the new champion Version, once the merge is done (409 when refused)
POST   /api/harness/check                            { pairs? } → 201 Idea
GET    /api/harness                                  HarnessStatus

GET    /api/nets | /api/nets/{id}                    Net[] (newest first) | Net
GET    /api/datasets                                 DataStatus { cap_bytes, used_bytes, disk_free_bytes, keep_free_bytes, datasets }
POST   /api/datasets/datagen                         DatagenRequest { name?, positions, threads?, soft_nodes?, opening?, version? } → 201 Work
POST   /api/datasets/import                          ImportRequest { url, expected_bytes, sha256?, name, confirm } → 201 Work (400 without confirm)
DELETE /api/datasets/{id}                            { confirm: true } → Dataset
POST   /api/ideas/{id}/dataset                       { name? } → 201 Dataset from the idea's trial games
POST   /api/train                                    TrainSpec → 201 Work (a run outside any idea; 409 without a GPU)
GET    /api/ideas/{id}/train                         TrainView { spec, steps, net } (net ideas: POST /api/ideas with kind: net and train)
GET    /api/work/{id}/train                          TrainStep[]

GET    /api/work?recent=20                           { running, queued, recent }
GET    /api/work/{id}                                Work
POST   /api/work/{id}/stop | /pause | /resume        Work
GET    /api/work/{id}/log                            text/plain

GET    /api/connections                              { you: ConnectionStatus, bot: ConnectionStatus }
POST   /api/connections/you/login                    { return_to? } → { url, expires_in_seconds } (OAuth PKCE; the redirect is
                                                     http://<the request's Host>/api/connections/you/callback)
GET    /api/connections/you/callback                 outside the guard and the OpenAPI document: 303 to return_to
PUT    /api/connections/{key}/token                  { token } → ConnectionStatus
DELETE /api/connections/{key}                        log out (revokes at Lichess for you) → ConnectionStatus
POST   /api/connections/you/seek                     { time, rated, color, rating_range? } → ConnectionStatus;  DELETE to cancel
POST   /api/connections/{key}/challenge              { username | ai_level | my_bot, time, rated, color } → 201 ChallengeSent
POST   /api/connections/{key}/challenges/{id}/accept | /decline { reason? } ;  DELETE …/challenges/{id}
                                                     (you only: the bot answers challenges by its settings, 400)
PUT    /api/connections/bot/settings                 BotSettings → BotSettings
POST   /api/connections/bot/online                   { online: bool } → ConnectionStatus
POST   /api/connections/bot/seek                     { mode: once|always|off } → ConnectionStatus
GET    /api/connections/bot/bots                     OnlineBot[]

GET    /api/events?since=&topics=&token=             SSE
```

**The event stream**: each event is `id: <id>`, `event: <kind>`, `data: { id, topic, kind, data }` (the `Event` schema: a union of every kind with its typed payload). `since` or `Last-Event-ID` replays the kept events after it, then live ones follow. When events after that id are no longer kept, or a slow client fell behind and events were dropped, a `reload` event (no SSE id; envelope id 0, topic `stream`, `data: { reason }`) tells the client to fetch what it shows again. A comment every 15 seconds keeps idle connections open; streams end when the app stops.

**The CLI** (`crates/server/src/cli`) mirrors the routes; `dameless::cli::COMMANDS` maps every operation id to its command, and a test checks it against the document. Every command takes `--json` (one JSON document on stdout; errors on stderr, non-zero exit); `--wait` (work, ideas, builds) and `--follow` (games) where something runs. It finds the app at `DAMELESS_URL` (default http://127.0.0.1:7979), with `DAMELESS_TOKEN` or the token in the data directory (`DAMELESS_DATA`). Tokens for Lichess are read from stdin, never from the command line.

```
dameless serve | openapi | health | status | diagnostics | token | setup | push | events [--topics] [--since]
dameless games [--player --source --status --speed --rated --trial --calibration --limit]
dameless game <id> [--pgn | --review [--version --nodes] | --follow]
dameless play --white you --black dameless@5 --tc 5+3 | --nodes N | --movetime MS [--fen --moves … | --book] [--follow]
dameless move <game> e4 [--no-wait] | draw <game> [--accept|--decline] | takeback <game> [--accept|--decline]
dameless resign|abort|stop <game> | claim <game> [--draw]
dameless import you | lichess:<name> [--max --since --speeds --rated --wait] | import --pgn <file|-> [--you white]
dameless review <game>… | --player you [--limit --version --nodes --wait]
dameless player <key> | profile <key> | compare <key>… [--speeds --rated --since --limit] | players top [speed] [--nb]
dameless repertoire --side white | progress | levels [plan | calibrate] | drills [today | make | answer <id> <move> | delete <id> | trend]
dameless eval [--fen --moves --version --depth|--nodes --multipv] (streams until done) | eval --list | eval --stop <session>
dameless explorer [--db --fen --play …] | tablebase --fen <fen, _ for spaces>
dameless dameless | versions [--line] | version make <ref> [--wait] | builds <version> | build delete <id> | build prune [--all --dry-run]
dameless ideas | idea new [--kind net …]|list|show|trial|tune|stop|resume|promote|batch|check-branch|fix-bench …
dameless test --dev x --base main --preset vstc --bounds 0,3 [--pairs N] [--wait]       (a custom idea)
dameless tune --branch spsa/tunables [--params a,b --iterations --pairs --tc 8+0.08 | --nodes] [--wait]
dameless tunables <branch> | harness [check --pairs 2000 --wait]
dameless nets [--id N] | data [datagen --positions 20M | import <url> --bytes N --name d [--yes] | delete <id|name> --yes | from-trials <idea>]
dameless train [--dataset 3 … --parent N --from-scratch --superbatches 20 …] [--wait] | train show <idea> | train steps <work>
dameless work [show|stop|pause|resume|log|wait <id>]
dameless lichess | lichess you [login | token | logout | seek --tc 10+5 | seek --stop | challenge … | accept|decline|cancel <id>]
dameless lichess bot [token | logout | settings --set key=value | online | offline | seek once|always|off | challenge … | bots]
```

## The web app (`apps/web`)

### Layout

```
┌ header: Dameless · [command bar ⌘K] · You 1840 · Dameless v14 · ⚙ ──────────────────────────────────────┐
├ Games rail ─────────┬────────────── Board ──────────────────────┬ Panel ────────────────────────────────┤
│ filter: All · You · │ opponent strip (name, rating, clock)       │ mode tabs: Play · Watch · Review ·     │
│ Dameless · Lichess  │                                            │ Explore (only those that apply)        │
│ · Tests             │        board (chessground) + eval bar      │                                        │
│ live first, then    │                                            │ the mode's content                     │
│ newest; an idea's   │ your strip (name, rating, clock)           │                                        │
│ games fold into one │ transport + eval graph scrubber            │ moves list (always, under the content) │
│ row                 │                                            │                                        │
├─────────────────────┴────────────────────────────────────────────┴────────────────────────────────────────┤
│ status line: ● Testing deeper-lmr · short games · 412 pairs · ~40 min        Analysis is off (when it is) │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Routes** (the address is the board's state): `/` (the board; `?game=<id>&ply=<n>` or `?fen=…`), `/you`, `/dameless`, `/dameless/ideas/<id>`, `/players/<key>`, `/compare?players=…`, `/settings`. Profiles and settings are **sheets** over the board's right side on wide screens (the board stays visible and live), full screens on narrow ones; closing returns to the board as it was.
- **What opens first** at `/` with nothing chosen: a live game (yours first, then the bot's, then a trial's), else your last finished game, else the start position in Explore.
- **Modes** (the panel's tabs, only those that apply to what is on the board):
  - **Play:** you are a seat in a live game. Clocks, offers (draw, takeback, abort/resign), the opponent leaving and claims. On Lichess, nothing else is on screen (no eval bar, no engine, no graph).
  - **Watch:** a live game you are not in: the thinking engine's line and score, both clocks, the eval graph growing. For a trial game, a line linking its idea.
  - **Review:** a finished game. Accuracy per side, class counts (each jumps to the next such move), the selected move's judgement, the better move as an arrow and "Show line", **Play this out** (a local game from here against Dameless), the eval graph with judged dots. Not reviewed yet: one button, "Review this game" (about 20 s).
  - **Explore:** free analysis from any position: engine lines (MultiPV), the explorer (Lichess, Masters, your games), the tablebase at 7 pieces or fewer. Play moves freely; "Play from here" starts a game.
- **New game** (the one filled button in the header area when no game is live, and `N`): opponent (Dameless at a version and strength, Lichess: find a game, challenge someone, Stockfish N), time control, colour, rated. One dialog, the choices that apply to the opponent.
- **Games rail:** one list of `GameSummary`, filtered All · You · Dameless · Lichess · Tests, live games pinned on top with a pulse, then newest first. Trial games fold into one row per trial ("deeper-lmr vs v14 · short games · 412 pairs"), which opens into its games. Each row: players, result, accuracy if reviewed, time control, when.
- **You** (`/you`): rating and accuracy trend, phases, openings, time, best moments and worst blunders (links onto the board), weak spots with Play this out, Compare with… (adds players), your Lichess connection's status (log in when not linked), your challenges.
- **Dameless** (`/dameless`): the strength line (champion versions and cumulative Elo), the champion, ideas running and queued, the logbook (every idea newest first: title, author, verdict, Elo, notes), New idea (gain / neutral / speed / tune, or a custom test), and **Out in the world** (the bot: online switch, its rating, seeking, rules, recent challenges). An idea (`/dameless/ideas/<id>`) shows its hypothesis, notes, the funnel as a row of stages with the deciding numbers, the selected trial's evidence (LLR gauge, Elo ±, pentanomial, chart), its games (onto the board), a tune's trajectory, and Promote when kept.
- **Settings** (`/settings`): Lichess connections (your login, the bot's token), the machine (cores, the engine checkout, builds and disk), defaults (review nodes, test concurrency), theme, the runner token for the hosted UI.
- **Command bar** (`⌘K` / `Ctrl K`): places, games, players, ideas, versions, and verbs in the CLI's words (`play 5+3 dameless`, `review last 20`, `compare cutemouse83`, `new idea`).
- **Status line:** what background work is doing in one sentence, the queue on click (a popover: running, queued, recent, with stop/pause), and "Analysis is off while you're playing on Lichess" when the policy says so. It is the only place background work appears.
- **Toasts:** a game of yours starting elsewhere ("Your game against X started" — Open), an incoming challenge (Accept · Decline), an idea finishing.

### Look and rules

Carried over from the old `web/DESIGN.md`: VLC + Lichess; dense and sharp (radius 3, depth 1, compact density, snappy motion); hairline dividers, flat panes; Hanken Grotesk for reading, JetBrains Mono for numbers, Fraunces only for the wordmark; colour carries meaning (cognac for the current item and the primary action; success/warning/danger for verdicts; Lichess's glyph colours for move classes); one fact once per screen; plain words first, `Term` toggletips for jargon; every control an eos component imported by path, never a native control; parent owns space; one filled button per screen; sentence case; logical properties; `prefers-reduced-motion`; keyboard first. The stack breakpoint is 75rem. The board's side is rounded to chessground's 8-device-pixel step and must meet its frame exactly.

Keyboard: `←/→` moves, `Home/End`, `F` flip, `Space` engine on/off (Explore), `X`/`Shift X` next/previous mistake (Review), `N` new game, `⌘K` command bar, `G Y` / `G D` / `G S` go to You / Dameless / Settings, `?` the shortcuts.

### Client architecture

- `api/client.ts`: openapi-fetch typed by `schema.d.ts`; the base URL and token (same origin, or a paired remote runner for the hosted site).
- `api/stream.ts`: one SSE connection, typed events, resumes with the last id, dispatches into stores.
- `stores/` (Pinia): `games` (summaries by id, the open game in detail), `ideas`, `work`, `connections`, `policy`, `versions`, `players`. Each loads with one GET and stays current from events. No component fetches on its own.
- `board/`: `Board.vue` (chessground wrapper), `EvalBar`, `EvalGraph` (uPlot), `MoveList`, `Strip` (a seat: name, rating, clock), `Transport`. `position.ts` (chessops) holds the game tree and the cursor.
- `panel/`: `PlayPanel`, `WatchPanel`, `ReviewPanel`, `ExplorePanel`.
- `rail/`, `sheets/` (`You`, `Dameless`, `Idea`, `Player`, `Compare`, `Settings`), `shell/` (header, command bar, status line, toasts, new-game dialog).

## Working on the rebuild

- Branch `next`, worktree `C:\dev\chess-next`. The old app keeps running from `C:\dev\chess-live` on port 7878 and **must not be disturbed**: never build into `C:\dev\chess\target`, never stop the preview server `chess-runner`, never use port 7878 or the old data folder `chess-harness`.
- The old code is the reference for every ported part: `C:\dev\chess\runner\src` and `C:\dev\chess\web\src` (branch `main`). Port what is correct (stats, scaling, judge, compare, Lichess rules, seek, quota, OAuth, import, fastchess driving, UCI) with its tests; reshape it to this design rather than copying its structure.
- No data migration: the new app starts with an empty database.
- **Never touch real accounts.** No agent logs in to Lichess, stores a real token, or talks to lichess.org; every Lichess path is tested against the mock server (`dameless_lichess::testing::MockLichess`, behind the crate's `mock` feature so `play` and `server` test against the same mock).
- Each agent owns its crate or directory and commits only its own paths: `git commit -m "…" -- <paths>`, never `git add -A`, never leave files staged. Use your own `CARGO_TARGET_DIR` (`C:\dev\chess-next\target-<you>`); `target*/` is ignored.
- Gates per crate before committing: `cargo fmt`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Web: `corepack pnpm lint`, `format:check`, `typecheck`, `test`, `knip`.
- Doc comments say what a thing is and why, in plain sentences, as the old code does. No commented-out code, no TODOs without an owner.
