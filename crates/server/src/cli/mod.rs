//! The `chess` command line. `chess serve` runs the app; every other command is one API call (or a few, or a
//! poll, or the event stream) against a running app, printed as one JSON document with `--json` and as a short
//! summary otherwise. Errors go to stderr with a non-zero exit. [`COMMANDS`] says which command calls which route,
//! and a test checks that every route of the OpenAPI document has one.

pub mod client;

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use chess_core::TimeControl;
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};

pub use client::Client;
use client::print;

/// The command line.
#[derive(Parser)]
#[command(name = "chess", version, about = "Play, review and analyze chess, with any UCI engine")]
pub struct Cli {
    /// The data directory (database, token, engines). The platform's local data folder by default.
    #[arg(long, global = true, env = "CHESS_DATA")]
    pub data: Option<PathBuf>,

    /// The app's URL, for every command but `serve`.
    #[arg(long, global = true, env = "CHESS_URL", default_value = "http://127.0.0.1:7979")]
    pub url: String,

    /// The API token. A local app's is read from its data directory.
    #[arg(long, global = true, env = "CHESS_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    /// Print one JSON document instead of a summary.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run the app: the API, the event stream and the web UI (`CHESS_LISTEN`, default 127.0.0.1:7979).
    Serve,
    /// Print the OpenAPI document.
    Openapi,
    /// Whether the app is up.
    Health,
    /// The app, the machine, the cores and fair play.
    Status,
    /// The machine's health: the analysis engine, Maia, disk, database.
    Diagnostics,
    /// Print the API token (for the hosted UI).
    Token,
    /// Download the built-in engines (Stockfish 19, lc0 and Maia's nets): shows the sizes and asks first.
    Setup {
        /// Only these: stockfish, maia (comma-separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Download without asking.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
    },
    /// The engines list: `engines`, `engines add|remove|analysis`.
    Engines(EnginesArgs),
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
        /// With `--review`: the engine that reviewed it (its id).
        #[arg(long)]
        engine: Option<String>,
        /// With `--review`: its node budget.
        #[arg(long)]
        nodes: Option<u64>,
        #[arg(long)]
        follow: bool,
    },
    /// Start a local game: `--white you --black stockfish@5 --tc 5+3`.
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
    /// Your opening lines with one colour: how you score in each and where you go wrong early.
    Prep {
        #[arg(long, default_value = "white")]
        side: String,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Your progress: rating, accuracy and drills.
    Progress {
        #[arg(long)]
        window: Option<usize>,
    },
    /// The levels of the analysis engine; `levels plan`, `levels calibrate`.
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

    /// Background work: running, queued, recent; or `work show|stop|pause|resume|log|wait <id>`.
    Work(WorkArgs),
    /// Your Lichess connection: `lichess you …`, or its status.
    Lichess(LichessArgs),
}

