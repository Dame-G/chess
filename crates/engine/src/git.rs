//! The fork's checkout, through git: branches, commits and their `Bench:` lines, build worktrees, the branches a tune
//! writes, promoting a branch into `main` and pushing.
//!
//! Builds never touch the user's checkout: each builds in a worktree under a directory the caller gives, so uncommitted
//! work in the checkout is never built by accident and never disturbed. Writing a branch ([`Repo::commit_files`],
//! [`Repo::reword`]) uses git's plumbing with a private index, so it needs no worktree at all.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::proc;

/// The id git gives "no object", for `update-ref`'s check that a ref does not exist yet.
const NO_COMMIT: &str = "0000000000000000000000000000000000000000";

/// One commit, as a build or a version needs it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    /// The tree: what was committed, whatever the message says. Builds are keyed by it, so rewording a commit (to add
    /// its `Bench:` line) never rebuilds.
    pub tree: String,
    pub subject: String,
    pub message: String,
    /// The node count the commit message claims (`Bench: N`), Reckless's functional fingerprint.
    pub bench: Option<u64>,
    /// When it was committed, ISO 8601.
    pub date: String,
}

/// A branch of the checkout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    /// The name to build or test, such as `main` or `upstream/main`.
    pub name: String,
    pub sha: String,
    pub subject: String,
    /// When its commit was made, ISO 8601.
    pub date: String,
    /// The node count the commit message claims (`Bench: N`).
    pub bench: Option<u64>,
    /// Upstream's (`upstream/…`) rather than the fork's own.
    pub upstream: bool,
}

/// A worktree of the repository, as `git worktree list` gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// The branch checked out there (without `refs/heads/`), or `None` when detached.
    pub branch: Option<String>,
}

/// What [`Repo::promote`] does when the target branch is checked out in some worktree (usually the user's checkout),
/// since moving a checked-out branch's ref would leave that worktree's files behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedOut {
    /// Fail, naming the worktree.
    Refuse,
    /// Fast-forward that worktree with `git merge --ff-only` when it has no uncommitted changes to tracked files, as if
    /// the merge had been made there; fail otherwise.
    FastForwardIfClean,
}

/// A request to merge a branch into another (promoting an idea into `main`).
#[derive(Clone, Debug)]
pub struct Promote<'a> {
    /// The branch (or any commit) to merge.
    pub branch: &'a str,
    /// The branch it goes into, usually `main`.
    pub into: &'a str,
    /// The merge commit's message, when a fast-forward is not possible. The caller includes its `Bench:` line.
    pub message: &'a str,
    /// A scratch worktree the merge is made in, under the caller's directory. It is created or reused.
    pub worktree: &'a Path,
    pub checked_out: CheckedOut,
}

/// What a promotion did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Promoted {
    /// The new tip of the target branch.
    pub commit: String,
    /// Its tip before.
    pub previous: String,
    /// Whether the target was simply moved forward to the branch (no merge commit).
    pub fast_forward: bool,
}

/// A new branch made of one commit on top of another, with some files replaced (a tune's new defaults).
#[derive(Clone, Debug)]
pub struct CommitFiles<'a> {
    /// The new branch. It must not exist yet.
    pub branch: &'a str,
    /// The commit it starts from.
    pub from: &'a str,
    /// Paths relative to the repository root, `/`-separated, and their new contents.
    pub files: &'a [(&'a str, &'a [u8])],
    pub message: &'a str,
}

/// The last `Bench: N` line of a commit message.
pub fn bench_line(message: &str) -> Option<u64> {
    message.lines().rev().find_map(|l| l.trim().strip_prefix("Bench:")?.trim().parse().ok())
}

/// Whether a branch name is upstream's (`upstream/main`) rather than the fork's own.
pub fn is_upstream(name: &str) -> bool {
    name.starts_with("upstream/")
}

/// A git repository (the fork's checkout).
#[derive(Clone, Debug)]
pub struct Repo {
    pub path: PathBuf,
}

impl Repo {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Runs git in the repository and returns its trimmed output.
    pub async fn git(&self, args: &[&str]) -> Result<String> {
        git_in(&self.path, args, &[]).await
    }

