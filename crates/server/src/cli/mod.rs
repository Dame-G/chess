//! The `dameless` command line. `dameless serve` runs the app; every other command is one API call (or a few, or a
//! poll, or the event stream) against a running app, printed as one JSON document with `--json` and as a short
//! summary otherwise. Errors go to stderr with a non-zero exit. [`COMMANDS`] says which command calls which route,
//! and a test checks that every route of the OpenAPI document has one.

pub mod client;

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use dameless_core::TimeControl;
use serde_json::{Value, json};

pub use client::Client;
use client::print;

/// The command line.
#[derive(Parser)]
#[command(name = "dameless", version, about = "You and your engine, getting stronger at one board")]
pub struct Cli {
    /// The data directory (database, token, builds). The platform's local data folder by default.
    #[arg(long, global = true, env = "DAMELESS_DATA")]
    pub data: Option<PathBuf>,

    /// The app's URL, for every command but `serve`.
    #[arg(long, global = true, env = "DAMELESS_URL", default_value = "http://127.0.0.1:7979")]
    pub url: String,

    /// The API token. A local app's is read from its data directory.
    #[arg(long, global = true, env = "DAMELESS_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    /// Print one JSON document instead of a summary.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run the app: the API, the event stream and the web UI (`DAMELESS_LISTEN`, default 127.0.0.1:7979).
    Serve,
    /// Print the OpenAPI document.
    Openapi,
    /// Whether the app is up.
    Health,
    /// The app, the machine, the cores and fair play.
    Status,
    /// The machine's health: tools, engine checkout, disk, harness.
    Diagnostics,
    /// Print the API token (for the hosted UI).
    Token,
    /// Download fastchess and the opening book (explicit; a few hundred MB).
    Setup,
    /// Push the fork's main to origin (explicit).
    Push,
    /// Follow the event stream, one JSON line per event.
    Events {
        /// Topics, comma-separated (`games,work`, `game:12`, `eval:*`).
        #[arg(long)]
        topics: Option<String>,
        #[arg(long, default_value_t = 0)]
        since: u64,
    },

