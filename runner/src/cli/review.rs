//! `chess review`, `chess compare` and `chess lichess players`: game review, comparing players, other players.

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use super::{job_line, lines, wait_for};
use crate::client::{Client, print};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ReviewCommand {
    #[command(subcommand)]
    action: Option<ReviewAction>,
    /// Matches to review, by id. Or --account / --player instead.
    matches: Vec<i64>,
    /// The newest unreviewed finished games of `me` (your Lichess account) or `bot`.
    #[arg(long)]
    account: Option<String>,
    /// The newest unreviewed imported games of this Lichess player.
    #[arg(long)]
    player: Option<String>,
    /// How many games for --account or --player (default 20).
    #[arg(long)]
    limit: Option<u32>,
    /// Only these speeds, comma-separated (bullet, blitz, rapid, classical, correspondence).
    #[arg(long, value_delimiter = ',')]
    speeds: Vec<String>,
    /// Only rated games.
    #[arg(long)]
    rated: bool,
    /// The build: an id or a ref (default: the newest build of main).
    #[arg(long)]
    build: Option<String>,
    /// Nodes per position (default 200000).
    #[arg(long)]
    nodes: Option<u64>,
    /// Engines at once, one thread each.
    #[arg(long)]
    concurrency: Option<u32>,
    /// Wait for the job; with a single match, print its review.
    #[arg(long)]
    wait: bool,
}

#[derive(Subcommand)]
pub enum ReviewAction {
    /// Print a game's review: each side's accuracy and counts, then every move.
    Show { id: i64 },
}

#[derive(Subcommand)]
pub enum PlayersAction {
    /// Import a Lichess player's newest public games, for comparison.
    Import {
        username: String,
        #[arg(long, default_value_t = 50)]
        max: u32,
        #[arg(long, value_delimiter = ',')]
        speeds: Vec<String>,
        #[arg(long)]
        rated: bool,
        #[arg(long)]
        wait: bool,
    },
    /// Lichess's leaderboard at a speed (ultraBullet, bullet, blitz, rapid, classical).
    Top {
        #[arg(default_value = "blitz")]
        perf: String,
        #[arg(long, default_value_t = 10)]
        nb: u32,
    },
    /// A Lichess player's profile and how many of their games are here.
    Show { username: String },
}

fn num(v: &Value) -> String {
    v.as_f64().map_or("-".into(), |x| format!("{x:.1}"))
}

fn counts_text(c: &Value) -> String {
    let n = |k: &str| c[k].as_u64().unwrap_or(0);
    format!(
        "!! {}  ! {}  best {}  excellent {}  good {}  book {}  ?! {}  ? {}  ?? {}  miss {}",
        n("brilliant"),
        n("great"),
        n("best"),
        n("excellent"),
        n("good"),
        n("book"),
        n("inaccuracy"),
        n("mistake"),
        n("blunder"),
        n("miss")
    )
}

fn review_text(r: &Value) -> String {
    let s = &r["summary"];
    let mut out = format!("review {} of match {} (build {}, {} nodes)", r["id"], r["match_id"], r["build"], r["nodes"]);
    for side in ["white", "black"] {
        let x = &s[side];
        out += &format!(
            "\n{side:<6} accuracy {}  acpl {}  {}",
            num(&x["accuracy"]),
            x["acpl"].as_u64().map_or("-".into(), |a| a.to_string()),
            counts_text(&x["counts"])
        );
    }
    for p in r["plies"].as_array().into_iter().flatten() {
        let ply = p["ply"].as_u64().unwrap_or(0);
        let number = if ply % 2 == 1 { format!("{}.", ply.div_ceil(2)) } else { format!("{}...", ply / 2) };
        let class = p["classification"].as_str().unwrap_or("?");
        let best = if p["uci"] == p["best_uci"] {
            String::new()
        } else {
            format!("  best {}", p["best_san"].as_str().unwrap_or("?"))
        };
        out += &format!(
            "\n{number:>6} {:<8} {class:<10} win {:>5} -> {:>5}  acc {:>5}{best}",
            p["san"].as_str().unwrap_or("?"),
            num(&p["win_before"]),
            num(&p["win_after"]),
            num(&p["accuracy"])
        );
    }
    out
}

pub async fn review(client: &Client, cmd: ReviewCommand, json: bool) -> Result<()> {
    if let Some(ReviewAction::Show { id }) = cmd.action {
        print(&client.get(&format!("/api/matches/{id}/review")).await?, json, review_text);
        return Ok(());
    }
    if cmd.matches.is_empty() && cmd.account.is_none() && cmd.player.is_none() {
        bail!("give match ids, or --account me, or --player <name>");
    }
    let build =
        cmd.build.as_ref().map(|b| b.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::from(b.clone())));
    let body = json!({
        "matches": cmd.matches,
        "account": cmd.account,
        "player": cmd.player,
        "limit": cmd.limit,
        "speeds": cmd.speeds,
        "rated": cmd.rated.then_some(true),
        "build": build,
        "nodes": cmd.nodes,
        "concurrency": cmd.concurrency,
    });
    let mut job = client.post("/api/reviews", body).await?;
    if !cmd.wait {
        print(&job, json, job_line);
        return Ok(());
    }
    let id = job["id"].as_i64().unwrap_or_default();
    job = wait_for(client, id, json).await?;
    if let [only] = cmd.matches[..] {
        print(&client.get(&format!("/api/matches/{only}/review")).await?, json, review_text);
        return Ok(());
    }
    print(&job, json, |j| {
        let r = &j["result"];
        format!(
            "{}: {} reviewed, {} skipped, {} failed ({} positions searched, {} cached)",
            job_line(j),
            r["reviewed"],
            r["skipped"],
            r["failed"],
            r["positions_searched"],
            r["positions_cached"]
        )
    });
    Ok(())
}

