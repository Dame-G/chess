//! The client commands: each is one API call (or a poll of one), printed as JSON with `--json` and as a short text
//! summary otherwise. Errors exit non-zero with the runner's message on stderr.

use std::time::Duration;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, EnPassantMode, fen::Fen, san::SanPlus, uci::UciMove};

use crate::client::{Client, print};

mod tune;

#[derive(Subcommand)]
pub enum ClientCommand {
    /// Show the runner's status.
    Status,
    /// Build a ref of the Reckless checkout (a branch, tag or commit) and bench it; or `build delete <id>` /
    /// `build prune` to free disk.
    Build(BuildCommand),
    /// List the branches that can be built or tested, most recently committed first.
    Branches,
    /// One line per branch (a version): its latest verdict, tune, builds and whether the Lichess bot plays it.
    Versions {
        /// Include hidden versions.
        #[arg(long)]
        all: bool,
    },
    /// Hide, show or delete a version (a branch of the fork).
    #[command(subcommand)]
    Version(VersionCommand),
    /// List builds, newest first.
    Builds,
    /// List recent jobs, newest first.
    Jobs {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Show one job.
    Job { id: i64 },
    /// Print a job's output.
    Log { id: i64 },
    /// Wait for a job to finish and print it. Exits non-zero unless it is done.
    Wait { id: i64 },
    /// Cancel a queued job or stop a running one.
    Cancel { id: i64 },
    /// Search a position to a limit and print the best move and lines.
    Analyse {
        /// The position as FEN. Defaults to the start position.
        #[arg(long)]
        fen: Option<String>,
        /// Moves from that position, in UCI notation.
        #[arg(long, num_args = 1.., value_delimiter = ' ')]
        moves: Vec<String>,
        /// The build to use. Defaults to the newest.
        #[arg(long)]
        build: Option<i64>,
        #[arg(long)]
        depth: Option<u32>,
        #[arg(long)]
        nodes: Option<u64>,
        /// Milliseconds.
        #[arg(long)]
        movetime: Option<u64>,
        #[arg(long, default_value_t = 1)]
        multipv: u32,
        #[arg(long, default_value_t = 1)]
        threads: u32,
    },
    /// Engine-vs-engine matches you can watch: start one, or `match show <id>` / `match stop <id>`.
    Match(MatchCommand),
    /// Play your move in a match where you hold a seat, then wait for the engine's reply and print both.
    Move {
        /// The match.
        id: i64,
        /// The move, in UCI (e2e4, e7e8q) or SAN (e4, Nf3, O-O, exd8=Q+).
        #[arg(value_name = "MOVE", allow_hyphen_values = true)]
        mv: String,
        /// Return once the move is on the board, without waiting for the reply.
        #[arg(long)]
        no_wait: bool,
    },
    /// Take back your last move and the engine's reply, on your turn, in a game without a clock.
    Takeback { id: i64 },
    /// Offer the engine a draw, on your turn. It accepts or declines at once.
    Draw { id: i64 },
    /// List recent matches, newest first.
    Matches {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// The Lichess bot: with no subcommand, its status. A build plays as your Lichess BOT account.
    Lichess(LichessCommand),
    /// Experiments: a dev ref against a base ref through a funnel of stages.
    #[command(subcommand)]
    Experiment(ExperimentCommand),
    /// Several experiments submitted, waited for and read together.
    #[command(subcommand)]
    Batch(BatchCommand),
    /// SPSA tunes of a build's declared tunables, and applying their result as a branch.
    #[command(subcommand)]
    Tune(tune::TuneCommand),
    /// One match between two refs: shorthand for an experiment with a single match stage.
    Test {
        #[arg(long)]
        dev: String,
        #[arg(long, default_value = "main")]
        base: String,
        /// vstc, stc, ltc, stc-hash, ltc-hash, smp or nodes.
        #[arg(long, default_value = "stc")]
        preset: String,
        /// SPRT bounds in normalized Elo, such as 0,3 or -2.75,0.25. Default 0,3 unless --pairs is given.
        #[arg(long, value_delimiter = ',', num_args = 2, allow_hyphen_values = true)]
        bounds: Option<Vec<f64>>,
        /// Play this many game pairs instead of an SPRT.
        #[arg(long)]
        pairs: Option<u32>,
        #[arg(long)]
        wait: bool,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct BuildCommand {
    #[command(subcommand)]
    action: Option<BuildAction>,
    /// The branch, tag or commit to build.
    #[arg(value_name = "REF", required = true)]
    refname: Option<String>,
    /// Wait for the build and print it, instead of printing the queued job.
    #[arg(long)]
    wait: bool,
}

#[derive(Subcommand)]
pub enum BuildAction {
    /// Delete a build's binary. Its record stays, so what used it still reads; its commit builds again when needed.
    Delete { id: i64 },
    /// Delete the binaries of builds nothing uses, keeping each branch's newest.
    Prune {
        /// Only builds at least this many days old.
        #[arg(long)]
        older_than_days: Option<u32>,
        /// Prune each branch's newest builds too.
        #[arg(long)]
        all: bool,
        /// Say what would go, deleting nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum VersionCommand {
    /// Hide a version from the Versions view (it can still be built and tested).
    Hide { name: String },
    /// Show a hidden version again.
    Show { name: String },
    /// Delete a branch of the fork. Experiments and tunes that used it keep their records.
    Delete {
        name: String,
        /// Delete it on `origin` (GitHub) too.
        #[arg(long)]
        remote: bool,
        /// Delete its builds' binaries too.
        #[arg(long)]
        builds: bool,
        /// Confirm the deletion; nothing is deleted without it.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct MatchCommand {
    #[command(subcommand)]
    action: Option<MatchAction>,
    #[command(flatten)]
    new: NewMatch,
}

#[derive(Subcommand)]
pub enum MatchAction {
    /// Show a match: its headers, moves and result.
    Show { id: i64 },
    /// Stop a running match; it is recorded as aborted.
    Stop { id: i64 },
}

/// Starts a match. One match runs at a time.
#[derive(Args)]
pub struct NewMatch {
    /// White: a build id, a ref (the newest build of it), or `human` for you (move with `chess move`).
    #[arg(long)]
    white: Option<String>,
    /// Black: a build id, a ref, or `human`. At least one side must be a build.
    #[arg(long)]
    black: Option<String>,
    /// Clocks as `base+increment` seconds, such as 10+0.1.
    #[arg(long, conflicts_with_all = ["nodes", "movetime"])]
    tc: Option<String>,
    /// A fixed number of nodes per move instead of a clock.
    #[arg(long, conflicts_with = "movetime")]
    nodes: Option<u64>,
    /// A fixed time per move in milliseconds instead of a clock.
    #[arg(long)]
    movetime: Option<u64>,
    /// Start from this position (a FEN) instead of the start position.
    #[arg(long, conflicts_with = "book")]
    fen: Option<String>,
    /// Moves from that position, in UCI notation.
    #[arg(long, num_args = 1.., value_delimiter = ' ', requires = "fen")]
    moves: Vec<String>,
    /// Start from a random line of the opening book the tests use.
    #[arg(long)]
    book: bool,
    /// Hash size in MiB for each engine. Default 16.
    #[arg(long)]
    hash: Option<u32>,
    /// Threads for each engine. Default 1.
    #[arg(long)]
    threads: Option<u32>,
    /// End decided games early, as the tests do.
    #[arg(long)]
    adjudicate: bool,
    /// Run the engines with Minimal on, as the tests do: they then report nothing while thinking.
    #[arg(long)]
    minimal: bool,
    /// Print each move as it is played, then the result.
    #[arg(long)]
    follow: bool,
}

#[derive(Args)]
pub struct LichessCommand {
    #[command(subcommand)]
    action: Option<LichessAction>,
}

#[derive(Subcommand)]
pub enum LichessAction {
    /// Store the bot's Lichess token (scope bot:play), read from stdin so it stays out of shell history. Lichess checks
    /// it, and the account it belongs to is shown.
    Token,
    /// Forget the token and take the bot offline. Games in progress are played out.
    Logout,
    /// Show the settings, or change the ones named by flags.
    Settings(LichessSettings),
    /// Take the bot online: it answers challenges and plays, and stays online across restarts.
    Start,
    /// Take the bot offline. Games in progress are played out.
    Stop,
    /// Challenge a Lichess player (usually another bot). The bot must be online.
    Challenge {
        username: String,
        /// The clock as Lichess writes it: minutes plus increment seconds, such as 3+2 (or 0.5+0 for 30 seconds).
        #[arg(long, default_value = "3+2")]
        tc: String,
        #[arg(long)]
        rated: bool,
        /// white, black or random: the colour our build plays.
        #[arg(long, default_value = "random")]
        color: String,
    },
    /// List the bots online on Lichess now, with their ratings.
    Bots,
    /// Look for a game: challenge random online bots near the bot's rating until one accepts. `seek stop` stops.
    Seek {
        /// Keep looking after every game, across restarts, until `chess lichess seek stop` or `chess lichess stop`.
        #[arg(long)]
        always: bool,
        #[command(subcommand)]
        action: Option<SeekAction>,
    },
    /// Turn the account into a BOT account. This cannot be undone, and Lichess only allows it for an account that has
    /// never played a game.
    Upgrade {
        /// Required: says you know the upgrade is permanent.
        #[arg(long)]
        yes_irreversible: bool,
    },
}

#[derive(Subcommand)]
pub enum SeekAction {
    /// Stop looking for games and withdraw the challenge waiting for an answer.
    Stop,
}

/// Flags change only what they name; the rest stays as it is.
#[derive(Args)]
pub struct LichessSettings {
    /// The build that plays: an id, or a ref meaning its newest build at the start of each game.
    #[arg(long)]
    build: Option<String>,
    /// Hash table size in MiB.
    #[arg(long)]
    hash: Option<u32>,
    /// Search threads per game.
    #[arg(long)]
    threads: Option<u32>,
    /// Milliseconds kept in hand on every move for network lag (Reckless's MoveOverhead).
    #[arg(long)]
    move_overhead: Option<u32>,
    /// Accept rated challenges: true or false.
    #[arg(long, value_name = "BOOL")]
    rated: Option<bool>,
    /// Accept casual challenges: true or false.
    #[arg(long, value_name = "BOOL")]
    casual: Option<bool>,
    /// The speeds accepted, such as bullet,blitz,rapid (classical too). Never correspondence.
    #[arg(long, value_delimiter = ',')]
    speeds: Option<Vec<String>>,
    /// The initial clocks accepted, in seconds, as MIN-MAX such as 60-1800.
    #[arg(long)]
    initial: Option<String>,
    /// The increments accepted, in seconds, as MIN-MAX such as 0-30.
    #[arg(long)]
    increment: Option<String>,
    /// Also accept standard chess from a set position: true or false.
    #[arg(long, value_name = "BOOL")]
    from_position: Option<bool>,
    /// Accept challenges from people: true or false.
    #[arg(long, value_name = "BOOL")]
    humans: Option<bool>,
    /// Accept challenges from other bots: true or false.
    #[arg(long, value_name = "BOOL")]
    bots: Option<bool>,
    /// Games at once.
    #[arg(long)]
    max_games: Option<u32>,
    /// The clocks to look for games at, as minutes+seconds such as 3+0,3+2,5+3. One is picked at random each time.
    #[arg(long, value_delimiter = ',')]
    seek_clocks: Option<Vec<String>>,
    /// Whether the challenges sent when looking for a game are rated: true or false.
    #[arg(long, value_name = "BOOL")]
    seek_rated: Option<bool>,
    /// The rating window for opponents when looking for a game (widened to twice it, then anyone, if nobody is in it).
    #[arg(long, value_name = "N")]
    seek_range: Option<u32>,
}

#[derive(Subcommand)]
pub enum ExperimentCommand {
    /// Queue an experiment.
    New {
        #[arg(long)]
        dev: String,
        #[arg(long, default_value = "main")]
        base: String,
        /// gainer, nonregression or nonfunctional: picks the funnel.
        #[arg(long, default_value = "gainer")]
        kind: String,
        #[arg(long)]
        name: Option<String>,
        /// What the change should do, and why.
        #[arg(long, default_value = "")]
        hypothesis: String,
        /// Add an LTC confirmation.
        #[arg(long)]
        ltc: bool,
        #[arg(long)]
        wait: bool,
    },
    /// List experiments, newest first.
    List {
        #[arg(long)]
        batch: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Show an experiment with its stages and matches.
    Show { id: i64 },
    /// Wait for an experiment to finish, then show it.
    Wait { id: i64 },
    /// Cancel a queued experiment or stop a running one.
    Cancel { id: i64 },
}

#[derive(Subcommand)]
pub enum BatchCommand {
    /// Queue a batch from a JSON file (`-` for stdin): an array of experiments, each like the body of
    /// POST /api/experiments ({ "dev": "...", "kind": "gainer", "hypothesis": "..." }).
    Submit {
        file: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        wait: bool,
    },
    /// Show a batch's experiments as one table.
    Show { name: String },
    /// Wait for every experiment of a batch to finish, then show the table.
    Wait { name: String },
}

pub async fn run(client: Client, command: ClientCommand, json: bool) -> Result<()> {
    match command {
        ClientCommand::Experiment(cmd) => return experiment(&client, cmd, json).await,
        ClientCommand::Batch(cmd) => return batch(&client, cmd, json).await,
        ClientCommand::Tune(cmd) => return tune::run(&client, cmd, json).await,
        ClientCommand::Match(cmd) => return matches(&client, cmd, json).await,
        ClientCommand::Move { id, mv, no_wait } => return play_move(&client, id, &mv, !no_wait, json).await,
        ClientCommand::Takeback { id } => {
            let m = client.post(&format!("/api/matches/{id}/takeback"), json!({})).await?;
            print(&m, json, |m| {
                let plies = m["moves"].as_array().map_or(0, Vec::len);
                format!("taken back: {plies} plies on the board, {} to move", m["turn"].as_str().unwrap_or("?"))
            });
            return Ok(());
        }
        ClientCommand::Draw { id } => {
            let answer = client.post(&format!("/api/matches/{id}/draw"), json!({})).await?;
            print(&answer, json, |a| {
                if a["accepted"].as_bool() == Some(true) { "Draw by agreement".into() } else { "Draw declined".into() }
            });
            return Ok(());
        }
        ClientCommand::Lichess(cmd) => return lichess(&client, cmd, json).await,
        ClientCommand::Matches { limit } => {
            let list = client.get(&format!("/api/matches?limit={limit}")).await?;
            print(&list, json, |l| lines(l, match_line));
            return Ok(());
        }
        ClientCommand::Test { dev, base, preset, bounds, pairs, wait } => {
            let body = json!({ "dev": dev, "base": base, "preset": preset, "bounds": bounds, "pairs": pairs });
            let e = client.post("/api/experiments", body).await?;
            return finish_experiment(&client, e, wait, json).await;
        }
        ClientCommand::Status => {
            let s = client.get("/api/status").await?;
            print(&s, json, |s| {
                format!(
                    "runner {} on {} ({} threads), reckless at {}",
                    s["version"].as_str().unwrap_or("?"),
                    s["cpu"]["name"].as_str().unwrap_or("?"),
                    s["cpu"]["threads"],
                    s["reckless_dir"].as_str().unwrap_or("?"),
                )
            });
        }
        ClientCommand::Build(BuildCommand { action: Some(action), .. }) => {
            return build_action(&client, action, json).await;
        }
        ClientCommand::Versions { all } => {
            let list = client.get("/api/versions").await?;
            let shown: Vec<Value> = list
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| all || v["hidden"].as_bool() != Some(true))
                .cloned()
                .collect();
            print(&Value::Array(shown), json, |l| lines(l, version_line));
        }
        ClientCommand::Version(cmd) => return version(&client, cmd, json).await,
        ClientCommand::Build(BuildCommand { action: None, refname, wait }) => {
            let job = client.post("/api/builds", json!({ "ref": refname })).await?;
            if !wait {
                print(&job, json, job_line);
                return Ok(());
            }
            let job = wait_for(&client, job["id"].as_i64().unwrap_or_default(), json).await?;
            let build = client.get(&format!("/api/builds/{}", job["result"]["build"])).await?;
            print(&build, json, build_line);
        }
        ClientCommand::Branches => {
            let branches = client.get("/api/branches").await?;
            print(&branches, json, |b| lines(b, branch_line));
        }
        ClientCommand::Builds => {
            let builds = client.get("/api/builds").await?;
            print(&builds, json, |b| lines(b, build_line));
        }
        ClientCommand::Jobs { limit } => {
            let jobs = client.get(&format!("/api/jobs?limit={limit}")).await?;
            print(&jobs, json, |j| lines(j, job_line));
        }
        ClientCommand::Job { id } => print(&client.get(&format!("/api/jobs/{id}")).await?, json, job_line),
        ClientCommand::Log { id } => print!("{}", client.text(&format!("/api/jobs/{id}/log")).await?),
        ClientCommand::Wait { id } => {
            let job = wait_for(&client, id, json).await?;
            print(&job, json, job_line);
        }
        ClientCommand::Cancel { id } => print(&client.delete(&format!("/api/jobs/{id}")).await?, json, job_line),
        ClientCommand::Analyse { fen, moves, build, depth, nodes, movetime, multipv, threads } => {
            let req = json!({
                "fen": fen.unwrap_or_else(|| "startpos".into()),
                "moves": moves,
                "build": build,
                "multipv": multipv,
                "threads": threads,
                "limits": { "depth": depth, "nodes": nodes, "movetime": movetime },
            });
            let result = client.post("/api/analyse", req).await?;
            print(&result, json, |r| {
                let mut out = format!("bestmove {}", r["bestmove"].as_str().unwrap_or("?"));
                for line in r["lines"].as_array().into_iter().flatten() {
                    let score = match (&line["score"]["cp"], &line["score"]["mate"]) {
                        (Value::Number(cp), _) => format!("{:+.2}", cp.as_f64().unwrap_or(0.0) / 100.0),
                        (_, Value::Number(m)) => format!("#{m}"),
                        _ => "?".into(),
                    };
                    let pv: Vec<&str> = line["pv"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    out += &format!("\n{score:>7}  depth {:<3} {}", line["depth"], pv.join(" "));
                }
                out
            });
        }
    }
    Ok(())
}

/// Polls a job until it finishes. Fails unless it is done, after printing it.
async fn wait_for(client: &Client, id: i64, json: bool) -> Result<Value> {
    loop {
        let job = client.get(&format!("/api/jobs/{id}")).await?;
        match job["status"].as_str() {
            Some("done") => return Ok(job),
            Some("failed" | "cancelled") => {
                print(&job, json, job_line);
                bail!("job {id} {}", job["status"].as_str().unwrap_or_default());
            }
            _ => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
}

fn lines(list: &Value, line: fn(&Value) -> String) -> String {
    list.as_array().into_iter().flatten().map(line).collect::<Vec<_>>().join("\n")
}

fn branch_line(b: &Value) -> String {
    let date = b["date"].as_str().unwrap_or_default();
    let day = date.get(..10).unwrap_or(date);
    format!("{:<28} {day} {}", b["name"].as_str().unwrap_or_default(), b["subject"].as_str().unwrap_or_default())
}

fn job_line(j: &Value) -> String {
    let what = match j["spec"]["kind"].as_str() {
        Some("build") => format!("build {}", j["spec"]["ref"].as_str().unwrap_or("?")),
        Some(kind) => kind.to_owned(),
        None => "?".into(),
    };
    let tail = j["error"].as_str().map(|e| format!(": {e}")).unwrap_or_default();
    format!("job {} {} {}{tail}", j["id"], what, j["status"].as_str().unwrap_or("?"))
}

fn build_line(b: &Value) -> String {
    let bench = b["bench"].as_u64().unwrap_or_default();
    let check = match b["bench_expected"].as_u64() {
        Some(e) if e == bench => " (matches commit)",
        Some(_) => " (DIFFERS from commit)",
        None => "",
    };
    format!(
        "build {} {} {}  bench {bench}{check}  {} nps  {}",
        b["id"],
        b["commit"].as_str().unwrap_or("?").get(..8).unwrap_or("?"),
        b["ref"].as_str().unwrap_or("?"),
        b["nps"],
        b["subject"].as_str().unwrap_or(""),
    ) + if b["deleted_at"].is_null() { "" } else { "  (deleted)" }
}

fn version_line(v: &Value) -> String {
    let mut tags = Vec::new();
    for (flag, tag) in [("is_main", "main"), ("on_lichess", "lichess"), ("tunables", "tunable"), ("hidden", "hidden")] {
        if v[flag].as_bool() == Some(true) {
            tags.push(tag);
        }
    }
    let verdict = match &v["verdict"] {
        Value::Null => "untested".to_owned(),
        e => {
            let elo = e["estimate"]["elo"].as_f64().map(|elo| format!(" {elo:+.1} Elo")).unwrap_or_default();
            format!(
                "{} vs {}{elo} (experiment {})",
                e["status"].as_str().unwrap_or("?"),
                v["tested_against"].as_str().unwrap_or("?"),
                e["experiment"]
            )
        }
    };
    let tune = match &v["tune"] {
        Value::Null => String::new(),
        t => {
            format!("  tune {} {} {}/{}", t["id"], t["status"].as_str().unwrap_or("?"), t["iteration"], t["iterations"])
        }
    };
    let built = v["latest_build_id"].as_i64().map(|id| format!("build {id}")).unwrap_or_else(|| "not built".into());
    let tags = if tags.is_empty() { String::new() } else { format!(" [{}]", tags.join(", ")) };
    format!("{:<28} {built:<10} {verdict}{tune}{tags}", v["name"].as_str().unwrap_or("?"))
}

async fn build_action(client: &Client, action: BuildAction, json: bool) -> Result<()> {
    match action {
        BuildAction::Delete { id } => {
            let b = client.delete(&format!("/api/builds/{id}")).await?;
            print(&b, json, |b| format!("deleted the binary of {}", build_line(b)));
        }
        BuildAction::Prune { older_than_days, all, dry_run } => {
            let body =
                json!({ "older_than_days": older_than_days, "keep_latest_per_branch": !all, "dry_run": dry_run });
            let r = client.post("/api/builds/prune", body).await?;
            print(&r, json, |r| {
                let ids = |k: &str| {
                    let ids: Vec<String> = r[k].as_array().into_iter().flatten().map(Value::to_string).collect();
                    if ids.is_empty() { "none".to_owned() } else { ids.join(", ") }
                };
                let mib = r["freed_bytes"].as_u64().unwrap_or_default() as f64 / 1048576.0;
                let verb = if dry_run { "would delete" } else { "deleted" };
                format!("{verb} builds {} ({mib:.1} MiB); in use: {}", ids("deleted"), ids("in_use"))
            });
        }
    }
    Ok(())
}

async fn version(client: &Client, cmd: VersionCommand, json: bool) -> Result<()> {
    let (name, hidden) = match cmd {
        VersionCommand::Hide { name } => (name, true),
        VersionCommand::Show { name } => (name, false),
        VersionCommand::Delete { name, remote, builds, yes } => {
            if !yes {
                bail!("deleting {name} cannot be undone: add --yes to confirm");
            }
            let path = format!("/api/versions/{}?remote={remote}&builds={builds}", urlencode(&name));
            let r = client.delete_with(&path, json!({ "confirm": name })).await?;
            print(&r, json, |r| {
                let builds = r["builds"].as_array().map_or(0, Vec::len);
                let origin = if r["remote"].as_bool() == Some(true) { " and on origin" } else { "" };
                format!("deleted {name} locally{origin}; {builds} build binaries deleted")
            });
            return Ok(());
        }
    };
    let r = client.put(&format!("/api/versions/{}/hidden", urlencode(&name)), json!({ "hidden": hidden })).await?;
    print(&r, json, |_| format!("{name} is {}", if hidden { "hidden" } else { "shown" }));
    Ok(())
}

async fn experiment(client: &Client, cmd: ExperimentCommand, json: bool) -> Result<()> {
    match cmd {
        ExperimentCommand::New { dev, base, kind, name, hypothesis, ltc, wait } => {
            let body =
                json!({ "dev": dev, "base": base, "kind": kind, "name": name, "hypothesis": hypothesis, "ltc": ltc });
            let e = client.post("/api/experiments", body).await?;
            finish_experiment(client, e, wait, json).await?;
        }
        ExperimentCommand::List { batch, limit } => {
            let query = match &batch {
                Some(b) => format!("?limit={limit}&batch={}", urlencode(b)),
                None => format!("?limit={limit}"),
            };
            let list = client.get(&format!("/api/experiments{query}")).await?;
            print(&list, json, |l| lines(l, experiment_line));
        }
        ExperimentCommand::Show { id } => {
            let detail = client.get(&format!("/api/experiments/{id}")).await?;
            print(&detail, json, experiment_text);
        }
        ExperimentCommand::Wait { id } => {
            let detail = wait_experiment(client, id).await?;
            print(&detail, json, experiment_text);
        }
        ExperimentCommand::Cancel { id } => {
            let e = client.delete(&format!("/api/experiments/{id}")).await?;
            print(&e, json, experiment_line);
        }
    }
    Ok(())
}

async fn batch(client: &Client, cmd: BatchCommand, json: bool) -> Result<()> {
    match cmd {
        BatchCommand::Submit { file, name, wait } => {
            let text =
                if file == "-" { std::io::read_to_string(std::io::stdin())? } else { std::fs::read_to_string(&file)? };
            let experiments: Value = serde_json::from_str(&text)?;
            let created = client.post("/api/batches", json!({ "name": name, "experiments": experiments })).await?;
            if wait {
                let table = wait_batch(client, &name).await?;
                print(&table, json, |t| lines(t, experiment_line));
            } else {
                print(&created, json, |l| lines(l, experiment_line));
            }
        }
        BatchCommand::Show { name } => {
            let list = client.get(&format!("/api/experiments?limit=1000&batch={}", urlencode(&name))).await?;
            print(&list, json, |l| lines(l, experiment_line));
        }
        BatchCommand::Wait { name } => {
            let table = wait_batch(client, &name).await?;
            print(&table, json, |t| lines(t, experiment_line));
        }
    }
    Ok(())
}

async fn finish_experiment(client: &Client, e: Value, wait: bool, json: bool) -> Result<()> {
    if wait {
        let detail = wait_experiment(client, e["id"].as_i64().unwrap_or_default()).await?;
        print(&detail, json, experiment_text);
    } else {
        print(&e, json, experiment_line);
    }
    Ok(())
}

fn finished(e: &Value) -> bool {
    matches!(e["status"].as_str(), Some("passed" | "failed" | "cancelled"))
}

/// Polls an experiment until it finishes. Passing and failing are both answers, so both exit zero.
async fn wait_experiment(client: &Client, id: i64) -> Result<Value> {
    loop {
        let detail = client.get(&format!("/api/experiments/{id}")).await?;
        if finished(&detail["experiment"]) {
            return Ok(detail);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn wait_batch(client: &Client, name: &str) -> Result<Value> {
    loop {
        let list = client.get(&format!("/api/experiments?limit=1000&batch={}", urlencode(name))).await?;
        let all = list.as_array().cloned().unwrap_or_default();
        if all.is_empty() {
            bail!("no batch named {name}");
        }
        if all.iter().all(finished) {
            return Ok(list);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn experiment_line(e: &Value) -> String {
    let status = e["status"].as_str().unwrap_or("?");
    let tail = match (e["current"].as_str(), e["summary"].as_str()) {
        (Some(stage), _) if status == "running" => format!("at {stage}"),
        (_, Some(summary)) => summary.to_owned(),
        _ => String::new(),
    };
    format!(
        "experiment {} {} ({} vs {}) {status}  {tail}",
        e["id"],
        e["name"].as_str().unwrap_or("?"),
        e["dev"].as_str().unwrap_or("?"),
        e["base"].as_str().unwrap_or("?")
    )
}

fn experiment_text(d: &Value) -> String {
    let e = &d["experiment"];
    let mut out = experiment_line(e);
    if let Some(h) = e["hypothesis"].as_str().filter(|h| !h.is_empty()) {
        out += &format!("\n  hypothesis: {h}");
    }
    for r in e["results"].as_array().into_iter().flatten() {
        let mark = if r["passed"].as_bool() == Some(true) { "pass" } else { "FAIL" };
        out += &format!("\n  {mark} {:<8} {}", r["stage"].as_str().unwrap_or("?"), r["detail"].as_str().unwrap_or(""));
    }
    for t in d["tests"].as_array().into_iter().flatten().filter(|t| t["running"].as_bool() == Some(true)) {
        let llr = t["llr"].as_f64().map(|l| format!("LLR {l:.2}")).unwrap_or_default();
        out += &format!("\n  running {:<5} {} games {llr}", t["stage"].as_str().unwrap_or("?"), t["games"]);
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------
// Matches
// ---------------------------------------------------------------------------------------------------------------------

/// A build id, or the newest build of a ref (or of a commit prefix).
async fn resolve_build(client: &Client, spec: &str) -> Result<i64> {
    if let Ok(id) = spec.parse::<i64>() {
        return Ok(id);
    }
    let builds = client.get("/api/builds").await?;
    builds
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["deleted_at"].is_null())
        .find(|b| b["ref"].as_str() == Some(spec) || b["commit"].as_str().is_some_and(|c| c.starts_with(spec)))
        .and_then(|b| b["id"].as_i64())
        .ok_or_else(|| anyhow::anyhow!("no build of {spec}: build it first with `chess build {spec}`"))
}

/// `human` for your own seat, else a build.
async fn resolve_seat(client: &Client, spec: &str) -> Result<Value> {
    if spec.eq_ignore_ascii_case("human") {
        return Ok(json!("human"));
    }
    Ok(json!(resolve_build(client, spec).await?))
}

/// The position a match stands at: after its last move, or where it started.
fn current_position(m: &Value) -> Result<Chess> {
    let fen = m["moves"]
        .as_array()
        .and_then(|moves| moves.last())
        .and_then(|mv| mv["fen_after"].as_str())
        .or_else(|| m["start_fen"].as_str())
        .unwrap_or("startpos");
    if fen == "startpos" {
        return Ok(Chess::default());
    }
    Ok(fen.parse::<Fen>()?.into_position(CastlingMode::Standard)?)
}

/// A move in UCI or SAN as the UCI the runner takes, checked against the position.
fn to_uci(pos: &Chess, text: &str) -> Result<String> {
    let text = text.trim();
    let mv = text
        .parse::<UciMove>()
        .ok()
        .and_then(|u| u.to_move(pos).ok())
        .or_else(|| text.parse::<SanPlus>().ok().and_then(|s| s.san.to_move(pos).ok()));
    match mv {
        Some(m) => Ok(m.to_uci(CastlingMode::Standard).to_string()),
        None => bail!("{text} is not a legal move in {}", Fen::from_position(pos, EnPassantMode::Legal)),
    }
}

/// Plays your move; with `wait`, also waits for the engine's reply (or the end of the game) and prints both.
async fn play_move(client: &Client, id: i64, text: &str, wait: bool, json: bool) -> Result<()> {
    let before = client.get(&format!("/api/matches/{id}")).await?;
    if before["status"].as_str() != Some("running") {
        bail!("match {id} is over: {}", match_outcome(&before));
    }
    let uci = to_uci(&current_position(&before)?, text)?;
    let mut m = client.post(&format!("/api/matches/{id}/moves"), json!({ "uci": uci })).await?;
    let mine = m["moves"].as_array().map_or(0, Vec::len);
    let engine_to_move = |m: &Value| {
        let side = m["turn"].as_str().unwrap_or("white");
        m["status"].as_str() == Some("running") && m[side]["human"].as_bool() != Some(true)
    };
    if wait && engine_to_move(&m) {
        // The reply is on the board once the move count grows past ours, or the game is over.
        while m["status"].as_str() == Some("running") && m["moves"].as_array().map_or(0, Vec::len) <= mine {
            tokio::time::sleep(Duration::from_millis(200)).await;
            m = client.get(&format!("/api/matches/{id}")).await?;
        }
    }
    print(&m, json, |m| {
        let start = m["start_fen"].as_str().unwrap_or("startpos");
        let moves = m["moves"].as_array().cloned().unwrap_or_default();
        let mut out: Vec<String> = moves.iter().skip(mine.saturating_sub(1)).map(|mv| move_line(mv, start)).collect();
        if m["status"].as_str() != Some("running") {
            out.push(match_outcome(m));
        }
        out.join("\n")
    });
    Ok(())
}

fn parse_tc(tc: &str) -> Result<Value> {
    let (base, inc) = tc.split_once('+').unwrap_or((tc, "0"));
    let (Ok(seconds), Ok(increment)) = (base.parse::<f64>(), inc.parse::<f64>()) else {
        bail!("a time control looks like 10+0.1 (seconds plus increment), not {tc}");
    };
    Ok(json!({ "type": "clock", "seconds": seconds, "increment": increment }))
}

/// The result in plain words: who won and why.
fn match_outcome(m: &Value) -> String {
    let who = |side: &str| {
        let p = &m[side];
        format!("{} ({})", if side == "white" { "White" } else { "Black" }, p["name"].as_str().unwrap_or("?"))
    };
    let winner = match m["result"].as_str() {
        Some("1-0") => Some(who("white")),
        Some("0-1") => Some(who("black")),
        _ => None,
    };
    let reason = m["reason"].as_str().unwrap_or("");
    match (winner, reason) {
        (_, "aborted") => "Stopped before the end".into(),
        (Some(w), "checkmate") => format!("{w} won by checkmate"),
        (Some(w), "time") => format!("{w} won on time"),
        (Some(w), "resignation") => format!("{w} won by adjudication"),
        (Some(w), "crash") => format!("{w} won: the other engine crashed"),
        (Some(w), "illegal_move") => format!("{w} won: the other engine played an illegal move"),
        (Some(w), "resigned") => format!("{w} won by resignation"),
        (Some(w), "abandoned") => format!("{w} won: the other side left"),
        (Some(w), "no_start") => format!("{w} won: the other side never moved"),
        (None, "agreement") => "Draw by agreement".into(),
        (None, "abandoned") => "Draw: a side left".into(),
        (Some(w), _) => format!("{w} won"),
        (None, "stalemate") => "Draw by stalemate".into(),
        (None, "repetition") => "Draw by threefold repetition".into(),
        (None, "fifty_moves") => "Draw by the fifty-move rule".into(),
        (None, "insufficient_material") => "Draw by insufficient material".into(),
        (None, "draw_adjudication") => "Draw by adjudication".into(),
        (None, _) if m["status"].as_str() == Some("running") => "Running".into(),
        (None, _) => "Draw".into(),
    }
}

fn time_text(t: &Value) -> String {
    match t["type"].as_str() {
        Some("clock") => format!("{}+{}", t["seconds"], t["increment"]),
        Some("nodes") => format!("{} nodes", t["nodes"]),
        Some("movetime") => format!("{} ms/move", t["ms"]),
        _ => "?".into(),
    }
}

fn match_line(m: &Value) -> String {
    let plies = m["plies"].as_u64().or_else(|| m["moves"].as_array().map(|a| a.len() as u64)).unwrap_or(0);
    format!(
        "match {} {} vs {}  {}  {}  {} plies  {}",
        m["id"],
        m["white"]["name"].as_str().unwrap_or("?"),
        m["black"]["name"].as_str().unwrap_or("?"),
        time_text(&m["time"]),
        m["status"].as_str().unwrap_or("?"),
        plies,
        if m["status"].as_str() == Some("running") { String::new() } else { match_outcome(m) },
    )
}

fn score_text(s: &Value) -> String {
    match (&s["cp"], &s["mate"]) {
        (Value::Number(cp), _) => format!("{:+.2}", cp.as_f64().unwrap_or(0.0) / 100.0),
        (_, Value::Number(m)) => format!("#{m}"),
        _ => "-".into(),
    }
}

fn clock_text(ms: &Value) -> String {
    ms.as_u64().map(|ms| format!("{:.1}s", ms as f64 / 1000.0)).unwrap_or_else(|| "-".into())
}

/// `1. e4` or `1... e5`, from the ply and the position the game started from.
fn move_line(mv: &Value, start_fen: &str) -> String {
    let ply = mv["ply"].as_u64().unwrap_or(1) as usize;
    let mut fields = start_fen.split_whitespace().skip(1);
    let black_first = start_fen != "startpos" && fields.next() == Some("b");
    let first: usize =
        if start_fen == "startpos" { 1 } else { fields.nth(3).and_then(|n| n.parse().ok()).unwrap_or(1) };
    let index = ply - 1 + usize::from(black_first);
    let label = format!("{}{}", first + index / 2, if index.is_multiple_of(2) { "." } else { "..." });
    format!(
        "{label:<6} {:<8} {:>7} d{:<3} {:>6.2}s   W {:>7}  B {:>7}",
        mv["san"].as_str().unwrap_or("?"),
        score_text(&mv["eval"]),
        mv["depth"].as_u64().unwrap_or(0),
        mv["time_ms"].as_f64().unwrap_or(0.0) / 1000.0,
        clock_text(&mv["clock_white_ms"]),
        clock_text(&mv["clock_black_ms"]),
    )
}

fn match_text(m: &Value) -> String {
    let start = m["start_fen"].as_str().unwrap_or("startpos");
    let mut out = match_line(m);
    if start != "startpos" {
        out += &format!("\n  from {start}");
    }
    for mv in m["moves"].as_array().into_iter().flatten() {
        out += &format!("\n  {}", move_line(mv, start));
    }
    out
}

async fn matches(client: &Client, cmd: MatchCommand, json: bool) -> Result<()> {
    match cmd.action {
        Some(MatchAction::Show { id }) => {
            let m = client.get(&format!("/api/matches/{id}")).await?;
            print(&m, json, match_text);
        }
        Some(MatchAction::Stop { id }) => {
            let m = client.delete(&format!("/api/matches/{id}")).await?;
            print(&m, json, match_line);
        }
        None => {
            let n = cmd.new;
            let time = match (&n.tc, n.nodes, n.movetime) {
                (Some(tc), _, _) => parse_tc(tc)?,
                (_, Some(nodes), _) => json!({ "type": "nodes", "nodes": nodes }),
                (_, _, Some(ms)) => json!({ "type": "movetime", "ms": ms }),
                _ => bail!("give a time control: --tc 10+0.1, --nodes N or --movetime MS"),
            };
            let opening = match (&n.fen, n.book) {
                (Some(fen), _) => json!({ "type": "position", "fen": fen, "moves": n.moves }),
                (_, true) => json!({ "type": "book" }),
                _ => json!({ "type": "start" }),
            };
            let body = json!({
                "white": resolve_seat(client, n.white.as_deref().ok_or_else(|| anyhow::anyhow!("--white is required"))?).await?,
                "black": resolve_seat(client, n.black.as_deref().ok_or_else(|| anyhow::anyhow!("--black is required"))?).await?,
                "time": time,
                "opening": opening,
                "hash": n.hash,
                "threads": n.threads,
                "adjudicate": n.adjudicate,
                "minimal": n.minimal,
            });
            let m = client.post("/api/matches", body).await?;
            if !n.follow {
                print(&m, json, match_line);
                return Ok(());
            }
            let id = m["id"].as_i64().unwrap_or_default();
            let start = m["start_fen"].as_str().unwrap_or("startpos").to_owned();
            // Progress goes to stderr under --json, so stdout stays one document.
            let say = |line: String| {
                if json {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
            };
            say(match_line(&m));
            for side in ["white", "black"].into_iter().filter(|s| m[*s]["human"].as_bool() == Some(true)) {
                say(format!("you play {side}: move with `chess move {id} <move>` (UCI or SAN) from another shell"));
            }
            follow_match(client, id, &start, &say).await?;
            let done = client.get(&format!("/api/matches/{id}")).await?;
            if json {
                print(&done, true, match_text);
            } else {
                println!("{}", match_outcome(&done));
            }
        }
    }
    Ok(())
}

/// Prints each move of a match as the event stream delivers it, until the match ends.
async fn follow_match(client: &Client, id: i64, start: &str, say: &dyn Fn(String)) -> Result<()> {
    let mut last = 0u64;
    let mut buffer = String::new();
    loop {
        let Ok(mut res) = client.events(last).await else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        while let Ok(Some(chunk)) = res.chunk().await {
            buffer += &String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
            while let Some(end) = buffer.find("\n\n") {
                let block: String = buffer.drain(..end + 2).collect();
                let (mut kind, mut data) = (String::new(), String::new());
                for line in block.lines() {
                    if let Some(v) = line.strip_prefix("id:") {
                        last = v.trim().parse().unwrap_or(last);
                    } else if let Some(v) = line.strip_prefix("event:") {
                        kind = v.trim().to_owned();
                    } else if let Some(v) = line.strip_prefix("data:") {
                        data += v.trim_start();
                    }
                }
                let Ok(event) = serde_json::from_str::<Value>(&data) else { continue };
                if event["id"].as_i64() != Some(id) {
                    continue;
                }
                match kind.as_str() {
                    "match.move" => say(move_line(&event, start)),
                    "match.takeback" => say(format!("taken back to {} plies", event["plies"])),
                    "match.ended" => return Ok(()),
                    _ => {}
                }
            }
        }
        // The stream dropped: the match may have ended meanwhile.
        let m = client.get(&format!("/api/matches/{id}")).await?;
        if m["status"].as_str() != Some("running") {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The Lichess bot
// ---------------------------------------------------------------------------------------------------------------------

fn lichess_text(s: &Value) -> String {
    let who = match s["account"]["username"].as_str() {
        Some(name) => match s["account"]["title"].as_str() {
            Some(title) => format!("{title} {name}"),
            None => name.to_owned(),
        },
        None if s["configured"].as_bool() == Some(true) => "token stored, account not checked yet".into(),
        None => "no token: store one with `chess lichess token`".into(),
    };
    let mut out = format!("lichess: {who}, {}", s["state"].as_str().unwrap_or("?"));
    if let Some(e) = s["error"].as_str() {
        out += &format!(" ({e})");
    }
    if s["account"]["bot"].as_bool() == Some(false) {
        out += "\n  not a BOT account: `chess lichess upgrade --yes-irreversible` converts it, for good";
    }
    let playing: Vec<String> = s["playing"].as_array().into_iter().flatten().map(|m| format!("match {m}")).collect();
    if !playing.is_empty() {
        out += &format!("\n  playing {}", playing.join(", "));
    }
    if let Some(n) = s["bot_games_today"].as_u64() {
        out += &format!("\n  {n}/100 games against bots today");
        if let Some(until) = s["bot_limit_until"].as_str() {
            out += &format!(": no bot games until {until} (people's challenges are still taken)");
        }
    }
    let seek = &s["seek"];
    match seek["mode"].as_str() {
        Some("once") => out += "\n  looking for a game",
        Some("always") => out += "\n  looking for games, again after each one",
        _ => {}
    }
    if let Some(p) = seek["pending"].as_object() {
        out += &format!(
            "\n  waiting on {} ({}) at {}+{}: {}",
            p["opponent"]["name"].as_str().unwrap_or("?"),
            p["opponent"]["rating"],
            p["clock"]["initial"].as_u64().unwrap_or(0) as f64 / 60.0,
            p["clock"]["increment"],
            p["url"].as_str().unwrap_or("?"),
        );
    }
    if let Some(note) = seek["note"].as_str() {
        out += &format!("\n  {note}");
    }
    out + "\n" + &lichess_settings_text(&s["settings"])
}

fn lichess_settings_text(s: &Value) -> String {
    let on = |key: &str| s[key].as_bool() == Some(true);
    let speeds: Vec<&str> = s["speeds"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let modes = match (on("accept_rated"), on("accept_casual")) {
        (true, true) => "rated and casual",
        (true, false) => "rated only",
        (false, true) => "casual only",
        (false, false) => "nothing",
    };
    let from = match (on("accept_humans"), on("accept_bots")) {
        (true, true) => "people and bots",
        (true, false) => "people only",
        (false, true) => "bots only",
        (false, false) => "nobody",
    };
    let clocks: Vec<&str> = s["seek_clocks"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    format!(
        "  build {}, hash {} MiB, {} thread(s), move overhead {} ms\n  accepts {modes} {} at {}-{}s +{}-{}s{}, from \
         {from}, {} game(s) at once\n  looks for {} games at {} against bots within {} points",
        s["build"].as_str().unwrap_or("(none chosen)"),
        s["hash"],
        s["threads"],
        s["move_overhead"],
        speeds.join("/"),
        s["min_initial"],
        s["max_initial"],
        s["min_increment"],
        s["max_increment"],
        if on("from_position") { ", from set positions too" } else { "" },
        s["max_games"],
        if on("seek_rated") { "rated" } else { "casual" },
        clocks.join(", "),
        s["seek_range"],
    )
}

/// `MIN-MAX` as two numbers.
fn parse_range(text: &str) -> Result<(u32, u32)> {
    let (lo, hi) = text.split_once('-').unwrap_or((text, text));
    match (lo.trim().parse(), hi.trim().parse()) {
        (Ok(lo), Ok(hi)) => Ok((lo, hi)),
        _ => bail!("a range looks like 60-1800, not {text}"),
    }
}

/// The settings with the flags applied over them.
fn apply_settings(mut s: Value, a: LichessSettings) -> Result<Value> {
    let mut set = |key: &str, value: Value| s[key] = value;
    if let Some(b) = a.build {
        set("build", json!(b));
    }
    if let Some(v) = a.hash {
        set("hash", json!(v));
    }
    if let Some(v) = a.threads {
        set("threads", json!(v));
    }
    if let Some(v) = a.move_overhead {
        set("move_overhead", json!(v));
    }
    if let Some(v) = a.rated {
        set("accept_rated", json!(v));
    }
    if let Some(v) = a.casual {
        set("accept_casual", json!(v));
    }
    if let Some(v) = a.speeds {
        set("speeds", json!(v.iter().map(|s| s.trim().to_lowercase()).collect::<Vec<_>>()));
    }
    if let Some(r) = a.initial {
        let (lo, hi) = parse_range(&r)?;
        set("min_initial", json!(lo));
        set("max_initial", json!(hi));
    }
    if let Some(r) = a.increment {
        let (lo, hi) = parse_range(&r)?;
        set("min_increment", json!(lo));
        set("max_increment", json!(hi));
    }
    if let Some(v) = a.from_position {
        set("from_position", json!(v));
    }
    if let Some(v) = a.humans {
        set("accept_humans", json!(v));
    }
    if let Some(v) = a.bots {
        set("accept_bots", json!(v));
    }
    if let Some(v) = a.max_games {
        set("max_games", json!(v));
    }
    if let Some(v) = a.seek_clocks {
        set("seek_clocks", json!(v.iter().map(|c| c.trim()).collect::<Vec<_>>()));
    }
    if let Some(v) = a.seek_rated {
        set("seek_rated", json!(v));
    }
    if let Some(v) = a.seek_range {
        set("seek_range", json!(v));
    }
    Ok(s)
}

fn bot_line(bot: &Value) -> String {
    let r = |speed: &str| bot["ratings"][speed].as_u64().map_or_else(|| "-".to_owned(), |n| n.to_string());
    format!(
        "{:<24} bullet {:>5}  blitz {:>5}  rapid {:>5}  classical {:>5}",
        bot["name"].as_str().unwrap_or("?"),
        r("bullet"),
        r("blitz"),
        r("rapid"),
        r("classical"),
    )
}

async fn lichess(client: &Client, cmd: LichessCommand, json: bool) -> Result<()> {
    let Some(action) = cmd.action else {
        print(&client.get("/api/lichess").await?, json, lichess_text);
        return Ok(());
    };
    match action {
        LichessAction::Token => {
            use std::io::{BufRead, IsTerminal};
            if std::io::stdin().is_terminal() {
                eprint!("Paste the bot's Lichess token (scope bot:play) and press Enter: ");
            }
            let mut token = String::new();
            std::io::stdin().lock().read_line(&mut token)?;
            let token = token.trim();
            if token.is_empty() {
                bail!("no token on stdin");
            }
            print(&client.put("/api/lichess/token", json!({ "token": token })).await?, json, lichess_text);
        }
        LichessAction::Logout => print(&client.delete("/api/lichess/token").await?, json, lichess_text),
        LichessAction::Settings(a) => {
            let current = client.get("/api/lichess/settings").await?;
            let stored = client.put("/api/lichess/settings", apply_settings(current, a)?).await?;
            print(&stored, json, lichess_settings_text);
        }
        LichessAction::Start => print(&client.post("/api/lichess/start", json!({})).await?, json, lichess_text),
        LichessAction::Stop => print(&client.post("/api/lichess/stop", json!({})).await?, json, lichess_text),
        LichessAction::Challenge { username, tc, rated, color } => {
            let (minutes, increment) = tc.split_once('+').unwrap_or((&tc, "0"));
            let (Ok(minutes), Ok(increment)) = (minutes.parse::<f64>(), increment.parse::<u32>()) else {
                bail!("a Lichess clock looks like 3+2 (minutes plus increment seconds), not {tc}");
            };
            let body = json!({
                "username": username,
                "clock_seconds": (minutes * 60.0).round() as u32,
                "increment": increment,
                "rated": rated,
                "color": color,
            });
            let sent = client.post("/api/lichess/challenge", body).await?;
            print(&sent, json, |c| {
                format!(
                    "challenged {} to {}+{} {}: {}",
                    c["username"].as_str().unwrap_or("?"),
                    c["clock_seconds"].as_u64().unwrap_or(0) as f64 / 60.0,
                    c["increment"],
                    if c["rated"].as_bool() == Some(true) { "rated" } else { "casual" },
                    c["url"].as_str().unwrap_or("?"),
                )
            });
        }
        LichessAction::Bots => print(&client.get("/api/lichess/bots").await?, json, |b| lines(b, bot_line)),
        LichessAction::Seek { action: Some(SeekAction::Stop), .. } => {
            print(&client.delete("/api/lichess/seek").await?, json, lichess_text);
        }
        LichessAction::Seek { always, action: None } => {
            let mode = if always { "always" } else { "once" };
            print(&client.post("/api/lichess/seek", json!({ "mode": mode })).await?, json, lichess_text);
        }
        LichessAction::Upgrade { yes_irreversible } => {
            if !yes_irreversible {
                bail!(
                    "upgrading to a BOT account cannot be undone, and only works for an account that has never played \
                     a game: pass --yes-irreversible to go ahead"
                );
            }
            print(&client.post("/api/lichess/upgrade", json!({ "confirm": true })).await?, json, lichess_text);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_reads_as_uci_or_san() {
        let start = Chess::default();
        assert_eq!(to_uci(&start, "e2e4").unwrap(), "e2e4");
        assert_eq!(to_uci(&start, "e4").unwrap(), "e2e4");
        assert_eq!(to_uci(&start, "Nf3").unwrap(), "g1f3");
        assert!(to_uci(&start, "e5").is_err());
        assert!(to_uci(&start, "e2e5").is_err());

        let pos = current_position(&json!({
            "start_fen": "r3k2r/1P6/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "moves": [],
        }))
        .unwrap();
        assert_eq!(to_uci(&pos, "O-O").unwrap(), "e1g1");
        assert_eq!(to_uci(&pos, "O-O-O").unwrap(), "e1c1");
        assert_eq!(to_uci(&pos, "e1g1").unwrap(), "e1g1");
        assert_eq!(to_uci(&pos, "bxa8=Q+").unwrap(), "b7a8q");
        assert_eq!(to_uci(&pos, "b8=N").unwrap(), "b7b8n");
    }

    #[test]
    fn the_position_is_after_the_last_move() {
        let m = json!({
            "start_fen": "startpos",
            "moves": [{ "uci": "e2e4", "fen_after": "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1" }],
        });
        let pos = current_position(&m).unwrap();
        assert_eq!(to_uci(&pos, "c5").unwrap(), "c7c5");
        assert_eq!(current_position(&json!({ "start_fen": "startpos", "moves": [] })).unwrap(), Chess::default());
    }
}
