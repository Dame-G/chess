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