    /// The commit a ref names (a branch, a tag, a sha or a prefix of one).
    pub async fn resolve(&self, refname: &str) -> Result<Commit> {
        let spec = format!("{refname}^{{commit}}");
        let sha = self
            .git(&["rev-parse", "--verify", "--quiet", &spec])
            .await
            .map_err(|_| anyhow::anyhow!("no commit named {refname} in {}", self.path.display()))?;
        let out = self.git(&["log", "-1", "--format=%T%x00%cI%x00%B", &sha]).await?;
        let mut parts = out.splitn(3, '\0');
        let (tree, date, message) =
            (parts.next().unwrap_or_default(), parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
        Ok(Commit {
            subject: message.lines().next().unwrap_or_default().to_owned(),
            bench: bench_line(message),
            message: message.trim_end().to_owned(),
            tree: tree.to_owned(),
            date: date.to_owned(),
            sha,
        })
    }

    /// The fork's own branches and upstream's, most recently committed first.
    pub async fn branches(&self) -> Result<Vec<Branch>> {
        let format =
            "%(refname:short)%00%(objectname)%00%(committerdate:iso-strict)%00%(contents:subject)%00%(contents)%1e";
        let out = self
            .git(&[
                "for-each-ref",
                "--sort=-committerdate",
                &format!("--format={format}"),
                "refs/heads",
                "refs/remotes/upstream",
            ])
            .await?;
        Ok(parse_branches(&out))
    }

    /// The branch the checkout itself has checked out, if any.
    pub async fn checked_out(&self) -> Option<String> {
        self.git(&["symbolic-ref", "--quiet", "--short", "HEAD"]).await.ok()
    }

    /// Whether the fork has a local branch of this name.
    pub async fn has_branch(&self, name: &str) -> bool {
        self.git(&["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).await.is_ok()
    }

    /// Deletes a local branch, merged or not.
    pub async fn delete_branch(&self, name: &str) -> Result<()> {
        self.git(&["branch", "-D", "--", name]).await.map(drop)
    }

    /// Deletes a branch on `origin`, when it is there. Returns whether it was.
    pub async fn delete_remote_branch(&self, name: &str) -> Result<bool> {
        let heads = self.git(&["ls-remote", "--heads", "origin", &format!("refs/heads/{name}")]).await?;
        if heads.is_empty() {
            return Ok(false);
        }
        self.git(&["push", "origin", "--delete", name]).await?;
        Ok(true)
    }

    /// Pushes a local branch to `origin` (and sets it as the branch's upstream). Only ever called on an explicit
    /// request: nothing here pushes by itself.
    pub async fn push(&self, branch: &str) -> Result<()> {
        self.git(&["push", "-u", "origin", &format!("refs/heads/{branch}:refs/heads/{branch}")]).await.map(drop)
    }

    /// Which of these commits have `needle` in the file at `path`: one `git grep` over all their trees. A commit without
    /// the file, or a failed grep, counts as not having it.
    pub async fn commits_containing(
        &self,
        shas: &[&str],
        path: &str,
        needle: &str,
    ) -> std::collections::HashSet<String> {
        let mut found = std::collections::HashSet::new();
        // Chunked, so a long list of branches stays well inside a command line.
        for chunk in shas.chunks(200) {
            let mut args = vec!["grep", "-l", "-F", "-e", needle];
            args.extend_from_slice(chunk);
            args.extend(["--", path]);
            if let Ok(out) = self.git(&args).await {
                found.extend(out.lines().filter_map(|l| l.split_once(':').map(|(sha, _)| sha.to_owned())));
            }
        }
        found
    }

    /// A file's contents at a commit.
    pub async fn read_file(&self, commit: &str, path: &str) -> Result<String> {
        self.git(&["show", &format!("{commit}:{path}")]).await
    }

    /// Every worktree of the repository, the main one first.
    pub async fn worktrees(&self) -> Result<Vec<Worktree>> {
        Ok(parse_worktrees(&self.git(&["worktree", "list", "--porcelain"]).await?))
    }

    /// Checks `sha` out, detached, in a build worktree at `worktree`, creating it on first use. A worktree that moved
    /// or was deleted is made again.
    pub async fn checkout_worktree(&self, worktree: &Path, sha: &str) -> Result<()> {
        if worktree.join(".git").exists() {
            if git_in(worktree, &["checkout", "--detach", "--force", sha], &[]).await.is_ok() {
                return Ok(());
            }
            let _ = std::fs::remove_dir_all(worktree);
        }
        self.git(&["worktree", "prune"]).await?;
        if let Some(parent) = worktree.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path = worktree.to_str().context("non-UTF-8 worktree path")?;
        self.git(&["worktree", "add", "--detach", "--force", path, sha]).await?;
        Ok(())
    }

    /// Removes a worktree made by [`Repo::checkout_worktree`].
    pub async fn remove_worktree(&self, worktree: &Path) -> Result<()> {
        let path = worktree.to_str().context("non-UTF-8 worktree path")?;
        let _ = self.git(&["worktree", "remove", "--force", path]).await;
        let _ = std::fs::remove_dir_all(worktree);
        self.git(&["worktree", "prune"]).await.map(drop)
    }

    /// Makes a new branch of one commit on top of `from`, with the given files replaced, and returns its sha. Nothing
    /// is checked out: the commit is written with a private index, so no worktree (and not the user's) changes.
    pub async fn commit_files(&self, c: &CommitFiles<'_>) -> Result<String> {
        if self.has_branch(c.branch).await {
            bail!("branch {} already exists in {}", c.branch, self.path.display());
        }
        let from = self.resolve(c.from).await?.sha;
        let scratch = Scratch::new()?;
        let index = scratch.path("index");
        let env: Vec<(OsString, OsString)> = vec![("GIT_INDEX_FILE".into(), index.clone().into_os_string())];
        git_in(&self.path, &["read-tree", &from], &env).await?;
        for (i, (path, contents)) in c.files.iter().enumerate() {
            let file = scratch.path(&format!("blob-{i}"));
            std::fs::write(&file, contents)?;
            let file = file.to_str().context("non-UTF-8 temp path")?;
            let blob = self.git(&["hash-object", "-w", "--path", path, file]).await?;
            // Keep an executable bit the file had; a new file is a plain one.
            let mode = self
                .git(&["ls-tree", &from, "--", path])
                .await
                .ok()
                .and_then(|l| l.split_whitespace().next().map(str::to_owned))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "100644".to_owned());
            git_in(&self.path, &["update-index", "--add", "--cacheinfo", &format!("{mode},{blob},{path}")], &env)
                .await?;
        }
        let tree = git_in(&self.path, &["write-tree"], &env).await?;
        let sha = self.commit_tree(&scratch, &tree, &from, c.message).await?;
        self.git(&["update-ref", "-m", "dameless: new branch", &format!("refs/heads/{}", c.branch), &sha, NO_COMMIT])
            .await?;
        Ok(sha)
    }

    /// Gives a branch's tip a new message (adding its `Bench:` line once it is built) and returns the new sha. The
    /// tree and parents stay, so the build of the old commit is the build of the new one. Fails if the branch moved
    /// meanwhile.
    pub async fn reword(&self, branch: &str, message: &str) -> Result<String> {
        let old = self.git(&["rev-parse", "--verify", &format!("refs/heads/{branch}")]).await?;
        let tree = self.git(&["rev-parse", &format!("{old}^{{tree}}")]).await?;
        let parents = self.git(&["log", "-1", "--format=%P", &old]).await?;
        let scratch = Scratch::new()?;
        let msg = scratch.path("message");
        std::fs::write(&msg, message)?;
        let msg = msg.to_str().context("non-UTF-8 temp path")?;
        let mut args = vec!["commit-tree", tree.as_str()];
        for p in parents.split_whitespace() {
            args.extend(["-p", p]);
        }
        args.extend(["-F", msg]);
        let sha = self.git(&args).await?;
        self.git(&["update-ref", "-m", "dameless: reword", &format!("refs/heads/{branch}"), &sha, &old]).await?;
        Ok(sha)
    }

    async fn commit_tree(&self, scratch: &Scratch, tree: &str, parent: &str, message: &str) -> Result<String> {
        let msg = scratch.path("message");
        std::fs::write(&msg, message)?;
        let msg = msg.to_str().context("non-UTF-8 temp path")?;
        self.git(&["commit-tree", tree, "-p", parent, "-F", msg]).await
    }

    /// Merges a branch into another (an idea into `main`): a fast-forward when the target is behind the branch, else a
    /// merge commit with the caller's message, made in a scratch worktree. A conflict fails and changes nothing.
    pub async fn promote(&self, p: &Promote<'_>) -> Result<Promoted> {
        let target = format!("refs/heads/{}", p.into);
        let previous =
            self.git(&["rev-parse", "--verify", &target]).await.with_context(|| format!("no branch {}", p.into))?;
        let branch = self.resolve(p.branch).await?.sha;
        if self.is_ancestor(&branch, &previous).await {
            bail!("{} is already in {}", p.branch, p.into);
        }
        let fast_forward = self.is_ancestor(&previous, &branch).await;
        let commit = if fast_forward {
            branch.clone()
        } else {
            self.checkout_worktree(p.worktree, &previous).await?;
            let scratch = Scratch::new()?;
            let msg = scratch.path("message");
            std::fs::write(&msg, p.message)?;
            let msg = msg.to_str().context("non-UTF-8 temp path")?;
            if let Err(e) = git_in(p.worktree, &["merge", "--no-ff", "--no-edit", "-F", msg, &branch], &[]).await {
                let _ = git_in(p.worktree, &["merge", "--abort"], &[]).await;
                bail!("{} does not merge cleanly into {}: {e}", p.branch, p.into);
            }
            git_in(p.worktree, &["rev-parse", "HEAD"], &[]).await?
        };

        let checked_out = self.worktrees().await?.into_iter().find(|w| w.branch.as_deref() == Some(p.into));
        match (checked_out, p.checked_out) {
            (None, _) => {
                self.git(&["update-ref", "-m", "dameless: promote", &target, &commit, &previous]).await?;
            }
            (Some(w), CheckedOut::Refuse) => {
                bail!("{} is checked out in {}: switch it to another branch first", p.into, w.path.display())
            }
            (Some(w), CheckedOut::FastForwardIfClean) => {
                let dirty = git_in(&w.path, &["status", "--porcelain", "--untracked-files=no"], &[]).await?;
                if !dirty.is_empty() {
                    bail!("{} is checked out in {} with uncommitted changes", p.into, w.path.display());
                }
                git_in(&w.path, &["merge", "--ff-only", &commit], &[]).await?;
            }
        }
        Ok(Promoted { commit, previous, fast_forward })
    }

    async fn is_ancestor(&self, ancestor: &str, of: &str) -> bool {
        self.git(&["merge-base", "--is-ancestor", ancestor, of]).await.is_ok()
    }
}

/// Runs git in `dir` (with extra environment) and returns its trimmed output.
async fn git_in(dir: &Path, args: &[&str], env: &[(OsString, OsString)]) -> Result<String> {
    let mut c = proc::Command::new("git").arg("-C").arg(dir.as_os_str()).args(args);
    for (k, v) in env {
        c = c.env(k, v);
    }
    Ok(proc::output(&c).await?.trim().to_owned())
}

fn parse_branches(out: &str) -> Vec<Branch> {
    out.split('\u{1e}')
        .filter_map(|record| {
            let mut f = record.trim_start_matches(['\n', '\r']).split('\0');
            let (name, sha, date, subject, message) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
            // `upstream` alone is the remote's HEAD, an alias of one of its branches.
            (!name.is_empty() && name != "upstream" && !name.ends_with("/HEAD")).then(|| Branch {
                name: name.to_owned(),
                sha: sha.to_owned(),
                subject: subject.to_owned(),
                date: date.to_owned(),
                bench: bench_line(message),
                upstream: is_upstream(name),
            })
        })
        .collect()
}

fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut all = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in out.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            all.extend(current.take());
            current = Some(Worktree { path: PathBuf::from(path), head: None, branch: None });
        } else if let Some(w) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                w.head = Some(head.to_owned());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                w.branch = Some(branch.strip_prefix("refs/heads/").unwrap_or(branch).to_owned());
            }
        }
    }
    all.extend(current);
    all
}

