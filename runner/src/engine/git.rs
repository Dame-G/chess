//! The Reckless checkout, read through git. Builds never touch the checkout itself: each builds in a worktree in the
//! data directory, so uncommitted work in the checkout is never built by accident and never disturbed.

use std::{collections::HashSet, path::Path};

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
    /// Hidden from the Versions view (`PUT /api/versions/{name}/hidden`). It can still be built and tested.
    pub hidden: bool,
}

/// Whether a branch name is upstream's (`upstream/main`) rather than the fork's own.
pub fn is_upstream(name: &str) -> bool {
    name.starts_with("upstream/")
}

/// The branch the checkout itself has checked out, if any.
pub async fn checked_out(repo: &Path) -> Option<String> {
    git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await.ok()
}

/// Whether the fork has a local branch of this name.
pub async fn has_branch(repo: &Path, name: &str) -> bool {
    git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).await.is_ok()
}

/// Which of these commits have `needle` in the file at `path`: one `git grep` over all their trees. A commit without
/// the file, or a failed grep, counts as not having it.
pub async fn commits_containing(repo: &Path, shas: &[&str], path: &str, needle: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    // Chunked, so a long list of branches stays well inside a command line.
    for chunk in shas.chunks(200) {
        let mut args = vec!["grep", "-l", "-F", "-e", needle];
        args.extend_from_slice(chunk);
        args.extend(["--", path]);
        if let Ok(out) = git(repo, &args).await {
            found.extend(out.lines().filter_map(|l| l.split_once(':').map(|(sha, _)| sha.to_owned())));
        }
    }
    found
}

/// Deletes a local branch, merged or not.
pub async fn delete_branch(repo: &Path, name: &str) -> Result<()> {
    git(repo, &["branch", "-D", "--", name]).await.map(drop)
}

/// Deletes a branch on `origin`, when it is there. Returns whether it was.
pub async fn delete_remote_branch(repo: &Path, name: &str) -> Result<bool> {
    let heads = git(repo, &["ls-remote", "--heads", "origin", &format!("refs/heads/{name}")]).await?;
    if heads.is_empty() {
        return Ok(false);
    }
    git(repo, &["push", "origin", "--delete", name]).await?;
    Ok(true)
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
                hidden: false,
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
