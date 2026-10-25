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

async fn git(dir: &Path, args: &[&str]) -> Result<String> {
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