#[derive(Args)]
pub struct GamesArgs {
    #[arg(long)]
    pub player: Option<String>,
    /// local, lichess, import (comma-separated).
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub speed: Option<String>,
    #[arg(long)]
    pub rated: Option<bool>,
    /// Calibration games: true (only them) or all.
    #[arg(long)]
    pub calibration: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

#[derive(Args)]
pub struct PlayArgs {
    /// you, an engine's id (stockfish, maia-1500) or <engine id>@<level> (stockfish@5).
    #[arg(long, default_value = "you")]
    pub white: String,
    #[arg(long, default_value = "stockfish")]
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
    /// The engine to review with (its id); the analysis engine when none.
    #[arg(long)]
    pub engine: Option<String>,
    #[arg(long)]
    pub nodes: Option<u64>,
    #[arg(long)]
    pub concurrency: Option<u32>,
    /// Review in the background, at the lowest priority.
    #[arg(long)]
    pub background: bool,
    /// With `--player`: every game of theirs not reviewed yet, newest first, in the background.
    #[arg(long, requires = "player", conflicts_with = "limit")]
    pub all: bool,
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
    /// Today's drills (the answers stay hidden); `--theme` or `--games` makes a session on a theme or opening line.
    Today {
        #[arg(long)]
        new_per_day: Option<u32>,
        #[arg(long)]
        limit: Option<usize>,
        /// time_trouble, king_safety, hanging_piece, missed_tactic, opening_error, endgame_technique or other.
        #[arg(long)]
        theme: Option<String>,
        /// Only drills from these games (comma-separated ids).
        #[arg(long)]
        games: Option<String>,
        /// Only mistakes made by this ply.
        #[arg(long)]
        max_ply: Option<u32>,
    },
    /// The next hint at a drill: what kind of move, then which piece. A right answer after one counts half.
    Hint {
        id: i64,
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
pub struct EvalArgs {
    #[arg(long)]
    pub fen: Option<String>,
    #[arg(long, num_args = 1.., value_delimiter = ' ')]
    pub moves: Vec<String>,
    /// The engine to search with (its id); the analysis engine when none.
    #[arg(long)]
    pub engine: Option<String>,
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
#[command(args_conflicts_with_subcommands = true)]
pub struct EnginesArgs {
    #[command(subcommand)]
    pub action: Option<EnginesAction>,
}

#[derive(Subcommand)]
pub enum EnginesAction {
    /// Add a UCI engine by path: `engines add --name "Reckless dev" --path ./reckless`.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        path: String,
        /// Its id (a slug); made from the name when left out.
        #[arg(long)]
        id: Option<String>,
        /// An argument it is started with (repeat for more).
        #[arg(long = "arg", allow_hyphen_values = true)]
        args: Vec<String>,
        /// A UCI option set after the handshake: `--option Threads=2` (repeat for more).
        #[arg(long = "option", value_name = "NAME=VALUE")]
        options: Vec<String>,
        /// Nodes it searches a move whatever the clock (as Maia's one).
        #[arg(long)]
        nodes: Option<u64>,
        /// The rating it plays at, when it has one by design.
        #[arg(long)]
        rating: Option<u32>,
    },
    /// Remove an engine from the list.
    Remove { id: String },
    /// Choose the analysis engine: reviews, evaluation, drills and the levels run on it.
    Analysis { id: String },
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
    /// Challenge a player, or Lichess's AI (`--ai 1-8`).
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
    #[arg(long, conflicts_with = "username")]
    pub ai: Option<u8>,
    /// minutes+seconds: 5+3.
    #[arg(long)]
    pub tc: String,
    #[arg(long)]
    pub rated: bool,
    #[arg(long, default_value = "random")]
    pub color: String,
}

/// Which command calls each route, by operation id: what the parity test checks against the OpenAPI document, and
/// the CLI's words for each route.
pub const COMMANDS: &[(&str, &str)] = &[
    ("health", "chess health"),
    ("status", "chess status"),
    ("diagnostics", "chess diagnostics"),
    ("token", "chess token"),
    ("setup_plan", "chess setup"),
    ("setup", "chess setup --yes"),
    ("list_engines", "chess engines"),
    ("add_engine", "chess engines add --name Reckless --path ./reckless"),
    ("remove_engine", "chess engines remove reckless"),
    ("set_analysis_engine", "chess engines analysis stockfish"),
    ("follow_events", "chess events --topics games"),
    ("list_games", "chess games"),
    ("start_game", "chess play --white you --black stockfish@5 --tc 5+3"),
    ("import_pgn", "chess import --pgn game.pgn"),
    ("get_game", "chess game 12"),
    ("get_game_pgn", "chess game 12 --pgn"),
    ("act_in_game", "chess move 12 e4"),
    ("stop_game", "chess stop 12"),
    ("get_review", "chess game 12 --review"),
    ("start_eval", "chess eval --depth 20"),
    ("list_evals", "chess eval --list"),
    ("stop_eval", "chess eval --stop 00000000000a"),
    ("explorer", "chess explorer --db masters"),
    ("tablebase", "chess tablebase --fen 8/8/8/8/8/8/6k1/4K2R_w_K_-_0_1"),
    ("review_games", "chess review 12 13"),
    ("top_players", "chess players top blitz"),
    ("get_player", "chess player lichess:cutemouse83"),
    ("get_profile", "chess profile you"),
    ("compare_players", "chess compare you lichess:cutemouse83"),
    ("import_player", "chess import lichess:cutemouse83"),
    ("get_repertoire", "chess repertoire --side white"),
    ("get_prep", "chess prep --side white"),
    ("get_progress", "chess progress"),
    ("list_levels", "chess levels"),
    ("plan_levels", "chess levels plan"),
    ("calibrate_levels", "chess levels calibrate"),
    ("list_drills", "chess drills"),
    ("make_drills", "chess drills make"),
    ("drills_today", "chess drills today"),
    ("hint_drill", "chess drills hint 3"),
    ("answer_drill", "chess drills answer 3 Nf3"),
    ("delete_drill", "chess drills delete 3"),
    ("drills_trend", "chess drills trend"),
    ("list_work", "chess work"),
    ("get_work", "chess work show 7"),
    ("stop_work", "chess work stop 7"),
    ("pause_work", "chess work pause 7"),
    ("resume_work", "chess work resume 7"),
    ("work_log", "chess work log 7"),
    ("list_connections", "chess lichess"),
    ("login", "chess lichess you login"),
    ("set_connection_token", "chess lichess you token"),
    ("logout", "chess lichess you logout"),
    ("seek", "chess lichess you seek --tc 10+5"),
    ("cancel_seek", "chess lichess you seek --stop"),
    ("challenge", "chess lichess you challenge alice --tc 5+3"),
    ("accept_challenge", "chess lichess you accept abcd1234"),
    ("decline_challenge", "chess lichess you decline abcd1234"),
    ("cancel_challenge", "chess lichess you cancel abcd1234"),
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

/// A game in one line: `#12    finished  Chess 1400 vs Chess 1800  0-1 checkmate  1+0  29 moves  2026-10-27 05:07`.
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
            format!(
                "Chess {} · {} cores ({} free) · analysis {}\ndata: {}",
                str_of(&v["version"]),
                v["cores"],
                v["ledger"]["free"],
                str_of(&v["policy"]["assist"]),
                str_of(&v["machine"]["data_dir"])
            )
        }),
        Command::Diagnostics => print(&c.get("/api/diagnostics").await?, json, |v| {
            lines(&v["items"], |i| {
                format!("{:<5} {:<10} {}", str_of(&i["level"]), str_of(&i["key"]), str_of(&i["title"]))
            })
        }),
        Command::Token => print(&c.get("/api/token").await?, json, |v| str_of(&v["token"])),
        Command::Setup { only, yes, wait } => setup(&c, only, yes, wait, json).await?,
        Command::Engines(a) => engines(&c, a, json).await?,
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
            push(&mut q, "calibration", a.calibration);
            push(&mut q, "limit", Some(a.limit.to_string()));
            print(&c.get(&format!("/api/games{}", query(&q))).await?, json, |v| lines(v, game_line));
        }
        Command::Game { id, pgn, review, engine, nodes, follow } => {
            if pgn {
                print!("{}", c.text(&format!("/api/games/{id}/pgn")).await?);
            } else if review {
                let mut q = Vec::new();
                push(&mut q, "engine", engine);
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
            let from = if a.fen.is_some() || !a.moves.is_empty() {
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
                "engine": a.engine, "nodes": a.nodes, "concurrency": a.concurrency,
                "background": a.background || a.all,
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
        Command::Prep { side, limit } => {
            let mut q = vec![("side", side)];
            push(&mut q, "limit", limit.map(|n| n.to_string()));
            let v = c.get(&format!("/api/players/you/prep{}", query(&q))).await?;
            print(&v, json, |p| {
                lines(&p["lines"], |l| {
                    let flags = [("scores_badly", "scores badly"), ("goes_wrong_early", "goes wrong early")]
                        .iter()
                        .filter(|(k, _)| l[*k].as_bool() == Some(true))
                        .map(|(_, w)| *w)
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "{}  {} games  score {}  early mistakes {}  {}  {}",
                        str_of(&l["name"]),
                        l["games"],
                        l["score"].as_f64().map_or("–".to_owned(), |s| format!("{:.0}%", s * 100.0)),
                        l["early_mistakes"],
                        str_of(&l["pgn"]),
                        flags
                    )
                })
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
        Some(LevelsAction::Calibrate { count, games_per_pair, wait }) => {
            let body = json!({ "count": count, "games_per_pair": games_per_pair });
            let w = c.post("/api/levels/calibrate", body).await?;
            finish_work(c, w, wait, json).await?;
        }
    }
    Ok(())
}

async fn drills(c: &Client, a: DrillsArgs, json: bool) -> Result<()> {
    match a.action {
        None => print(&c.get("/api/drills").await?, json, |v| lines(v, drill_line)),
        Some(DrillsAction::Hint { id }) => {
            let v = c.post(&format!("/api/drills/{id}/hint"), json!({})).await?;
            print(&v, json, |h| {
                format!(
                    "hint {}: {}{}",
                    h["level"],
                    str_of(&h["text"]),
                    h["from"].as_str().map_or(String::new(), |f| format!(" (from {f})"))
                )
            });
        }
        Some(DrillsAction::Today { new_per_day, limit, theme, games, max_ply }) => {
            let mut q = Vec::new();
            push(&mut q, "new_per_day", new_per_day.map(|n| n.to_string()));
            push(&mut q, "limit", limit.map(|n| n.to_string()));
            push(&mut q, "theme", theme);
            push(&mut q, "games", games);
            push(&mut q, "max_ply", max_ply.map(|n| n.to_string()));
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
        "fen": a.fen, "moves": a.moves, "engine": a.engine, "multipv": a.multipv, "budget": budget,
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
        // The last word on seeking, for people.
        for note in [&s["error"], &s["you"]["seek"]["note"]] {
            if let Some(note) = note.as_str().filter(|n| !n.is_empty()) {
                text.push_str(&format!("\n  {note}"));
            }
        }
        text
    };
    match a.account {
        None => {
            let v = c.get("/api/connections").await?;
            print(&v, json, |v| status_text(&v["you"]));
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
                    "username": ch.username, "ai_level": ch.ai, "time": clock(&ch.tc)?,
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
    }
    Ok(())
}

/// Shows what setup would download and its size, asks (unless `--yes`), and queues it.
async fn setup(c: &Client, only: Vec<String>, yes: bool, wait: bool, json: bool) -> Result<()> {
    let plan = c.get("/api/setup").await?;
    let downloads = plan["downloads"].as_array().cloned().unwrap_or_default();
    let wanted: Vec<&Value> =
        downloads.iter().filter(|d| only.is_empty() || only.iter().any(|o| d["package"] == json!(o))).collect();
    if !json {
        for note in plan["notes"].as_array().into_iter().flatten() {
            println!("{}", str_of(note));
        }
    }
    if wanted.is_empty() {
        print(&plan, json, |_| "Nothing to download: the engines are installed.".to_owned());
        return Ok(());
    }
    let mb = |b: &Value| b.as_u64().map_or("size unknown".to_owned(), |n| format!("{:.1} MB", n as f64 / 1e6));
    if !yes {
        let total: u64 = wanted.iter().filter_map(|d| d["bytes"].as_u64()).sum();
        eprintln!("Setup downloads into the data directory's tools/:");
        for d in &wanted {
            eprintln!("  {:<24} {:>12}  {}", str_of(&d["name"]), mb(&d["bytes"]), str_of(&d["url"]));
        }
        eprint!("Download {} files ({})? [y/N] ", wanted.len(), mb(&json!(total)));
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            bail!("nothing downloaded");
        }
    }
    let w = c.post("/api/setup", json!({ "packages": only })).await?;
    finish_work(c, w, wait, json).await
}

/// `engines`: the list, or adding, removing and choosing the analysis engine.
async fn engines(c: &Client, a: EnginesArgs, json: bool) -> Result<()> {
    let list = |v: &Value| {
        lines(&v["engines"], |e| {
            let state = match (e["installed"].as_bool(), e["analysis"].as_bool()) {
                (Some(true), Some(true)) => "analysis",
                (Some(true), _) => "installed",
                _ => "not installed",
            };
            let extra = match (e["rating"].as_u64(), e["nodes"].as_u64()) {
                (Some(r), Some(n)) => format!("  {r}, {n} node a move"),
                (Some(r), None) => format!("  {r}"),
                _ => String::new(),
            };
            format!(
                "{:<12} {:<16} {:<14} {}{extra}",
                str_of(&e["id"]),
                str_of(&e["name"]),
                state,
                str_of(&e["id_name"])
            )
        })
    };
    match a.action {
        None => print(&c.get("/api/engines").await?, json, list),
        Some(EnginesAction::Add { name, path, id, args, options, nodes, rating }) => {
            let mut opts = serde_json::Map::new();
            for pair in options {
                let Some((k, v)) = pair.split_once('=') else { bail!("--option {pair}: use NAME=VALUE") };
                opts.insert(k.trim().to_owned(), Value::String(v.trim().to_owned()));
            }
            let body = json!({ "name": name, "path": path, "id": id, "args": args, "options": opts, "nodes": nodes, "rating": rating });
            print(&c.post("/api/engines", body).await?, json, |e| {
                format!("added {} ({}): {}", str_of(&e["id"]), str_of(&e["id_name"]), str_of(&e["path"]))
            });
        }
        Some(EnginesAction::Remove { id }) => {
            print(&c.delete(&format!("/api/engines/{}", seg(&id))).await?, json, list)
        }
        Some(EnginesAction::Analysis { id }) => {
            print(&c.put("/api/engines/analysis", json!({ "engine": id })).await?, json, list)
        }
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
