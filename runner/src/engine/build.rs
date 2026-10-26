//! Engine builds. A build is a commit compiled once with a given feature set, then benched; it is what gets
//! tested and analysed, never a loose binary. Builds are kept in the data directory and never rebuilt, unless their
//! binary was deleted to free space: the record stays, and the next build of the commit is a new one.

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
    /// `syzygy` with tablebase support (needs clang), else `no-syzygy`; `+spsa` when its tunables are UCI options.
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
    /// When its binary was deleted. The record stays, so the experiments, tests, tunes and matches that used it still
    /// read; a later build of the same commit is a new build.
    pub deleted_at: Option<String>,
    /// The tree and features: what the build is, and the folder its binary lives in.
    #[serde(skip)]
    pub key: String,
}

const COLUMNS: &str =
    "id, ref, commit_sha, subject, features, network, bench, bench_expected, nps, created_at, key, deleted_at";

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
        deleted_at: r.get(11)?,
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

/// The newest build of a ref (or of a commit prefix) that still has its binary.
pub fn newest_of(c: &Connection, spec: &str) -> Result<Option<Build>> {
    Ok(list(c)?.into_iter().find(|b| b.deleted_at.is_none() && (b.refname == spec || b.commit.starts_with(spec))))
}

/// What still needs a build's binary, in words, or `None` when nothing does: a running experiment or test that plays
/// it, a queued or running tune of it, or a match in progress. (A queued experiment has no builds yet; it builds when
/// it starts.) The Lichess bot's build is the caller's to check.
pub fn user(c: &Connection, id: i64) -> Result<Option<String>> {
    let found = |sql: &str| -> Result<Option<i64>> { Ok(c.query_row(sql, [id], |r| r.get(0)).optional()?) };
    if let Some(e) = found(
        "SELECT id FROM experiments WHERE status IN ('queued', 'running') AND (base_build = ?1 OR dev_build = ?1)",
    )? {
        return Ok(Some(format!("experiment {e} is using it")));
    }
    if let Some(t) = found("SELECT id FROM tests WHERE running = 1 AND (base_build = ?1 OR dev_build = ?1)")? {
        return Ok(Some(format!("test {t} is playing it")));
    }
    if let Some(t) = found("SELECT id FROM tunes WHERE status IN ('queued', 'running') AND build_id = ?1")? {
        return Ok(Some(format!("tune {t} is using it")));
    }
    if let Some(m) = found(
        "SELECT id FROM matches WHERE status = 'running'
         AND (json_extract(data, '$.white.build') = ?1 OR json_extract(data, '$.black.build') = ?1)",
    )? {
        return Ok(Some(format!("match {m} is playing it")));
    }
    Ok(None)
}

/// Deletes a build's binary and marks it deleted. Its key is renamed so a later build of the same tree is a new row.
/// Returns the bytes freed.
pub fn delete(c: &Connection, data: &Path, build: &Build) -> Result<u64> {
    let dir = data.join("builds").join(&build.key);
    let freed = size(data, build);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("delete {}", dir.display()))?;
    }
    mark_deleted(c, build.id)?;
    Ok(freed)
}

fn mark_deleted(c: &Connection, id: i64) -> Result<()> {
    c.execute(
        "UPDATE builds SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), key = key || '#deleted-' || id
         WHERE id = ?1 AND deleted_at IS NULL",
        [id],
    )?;
    Ok(())
}

