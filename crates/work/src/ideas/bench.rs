//! `Bench:` lines on an idea's branch. Every engine commit ends with `Bench: <nodes>`, the node count of `bench`,
//! and the fingerprint stage fails a build that disagrees with its commit. A branch whose tip has no line, or a stale
//! one (the code changed after the line was written), would queue an idea only to fail it minutes later, so creating
//! the idea checks first, and [`Ideas::fix_bench_line`] rewords the tip with the measured bench when asked.
//!
//! Fixing changes the user's fork branch (locally; nothing is pushed). It is only done on an explicit request: the
//! `fix-bench` route, or `fix_bench: true` on a new idea.

use dameless_core::{Error, version::BuildStatus};
use dameless_engine::git::{self, Commit};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::Ideas;
use crate::versions::{MAIN, VersionRef};

/// How a branch's `Bench:` line stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BenchLineStatus {
    /// The line matches what the tip's build measured.
    Ok,
    /// The tip's message has no `Bench:` line.
    Missing,
    /// The tip's build measured another node count than its line says.
    Stale,
    /// The tip has a line, but has not been built, so it cannot be told yet.
    Unbuilt,
}

/// `GET /api/ideas/check-branch?branch=`: a branch's tip and its `Bench:` line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BranchCheck {
    pub branch: String,
    /// The tip's full sha.
    pub commit: String,
    /// What the tip's message says.
    pub bench_line: Option<u64>,
    /// What its build measured, when it has been built.
    pub measured: Option<u64>,
    pub status: BenchLineStatus,
    /// One sentence for people.
    pub detail: String,
}

impl BranchCheck {
    /// Whether an idea with a fingerprint stage would fail on it.
    pub fn blocks(&self) -> bool {
        matches!(self.status, BenchLineStatus::Missing | BenchLineStatus::Stale)
    }
}

/// `POST /api/ideas/fix-bench`: the branch to fix, and optionally the tip it was checked at, so a branch that moved
/// since is not reworded blind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FixBenchRequest {
    pub branch: String,
    #[serde(default)]
    pub commit: Option<String>,
}

/// The check's verdict from a tip and what its build measured.
fn judge(branch: &str, tip: &Commit, measured: Option<u64>) -> BranchCheck {
    let short = tip.sha.get(..8).unwrap_or(&tip.sha);
    let (status, detail) = match (tip.bench, measured) {
        (None, Some(m)) => (BenchLineStatus::Missing, format!("{branch} ({short}) has no Bench: line; it benches {m}")),
        (None, None) => (BenchLineStatus::Missing, format!("{branch} ({short}) has no Bench: line")),
        (Some(_), None) => (BenchLineStatus::Unbuilt, format!("{branch} ({short}) is not built yet")),
        (Some(l), Some(m)) if l == m => (BenchLineStatus::Ok, format!("{branch} ({short}): Bench: {l} matches")),
        (Some(l), Some(m)) => {
            (BenchLineStatus::Stale, format!("{branch} ({short}) says Bench: {l} but benches {m}: the line is stale"))
        }
    };
    BranchCheck { branch: branch.to_owned(), commit: tip.sha.clone(), bench_line: tip.bench, measured, status, detail }
}

/// `message` with its `Bench:` lines replaced by one for `nodes`, at the end after a blank line.
pub(crate) fn with_bench_line(message: &str, nodes: u64) -> String {
    let kept: Vec<&str> = message.lines().filter(|l| !l.trim_start().starts_with("Bench:")).collect();
    let body = kept.join("\n");
    format!("{}\n\nBench: {nodes}\n", body.trim_end())
}

impl Ideas {
    /// A branch's tip and its `Bench:` line, without building: `unbuilt` when the tip has a line but no build yet.
    pub async fn check_branch(&self, branch: &str) -> Result<BranchCheck, Error> {
        let branch = branch.trim();
        let tip = self
            .versions()
            .repo()
            .resolve(branch)
            .await
            .map_err(|_| Error::not_found(format!("No branch {branch} in the engine checkout")))?;
        let measured = self
            .hub()
            .store
            .cached_build(&tip.sha, &[])
            .await?
            .filter(|b| b.status == BuildStatus::Ready && std::path::Path::new(&b.path).is_file())
            .and_then(|b| b.bench);
        Ok(judge(branch, &tip, measured))
    }

    /// Builds a branch's tip if needed, then rewords the tip's message with the measured `Bench: N` (local only; never
    /// pushed). Refused for `main` and upstream's branches, when the branch is not at `expected` (the tip it was
    /// checked at), and when it moves while being built. Returns the check of the new tip.
    pub async fn fix_bench_line(&self, branch: &str, expected: Option<&str>) -> Result<BranchCheck, Error> {
        let branch = branch.trim();
        let repo = self.versions().repo();
        if branch == MAIN || git::is_upstream(branch) {
            return Err(Error::bad_request(format!("Only an idea's own branch can be fixed, not {branch}")));
        }
        if !repo.has_branch(branch).await {
            return Err(Error::not_found(format!("No branch {branch} in the engine checkout")));
        }
        let tip = repo.resolve(branch).await.map_err(|e| Error::internal(format!("git: {e:#}")))?;
        if let Some(expected) = expected.filter(|e| !tip.sha.starts_with(e)) {
            return Err(Error::conflict(format!(
                "{branch} moved since it was checked (was {expected}, now {}): check it again",
                &tip.sha[..8.min(tip.sha.len())]
            )));
        }
        let version = self.versions().resolve(&VersionRef::Name(branch.to_owned())).await?;
        if version.commit != tip.sha {
            return Err(Error::conflict(format!("{branch} moved while it was being looked at: check it again")));
        }
        let (build, _) = self.versions().binary_of(&version, &[]).await?;
        let measured = build.bench.ok_or_else(|| Error::internal(format!("{branch}'s build has no bench")))?;
        let now = repo.resolve(branch).await.map_err(|e| Error::internal(format!("git: {e:#}")))?;
        if now.sha != tip.sha {
            return Err(Error::conflict(format!("{branch} moved while it was being built: check it again")));
        }
        if tip.bench == Some(measured) {
            return Ok(judge(branch, &tip, Some(measured)));
        }
        repo.reword(branch, &with_bench_line(&tip.message, measured))
            .await
            .map_err(|e| Error::conflict(format!("Could not reword {branch}: {e:#}")))?;
        let new = repo.resolve(branch).await.map_err(|e| Error::internal(format!("git: {e:#}")))?;
        // Same tree, new message: the build of the old commit is the build of the new one.
        self.versions().alias_build(&build, version.id, &new.sha).await?;
        self.versions().resolve(&VersionRef::Name(branch.to_owned())).await?;
        Ok(judge(branch, &new, Some(measured)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bench_line_is_replaced_or_added_at_the_end() {
        assert_eq!(with_bench_line("Deeper LMR", 123), "Deeper LMR\n\nBench: 123\n");
        assert_eq!(with_bench_line("Deeper LMR\n\nWhy.\n\nBench: 9\n", 123), "Deeper LMR\n\nWhy.\n\nBench: 123\n");
        assert_eq!(git::bench_line(&with_bench_line("x\nBench: 1\ny", 5)), Some(5));
    }
}
