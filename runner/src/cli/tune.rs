//! `chess tune`: SPSA tunes of a build's declared tunables.

use std::time::Duration;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::{Value, json};

use super::{lines, wait_for};
use crate::client::{Client, print};

#[derive(Subcommand)]
pub enum TuneCommand {
    /// Queue a tune: the ref is built with the spsa feature, then each iteration plays θ+ against θ−.
    New {
        /// The ref to tune, or the id of a build whose commit to tune.
        #[arg(long)]
        build: String,
        /// The parameters to tune, comma-separated. Default: every tunable the engine declares.
        #[arg(long, value_delimiter = ',')]
        params: Vec<String>,
        #[arg(long, default_value_t = 1000)]
        iterations: u32,
        /// Game pairs per iteration (OpenBench's default is 8).
        #[arg(long, default_value_t = 8)]
        pairs: u32,
        /// The games' time control and hash: vstc, stc, ltc, stc-hash, ltc-hash, smp or nodes.
        #[arg(long, default_value = "stc")]
        preset: String,
        /// Nodes per move instead of the preset's clock.
        #[arg(long)]
        nodes: Option<u64>,
        /// Hash in MiB instead of the preset's.
        #[arg(long)]
        hash: Option<u32>,
        /// Games at once. Default: the machine's threads less two.
        #[arg(long)]
        concurrency: Option<u32>,
        #[arg(long)]
        name: Option<String>,
        /// Wait for the tune to finish, then show it.
        #[arg(long)]
        wait: bool,
    },
    /// List tunes, newest first.
    List {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Show a tune: each parameter's start and current value, and the last iterations.
    Show { id: i64 },
    /// Wait for a tune to finish, then show it. Exits non-zero unless it is done.
    Wait { id: i64 },
    /// Cancel a queued tune or stop a running one (its iterations are kept).
    Cancel { id: i64 },
    /// Carry on a failed or cancelled tune from its last iteration.
    Resume { id: i64 },
    /// Bake a tune's current values into a new branch of the fork, with its Bench: line, and queue a gainer
    /// experiment of it at SPRT [0, 4].
    Apply {
        id: i64,
        /// The branch to create. Default tune/<id>.
        #[arg(long)]
        branch: Option<String>,
        /// The experiment's baseline.
        #[arg(long, default_value = "main")]
        base: String,
        /// Only make the branch; queue no experiment.
        #[arg(long)]
        no_experiment: bool,
        /// Add an LTC [0, 4] confirmation.
        #[arg(long)]
        ltc: bool,
        /// Push the branch to the fork's origin.
        #[arg(long)]
        push: bool,
        /// Wait for the branch to be made and print it.
        #[arg(long)]
        wait: bool,
    },
    /// List the tunables an spsa build declares (a tune's build).
    Tunables { build: i64 },
}

pub async fn run(client: &Client, cmd: TuneCommand, json: bool) -> Result<()> {
    match cmd {
        TuneCommand::New { build, params, iterations, pairs, preset, nodes, hash, concurrency, name, wait } => {
            let params: Vec<Value> = params.iter().map(|p| json!({ "name": p })).collect();
            let body = json!({
                "build": build, "params": params, "iterations": iterations, "pairs_per_iteration": pairs,
                "preset": preset, "nodes": nodes, "hash": hash, "concurrency": concurrency, "name": name,
            });
            let t = client.post("/api/tunes", body).await?;
            if wait {
                let detail = wait_tune(client, t["id"].as_i64().unwrap_or_default()).await?;
                print(&detail, json, tune_text);
            } else {
                print(&t, json, tune_line);
            }
        }
        TuneCommand::List { limit } => {
            let list = client.get(&format!("/api/tunes?limit={limit}")).await?;
            print(&list, json, |l| lines(l, tune_line));
        }
        TuneCommand::Show { id } => {
            let detail = client.get(&format!("/api/tunes/{id}")).await?;
            print(&detail, json, tune_text);
        }
        TuneCommand::Wait { id } => {
            let detail = wait_tune(client, id).await?;
            print(&detail, json, tune_text);
            if detail["tune"]["status"].as_str() != Some("done") {
                bail!("tune {id} {}", detail["tune"]["status"].as_str().unwrap_or_default());
            }
        }
        TuneCommand::Cancel { id } => print(&client.delete(&format!("/api/tunes/{id}")).await?, json, tune_line),
        TuneCommand::Resume { id } => {
            print(&client.post(&format!("/api/tunes/{id}/resume"), json!({})).await?, json, tune_line)
        }
        TuneCommand::Apply { id, branch, base, no_experiment, ltc, push, wait } => {
            let body =
                json!({ "branch": branch, "base": base, "experiment": !no_experiment, "ltc": ltc, "push": push });
            let job = client.post(&format!("/api/tunes/{id}/apply"), body).await?;
            if wait {
                let job = wait_for(client, job["id"].as_i64().unwrap_or_default(), json).await?;
                print(&job["result"], json, applied_text);
            } else {
                print(&job, json, |j| format!("job {} applies tune {id}: chess wait {}", j["id"], j["id"]));
            }
        }
        TuneCommand::Tunables { build } => {
            let list = client.get(&format!("/api/builds/{build}/tunables")).await?;
            print(&list, json, |l| lines(l, param_line));
        }
    }
    Ok(())
}

async fn wait_tune(client: &Client, id: i64) -> Result<Value> {
    loop {
        let detail = client.get(&format!("/api/tunes/{id}")).await?;
        if matches!(detail["tune"]["status"].as_str(), Some("done" | "failed" | "cancelled")) {
            return Ok(detail);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn tune_line(t: &Value) -> String {
    let tail = t["summary"].as_str().map(|s| format!(": {s}")).unwrap_or_default();
    format!(
        "tune {} {} ({}) {} {}/{} iterations, {} params{tail}",
        t["id"],
        t["name"].as_str().unwrap_or("?"),
        t["ref"].as_str().unwrap_or("?"),
        t["status"].as_str().unwrap_or("?"),
        t["iteration"],
        t["iterations"],
        t["params"].as_array().map_or(0, Vec::len),
    )
}

fn tune_text(d: &Value) -> String {
    let t = &d["tune"];
    let mut out = vec![tune_line(t)];
    for p in t["params"].as_array().into_iter().flatten() {
        let (start, value) = (p["start"].as_f64().unwrap_or_default(), p["value"].as_f64().unwrap_or_default());
        out.push(format!(
            "  {:<24} {:>8} -> {:>10.2} ({:+.2})  [{}, {}]",
            p["name"].as_str().unwrap_or("?"),
            start,
            value,
            value - start,
            p["min"],
            p["max"]
        ));
    }
    let history = d["history"].as_array().cloned().unwrap_or_default();
    for i in history.iter().rev().take(5).rev() {
        out.push(format!("  iteration {}: {} {:+}", i["iteration"], i["penta"], i["result"].as_i64().unwrap_or(0)));
    }
    out.join("\n")
}

fn applied_text(r: &Value) -> String {
    let experiment = r["experiment"].as_i64().map(|e| format!(", experiment {e} queued")).unwrap_or_default();
    format!(
        "{} at {} with Bench: {}{experiment}",
        r["branch"].as_str().unwrap_or("?"),
        r["commit"].as_str().unwrap_or("?").get(..8).unwrap_or("?"),
        r["bench"]
    )
}

fn param_line(p: &Value) -> String {
    format!(
        "{:<24} {:>8}  [{}, {}]  c_end {}  r_end {}",
        p["name"].as_str().unwrap_or("?"),
        p["start"],
        p["min"],
        p["max"],
        p["c_end"],
        p["r_end"]
    )
}
