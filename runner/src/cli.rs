//! The client commands: each is one API call (or a poll of one), printed as JSON with `--json` and as a short text
//! summary otherwise. Errors exit non-zero with the runner's message on stderr.

use std::time::Duration;

use anyhow::{Result, bail};
use clap::Subcommand;
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
}

pub async fn run(client: Client, command: ClientCommand, json: bool) -> Result<()> {
    match command {
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
