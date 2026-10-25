//! The client commands: each is one API call (or a poll of one), printed as JSON with `--json` and as a short text
//! summary otherwise. Errors exit non-zero with the runner's message on stderr.

use std::time::Duration;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::client::{Client, print};

#[derive(Subcommand)]
pub enum ClientCommand {
    /// Show the runner's status.
    Status,
    /// Build a ref of the Reckless checkout (a branch, tag or commit) and bench it.
    Build {
        #[arg(value_name = "REF")]
        refname: String,
        /// Wait for the build and print it, instead of printing the queued job.
        #[arg(long)]
        wait: bool,
    },
    /// List the branches that can be built or tested, most recently committed first.
    Branches,
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
    /// List recent matches, newest first.
    Matches {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Experiments: a dev ref against a base ref through a funnel of stages.
    #[command(subcommand)]
    Experiment(ExperimentCommand),
    /// Several experiments submitted, waited for and read together.
    #[command(subcommand)]
    Batch(BatchCommand),
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
    /// White's build: an id, or a ref (the newest build of it).
    #[arg(long)]
    white: Option<String>,
    /// Black's build: an id, or a ref.
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
        ClientCommand::Match(cmd) => return matches(&client, cmd, json).await,
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
        ClientCommand::Build { refname, wait } => {
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
    )
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
        .find(|b| b["ref"].as_str() == Some(spec) || b["commit"].as_str().is_some_and(|c| c.starts_with(spec)))
        .and_then(|b| b["id"].as_i64())
        .ok_or_else(|| anyhow::anyhow!("no build of {spec}: build it first with `chess build {spec}`"))
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
                "white": resolve_build(client, n.white.as_deref().ok_or_else(|| anyhow::anyhow!("--white is required"))?).await?,
                "black": resolve_build(client, n.black.as_deref().ok_or_else(|| anyhow::anyhow!("--black is required"))?).await?,
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