fn compare_text(c: &Value) -> String {
    let players: Vec<&Value> = c["players"].as_array().into_iter().flatten().collect();
    let mut out = String::new();
    let row = |label: &str, f: &dyn Fn(&Value) -> String| {
        let mut line = format!("{label:<22}");
        for p in &players {
            line += &format!("{:>18}", f(p));
        }
        line + "\n"
    };
    out += &row("", &|p| p["username"].as_str().or(p["player"].as_str()).unwrap_or("?").to_owned());
    out += &row("games (reviewed)", &|p| format!("{} ({})", p["games"], p["reviewed"]));
    out += &row("W/D/L", &|p| {
        let o = &p["results"]["overall"];
        format!("{}/{}/{}", o["wins"], o["draws"], o["losses"])
    });
    out += &row("score %", &|p| num(&p["results"]["overall"]["score"]));
    out += &row("accuracy", &|p| num(&p["accuracy"]["overall"]));
    for phase in ["opening", "middlegame", "endgame"] {
        out += &row(&format!("  {phase}"), &|p| num(&p["accuracy"][phase]));
    }
    out += &row("acpl", &|p| num(&p["acpl"]));
    for k in ["brilliant", "great", "best", "inaccuracy", "mistake", "blunder", "miss"] {
        out += &row(&format!("{k} / game"), &|p| format!("{:.2}", p["per_game"][k].as_f64().unwrap_or(0.0)));
    }
    out += &row("opponents avg", &|p| num(&p["opponents"]["average_rating"]));
    out += &row("performance", &|p| num(&p["opponents"]["performance"]));
    out += &row("avg move (s)", &|p| {
        p["time"]["average_ms"].as_f64().map_or("-".into(), |ms| format!("{:.1}", ms / 1000.0))
    });
    out += &row("time-trouble ??", &|p| p["time"]["time_trouble_blunders"].to_string());
    out += &row("longest win streak", &|p| p["longest_win_streak"].to_string());
    out += &row("top opening", &|p| {
        p["openings"][0]["name"].as_str().map_or("-".into(), |n| n.chars().take(17).collect())
    });
    out.trim_end().to_owned()
}

pub async fn compare(
    client: &Client,
    players: &[String],
    speeds: &[String],
    rated: bool,
    since: Option<String>,
    limit: Option<u32>,
    json: bool,
) -> Result<()> {
    let mut path = format!("/api/compare?players={}", players.join(","));
    if !speeds.is_empty() {
        path += &format!("&speeds={}", speeds.join(","));
    }
    if rated {
        path += "&rated=true";
    }
    if let Some(s) = since {
        path += &format!("&since={s}");
    }
    if let Some(l) = limit {
        path += &format!("&limit={l}");
    }
    print(&client.get(&path).await?, json, compare_text);
    Ok(())
}

pub async fn players(client: &Client, action: PlayersAction, json: bool) -> Result<()> {
    match action {
        PlayersAction::Import { username, max, speeds, rated, wait } => {
            let body = json!({ "max": max, "speeds": speeds, "rated": rated.then_some(true) });
            let mut job = client.post(&format!("/api/lichess/players/{username}/import"), body).await?;
            if wait {
                let id = job["id"].as_i64().unwrap_or_default();
                job = wait_for(client, id, json).await?;
            }
            print(&job, json, |j| {
                let r = &j["result"];
                if r.is_object() {
                    format!("{}: {} new games, {} already here", job_line(j), r["imported"], r["skipped"])
                } else {
                    job_line(j)
                }
            });
        }
        PlayersAction::Top { perf, nb } => {
            let list = client.get(&format!("/api/lichess/players/top?perf={perf}&nb={nb}")).await?;
            print(&list, json, |l| {
                lines(l, |p| {
                    let title = p["title"].as_str().map(|t| format!("{t} ")).unwrap_or_default();
                    format!("{:>5}  {title}{}", p["rating"], p["username"].as_str().unwrap_or("?"))
                })
            });
        }
        PlayersAction::Show { username } => {
            let p = client.get(&format!("/api/lichess/players/{username}")).await?;
            print(&p, json, |p| {
                let mut out = format!(
                    "{}{}  {}",
                    p["title"].as_str().map(|t| format!("{t} ")).unwrap_or_default(),
                    p["username"].as_str().unwrap_or("?"),
                    p["url"].as_str().unwrap_or("")
                );
                for perf in ["bullet", "blitz", "rapid", "classical"] {
                    let r = &p["ratings"][perf];
                    if r.is_object() {
                        let prov = if r["provisional"].as_bool() == Some(true) { "?" } else { "" };
                        out += &format!("\n{perf:<10} {}{prov} ({} games)", r["rating"], r["games"]);
                    }
                }
                out + &format!("\nhere: {} games imported, {} reviewed", p["imported"], p["reviewed"])
            });
        }
    }
    Ok(())
}