/// A private temp directory for an index file or a commit message, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dameless-git-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn reads_the_bench_line() {
        assert_eq!(bench_line("Minor cleanups (#1123)\n\nNo functional change.\n\nBench: 2837905\n"), Some(2837905));
        assert_eq!(bench_line("Bench: 1\n\nBench: 2"), Some(2));
        assert_eq!(bench_line("No bench here"), None);
    }

    #[test]
    fn parses_worktree_lists() {
        let out = "worktree C:/dev/reckless\nHEAD 69aea90\nbranch refs/heads/main\n\nworktree C:/data/worktree\nHEAD 85865a3\ndetached\n";
        let w = parse_worktrees(out);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(w[1].branch, None);
        assert_eq!(w[1].path, PathBuf::from("C:/data/worktree"));
    }

    /// A small repository: `main` with two commits, a branch `idea` on top of the first, and an upstream branch.
    async fn fixture(dir: &Path) -> Repo {
        let repo = Repo::new(dir.join("repo"));
        std::fs::create_dir_all(&repo.path).unwrap();
        repo.git(&["init", "-q", "-b", "main"]).await.unwrap();
        repo.git(&["config", "user.name", "Tests"]).await.unwrap();
        repo.git(&["config", "user.email", "tests@example.invalid"]).await.unwrap();
        repo.git(&["config", "core.autocrlf", "false"]).await.unwrap();
        let commit = |file: &'static str, body: &'static str, message: &'static str| {
            let repo = repo.clone();
            async move {
                std::fs::create_dir_all(repo.path.join("src")).unwrap();
                std::fs::write(repo.path.join(file), body).unwrap();
                repo.git(&["add", file]).await.unwrap();
                repo.git(&["commit", "-q", "-m", message]).await.unwrap();
            }
        };
        commit("src/parameters.rs", "rfp 100\n", "Start\n\nBench: 5").await;
        repo.git(&["branch", "idea"]).await.unwrap();
        commit("README", "main moves on\n", "Docs\n\nBench: 5").await;
        let head = repo.git(&["rev-parse", "HEAD"]).await.unwrap();
        repo.git(&["update-ref", "refs/remotes/upstream/main", &head]).await.unwrap();
        repo
    }

    #[tokio::test]
    async fn lists_branches_and_resolves_commits() {
        let dir = TempDir::new("git-branches");
        let repo = fixture(dir.path()).await;
        let branches = repo.branches().await.unwrap();
        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"main") && names.contains(&"idea") && names.contains(&"upstream/main"), "{names:?}");
        assert!(branches.iter().find(|b| b.name == "upstream/main").unwrap().upstream);
        assert!(branches.iter().all(|b| b.bench == Some(5)));

        let c = repo.resolve("main").await.unwrap();
        assert_eq!((c.subject.as_str(), c.bench, c.sha.len(), c.tree.len()), ("Docs", Some(5), 40, 40));
        assert!(c.date.contains('T'));
        assert!(repo.resolve("nothing").await.is_err());
        assert_eq!(repo.checked_out().await.as_deref(), Some("main"));
        assert!(repo.has_branch("idea").await && !repo.has_branch("nope").await);
        assert_eq!(repo.read_file("idea", "src/parameters.rs").await.unwrap(), "rfp 100");
    }

    #[tokio::test]
    async fn checks_commits_out_in_a_build_worktree() {
        let dir = TempDir::new("git-worktree");
        let repo = fixture(dir.path()).await;
        let wt = dir.path().join("builds").join("worktree");
        let idea = repo.resolve("idea").await.unwrap().sha;
        repo.checkout_worktree(&wt, &idea).await.unwrap();
        assert!(!wt.join("README").exists());
        let main = repo.resolve("main").await.unwrap().sha;
        repo.checkout_worktree(&wt, &main).await.unwrap();
        assert!(wt.join("README").exists());
        // The user's checkout is untouched.
        assert_eq!(repo.checked_out().await.as_deref(), Some("main"));
        repo.remove_worktree(&wt).await.unwrap();
        assert!(!wt.exists());
    }

    #[tokio::test]
    async fn commits_files_on_a_new_branch_and_rewords_it() {
        let dir = TempDir::new("git-commit");
        let repo = fixture(dir.path()).await;
        let status_before = repo.git(&["status", "--porcelain"]).await.unwrap();
        let files: &[(&str, &[u8])] = &[("src/parameters.rs", b"rfp 120\n")];
        let sha = repo
            .commit_files(&CommitFiles { branch: "tune/1", from: "idea", files, message: "Apply tune 1" })
            .await
            .unwrap();
        assert_eq!(repo.read_file("tune/1", "src/parameters.rs").await.unwrap(), "rfp 120");
        let c = repo.resolve("tune/1").await.unwrap();
        assert_eq!((c.sha.as_str(), c.bench), (sha.as_str(), None));
        // Nothing in the checkout changed.
        assert_eq!(repo.git(&["status", "--porcelain"]).await.unwrap(), status_before);
        // The branch exists now: a second one of the same name is refused.
        assert!(repo.commit_files(&CommitFiles { branch: "tune/1", from: "idea", files, message: "x" }).await.is_err());

        let reworded = repo.reword("tune/1", "Apply tune 1\n\nBench: 7\n").await.unwrap();
        let r = repo.resolve("tune/1").await.unwrap();
        assert_eq!((r.sha.as_str(), r.bench, r.tree.as_str()), (reworded.as_str(), Some(7), c.tree.as_str()));
        assert_ne!(reworded, sha);
    }

    #[tokio::test]
    async fn promotes_by_fast_forward_or_merge() {
        let dir = TempDir::new("git-promote");
        let repo = fixture(dir.path()).await;
        let wt = dir.path().join("merge-worktree");

        // `idea` forked before main's second commit: a merge commit, refused while main is checked out…
        let promote = |checked_out| Promote {
            branch: "idea2",
            into: "main",
            message: "Promote idea2\n\nBench: 6",
            worktree: &wt,
            checked_out,
        };
        let files: &[(&str, &[u8])] = &[("src/parameters.rs", b"rfp 90\n")];
        repo.commit_files(&CommitFiles { branch: "idea2", from: "idea", files, message: "Idea 2\n\nBench: 6" })
            .await
            .unwrap();
        let err = repo.promote(&promote(CheckedOut::Refuse)).await.unwrap_err().to_string();
        assert!(err.contains("checked out"), "{err}");

        // …and made into the clean checkout when allowed.
        let p = repo.promote(&promote(CheckedOut::FastForwardIfClean)).await.unwrap();
        assert!(!p.fast_forward);
        let main = repo.resolve("main").await.unwrap();
        assert_eq!((main.sha.as_str(), main.bench), (p.commit.as_str(), Some(6)));
        assert_eq!(std::fs::read_to_string(repo.path.join("src/parameters.rs")).unwrap(), "rfp 90\n");
        assert!(repo.promote(&promote(CheckedOut::FastForwardIfClean)).await.is_err(), "already merged");

        // A branch on top of main fast-forwards; with main not checked out anywhere, only the ref moves.
        repo.git(&["checkout", "-q", "--detach"]).await.unwrap();
        let files: &[(&str, &[u8])] = &[("README", b"more\n")];
        let tip = repo
            .commit_files(&CommitFiles { branch: "idea3", from: "main", files, message: "Idea 3\n\nBench: 6" })
            .await
            .unwrap();
        let p = repo
            .promote(&Promote {
                branch: "idea3",
                into: "main",
                message: "unused",
                worktree: &wt,
                checked_out: CheckedOut::Refuse,
            })
            .await
            .unwrap();
        assert!(p.fast_forward);
        assert_eq!(p.commit, tip);
        assert_eq!(repo.resolve("main").await.unwrap().sha, tip);
    }

    #[tokio::test]
    async fn pushes_only_when_asked() {
        let dir = TempDir::new("git-push");
        let repo = fixture(dir.path()).await;
        let origin = dir.path().join("origin.git");
        let origin_str = origin.to_str().unwrap();
        git_in(dir.path(), &["init", "-q", "--bare", origin_str], &[]).await.unwrap();
        repo.git(&["remote", "add", "origin", origin_str]).await.unwrap();
        repo.push("idea").await.unwrap();
        let bare = Repo::new(&origin);
        assert_eq!(bare.resolve("idea").await.unwrap().sha, repo.resolve("idea").await.unwrap().sha);
        assert!(repo.delete_remote_branch("idea").await.unwrap());
        assert!(!repo.delete_remote_branch("idea").await.unwrap());
    }
}
