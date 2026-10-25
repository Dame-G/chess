//! Engine builds. A build is a commit compiled once with a given feature set, then benched; it is what gets
//! tested and analysed, never a loose binary. Builds are kept in the data directory and never rebuilt.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::git;
use crate::proc;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Build {
    pub id: i64,
    /// The ref that was asked for, such as a branch name.
    #[serde(rename = "ref")]
    pub refname: String,
    pub commit: String,
    pub subject: String,
    /// `syzygy` with tablebase support (needs clang), else `no-syzygy`.
    pub features: String,
    /// The network file the build embeds.
    pub network: Option<String>,
    /// Nodes searched by `bench`: the functional fingerprint.
    pub bench: u64,
    /// The `Bench:` line of the commit message, when it has one. A build whose bench differs from it is suspect.
    pub bench_expected: Option<u64>,
    /// Nodes per second during `bench` on this machine.
    pub nps: u64,
    pub created_at: String,
    /// The tree and features: what the build is, and the folder its binary lives in.
    #[serde(skip)]
    pub key: String,
}

const COLUMNS: &str = "id, ref, commit_sha, subject, features, network, bench, bench_expected, nps, created_at, key";

fn from_row(r: &Row) -> rusqlite::Result<Build> {
    Ok(Build {
        id: r.get(0)?,
        refname: r.get(1)?,
        commit: r.get(2)?,
        subject: r.get(3)?,
        features: r.get(4)?,
        network: r.get(5)?,
        bench: r.get::<_, i64>(6)? as u64,
        bench_expected: r.get::<_, Option<i64>>(7)?.map(|n| n as u64),
        nps: r.get::<_, i64>(8)? as u64,
        created_at: r.get(9)?,
        key: r.get(10)?,
    })
}

pub fn list(c: &Connection) -> Result<Vec<Build>> {
    let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM builds ORDER BY id DESC"))?;
    Ok(stmt.query_map([], from_row)?.collect::<rusqlite::Result<_>>()?)
}

pub fn get(c: &Connection, id: i64) -> Result<Option<Build>> {
    Ok(c.query_row(&format!("SELECT {COLUMNS} FROM builds WHERE id = ?1"), [id], from_row).optional()?)
}

fn by_key(c: &Connection, key: &str) -> Result<Option<Build>> {
    Ok(c.query_row(&format!("SELECT {COLUMNS} FROM builds WHERE key = ?1"), [key], from_row).optional()?)
}

/// Where a build's binary lives.
pub fn binary(data: &Path, build: &Build) -> PathBuf {
    data.join("builds").join(&build.key).join(exe("reckless"))
}

/// Two commits with the same tree build the same binary: rewording a message (to add its `Bench:` line) never rebuilds.
fn key(tree: &str, features: &str) -> String {
    format!("{tree}-{features}")
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

pub struct BuildContext<'a> {
    pub data: &'a Path,
    pub reckless: &'a Path,
    pub store: &'a crate::store::Store,
    pub cancel: &'a CancellationToken,
    pub log: &'a (dyn Fn(&str) + Sync),
}