/// The bytes a build's folder takes.
pub fn size(data: &Path, build: &Build) -> u64 {
    dir_size(&data.join("builds").join(&build.key))
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Where a build's binary lives.
pub fn binary(data: &Path, build: &Build) -> PathBuf {
    data.join("builds").join(&build.key).join(exe("reckless"))
}

/// Two commits with the same tree build the same binary: rewording a message (to add its `Bench:` line) never rebuilds.
fn key(tree: &str, features: &str) -> String {
    format!("{tree}-{features}")
}

/// The features a build is made with, as stored and as part of its key.
fn features(syzygy: bool, spsa: bool) -> String {
    let base = if syzygy { "syzygy" } else { "no-syzygy" };
    if spsa { format!("{base}+spsa") } else { base.to_owned() }
}

/// Whether a build has its tunables as UCI options.
pub fn is_spsa(build: &Build) -> bool {
    build.features.ends_with("+spsa")
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
    build_with(cx, refname, false).await
}

/// Builds `refname` with Reckless's `spsa` feature, which makes its declared tunables UCI options (for tuning). It
/// is a build of its own, cached apart from the plain one.
pub async fn build_spsa(cx: BuildContext<'_>, refname: &str) -> Result<Build> {
    build_with(cx, refname, true).await
}

async fn build_with(cx: BuildContext<'_>, refname: &str, spsa: bool) -> Result<Build> {
    let commit = git::resolve(cx.reckless, refname).await?;
    let syzygy = has_clang().await;
    let features = features(syzygy, spsa);
    let key = key(&commit.tree, &features);
    let existing = {
        cx.store
            .call({
                let key = key.clone();
                move |c| by_key(c, &key)
            })
            .await?
    };
    if let Some(build) = existing {
        if binary(cx.data, &build).exists() {
            (cx.log)(&format!("{} is already built (build {})", &commit.sha[..8], build.id));
            return Ok(build);
        }
        // Its binary went missing outside the runner: keep the record as deleted and build afresh.
        (cx.log)(&format!("build {}'s binary is missing: building again", build.id));
        cx.store.call(move |c| mark_deleted(c, build.id)).await?;
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
    if !syzygy {
        args.push("--no-default-features");
    }
    if spsa {
        args.extend(["--features", "spsa"]);
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

    fn insert(c: &Connection, key: &str) -> i64 {
        c.execute(
            "INSERT INTO builds (key, ref, commit_sha, subject, features, bench, nps)
             VALUES (?1, 'main', 'abcdef1234567890', 'A build', 'no-syzygy', 1, 1)",
            [key],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    #[tokio::test]
    async fn a_deleted_build_keeps_its_record_and_its_commit_builds_again() {
        let store = crate::store::Store::memory().unwrap();
        let data = std::env::temp_dir().join(format!("chess-build-delete-{}", std::process::id()));
        let dir = data.join("builds").join("tree-no-syzygy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(exe("reckless")), b"binary").unwrap();
        let (first, again, newest) = store
            .call(move |c| {
                let id = insert(c, "tree-no-syzygy");
                let b = get(c, id)?.unwrap();
                assert!(binary(&data, &b).exists());
                assert_eq!(delete(c, &data, &b)?, 6);
                assert!(!binary(&data, &get(c, id)?.unwrap()).exists());
                // The same tree builds again as a new row: the key is free.
                assert!(by_key(c, "tree-no-syzygy")?.is_none());
                let again = insert(c, "tree-no-syzygy");
                let _ = std::fs::remove_dir_all(&data);
                Ok((get(c, id)?.unwrap(), again, newest_of(c, "main")?.map(|b| b.id)))
            })
            .await
            .unwrap();
        assert!(first.deleted_at.is_some());
        assert_eq!(first.commit, "abcdef1234567890");
        assert_eq!(newest, Some(again));
    }

    #[tokio::test]
    async fn a_build_in_use_says_by_what() {
        let store = crate::store::Store::memory().unwrap();
        store
            .call(|c| {
                let id = insert(c, "k");
                assert_eq!(user(c, id)?, None);
                c.execute(
                    "INSERT INTO experiments (name, kind, base_ref, dev_ref, dev_build, funnel, status, results)
                     VALUES ('e', 'gainer', 'main', 'x', ?1, '[]', 'running', '[]')",
                    [id],
                )?;
                assert_eq!(user(c, id)?.as_deref(), Some("experiment 1 is using it"));
                c.execute("UPDATE experiments SET status = 'passed'", [])?;
                c.execute(
                    "INSERT INTO matches (status, data) VALUES ('running', json_object('white', json_object('build', ?1)))",
                    [id],
                )?;
                assert_eq!(user(c, id)?.as_deref(), Some("match 1 is playing it"));
                Ok(())
            })
            .await
            .unwrap();
    }

    #[test]
    fn spsa_builds_are_keyed_apart() {
        assert_eq!(features(true, false), "syzygy");
        assert_eq!(features(false, true), "no-syzygy+spsa");
        assert_ne!(key("t", &features(true, false)), key("t", &features(true, true)));
    }
}
