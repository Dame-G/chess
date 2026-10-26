//! The Reckless checkout, read through git. Builds never touch the checkout itself: each builds in a worktree in the
//! data directory, so uncommitted work in the checkout is never built by accident and never disturbed.

use std::path::Path;

use anyhow::{Context, Result};

use crate::proc;

pub struct Commit {
    pub sha: String,
    /// The tree: what was committed, whatever the message says. Builds are keyed by it.
    pub tree: String,
    pub subject: String,
    /// The node count the commit message claims (`Bench: N`), Reckless's functional fingerprint.
    pub bench: Option<u64>,
}

pub async fn resolve(repo: &Path, refname: &str) -> Result<Commit> {
    let spec = format!("{refname}^{{commit}}");
    let sha = git(repo, &["rev-parse", "--verify", "--quiet", &spec])
        .await
        .map_err(|_| anyhow::anyhow!("no commit named {refname} in {}", repo.display()))?;
    let message = git(repo, &["log", "-1", "--format=%B", &sha]).await?;
    let tree = git(repo, &["rev-parse", &format!("{sha}^{{tree}}")]).await?;
    Ok(Commit {
        subject: message.lines().next().unwrap_or_default().to_owned(),
        bench: bench_line(&message),
        sha,
        tree,
    })
}

/// The last `Bench: N` line of a commit message.
pub fn bench_line(message: &str) -> Option<u64> {
    message.lines().rev().find_map(|l| l.trim().strip_prefix("Bench:")?.trim().parse().ok())
}

/// A branch of the checkout, newest commit first.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct Branch {
    /// The name to build or test, such as `main` or `upstream/main`.
    pub name: String,
    pub sha: String,
    pub subject: String,
    /// When its commit was made, ISO 8601.
    pub date: String,
    /// The node count the commit message claims (`Bench: N`).
    pub bench: Option<u64>,
}

/// The checkout's own branches and upstream's, most recently committed first.
pub async fn branches(repo: &Path) -> Result<Vec<Branch>> {
    let format =
        "%(refname:short)%00%(objectname)%00%(committerdate:iso-strict)%00%(contents:subject)%00%(contents)%1e";
    let out = git(
        repo,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            &format!("--format={format}"),
            "refs/heads",
            "refs/remotes/upstream",
        ],
    )
    .await?;
    Ok(out
        .split('\u{1e}')
        .filter_map(|record| {
            let mut f = record.trim_start_matches('\n').split('\0');
            let (name, sha, date, subject, message) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
            // `upstream` alone is the remote's HEAD, an alias of one of its branches.
            (!name.is_empty() && name != "upstream" && !name.ends_with("/HEAD")).then(|| Branch {
                name: name.to_owned(),
                sha: sha.to_owned(),
                subject: subject.to_owned(),
                date: date.to_owned(),
                bench: bench_line(message),
            })
        })
        .collect())
}

/// Checks `sha` out in the build worktree, creating it on first use.
pub async fn checkout(repo: &Path, worktree: &Path, sha: &str) -> Result<()> {
    if worktree.join(".git").exists() {
        if git(worktree, &["checkout", "--detach", "--force", sha]).await.is_ok() {
            return Ok(());
        }
        // A worktree whose checkout moved or was deleted: start again.
        let _ = std::fs::remove_dir_all(worktree);
    }
    git(repo, &["worktree", "prune"]).await?;
    let path = worktree.to_string_lossy();
    git(repo, &["worktree", "add", "--detach", "--force", &path, sha]).await?;
    Ok(())
}

/// Runs git in `dir` and returns its trimmed output.
pub async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec!["-C", dir.to_str().context("non-UTF-8 path")?];
    all.extend_from_slice(args);
    Ok(proc::output(Path::new("git"), &all, None).await?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_bench_line() {
        assert_eq!(bench_line("Minor cleanups (#1123)\n\nNo functional change.\n\nBench: 2837905\n"), Some(2837905));
        assert_eq!(bench_line("Bench: 1\n\nBench: 2"), Some(2));
        assert_eq!(bench_line("No bench here"), None);
    }
}