    /// Games, live first then newest.
    Games(GamesArgs),
    /// One game; `--pgn` prints its PGN, `--review` its review, `--follow` its moves as they are played.
    Game {
        id: i64,
        #[arg(long)]
        pgn: bool,
        #[arg(long)]
        review: bool,
        /// With `--review`: the version that reviewed it.
        #[arg(long)]
        version: Option<String>,
        /// With `--review`: its node budget.
        #[arg(long)]
        nodes: Option<u64>,
        #[arg(long)]
        follow: bool,
    },
    /// Start a local game: `--white you --black dameless@5 --tc 5+3`.
    Play(PlayArgs),
    /// Play your move (UCI or SAN) and, unless `--no-wait`, wait for the reply.
    Move {
        game: i64,
        #[arg(value_name = "MOVE", allow_hyphen_values = true)]
        mv: String,
        #[arg(long)]
        no_wait: bool,
    },
    /// Offer a draw (or `--accept` / `--decline` the opponent's).
    Draw {
        game: i64,
        #[arg(long, conflicts_with = "decline")]
        accept: bool,
        #[arg(long)]
        decline: bool,
    },
    /// Resign.
    Resign { game: i64 },
    /// Ask to take back your last move (or `--accept` / `--decline` the opponent's request).
    Takeback {
        game: i64,
        #[arg(long, conflicts_with = "decline")]
        accept: bool,
        #[arg(long)]
        decline: bool,
    },
    /// Abort a game that has not got going.
    Abort { game: i64 },
    /// Claim the game (`--draw`: a draw) once the opponent has been gone long enough.
    Claim {
        game: i64,
        #[arg(long)]
        draw: bool,
    },
    /// Stop a live game: resigned if you sit in it, aborted if it is an engine game.
    Stop { game: i64 },
    /// Import games: `import lichess:<name>` (their history), `import you` (yours), or `import --pgn <file|->`.
    Import(ImportArgs),
    /// Review games with the engine: `review <game>…` or `review --player you`.
    Review(ReviewArgs),
    /// What is known of a player.
    Player { key: String },
    /// A player's profile: results, accuracy, openings, time, moments, weak spots.
    Profile {
        key: String,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Players side by side: `compare you lichess:cutemouse83`.
    Compare {
        players: Vec<String>,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Lichess's players: `players top [blitz]`.
    #[command(subcommand)]
    Players(PlayersCommand),
    /// Your repertoire with one colour.
    Repertoire {
        #[arg(long, default_value = "white")]
        side: String,
        #[arg(long)]
        max_plies: Option<u32>,
        #[arg(long)]
        min_games: Option<u32>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Both players' progress: your rating, accuracy and drills, Dameless's Elo.
    Progress {
        #[arg(long)]
        window: Option<usize>,
    },
    /// Dameless's levels; `levels plan`, `levels calibrate`.
    Levels(LevelsArgs),
    /// Your drills; `drills today|make|answer <id> <move>|delete <id>|trend`.
    Drills(DrillsArgs),
    /// Evaluate a position, streaming the engine's lines until its budget is reached (`--list`, `--stop <session>`).
    Eval(EvalArgs),
    /// Lichess's opening explorer.
    Explorer(ExplorerArgs),
    /// The tablebase, for seven pieces or fewer.
    Tablebase {
        #[arg(long)]
        fen: String,
    },

    /// The nets the app knows (`--id` for one).
    Nets {
        #[arg(long)]
        id: Option<i64>,
    },
    /// Datasets and the room left; `data datagen|import|delete|from-trials`.
    Data(DataArgs),
    /// A training run on the GPU (`train`), or a net idea's loss curve (`train show <idea>`).
    Train(Box<TrainArgs>),
    /// Dameless's logbook: champion, line, ideas, the bot, strength.
    Dameless,
    /// Ideas, newest first.
    Ideas(IdeasArgs),
    /// `idea new|list|show|trial|tune|stop|resume|promote|batch|check-branch|fix-bench`.
    #[command(subcommand)]
    Idea(IdeaCommand),
    /// A custom idea of one trial: `test --dev x --base main --preset vstc --bounds 0,3`.
    Test(TestArgs),
    /// A tune idea: SPSA over a branch's tunables, then confirmed as a gain.
    Tune(TuneArgs),
    /// The tunables a branch declares.
    Tunables { branch: String },
    /// The harness check: `harness` (status) or `harness check [--determinism-pairs 1000 --symmetry-pairs 4000] [--wait]`.
    Harness(HarnessArgs),
    /// Taking upstream Reckless: `upstream` (status), `upstream sync [--wait]`, `upstream auto on|off`.
    Upstream(UpstreamArgs),
    /// Versions, newest first.
    Versions {
        /// One line: champion, idea, upstream or other.
        #[arg(long)]
        line: Option<String>,
    },
    /// `version make <ref>`: a version of a branch, tag or commit, built.
    #[command(subcommand)]
    Version(VersionCommand),
    /// A version's builds (diagnostics).
    Builds { version: i64 },
    /// `build delete <id>`, `build prune`.
    #[command(subcommand)]
    Build(BuildCommand),
    /// Background work: running, queued, recent; or `work show|stop|pause|resume|log|wait <id>`.
    Work(WorkArgs),
    /// The Lichess connections: `lichess you …`, `lichess bot …`, or both's status.
    Lichess(LichessArgs),
}

#[derive(Args)]
pub struct GamesArgs {
    #[arg(long)]
    pub player: Option<String>,
    /// local, lichess, import, trial (comma-separated).
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub speed: Option<String>,
    #[arg(long)]
    pub rated: Option<bool>,
    #[arg(long)]
    pub trial: Option<i64>,
    /// Calibration games: true (only them) or all.
    #[arg(long)]
    pub calibration: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

#[derive(Args)]
pub struct PlayArgs {
    /// you, dameless, dameless:<version>, dameless@<level> or dameless:<version>@<level>.
    #[arg(long, default_value = "you")]
    pub white: String,
    #[arg(long, default_value = "dameless")]
    pub black: String,
    /// A clock as minutes+seconds: 5+3, 0.5+0.
    #[arg(long, conflicts_with_all = ["nodes", "movetime"])]
    pub tc: Option<String>,
    #[arg(long)]
    pub nodes: Option<u64>,
    #[arg(long)]
    pub movetime: Option<u64>,
    #[arg(long)]
    pub fen: Option<String>,
    #[arg(long, num_args = 1.., value_delimiter = ' ')]
    pub moves: Vec<String>,
    /// Start from a random line of the trials' opening book.
    #[arg(long, conflicts_with = "fen")]
    pub book: bool,
    #[arg(long)]
    pub hash: Option<u32>,
    #[arg(long)]
    pub threads: Option<u32>,
    #[arg(long)]
    pub adjudicate: bool,
    /// Print each move as it is played, then the result.
    #[arg(long)]
    pub follow: bool,
}

#[derive(Args)]
pub struct ImportArgs {
    /// `you` or `lichess:<name>`.
    pub player: Option<String>,
    /// A PGN file to store as local games (`-` for stdin).
    #[arg(long, conflicts_with = "player")]
    pub pgn: Option<PathBuf>,
    /// With `--pgn`: the side you played (white or black).
    #[arg(long)]
    pub you: Option<String>,
    #[arg(long)]
    pub max: Option<u32>,
    /// Games from this time on (ms since 1970).
    #[arg(long)]
    pub since: Option<i64>,
    #[arg(long, value_delimiter = ',')]
    pub speeds: Vec<String>,
    #[arg(long)]
    pub rated: Option<bool>,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args)]
pub struct ReviewArgs {
    pub games: Vec<i64>,
    #[arg(long, conflicts_with = "games")]
    pub player: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
    #[arg(long, value_delimiter = ',')]
    pub speeds: Vec<String>,
    #[arg(long)]
    pub rated: Option<bool>,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub nodes: Option<u64>,
    #[arg(long)]
    pub concurrency: Option<u32>,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Clone)]
pub struct FilterArgs {
    #[arg(long)]
    pub speeds: Option<String>,
    #[arg(long)]
    pub rated: Option<bool>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
}

impl FilterArgs {
    fn query(&self) -> Vec<(&'static str, String)> {
        let mut q = Vec::new();
        push(&mut q, "speeds", self.speeds.clone());
        push(&mut q, "rated", self.rated.map(|r| r.to_string()));
        push(&mut q, "since", self.since.clone());
        push(&mut q, "limit", self.limit.map(|l| l.to_string()));
        q
    }
}

#[derive(Subcommand)]
pub enum PlayersCommand {
    /// Lichess's leaderboard.
    Top {
        #[arg(default_value = "blitz")]
        speed: String,
        #[arg(long, default_value_t = 10)]
        nb: u32,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct LevelsArgs {
    #[command(subcommand)]
    pub action: Option<LevelsAction>,
}

#[derive(Subcommand)]
pub enum LevelsAction {
    /// The calibration matches that would teach the most now.
    Plan {
        #[arg(long, default_value_t = 3)]
        count: usize,
    },
    /// Queue calibration: levels play each other in the background.
    Calibrate {
        #[arg(long)]
        count: Option<usize>,
        #[arg(long)]
        games_per_pair: Option<u32>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        wait: bool,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DrillsArgs {
    #[command(subcommand)]
    pub action: Option<DrillsAction>,
}

#[derive(Subcommand)]
pub enum DrillsAction {
    /// Today's drills (the answers stay hidden).
    Today {
        #[arg(long)]
        new_per_day: Option<u32>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Make drills from your weak spots.
    Make {
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Answer a drill (UCI or SAN).
    Answer {
        id: i64,
        #[arg(value_name = "MOVE", allow_hyphen_values = true)]
        mv: String,
    },
    Delete {
        id: i64,
    },
    /// Your hit rate over time.
    Trend {
        #[arg(long)]
        window: Option<usize>,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DataArgs {
    #[command(subcommand)]
    pub action: Option<DataAction>,
}

#[derive(Subcommand)]
pub enum DataAction {
    /// Self-play data: `--positions 20M`.
    Datagen {
        /// Total positions, with an optional K, M or G suffix.
        #[arg(long)]
        positions: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        threads: Option<u32>,
        #[arg(long)]
        soft_nodes: Option<u64>,
        /// dfrc, chess960 or startpos.
        #[arg(long)]
        opening: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        wait: bool,
    },
    /// Download one shard into a dataset; asks before downloading unless `--yes`.
    Import {
        url: String,
        /// The download's size in bytes.
        #[arg(long)]
        bytes: u64,
        #[arg(long)]
        sha256: Option<String>,
        #[arg(long)]
        name: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
    },
    /// Delete a dataset (by id or name).
    Delete {
        dataset: String,
        #[arg(long)]
        yes: bool,
    },
    /// A dataset from an idea's trial games.
    FromTrials {
        idea: i64,
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Args, Clone, Default)]
pub struct ScheduleArgs {
    #[arg(long)]
    pub superbatches: Option<u32>,
    #[arg(long)]
    pub batches_per_superbatch: Option<u32>,
    #[arg(long)]
    pub batch_size: Option<u32>,
    #[arg(long)]
    pub lr: Option<f64>,
    #[arg(long)]
    pub final_lr: Option<f64>,
    /// cosine, linear, step or constant.
    #[arg(long)]
    pub lr_schedule: Option<String>,
    #[arg(long)]
    pub wdl: Option<f64>,
    #[arg(long)]
    pub final_wdl: Option<f64>,
    /// Datasets by id (every one in Reckless's units when none).
    #[arg(long = "dataset")]
    pub datasets: Vec<i64>,
    /// The net to fine-tune; the champion's when none.
    #[arg(long)]
    pub parent: Option<i64>,
    #[arg(long)]
    pub from_scratch: bool,
    /// The new net's name.
    #[arg(long)]
    pub net_name: Option<String>,
}

impl ScheduleArgs {
    /// The TrainSpec: the defaults with what was given laid over.
    fn spec(&self) -> Result<Value> {
        let mut spec = serde_json::to_value(dameless_core::training::TrainSpec::default())?;
        let sch = &mut spec["schedule"];
        let mut set = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                sch[k] = v;
            }
        };
        set("superbatches", self.superbatches.map(|v| json!(v)));
        set("batches_per_superbatch", self.batches_per_superbatch.map(|v| json!(v)));
        set("batch_size", self.batch_size.map(|v| json!(v)));
        set("lr", self.lr.map(|v| json!(v)));
        set("final_lr", self.final_lr.map(|v| json!(v)));
        set("lr_schedule", self.lr_schedule.clone().map(|v| json!(v)));
        set("wdl", self.wdl.map(|v| json!(v)));
        set("final_wdl", self.final_wdl.map(|v| json!(v)));
        spec["datasets"] = json!(self.datasets);
        spec["parent"] = json!(self.parent);
        spec["from_scratch"] = json!(self.from_scratch);
        spec["name"] = json!(self.net_name);
        Ok(spec)
    }
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct TrainArgs {
    #[command(subcommand)]
    pub action: Option<TrainAction>,
    #[command(flatten)]
    pub schedule: ScheduleArgs,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Subcommand)]
pub enum TrainAction {
    /// A net idea's training: the loss curve as text.
    Show { idea: i64 },
    /// One run's readings (any training work).
    Steps { work: i64 },
}

#[derive(Args)]
pub struct EvalArgs {
    #[arg(long)]
    pub fen: Option<String>,
    #[arg(long, num_args = 1.., value_delimiter = ' ')]
    pub moves: Vec<String>,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long, conflicts_with = "nodes")]
    pub depth: Option<u32>,
    #[arg(long)]
    pub nodes: Option<u64>,
    #[arg(long, default_value_t = 1)]
    pub multipv: u32,
    #[arg(long)]
    pub threads: Option<u32>,
    /// List the sessions running.
    #[arg(long, conflicts_with_all = ["stop", "fen"])]
    pub list: bool,
    /// Stop a session.
    #[arg(long)]
    pub stop: Option<String>,
}

#[derive(Args)]
pub struct ExplorerArgs {
    /// lichess, masters or player.
    #[arg(long, default_value = "lichess")]
    pub db: String,
    #[arg(long)]
    pub fen: Option<String>,
    /// Moves from the FEN, UCI, comma-separated.
    #[arg(long)]
    pub play: Option<String>,
    #[arg(long)]
    pub speeds: Option<String>,
    #[arg(long)]
    pub ratings: Option<String>,
    #[arg(long)]
    pub player: Option<String>,
    #[arg(long)]
    pub color: Option<String>,
}

#[derive(Args)]
pub struct IdeasArgs {
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
}

#[derive(Subcommand)]
pub enum IdeaCommand {
    /// A new idea: `idea new --kind gain --branch deeper-lmr --title … --hypothesis …`.
    New(Box<NewIdeaArgs>),
    /// Ideas, newest first.
    List(IdeasArgs),
    /// One idea with its trials.
    Show {
        id: i64,
        /// Wait until it ends.
        #[arg(long)]
        wait: bool,
    },
    /// One trial of an idea.
    Trial { idea: i64, trial: i64 },
    /// A tune idea's parameters and SPSA history.
    Tune { id: i64 },
    /// Stop it.
    Stop { id: i64 },
    /// Carry on a stopped or failed idea.
    Resume { id: i64 },
    /// Merge a kept idea into the fork's main: the next champion.
    Promote { id: i64 },
    /// Several ideas under one tag, from a JSON array of `idea new` bodies (a file, or `-` for stdin).
    Batch {
        file: PathBuf,
        #[arg(long)]
        tag: String,
    },
    /// Whether a branch's tip has a right `Bench:` line (never builds).
    CheckBranch { branch: String },
    /// Fix a branch's `Bench:` line: build its tip and reword it locally with the measured bench.
    FixBench {
        branch: String,
        /// The tip it was checked at; refused if the branch moved since.
        #[arg(long)]
        commit: Option<String>,
    },
}

#[derive(Args)]
pub struct NewIdeaArgs {
    #[arg(long)]
    pub title: String,
    #[arg(long, default_value = "")]
    pub hypothesis: String,
    /// gain, neutral, speed, tune or net.
    #[arg(long, default_value = "gain")]
    pub kind: String,
    /// The branch; a net idea may leave it out (the base's code).
    #[arg(long, default_value = "")]
    pub branch: String,
    #[arg(long)]
    pub base: Option<String>,
    /// you or claude.
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    #[arg(long)]
    pub notes: Option<String>,
    /// Add the long-games stage.
    #[arg(long)]
    pub long: bool,
    /// Fix a missing or stale `Bench:` line on the branch first.
    #[arg(long)]
    pub fix_bench: bool,
    /// A net idea's run.
    #[command(flatten)]
    pub train: ScheduleArgs,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args)]
pub struct TestArgs {
    #[arg(long)]
    pub dev: String,
    #[arg(long, default_value = "main")]
    pub base: String,
    /// vstc, stc, ltc, stc_hash, ltc_hash, smp or nodes.
    #[arg(long, default_value = "stc")]
    pub preset: String,
    /// SPRT bounds in normalized Elo: 0,3 or -2.75,0.25.
    #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
    pub bounds: Option<Vec<f64>>,
    #[arg(long)]
    pub pairs: Option<u32>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args)]
pub struct TuneArgs {
    #[arg(long)]
    pub branch: String,
    #[arg(long)]
    pub base: Option<String>,
    /// Tunables to tune, comma-separated; all declared when none.
    #[arg(long, value_delimiter = ',')]
    pub params: Vec<String>,
    #[arg(long, default_value_t = 1000)]
    pub iterations: u32,
    #[arg(long, default_value_t = 8)]
    pub pairs: u32,
    /// The clock as seconds+seconds, as tests write it: 8+0.08.
    #[arg(long, default_value = "8+0.08", conflicts_with = "nodes")]
    pub tc: String,
    #[arg(long)]
    pub nodes: Option<u64>,
    #[arg(long, default_value_t = 16)]
    pub hash: u32,
    #[arg(long)]
    pub concurrency: Option<u32>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub long: bool,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct HarnessArgs {
    #[command(subcommand)]
    pub action: Option<HarnessAction>,
}

#[derive(Subcommand)]
pub enum HarnessAction {
    /// Check the harness: main against an identical branch, for determinism at 25,000 nodes, then symmetry at VSTC.
    Check {
        /// Pairs of the determinism part (default 1,000): each must be the same game twice.
        #[arg(long)]
        determinism_pairs: Option<u32>,
        /// Pairs of the symmetry part (default 4,000): Elo must hold 0 and the pairs be symmetric.
        #[arg(long)]
        symmetry_pairs: Option<u32>,
        #[arg(long)]
        wait: bool,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct UpstreamArgs {
    #[command(subcommand)]
    pub action: Option<UpstreamAction>,
}

#[derive(Subcommand)]
pub enum UpstreamAction {
    /// Fetch upstream and take its new commits as the next champion now (local merge; never pushes).
    Sync {
        #[arg(long)]
        wait: bool,
    },
    /// Turn the six-hourly automatic sync on or off.
    Auto {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

#[derive(Subcommand)]
pub enum VersionCommand {
    /// Make a version of a branch, tag or commit and build it.
    Make {
        #[arg(value_name = "REF")]
        reference: String,
        #[arg(long)]
        wait: bool,
    },
}

#[derive(Subcommand)]
pub enum BuildCommand {
    /// Delete a build's binary (its record stays).
    Delete { id: i64 },
    /// Delete the binaries nothing uses, keeping each version's newest.
    Prune {
        #[arg(long)]
        older_than_days: Option<u32>,
        /// Prune each version's newest builds too.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct WorkArgs {
    #[command(subcommand)]
    pub action: Option<WorkAction>,
    #[arg(long, default_value_t = 20)]
    pub recent: u32,
}

#[derive(Subcommand)]
pub enum WorkAction {
    Show {
        id: i64,
    },
    Stop {
        id: i64,
    },
    Pause {
        id: i64,
    },
    Resume {
        id: i64,
    },
    Log {
        id: i64,
    },
    /// Wait until it ends; exits non-zero unless it is done.
    Wait {
        id: i64,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct LichessArgs {
    #[command(subcommand)]
    pub account: Option<LichessAccount>,
}

#[derive(Subcommand)]
pub enum LichessAccount {
    /// Your account: status, or login|token|logout|seek|challenge|accept|decline|cancel.
    You(YouArgs),
    /// Dameless's BOT account: status, or token|logout|settings|online|offline|seek|challenge|bots.
    Bot(BotArgs),
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct YouArgs {
    #[command(subcommand)]
    pub action: Option<YouAction>,
}

#[derive(Subcommand)]
pub enum YouAction {
    /// Print the Lichess login link (open it in a browser).
    Login {
        #[arg(long)]
        return_to: Option<String>,
    },
    /// Store a personal token, read from stdin (never from the command line).
    Token,
    /// Forget the token (revoked at Lichess).
    Logout,
    /// Seek a game in the lobby (rapid or slower): `--tc 10+5`; `--stop` cancels.
    Seek {
        #[arg(long)]
        tc: Option<String>,
        #[arg(long)]
        rated: bool,
        #[arg(long, default_value = "random")]
        color: String,
        #[arg(long)]
        range: Option<String>,
        #[arg(long, conflicts_with = "tc")]
        stop: bool,
    },
    /// Challenge a player, Lichess's AI (`--ai 1-8`) or your bot (`--bot`).
    Challenge(ChallengeArgs),
    Accept {
        id: String,
    },
    Decline {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Withdraw a challenge of yours.
    Cancel {
        id: String,
    },
}

#[derive(Args)]
pub struct ChallengeArgs {
    pub username: Option<String>,
    #[arg(long, conflicts_with_all = ["username", "bot"])]
    pub ai: Option<u8>,
    #[arg(long, conflicts_with = "username")]
    pub bot: bool,
    /// minutes+seconds: 5+3.
    #[arg(long)]
    pub tc: String,
    #[arg(long)]
    pub rated: bool,
    #[arg(long, default_value = "random")]
    pub color: String,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BotArgs {
    #[command(subcommand)]
    pub action: Option<BotAction>,
}

#[derive(Subcommand)]
pub enum BotAction {
    /// Store the BOT account's token, read from stdin (never from the command line).
    Token,
    Logout,
    /// Change settings: `--set version=champion --set hash=64` (any field of the bot's settings).
    Settings {
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Accept challenges from people (true or false).
        #[arg(long, alias = "humans", value_name = "BOOL")]
        people: Option<bool>,
    },
    /// Go online (across restarts, until offline).
    Online,
    /// Go offline; games in progress are played out.
    Offline,
    /// Look for games: once, always or off.
    Seek {
        #[arg(default_value = "once")]
        mode: String,
    },
    /// Challenge a player as the bot.
    Challenge {
        username: String,
        #[arg(long)]
        tc: String,
        #[arg(long)]
        rated: bool,
        #[arg(long, default_value = "random")]
        color: String,
    },
    /// Bots online now.
    Bots,
}

/// Which command calls each route, by operation id: what the parity test checks against the OpenAPI document, and
/// the CLI's words for each route.
pub const COMMANDS: &[(&str, &str)] = &[
    ("health", "dameless health"),
    ("status", "dameless status"),
    ("diagnostics", "dameless diagnostics"),
    ("token", "dameless token"),
    ("setup", "dameless setup"),
    ("follow_events", "dameless events --topics games"),
    ("list_games", "dameless games"),
    ("start_game", "dameless play --white you --black dameless@5 --tc 5+3"),
    ("import_pgn", "dameless import --pgn game.pgn"),
    ("get_game", "dameless game 12"),
    ("get_game_pgn", "dameless game 12 --pgn"),
    ("act_in_game", "dameless move 12 e4"),
    ("stop_game", "dameless stop 12"),
    ("get_review", "dameless game 12 --review"),
    ("start_eval", "dameless eval --depth 20"),
    ("list_evals", "dameless eval --list"),
    ("stop_eval", "dameless eval --stop 00000000000a"),
    ("explorer", "dameless explorer --db masters"),
    ("tablebase", "dameless tablebase --fen 8/8/8/8/8/8/6k1/4K2R_w_K_-_0_1"),
    ("review_games", "dameless review 12 13"),
    ("top_players", "dameless players top blitz"),
    ("get_player", "dameless player lichess:cutemouse83"),
    ("get_profile", "dameless profile you"),
    ("compare_players", "dameless compare you lichess:cutemouse83"),
    ("import_player", "dameless import lichess:cutemouse83"),
    ("get_repertoire", "dameless repertoire --side white"),
    ("get_progress", "dameless progress"),
    ("list_levels", "dameless levels"),
    ("plan_levels", "dameless levels plan"),
    ("calibrate_levels", "dameless levels calibrate"),
    ("list_drills", "dameless drills"),
    ("make_drills", "dameless drills make"),
    ("drills_today", "dameless drills today"),
    ("answer_drill", "dameless drills answer 3 Nf3"),
    ("delete_drill", "dameless drills delete 3"),
    ("drills_trend", "dameless drills trend"),
    ("list_nets", "dameless nets"),
    ("get_net", "dameless nets --id 3"),
    ("list_datasets", "dameless data"),
    ("datagen", "dameless data datagen --positions 20M"),
    ("import_dataset", "dameless data import https://example.com/a.binpack --bytes 1000 --name a --yes"),
    ("delete_dataset", "dameless data delete 3 --yes"),
    ("dataset_from_trials", "dameless data from-trials 12"),
    ("get_idea_train", "dameless train show 12"),
    ("train", "dameless train --superbatches 20"),
    ("work_train", "dameless train steps 7"),
    ("get_dameless", "dameless dameless"),
    ("push", "dameless push"),
    ("upstream_status", "dameless upstream"),
    ("set_upstream_settings", "dameless upstream auto off"),
    ("sync_upstream", "dameless upstream sync"),
    ("list_versions", "dameless versions"),
    ("make_version", "dameless version make deeper-lmr"),
    ("list_builds", "dameless builds 3"),
    ("delete_build", "dameless build delete 4"),
    ("prune_builds", "dameless build prune --dry-run"),
    ("list_tunables", "dameless tunables spsa/tunables"),
    ("list_ideas", "dameless ideas"),
    ("create_idea", "dameless idea new --kind gain --branch deeper-lmr --title Deeper"),
    ("create_batch", "dameless idea batch ideas.json --tag lmr"),
    ("get_idea", "dameless idea show 5"),
    ("get_trial", "dameless idea trial 5 9"),
    ("get_tune", "dameless idea tune 5"),
    ("stop_idea", "dameless idea stop 5"),
    ("resume_idea", "dameless idea resume 5"),
    ("promote_idea", "dameless idea promote 5"),
    ("check_branch", "dameless idea check-branch deeper-lmr"),
    ("fix_bench", "dameless idea fix-bench deeper-lmr"),
    ("check_harness", "dameless harness check --symmetry-pairs 4000"),
    ("harness_status", "dameless harness"),
    ("list_work", "dameless work"),
    ("get_work", "dameless work show 7"),
    ("stop_work", "dameless work stop 7"),
    ("pause_work", "dameless work pause 7"),
    ("resume_work", "dameless work resume 7"),
    ("work_log", "dameless work log 7"),
    ("list_connections", "dameless lichess"),
    ("login", "dameless lichess you login"),
    ("set_connection_token", "dameless lichess you token"),
    ("logout", "dameless lichess you logout"),
    ("seek", "dameless lichess you seek --tc 10+5"),
    ("cancel_seek", "dameless lichess you seek --stop"),
    ("challenge", "dameless lichess you challenge alice --tc 5+3"),
    ("accept_challenge", "dameless lichess you accept abcd1234"),
    ("decline_challenge", "dameless lichess you decline abcd1234"),
    ("cancel_challenge", "dameless lichess you cancel abcd1234"),
    ("set_bot_settings", "dameless lichess bot settings --set hash=64"),
    ("set_bot_online", "dameless lichess bot online"),
    ("set_bot_seek", "dameless lichess bot seek always"),
    ("online_bots", "dameless lichess bot bots"),
];

fn push(q: &mut Vec<(&'static str, String)>, key: &'static str, value: Option<String>) {
    if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
        q.push((key, v));
    }
}

/// `?a=1&b=2`, percent-encoded.
fn query(q: &[(&'static str, String)]) -> String {
    if q.is_empty() {
        return String::new();
    }
    let enc = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' | b':' | b'*' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    format!("?{}", q.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&"))
}

/// A path segment: branch names and player keys may hold `/`, `#` and the like.
fn seg(s: &str) -> String {
    s.replace('%', "%25").replace('/', "%2F").replace('#', "%23").replace(' ', "%20").replace('?', "%3F")
}

fn clock(tc: &str) -> Result<Value> {
    Ok(serde_json::to_value(TimeControl::parse_minutes(tc).map_err(|e| anyhow::anyhow!("--tc {tc}: {e}"))?)?)
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "-".into(),
        other => other.to_string(),
    }
}

fn lines(v: &Value, line: impl Fn(&Value) -> String) -> String {
    v.as_array().map(|a| a.iter().map(line).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

fn short(v: &Value) -> String {
    str_of(v).chars().take(8).collect()
}

/// A snake_case word of the API as people write it: `fifty_moves` → `fifty moves`.
fn words(v: &Value) -> String {
    str_of(v).replace('_', " ")
}

/// A timestamp to the minute, as the CLI shows them: `2026-10-27 05:07` (UTC).
fn when(v: &Value) -> String {
    v.as_str().map_or_else(|| "-".into(), |s| s.replace('T', " ").chars().take(16).collect())
}

/// A figure to `digits` decimals, or `-`.
fn fixed(v: &Value, digits: usize) -> String {
    v.as_f64().map_or_else(|| "-".into(), |n| format!("{n:.digits$}"))
}

/// A time control as the app writes it: `3+2`, `3 days`, `1 s a move`, `25k nodes`.
fn tc_text(tc: &Value) -> String {
    let n = |k: &str| tc[k].as_u64().unwrap_or(0);
    let num = |x: f64| if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x}") };
    match tc["type"].as_str() {
        Some("clock") => {
            let minutes = n("initial_ms") as f64 / 60_000.0;
            let base = if minutes >= 1.0 && minutes.fract() == 0.0 {
                num(minutes)
            } else {
                num(n("initial_ms") as f64 / 1000.0)
            };
            format!("{base}+{}", num(n("increment_ms") as f64 / 1000.0))
        }
        Some("correspondence") => format!("{} days", n("days")),
        Some("movetime") => format!("{} s a move", num(n("ms") as f64 / 1000.0)),
        Some("nodes") if n("nodes") >= 1000 => format!("{}k nodes", num(n("nodes") as f64 / 1000.0)),
        Some("nodes") => format!("{} nodes", n("nodes")),
        _ => "-".into(),
    }
}

/// The side to move and the move number of a FEN (the standard start without one).
fn fen_start(start_fen: &Value) -> (bool, usize) {
    let fields: Vec<&str> = start_fen.as_str().unwrap_or_default().split_whitespace().collect();
    let black = fields.get(1) == Some(&"b");
    let number = fields.get(5).and_then(|n| n.parse().ok()).unwrap_or(1).max(1);
    (black, number)
}

/// Moves written with their numbers, `21. Rfe8 Qd2 22. Bf4` or `21... Rfe8#`: `first` is the index of the first one
/// among the game's plies (0 for the first move played from `start_fen`).
fn numbered(sans: &[String], first: usize, start_fen: &Value) -> String {
    let (black_first, number) = fen_start(start_fen);
    let mut out = Vec::with_capacity(sans.len());
    for (i, san) in sans.iter().enumerate() {
        let k = first + i + usize::from(black_first);
        let n = number + k / 2;
        if k.is_multiple_of(2) {
            out.push(format!("{n}. {san}"));
        } else if i == 0 {
            out.push(format!("{n}... {san}"));
        } else {
            out.push(san.clone());
        }
    }
    out.join(" ")
}

/// A game in one line: `#12    finished  Dameless 1400 vs Dameless 1800  0-1 checkmate  1+0  29 moves  2026-10-27 05:07`.
fn game_line(g: &Value) -> String {
    let result = match g["outcome"].as_str() {
        Some("white_wins") => "1-0",
        Some("black_wins") => "0-1",
        Some("draw") => "½-½",
        _ => "*",
    };
    let ending = g["ending"].as_str().map(|_| format!(" {}", words(&g["ending"]))).unwrap_or_default();
    let plies = g["plies"].as_u64().unwrap_or(0);
    let moves = plies.div_ceil(2);
    format!(
        "#{:<5} {:<8}  {} vs {}  {result}{ending}  {}  {moves} {}  {}",
        g["id"].to_string(),
        str_of(&g["status"]),
        str_of(&g["white"]["name"]),
        str_of(&g["black"]["name"]),
        tc_text(&g["time"]),
        if moves == 1 { "move" } else { "moves" },
        when(&g["started_at"])
    )
}

/// `#1    v1                       69aea908  champion  bench 2837905  +3.1 ± 2.0 Elo`.
fn version_line(x: &Value) -> String {
    let bench = x["bench"].as_u64().map(|b| format!("  bench {b}")).unwrap_or_default();
    let elo = if x["elo"].is_object() {
        let e = &x["elo"];
        let (elo, low, high) =
            (e["elo"].as_f64().unwrap_or(0.0), e["low"].as_f64().unwrap_or(0.0), e["high"].as_f64().unwrap_or(0.0));
        format!("  {elo:+.1} ± {:.1} Elo", (high - low) / 2.0)
    } else {
        String::new()
    };
    let upstream = match x["upstream"]["commits"].as_array() {
        Some(c) => format!("  from upstream · {} commit{}", c.len(), if c.len() == 1 { "" } else { "s" }),
        None => String::new(),
    };
    format!(
        "#{:<4} {:<24} {}  {:<8}{bench}{elo}{upstream}",
        x["id"].to_string(),
        str_of(&x["name"]),
        short(&x["commit"]),
        str_of(&x["line"]["type"])
    )
    .trim_end()
    .to_owned()
}

/// `  build #3    ready     bench 2837905  1.21M nps  +spsa`.
fn build_line(b: &Value) -> String {
    let bench = b["bench"].as_u64().map(|n| format!("  bench {n}")).unwrap_or_default();
    let nps = b["nps"].as_f64().map(|n| format!("  {:.2}M nps", n / 1e6)).unwrap_or_default();
    let features: Vec<String> = b["features"].as_array().into_iter().flatten().map(str_of).collect();
    let features = if features.is_empty() { String::new() } else { format!("  +{}", features.join(",")) };
    format!("  build #{:<4} {:<8}{bench}{nps}{features}", b["id"].to_string(), str_of(&b["status"]))
}

fn idea_line(i: &Value) -> String {
    format!(
        "#{:<4} {:<8} {:<8} {}  {}",
        i["id"].to_string(),
        str_of(&i["kind"]),
        str_of(&i["status"]),
        str_of(&i["title"]),
        i["verdict"].as_str().unwrap_or("")
    )
}

fn work_line(w: &Value) -> String {
    let progress = w["progress"]["text"].as_str().map(|t| format!("  {t}")).unwrap_or_default();
    format!(
        "#{:<5} {:<9} {:<8} {}{progress}",
        w["id"].to_string(),
        str_of(&w["kind"]),
        str_of(&w["status"]),
        str_of(&w["title"])
    )
}

fn drill_line(d: &Value) -> String {
    format!("#{:<5} due {}  {:<5}  {}", d["id"].to_string(), when(&d["due"]), str_of(&d["side"]), str_of(&d["fen"]))
}

fn is_final_work(w: &Value) -> bool {
    matches!(w["status"].as_str(), Some("done" | "failed" | "stopped"))
}

fn is_final_idea(i: &Value) -> bool {
    matches!(i["status"].as_str(), Some("kept" | "dropped" | "stopped" | "failed"))
}

fn read_stdin() -> Result<String> {
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn read_file(path: &std::path::Path) -> Result<String> {
    if path.as_os_str() == "-" {
        read_stdin()
    } else {
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))
    }
}

/// Runs a client command.
pub async fn run(c: Client, command: Command, json: bool) -> Result<()> {
    match command {
        Command::Serve | Command::Openapi => bail!("serve and openapi run in the binary itself"),
        Command::Health => {
            print(&c.get("/api/health").await?, json, |v| format!("ok, version {}", str_of(&v["version"])))
        }
        Command::Status => print(&c.get("/api/status").await?, json, |v| {
            let m = &v["machine"];
            let engine = if m["engine"]["found"].as_bool() == Some(true) {
                format!("{} at {}", str_of(&m["engine"]["branch"]), short(&m["engine"]["commit"]))
            } else {
                "not found".into()
            };
            format!(
                "Dameless {} · {} cores ({} free) · analysis {}\nengine: {} ({engine})\ndata: {}",
                str_of(&v["version"]),
                v["cores"],
                v["ledger"]["free"],
                str_of(&v["policy"]["assist"]),
                str_of(&m["engine"]["dir"]),
                str_of(&m["data_dir"])
            )
        }),
        Command::Diagnostics => print(&c.get("/api/diagnostics").await?, json, |v| {
            lines(&v["items"], |i| {
                format!("{:<5} {:<10} {}", str_of(&i["level"]), str_of(&i["key"]), str_of(&i["title"]))
            })
        }),
        Command::Token => print(&c.get("/api/token").await?, json, |v| str_of(&v["token"])),
        Command::Setup => print(&c.post("/api/setup", json!({})).await?, json, |v| lines(&v["log"], str_of)),
        Command::Push => print(&c.post("/api/dameless/push", json!({})).await?, json, |v| {
            format!("pushed main ({}) to {}", short(&v["head"]), str_of(&v["remote"]))
        }),
        Command::Events { topics, since } => {
            c.follow(&topics.unwrap_or_default(), since, |e| {
                println!("{e}");
                true
            })
            .await?;
        }
        Command::Games(a) => {
            let mut q = Vec::new();
            push(&mut q, "player", a.player);
            push(&mut q, "source", a.source);
            push(&mut q, "status", a.status);
            push(&mut q, "speed", a.speed);
            push(&mut q, "rated", a.rated.map(|r| r.to_string()));
            push(&mut q, "trial", a.trial.map(|t| t.to_string()));
            push(&mut q, "calibration", a.calibration);
            push(&mut q, "limit", Some(a.limit.to_string()));
            print(&c.get(&format!("/api/games{}", query(&q))).await?, json, |v| lines(v, game_line));
        }
        Command::Game { id, pgn, review, version, nodes, follow } => {
            if pgn {
                print!("{}", c.text(&format!("/api/games/{id}/pgn")).await?);
            } else if review {
                let mut q = Vec::new();
                push(&mut q, "version", version);
                push(&mut q, "nodes", nodes.map(|n| n.to_string()));
                let v = c.get(&format!("/api/games/{id}/review{}", query(&q))).await?;
                let start_fen =
                    if json { Value::Null } else { c.get(&format!("/api/games/{id}")).await?["start_fen"].clone() };
                print(&v, json, |v| {
                    let s = &v["summary"];
                    format!(
                        "accuracy: White {} · Black {}\n{}",
                        fixed(&s["white"]["accuracy"], 1),
                        fixed(&s["black"]["accuracy"], 1),
                        lines(&v["plies"], |m| {
                            let index = m["ply"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                            let mv = numbered(&[str_of(&m["san"])], index, &start_fen);
                            format!("{mv:<14} {}", words(&m["classification"]))
                        })
                    )
                });
            } else if follow {
                follow_game(&c, id, json).await?;
            } else {
                print(&c.get(&format!("/api/games/{id}")).await?, json, |v| {
                    let moves: Vec<String> =
                        v["moves"].as_array().into_iter().flatten().map(|m| str_of(&m["san"])).collect();
                    format!("{}\n{}", game_line(&v["summary"]), numbered(&moves, 0, &v["start_fen"]))
                });
            }
        }
        Command::Play(a) => {
            let time = match (&a.tc, a.nodes, a.movetime) {
                (_, Some(nodes), _) => json!({ "type": "nodes", "nodes": nodes }),
                (_, _, Some(ms)) => json!({ "type": "movetime", "ms": ms }),
                (Some(tc), _, _) => clock(tc)?,
                (None, None, None) => clock("5+3")?,
            };
            let from = if a.book {
                json!({ "type": "book" })
            } else if a.fen.is_some() || !a.moves.is_empty() {
                json!({ "type": "position", "fen": a.fen.unwrap_or_else(|| "startpos".into()), "moves": a.moves })
            } else {
                Value::Null
            };
            let body = json!({
                "white": a.white, "black": a.black, "time": time, "from": from,
                "hash": a.hash, "threads": a.threads, "adjudicate": a.adjudicate,
            });
            let v = c.post("/api/games", body).await?;
            let id = v["summary"]["id"].as_i64().unwrap_or_default();
            if a.follow {
                if !json {
                    println!("{}", game_line(&v["summary"]));
                }
                follow_game(&c, id, json).await?;
            } else {
                print(&v, json, |v| game_line(&v["summary"]));
            }
        }
        Command::Move { game, mv, no_wait } => {
            let before = c.get(&format!("/api/games/{game}")).await?;
            let plies = before["moves"].as_array().map_or(0, Vec::len);
            let mut v = act(&c, game, "move", json!({ "uci": mv })).await?;
            if !no_wait {
                // The reply: one more ply than ours, or the end of the game.
                let want = plies + 2;
                let path = format!("/api/games/{game}");
                let replied = c.poll(&path, |g| {
                    g["moves"].as_array().map_or(0, Vec::len) >= want
                        || !matches!(g["summary"]["status"].as_str(), Some("live" | "starting"))
                });
                v = tokio::time::timeout(Duration::from_secs(600), replied)
                    .await
                    .context("no reply within ten minutes")??;
            }
            print(&v, json, |g| {
                let tail: Vec<String> =
                    g["moves"].as_array().into_iter().flatten().skip(plies).map(|m| str_of(&m["san"])).collect();
                let moves = numbered(&tail, plies, &g["start_fen"]);
                match g["summary"]["status"].as_str() {
                    Some("live" | "starting") => moves,
                    _ => format!("{moves}\n{}", game_line(&g["summary"])),
                }
            });
        }
        Command::Draw { game, accept, decline } => {
            let action = if accept {
                "accept_draw"
            } else if decline {
                "decline_draw"
            } else {
                "offer_draw"
            };
            print(&act(&c, game, action, Value::Null).await?, json, offers_text);
        }
        Command::Takeback { game, accept, decline } => {
            let action = if accept {
                "accept_takeback"
            } else if decline {
                "decline_takeback"
            } else {
                "takeback"
            };
            print(&act(&c, game, action, Value::Null).await?, json, offers_text);
        }
        Command::Resign { game } => {
            print(&act(&c, game, "resign", Value::Null).await?, json, |g| game_line(&g["summary"]))
        }
        Command::Abort { game } => {
            print(&act(&c, game, "abort", Value::Null).await?, json, |g| game_line(&g["summary"]))
        }
        Command::Claim { game, draw } => {
            let action = if draw { "claim_draw" } else { "claim_victory" };
            print(&act(&c, game, action, Value::Null).await?, json, |g| game_line(&g["summary"]));
        }
        Command::Stop { game } => {
            print(&c.post(&format!("/api/games/{game}/stop"), json!({})).await?, json, |g| game_line(&g["summary"]))
        }
        Command::Import(a) => {
            if let Some(path) = a.pgn {
                let pgn = read_file(&path)?;
                let v = c.post("/api/games/import", json!({ "pgn": pgn, "you": a.you })).await?;
                print(&v, json, |v| lines(v, game_line));
            } else {
                let Some(player) = a.player else { bail!("import you, import lichess:<name>, or import --pgn <file>") };
                let body = json!({ "max": a.max, "since": a.since, "speeds": a.speeds, "rated": a.rated });
                let w = c.post(&format!("/api/players/{}/import", seg(&player)), body).await?;
                finish_work(&c, w, a.wait, json).await?;
            }
        }
        Command::Review(a) => {
            let body = json!({
                "games": a.games, "player": a.player, "limit": a.limit, "speeds": a.speeds, "rated": a.rated,
                "version": a.version, "nodes": a.nodes, "concurrency": a.concurrency,
            });
            let w = c.post("/api/reviews", body).await?;
            finish_work(&c, w, a.wait, json).await?;
        }
        Command::Player { key } => print(&c.get(&format!("/api/players/{}", seg(&key))).await?, json, pretty),
        Command::Profile { key, filter } => {
            let v = c.get(&format!("/api/players/{}/profile{}", seg(&key), query(&filter.query()))).await?;
            print(&v, json, |p| {
                format!(
                    "{}: {} games ({} reviewed) · accuracy {} · ACPL {}",
                    str_of(&p["player"]["name"]),
                    p["games"],
                    p["reviewed"],
                    fixed(&p["accuracy"]["overall"], 1),
                    fixed(&p["acpl"], 0)
                )
            });
        }
        Command::Compare { players, filter } => {
            let mut q = vec![("players", players.join(","))];
            q.extend(filter.query());
            let v = c.get(&format!("/api/compare{}", query(&q))).await?;
            print(&v, json, |v| {
                lines(&v["profiles"], |p| {
                    format!(
                        "{:<20} {:>4} games  accuracy {}",
                        str_of(&p["player"]["name"]),
                        p["games"].to_string(),
                        fixed(&p["accuracy"]["overall"], 1)
                    )
                })
            });
        }
        Command::Players(PlayersCommand::Top { speed, nb }) => {
            let v = c.get(&format!("/api/players/top{}", query(&[("speed", speed), ("nb", nb.to_string())]))).await?;
            print(&v, json, |v| {
                lines(v, |p| {
                    format!(
                        "{:>5}  {} {}",
                        str_of(&p["rating"]),
                        p["title"].as_str().unwrap_or(""),
                        str_of(&p["username"])
                    )
                })
            });
        }
        Command::Repertoire { side, max_plies, min_games, limit } => {
            let mut q = vec![("side", side)];
            push(&mut q, "max_plies", max_plies.map(|n| n.to_string()));
            push(&mut q, "min_games", min_games.map(|n| n.to_string()));
            push(&mut q, "limit", limit.map(|n| n.to_string()));
            let v = c.get(&format!("/api/players/you/repertoire{}", query(&q))).await?;
            print(&v, json, |r| {
                format!(
                    "{} games, {} positions, {} departures from theory",
                    r["games"],
                    r["nodes"].as_array().map_or(0, Vec::len),
                    r["departures"].as_array().map_or(0, Vec::len)
                )
            });
        }
        Command::Progress { window } => {
            let mut q = Vec::new();
            push(&mut q, "window", window.map(|n| n.to_string()));
            let v = c.get(&format!("/api/progress{}", query(&q))).await?;
            print(&v, json, |p| {
                lines(&p["series"], |s| {
                    format!(
                        "{} {}: {} points",
                        str_of(&s["player"]),
                        str_of(&s["metric"]),
                        s["points"].as_array().map_or(0, Vec::len)
                    )
                })
            });
        }
        Command::Levels(a) => levels(&c, a, json).await?,
        Command::Drills(a) => drills(&c, a, json).await?,
        Command::Eval(a) => eval(&c, a, json).await?,
        Command::Explorer(a) => {
            let mut q = vec![("db", a.db)];
            push(&mut q, "fen", a.fen);
            push(&mut q, "play", a.play);
            push(&mut q, "speeds", a.speeds);
            push(&mut q, "ratings", a.ratings);
            push(&mut q, "player", a.player);
            push(&mut q, "color", a.color);
            print(&c.get(&format!("/api/explorer{}", query(&q))).await?, json, pretty);
        }
        Command::Tablebase { fen } => {
            // Underscores stand for spaces, so a FEN is one shell word.
            let fen = fen.replace('_', " ");
            print(&c.get(&format!("/api/tablebase{}", query(&[("fen", fen)]))).await?, json, pretty);
        }
        Command::Nets { id } => match id {
            Some(id) => print(&c.get(&format!("/api/nets/{id}")).await?, json, pretty),
            None => print(&c.get("/api/nets").await?, json, |v| {
                lines(v, |n| {
                    let mb = n["size_bytes"].as_u64().unwrap_or(0) / 1_000_000;
                    format!("#{:<4} {:<24} {}  {mb} MB", n["id"].to_string(), str_of(&n["name"]), short(&n["sha256"]))
                })
            }),
        },
        Command::Data(a) => data(&c, a, json).await?,
        Command::Train(a) => match a.action {
            Some(TrainAction::Show { idea }) => {
                print(&c.get(&format!("/api/ideas/{idea}/train")).await?, json, |v| lines(&v["steps"], step_line))
            }
            Some(TrainAction::Steps { work }) => {
                print(&c.get(&format!("/api/work/{work}/train")).await?, json, |v| lines(v, step_line))
            }
            None => {
                let w = c.post("/api/train", a.schedule.spec()?).await?;
                finish_work(&c, w, a.wait, json).await?;
            }
        },
        Command::Dameless => print(&c.get("/api/dameless").await?, json, |v| {
            let ideas = |k: &str| v["ideas"][k].as_array().map_or(0, Vec::len);
            let bot = if v["connection"].is_null() {
                "not connected".to_owned()
            } else {
                format!("online {}", v["connection"]["online"])
            };
            format!(
                "champion {} ({})\n{} ideas running, {} queued\nbot: {bot}",
                str_of(&v["champion"]["name"]),
                short(&v["champion"]["commit"]),
                ideas("running"),
                ideas("queued")
            )
        }),
        Command::Ideas(a) => list_ideas(&c, a, json).await?,
        Command::Idea(cmd) => idea(&c, cmd, json).await?,
        Command::Test(a) => {
            if a.bounds.as_ref().is_some_and(|b| b.len() != 2) {
                bail!("--bounds takes two numbers: 0,3");
            }
            let custom = json!({ "preset": a.preset, "bounds": a.bounds, "pairs": a.pairs });
            let title = a.title.unwrap_or_else(|| format!("{} vs {}", a.dev, a.base));
            let body = json!({ "title": title, "kind": "gain", "branch": a.dev, "base": a.base, "custom": custom });
            let v = c.post("/api/ideas", body).await?;
            finish_idea(&c, v, a.wait, json).await?;
        }
        Command::Tune(a) => {
            let time = match a.nodes {
                Some(nodes) => json!({ "type": "nodes", "nodes": nodes }),
                None => serde_json::to_value(
                    TimeControl::parse_seconds(&a.tc).map_err(|e| anyhow::anyhow!("--tc {}: {e}", a.tc))?,
                )?,
            };
            let mut tune = serde_json::to_value(dameless_core::spsa::TuneSpec::default())?;
            tune["params"] = Value::Array(a.params.iter().map(|p| json!({ "name": p })).collect());
            tune["iterations"] = json!(a.iterations);
            tune["pairs"] = json!(a.pairs);
            tune["time"] = time;
            tune["hash"] = json!(a.hash);
            tune["concurrency"] = json!(a.concurrency);
            let title = a.title.unwrap_or_else(|| format!("Tune {}", a.branch));
            let body = json!({ "title": title, "kind": "tune", "branch": a.branch, "base": a.base, "long": a.long, "tune": tune });
            let v = c.post("/api/ideas", body).await?;
            finish_idea(&c, v, a.wait, json).await?;
        }
        Command::Tunables { branch } => {
            let v = c.get(&format!("/api/tunables{}", query(&[("branch", branch)]))).await?;
            print(&v, json, |v| {
                lines(v, |t| format!("{:<28} {} [{}, {}]", str_of(&t["name"]), t["start"], t["min"], t["max"]))
            });
        }
        Command::Harness(a) => match a.action {
            None => print(&c.get("/api/harness").await?, json, |h| str_of(&h["summary"])),
            Some(HarnessAction::Check { determinism_pairs, symmetry_pairs, wait }) => {
                let body = json!({ "determinism_pairs": determinism_pairs, "symmetry_pairs": symmetry_pairs });
                let v = c.post("/api/harness/check", body).await?;
                finish_idea(&c, v, wait, json).await?;
            }
        },
        Command::Upstream(a) => match a.action {
            None => print(&c.get("/api/upstream").await?, json, upstream_text),
            Some(UpstreamAction::Sync { wait }) => {
                let w = c.post("/api/upstream/sync", json!({})).await?;
                finish_work(&c, w, wait, json).await?;
                if wait && !json {
                    println!("{}", upstream_text(&c.get("/api/upstream").await?));
                }
            }
            Some(UpstreamAction::Auto { state }) => {
                let v = c.put("/api/upstream", json!({ "auto": state == "on" })).await?;
                print(&v, json, upstream_text);
            }
        },
        Command::Versions { line } => {
            let mut q = Vec::new();
            push(&mut q, "line", line);
            let v = c.get(&format!("/api/versions{}", query(&q))).await?;
            print(&v, json, |v| lines(v, version_line));
        }
        Command::Version(VersionCommand::Make { reference, wait }) => {
            let v = c.post("/api/versions", json!({ "ref": reference })).await?;
            let id = v["id"].as_i64().unwrap_or_default();
            let v = if wait {
                let builds = c
                    .poll(&format!("/api/versions/{id}/builds"), |b| {
                        b.as_array()
                            .is_some_and(|a| a.iter().any(|b| matches!(b["status"].as_str(), Some("ready" | "failed"))))
                    })
                    .await?;
                json!({ "version": v, "builds": builds })
            } else {
                v
            };
            print(&v, json, |v| {
                if v["version"].is_object() {
                    format!("{}\n{}", version_line(&v["version"]), lines(&v["builds"], build_line))
                } else {
                    version_line(v)
                }
            });
        }
        Command::Builds { version } => {
            print(&c.get(&format!("/api/versions/{version}/builds")).await?, json, |v| lines(v, build_line))
        }
        Command::Build(BuildCommand::Delete { id }) => {
            print(&c.delete(&format!("/api/builds/{id}")).await?, json, build_line)
        }
        Command::Build(BuildCommand::Prune { older_than_days, all, dry_run }) => {
            let body = json!({ "older_than_days": older_than_days, "keep_latest": !all, "dry_run": dry_run });
            print(&c.post("/api/builds/prune", body).await?, json, |v| {
                format!(
                    "{} {} builds, {} MB",
                    if v["dry_run"].as_bool() == Some(true) { "would delete" } else { "deleted" },
                    v["builds"].as_array().map_or(0, Vec::len),
                    v["bytes"].as_u64().unwrap_or(0) / 1_000_000
                )
            });
        }
        Command::Work(a) => work(&c, a, json).await?,
        Command::Lichess(a) => lichess(&c, a, json).await?,
    }
    Ok(())
}

async fn act(c: &Client, game: i64, action: &str, body: Value) -> Result<Value> {
    c.post(&format!("/api/games/{game}/actions/{action}"), body).await
}

/// The game and its standing offers: `… · White offers a draw`.
fn offers_text(g: &Value) -> String {
    let mut offers = Vec::new();
    for (kind, what) in [("draw", "offers a draw"), ("takeback", "asks for a takeback")] {
        if let Some(side) = g["offers"][kind].as_str() {
            let side = if side == "white" { "White" } else { "Black" };
            offers.push(format!("{side} {what}"));
        }
    }
    let offers = if offers.is_empty() { "no offers".to_owned() } else { offers.join(", ") };
    format!("{} · {offers}", game_line(&g["summary"]))
}

/// Prints a game's moves as they come (to stderr with `--json`), then the game as it ended.
async fn follow_game(c: &Client, id: i64, json: bool) -> Result<()> {
    let game = c.get(&format!("/api/games/{id}")).await?;
    if matches!(game["summary"]["status"].as_str(), Some("live" | "starting")) {
        // The replay from the start of the stream's memory covers every move played so far.
        let start_fen = game["start_fen"].clone();
        c.follow(&format!("game:{id},games"), 0, |e| match e["kind"].as_str() {
            Some("game.moved") if e["data"]["id"] == json!(id) => {
                let index = e["data"]["ply"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                let line = numbered(&[str_of(&e["data"]["move"]["san"])], index, &start_fen);
                if json {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
                true
            }
            Some("game.upserted") if e["data"]["id"] == json!(id) => {
                !matches!(e["data"]["status"].as_str(), Some("finished" | "aborted"))
            }
            _ => true,
        })
        .await?;
    }
    let done = c.get(&format!("/api/games/{id}")).await?;
    print(&done, json, |g| game_line(&g["summary"]));
    Ok(())
}

/// `Automatic sync on, next at 2026-10-27T18:00:00.000Z. Last: v3 from upstream · 4 commits: …`.
fn upstream_text(v: &Value) -> String {
    let auto = if v["settings"]["auto"].as_bool().unwrap_or(false) {
        match v["next_at"].as_str() {
            Some(at) => format!("Automatic sync on, next at {at}."),
            None => "Automatic sync on.".to_owned(),
        }
    } else {
        "Automatic sync off.".to_owned()
    };
    let remote =
        if v["remote"].as_bool().unwrap_or(false) { "" } else { " The engine checkout has no upstream remote." };
    let last = match v["last"]["detail"].as_str() {
        Some(d) => format!(" Last ({}): {d}", str_of(&v["last"]["at"])),
        None => " Never synced.".to_owned(),
    };
    format!("{auto}{remote}{last}")
}

/// Prints queued work, or waits for it to end with `--wait` (non-zero unless it is done).
async fn finish_work(c: &Client, w: Value, wait: bool, json: bool) -> Result<()> {
    if !wait {
        print(&w, json, work_line);
        return Ok(());
    }
    let id = w["id"].as_i64().unwrap_or_default();
    let done = c.poll(&format!("/api/work/{id}"), is_final_work).await?;
    print(&done, json, work_line);
    if done["status"] != "done" {
        bail!("work {id} {}: {}", str_of(&done["status"]), str_of(&done["error"]));
    }
    Ok(())
}

/// Prints an idea, or waits for it to end with `--wait` (non-zero when it failed or was stopped).
async fn finish_idea(c: &Client, v: Value, wait: bool, json: bool) -> Result<()> {
    if !wait {
        print(&v, json, idea_line);
        return Ok(());
    }
    let id = v["id"].as_i64().unwrap_or_default();
    let done = c.poll(&format!("/api/ideas/{id}"), is_final_idea).await?;
    print(&done, json, idea_line);
    if matches!(done["status"].as_str(), Some("failed" | "stopped")) {
        bail!("idea {id} {}", str_of(&done["status"]));
    }
    Ok(())
}

async fn list_ideas(c: &Client, a: IdeasArgs, json: bool) -> Result<()> {
    let mut q = Vec::new();
    push(&mut q, "status", a.status);
    push(&mut q, "tag", a.tag);
    push(&mut q, "limit", a.limit.map(|l| l.to_string()));
    print(&c.get(&format!("/api/ideas{}", query(&q))).await?, json, |v| lines(v, idea_line));
    Ok(())
}

async fn idea(c: &Client, cmd: IdeaCommand, json: bool) -> Result<()> {
    match cmd {
        IdeaCommand::New(a) => {
            let mut body = json!({
                "title": a.title, "hypothesis": a.hypothesis, "kind": a.kind, "branch": a.branch, "base": a.base,
                "author": a.author.unwrap_or_else(|| "you".into()), "tags": a.tags, "notes": a.notes, "long": a.long,
                "fix_bench": a.fix_bench,
            });
            if a.kind == "net" {
                body["train"] = a.train.spec()?;
            }
            let v = c.post("/api/ideas", body).await?;
            finish_idea(c, v, a.wait, json).await?;
        }
        IdeaCommand::List(a) => list_ideas(c, a, json).await?,
        IdeaCommand::Show { id, wait } => {
            let path = format!("/api/ideas/{id}");
            let v = if wait { c.poll(&path, is_final_idea).await? } else { c.get(&path).await? };
            print(&v, json, |i| {
                let trials = lines(&i["trials"], |t| {
                    format!(
                        "  {:<12} {:<8} {}",
                        str_of(&t["stage"]),
                        str_of(&t["status"]),
                        t["detail"].as_str().unwrap_or("")
                    )
                });
                format!("{}\n{}\n{trials}", idea_line(i), i["hypothesis"].as_str().unwrap_or(""))
            });
        }
        IdeaCommand::Trial { idea, trial } => {
            print(&c.get(&format!("/api/ideas/{idea}/trials/{trial}")).await?, json, pretty)
        }
        IdeaCommand::Tune { id } => print(&c.get(&format!("/api/ideas/{id}/tune")).await?, json, |t| {
            format!(
                "{} parameters, {} steps",
                t["params"].as_array().map_or(0, Vec::len),
                t["steps"].as_array().map_or(0, Vec::len)
            )
        }),
        IdeaCommand::Stop { id } => print(&c.post(&format!("/api/ideas/{id}/stop"), json!({})).await?, json, idea_line),
        IdeaCommand::Resume { id } => {
            print(&c.post(&format!("/api/ideas/{id}/resume"), json!({})).await?, json, idea_line)
        }
        IdeaCommand::Promote { id } => {
            print(&c.post(&format!("/api/ideas/{id}/promote"), json!({})).await?, json, |v| {
                format!("{} is the champion ({})", str_of(&v["name"]), short(&v["commit"]))
            })
        }
        IdeaCommand::Batch { file, tag } => {
            let ideas: Value = serde_json::from_str(&read_file(&file)?).context("the batch is not a JSON array")?;
            let v = c.post("/api/ideas/batch", json!({ "ideas": ideas, "tag": tag })).await?;
            print(&v, json, |v| lines(v, idea_line));
        }
        IdeaCommand::CheckBranch { branch } => {
            let v = c.get(&format!("/api/ideas/check-branch{}", query(&[("branch", branch)]))).await?;
            print(&v, json, |b| format!("{}: {}", str_of(&b["status"]), str_of(&b["detail"])));
        }
        IdeaCommand::FixBench { branch, commit } => {
            let v = c.post("/api/ideas/fix-bench", json!({ "branch": branch, "commit": commit })).await?;
            print(&v, json, |b| format!("{}: {}", str_of(&b["status"]), str_of(&b["detail"])));
        }
    }
    Ok(())
}

fn step_line(s: &Value) -> String {
    format!(
        "sb {:>4} batch {:>5}  loss {}  lr {}",
        s["superbatch"].to_string(),
        s["batch"].to_string(),
        str_of(&s["loss"]),
        str_of(&s["lr"])
    )
}

/// `20M` → 20,000,000.
fn count(text: &str) -> Result<u64> {
    let t = text.trim().to_ascii_uppercase().replace('_', "");
    let (num, mult) = match t.chars().last() {
        Some('K') => (&t[..t.len() - 1], 1e3),
        Some('M') => (&t[..t.len() - 1], 1e6),
        Some('G') => (&t[..t.len() - 1], 1e9),
        _ => (&t[..], 1.0),
    };
    let n: f64 = num.parse().with_context(|| format!("{text} is not a count (20M, 500K, 1000)"))?;
    Ok((n * mult).round() as u64)
}

async fn data(c: &Client, a: DataArgs, json: bool) -> Result<()> {
    let gb = |b: &Value| format!("{:.1} GB", b.as_f64().unwrap_or(0.0) / 1e9);
    match a.action {
        None => print(&c.get("/api/datasets").await?, json, |v| {
            let sets = lines(&v["datasets"], |d| {
                format!(
                    "#{:<4} {:<20} {} positions  {}",
                    d["id"].to_string(),
                    str_of(&d["name"]),
                    d["positions"],
                    gb(&d["size_bytes"])
                )
            });
            format!(
                "{} of {} used, {} free on disk\n{sets}",
                gb(&v["used_bytes"]),
                gb(&v["cap_bytes"]),
                gb(&v["disk_free_bytes"])
            )
        }),
        Some(DataAction::Datagen { positions, name, threads, soft_nodes, opening, version, wait }) => {
            let body = json!({
                "positions": count(&positions)?, "name": name, "threads": threads, "soft_nodes": soft_nodes,
                "opening": opening, "version": version,
            });
            let w = c.post("/api/datasets/datagen", body).await?;
            finish_work(c, w, wait, json).await?;
        }
        Some(DataAction::Import { url, bytes, sha256, name, yes, wait }) => {
            if !yes {
                eprint!("Download {:.2} GB from {url}? [y/N] ", bytes as f64 / 1e9);
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    bail!("not downloaded");
                }
            }
            let body = json!({ "url": url, "expected_bytes": bytes, "sha256": sha256, "name": name, "confirm": true });
            let w = c.post("/api/datasets/import", body).await?;
            finish_work(c, w, wait, json).await?;
        }
        Some(DataAction::Delete { dataset, yes }) => {
            if !yes {
                bail!("deleting a dataset cannot be undone: add --yes");
            }
            let id = match dataset.parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    let all = c.get("/api/datasets").await?;
                    all["datasets"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|d| d["name"] == json!(dataset))
                        .and_then(|d| d["id"].as_i64())
                        .with_context(|| format!("no dataset {dataset}"))?
                }
            };
            let path = format!("/api/datasets/{id}");
            let v = c.send_json(reqwest::Method::DELETE, &path, json!({ "confirm": true })).await?;
            print(&v, json, |d| format!("deleted {}", str_of(&d["name"])));
        }
        Some(DataAction::FromTrials { idea, name }) => {
            print(&c.post(&format!("/api/ideas/{idea}/dataset"), json!({ "name": name })).await?, json, pretty)
        }
    }
    Ok(())
}

async fn levels(c: &Client, a: LevelsArgs, json: bool) -> Result<()> {
    match a.action {
        None => print(&c.get("/api/levels").await?, json, |v| {
            lines(&v["levels"], |l| {
                let rating = if l["rating"].is_null() {
                    format!("about {}", l["level"]["nominal"])
                } else {
                    format!("{:.0}", l["rating"]["elo"].as_f64().unwrap_or_default())
                };
                format!("{:>2}  {:<12} {} games", l["level"]["id"].to_string(), rating, l["games"])
            })
        }),
        Some(LevelsAction::Plan { count }) => {
            print(&c.get(&format!("/api/levels/plan?count={count}")).await?, json, |v| {
                lines(v, |m| format!("level {} vs level {}  {} games", m["a"], m["b"], m["games"]))
            })
        }
        Some(LevelsAction::Calibrate { count, games_per_pair, version, wait }) => {
            let body = json!({ "count": count, "games_per_pair": games_per_pair, "version": version });
            let w = c.post("/api/levels/calibrate", body).await?;
            finish_work(c, w, wait, json).await?;
        }
    }
    Ok(())
}

async fn drills(c: &Client, a: DrillsArgs, json: bool) -> Result<()> {
    match a.action {
        None => print(&c.get("/api/drills").await?, json, |v| lines(v, drill_line)),
        Some(DrillsAction::Today { new_per_day, limit }) => {
            let mut q = Vec::new();
            push(&mut q, "new_per_day", new_per_day.map(|n| n.to_string()));
            push(&mut q, "limit", limit.map(|n| n.to_string()));
            print(&c.get(&format!("/api/drills/today{}", query(&q))).await?, json, |t| {
                format!("{} due, {} new\n{}", t["due"], t["new"], lines(&t["drills"], drill_line))
            });
        }
        Some(DrillsAction::Make { limit }) => {
            print(&c.post("/api/drills", json!({ "limit": limit })).await?, json, |v| lines(v, drill_line))
        }
        Some(DrillsAction::Answer { id, mv }) => {
            let v = c.post(&format!("/api/drills/{id}/answer"), json!({ "move": mv })).await?;
            print(&v, json, |r| {
                format!(
                    "{}: best {} · next {}",
                    str_of(&r["outcome"]),
                    lines(&r["best_san"], str_of).replace('\n', ", "),
                    when(&r["drill"]["due"])
                )
            });
        }
        Some(DrillsAction::Delete { id }) => {
            c.delete(&format!("/api/drills/{id}")).await?;
            print(&json!({ "deleted": id }), json, |_| format!("deleted drill {id}"));
        }
        Some(DrillsAction::Trend { window }) => {
            let mut q = Vec::new();
            push(&mut q, "window", window.map(|n| n.to_string()));
            print(&c.get(&format!("/api/drills/trend{}", query(&q))).await?, json, |v| {
                lines(v, |p| {
                    format!(
                        "{}  {}/{}  rolling {}",
                        str_of(&p["date"]),
                        p["passes"],
                        p["answers"],
                        str_of(&p["rolling"])
                    )
                })
            });
        }
    }
    Ok(())
}

async fn work(c: &Client, a: WorkArgs, json: bool) -> Result<()> {
    match a.action {
        None => print(&c.get(&format!("/api/work?recent={}", a.recent)).await?, json, |v| {
            ["running", "queued", "recent"]
                .iter()
                .filter_map(|k| {
                    let items = v[*k].as_array()?;
                    (!items.is_empty()).then(|| format!("{k}:\n{}", lines(&v[*k], work_line)))
                })
                .collect::<Vec<_>>()
                .join("\n")
        }),
        Some(WorkAction::Show { id }) => print(&c.get(&format!("/api/work/{id}")).await?, json, work_line),
        Some(WorkAction::Stop { id }) => {
            print(&c.post(&format!("/api/work/{id}/stop"), json!({})).await?, json, work_line)
        }
        Some(WorkAction::Pause { id }) => {
            print(&c.post(&format!("/api/work/{id}/pause"), json!({})).await?, json, work_line)
        }
        Some(WorkAction::Resume { id }) => {
            print(&c.post(&format!("/api/work/{id}/resume"), json!({})).await?, json, work_line)
        }
        Some(WorkAction::Log { id }) => {
            let text = c.text(&format!("/api/work/{id}/log")).await?;
            if json {
                println!("{}", json!({ "id": id, "log": text }));
            } else {
                print!("{text}");
            }
        }
        Some(WorkAction::Wait { id }) => {
            let w = c.get(&format!("/api/work/{id}")).await?;
            finish_work(c, w, true, json).await?;
        }
    }
    Ok(())
}

async fn eval(c: &Client, a: EvalArgs, json: bool) -> Result<()> {
    if a.list {
        print(&c.get("/api/eval").await?, json, pretty);
        return Ok(());
    }
    if let Some(session) = a.stop {
        print(&c.delete(&format!("/api/eval/{}", seg(&session))).await?, json, pretty);
        return Ok(());
    }
    let budget = match (a.depth, a.nodes) {
        (_, Some(nodes)) => json!({ "type": "nodes", "nodes": nodes }),
        (Some(depth), _) => json!({ "type": "depth", "depth": depth }),
        (None, None) => json!({ "type": "depth", "depth": 20 }),
    };
    let body = json!({
        "fen": a.fen, "moves": a.moves, "version": a.version, "multipv": a.multipv, "budget": budget,
        "threads": a.threads, "client": format!("cli-{}", std::process::id()),
    });
    // Lines are never replayed, so the stream is opened before the search starts.
    let stream = c.open_stream("eval:*", 0).await?;
    let started = c.post("/api/eval", body).await?;
    let session = str_of(&started["session"]);
    let mut last = Value::Null;
    let follow = client::read_events(stream, 0, |e| {
        if e["kind"] == "eval.info" && e["data"]["session"] == json!(session) {
            if !json {
                for (k, l) in e["data"]["lines"].as_array().into_iter().flatten().enumerate() {
                    let pv: Vec<String> = l["pv"].as_array().into_iter().flatten().map(str_of).collect();
                    println!("depth {:>2} #{} {} {}", e["data"]["depth"], k + 1, l["score"], pv.join(" "));
                }
            }
            last = e["data"].clone();
        }
        true
    });
    // The session ends at its budget: then it is no longer listed.
    let ended = async {
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let running = c.get("/api/eval").await.unwrap_or(Value::Null);
            if !running.as_array().is_some_and(|a| a.iter().any(|s| s["session"] == json!(session))) {
                break;
            }
        }
    };
    tokio::select! {
        r = follow => { r?; }
        _ = ended => {}
        _ = tokio::signal::ctrl_c() => { let _ = c.delete(&format!("/api/eval/{session}")).await; }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&json!({ "session": session, "last": last }))?);
    }
    Ok(())
}

async fn lichess(c: &Client, a: LichessArgs, json: bool) -> Result<()> {
    let status_text = |s: &Value| {
        let account = s["account"]["username"].as_str().unwrap_or("not linked");
        let mut text = format!("{}: {account} ({})", str_of(&s["key"]), str_of(&s["state"]));
        // The last word on seeking, for people: "Alpha declined (too fast). Trying another bot."
        for note in [&s["error"], &s["bot"]["seek"]["note"], &s["you"]["seek"]["note"]] {
            if let Some(note) = note.as_str().filter(|n| !n.is_empty()) {
                text.push_str(&format!("\n  {note}"));
            }
        }
        text
    };
    match a.account {
        None => {
            let v = c.get("/api/connections").await?;
            print(&v, json, |v| format!("{}\n{}", status_text(&v["you"]), status_text(&v["bot"])));
        }
        Some(LichessAccount::You(y)) => match y.action {
            None => print(&c.get("/api/connections").await?["you"], json, status_text),
            Some(YouAction::Login { return_to }) => {
                let v = c.post("/api/connections/you/login", json!({ "return_to": return_to })).await?;
                print(&v, json, |v| {
                    format!("Open this link to log in (it works once, for ten minutes):\n{}", str_of(&v["url"]))
                });
            }
            Some(YouAction::Token) => {
                let token = read_stdin()?;
                print(&c.put("/api/connections/you/token", json!({ "token": token.trim() })).await?, json, status_text);
            }
            Some(YouAction::Logout) => print(&c.delete("/api/connections/you").await?, json, status_text),
            Some(YouAction::Seek { tc, rated, color, range, stop }) => {
                let v = if stop {
                    c.delete("/api/connections/you/seek").await?
                } else {
                    let tc = tc.context("seek --tc 10+5 (or --stop)")?;
                    let body = json!({ "time": clock(&tc)?, "rated": rated, "color": color, "rating_range": range });
                    c.post("/api/connections/you/seek", body).await?
                };
                print(&v, json, status_text);
            }
            Some(YouAction::Challenge(ch)) => {
                let body = json!({
                    "username": ch.username, "ai_level": ch.ai, "my_bot": ch.bot, "time": clock(&ch.tc)?,
                    "rated": ch.rated, "color": ch.color,
                });
                print(&c.post("/api/connections/you/challenge", body).await?, json, |v| str_of(&v["url"]));
            }
            Some(YouAction::Accept { id }) => {
                let v = c.post(&format!("/api/connections/you/challenges/{}/accept", seg(&id)), json!({})).await?;
                print(&v, json, status_text);
            }
            Some(YouAction::Decline { id, reason }) => {
                let path = format!("/api/connections/you/challenges/{}/decline", seg(&id));
                print(&c.post(&path, json!({ "reason": reason })).await?, json, status_text);
            }
            Some(YouAction::Cancel { id }) => {
                print(&c.delete(&format!("/api/connections/you/challenges/{}", seg(&id))).await?, json, status_text)
            }
        },
        Some(LichessAccount::Bot(b)) => match b.action {
            None => print(&c.get("/api/connections").await?["bot"], json, status_text),
            Some(BotAction::Token) => {
                let token = read_stdin()?;
                print(&c.put("/api/connections/bot/token", json!({ "token": token.trim() })).await?, json, status_text);
            }
            Some(BotAction::Logout) => print(&c.delete("/api/connections/bot").await?, json, status_text),
            Some(BotAction::Settings { set, people }) => {
                let status = c.get("/api/connections").await?;
                let mut settings = status["bot"]["bot"]["settings"].clone();
                if !settings.is_object() {
                    settings = json!({});
                }
                for pair in set {
                    let Some((k, v)) = pair.split_once('=') else { bail!("--set {pair}: use key=value") };
                    let value = if k == "speeds" || k == "seek_clocks" {
                        Value::Array(v.split(',').map(|s| Value::String(s.trim().to_owned())).collect())
                    } else {
                        serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_owned()))
                    };
                    settings[k.trim()] = value;
                }
                if let Some(people) = people {
                    settings["accept_people"] = Value::Bool(people);
                }
                print(&c.put("/api/connections/bot/settings", settings).await?, json, |v| {
                    let pairs = v.as_object().into_iter().flatten();
                    pairs.map(|(k, v)| format!("{k:<16} {}", str_of(v))).collect::<Vec<_>>().join("\n")
                });
            }
            Some(BotAction::Online) => {
                print(&c.post("/api/connections/bot/online", json!({ "online": true })).await?, json, status_text)
            }
            Some(BotAction::Offline) => {
                print(&c.post("/api/connections/bot/online", json!({ "online": false })).await?, json, status_text)
            }
            Some(BotAction::Seek { mode }) => {
                print(&c.post("/api/connections/bot/seek", json!({ "mode": mode })).await?, json, status_text)
            }
            Some(BotAction::Challenge { username, tc, rated, color }) => {
                let body = json!({ "username": username, "time": clock(&tc)?, "rated": rated, "color": color });
                print(&c.post("/api/connections/bot/challenge", body).await?, json, |v| str_of(&v["url"]));
            }
            Some(BotAction::Bots) => print(&c.get("/api/connections/bot/bots").await?, json, |v| {
                lines(v, |b| {
                    let r = |speed: &str| b["ratings"][speed]["rating"].as_u64().map_or("-".into(), |n| n.to_string());
                    format!(
                        "{:<24} bullet {:>5}  blitz {:>5}  rapid {:>5}",
                        str_of(&b["name"]),
                        r("bullet"),
                        r("blitz"),
                        r("rapid")
                    )
                })
            }),
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sans(moves: &[&str]) -> Vec<String> {
        moves.iter().map(|m| (*m).to_owned()).collect()
    }

    #[test]
    fn moves_are_numbered_as_people_write_them() {
        let start = Value::Null;
        assert_eq!(numbered(&sans(&["e4", "e5", "Nf3"]), 0, &start), "1. e4 e5 2. Nf3");
        // Ply 42 is Black's 21st move.
        assert_eq!(numbered(&sans(&["Rfe8#"]), 41, &start), "21... Rfe8#");
        let black_first = json!("8/8/8/8/8/8/8/K1k5 b - - 0 30");
        assert_eq!(numbered(&sans(&["Kc2", "Ka2"]), 0, &black_first), "30... Kc2 31. Ka2");
    }

    #[test]
    fn time_controls_read_as_the_app_writes_them() {
        assert_eq!(tc_text(&json!({ "type": "clock", "initial_ms": 180_000, "increment_ms": 2000 })), "3+2");
        assert_eq!(tc_text(&json!({ "type": "clock", "initial_ms": 8000, "increment_ms": 80 })), "8+0.08");
        assert_eq!(tc_text(&json!({ "type": "nodes", "nodes": 25_000 })), "25k nodes");
    }
}