/// Builds `refname`, or returns the existing build of the same commit and features.
pub async fn build(cx: BuildContext<'_>, refname: &str) -> Result<Build> {
    let commit = git::resolve(cx.reckless, refname).await?;
    let features = if has_clang().await { "syzygy" } else { "no-syzygy" };
    let key = key(&commit.tree, features);
    let existing = {
        cx.store
            .call({
                let key = key.clone();
                move |c| by_key(c, &key)
            })
            .await?
    };
    if let Some(build) = existing {
        (cx.log)(&format!("{} is already built (build {})", &commit.sha[..8], build.id));
        return Ok(build);
    }

    let worktree = cx.data.join("worktree");
    (cx.log)(&format!("checking out {} {}", &commit.sha[..8], commit.subject));
    git::checkout(cx.reckless, &worktree, &commit.sha).await?;

    // Networks are downloaded once and shared by every build that embeds the same file.
    let network = network_name(&worktree);
    let nets = cx.data.join("networks");
    std::fs::create_dir_all(&nets)?;
    let mut env: Vec<(&str, String)> = vec![("CARGO_TARGET_DIR", cx.data.join("target").display().to_string())];
    if let Some(name) = &network {
        let cached = nets.join(name);
        let in_checkout = cx.reckless.join("networks").join(name);
        if !cached.exists() && in_checkout.exists() {
            std::fs::copy(&in_checkout, &cached)?;
        }
        if cached.exists() {
            env.push(("EVALFILE", cached.display().to_string()));
        }
    }

    let mut args = vec!["build", "--release", "--bin", "reckless"];
    if features == "no-syzygy" {
        args.push("--no-default-features");
    }
    (cx.log)(&format!("cargo {}", args.join(" ")));
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut child = proc::spawn(proc::Spawn {
        program: Path::new("cargo"),
        args: &args,
        cwd: Some(&worktree),
        env: &env_refs,
        stdin: false,
    })?;
    let mut lines = proc::lines(&mut child);
    loop {
        tokio::select! {
            line = lines.recv() => match line {
                Some(line) => (cx.log)(&line),
                None => break,
            },
            _ = cx.cancel.cancelled() => bail!("cancelled"),
        }
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("cargo build failed ({status})");
    }

    if let Some(name) = &network {
        let downloaded = worktree.join("networks").join(name);
        if !nets.join(name).exists() && downloaded.exists() {
            std::fs::copy(&downloaded, nets.join(name))?;
        }
    }

    let dir = cx.data.join("builds").join(&key);
    std::fs::create_dir_all(&dir)?;
    let bin = dir.join(exe("reckless"));
    std::fs::copy(cx.data.join("target").join("release").join(exe("reckless")), &bin).context("copy the binary")?;

    (cx.log)("bench");
    let (bench, nps) = bench(&bin).await?;
    match commit.bench {
        Some(expected) if expected != bench => {
            (cx.log)(&format!("bench {bench} differs from the commit's Bench: {expected}"))
        }
        Some(_) => (cx.log)(&format!("bench {bench} matches the commit")),
        None => (cx.log)(&format!("bench {bench} (the commit has no Bench: line)")),
    }

    let refname = refname.to_owned();
    cx.store
        .call(move |c| {
            c.execute(
                "INSERT INTO builds (key, ref, commit_sha, subject, features, network, bench, bench_expected, nps)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    key,
                    refname,
                    commit.sha,
                    commit.subject,
                    features,
                    network,
                    bench as i64,
                    commit.bench.map(|n| n as i64),
                    nps as i64
                ],
            )?;
            Ok(get(c, c.last_insert_rowid())?.expect("just inserted"))
        })
        .await
}

/// `reckless bench`: its node count and speed.
pub async fn bench(bin: &Path) -> Result<(u64, u64)> {
    let out = proc::output(bin, &["bench"], None).await?;
    parse_bench(&out).with_context(|| format!("no Bench line in: {out}"))
}

fn parse_bench(out: &str) -> Option<(u64, u64)> {
    let line = out.lines().find_map(|l| l.trim().strip_prefix("Bench:"))?;
    let words: Vec<&str> = line.split_whitespace().collect();
    Some((words.first()?.parse().ok()?, words.get(2)?.parse().ok()?))
}

/// The network file a checkout embeds by default, from its build script.
fn network_name(worktree: &Path) -> Option<String> {
    let script = std::fs::read_to_string(worktree.join("build").join("build.rs")).ok()?;
    let line = script.lines().find(|l| l.contains("NETWORK_NAME") && l.contains('='))?;
    Some(line.split('"').nth(1)?.to_owned())
}

async fn has_clang() -> bool {
    proc::output(Path::new("clang"), &["--version"], None).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bench_output() {
        assert_eq!(parse_bench("Bench: 2837905 nodes 352413 nps\n"), Some((2837905, 352413)));
        assert_eq!(parse_bench("nothing"), None);
    }
}
