//! Taking upstream: Reckless's own `main` merged into the fork's, as the next champion.
//!
//! Upstream merges a handful of tested gainers a month, each one proven on its own testing fleet, so the fork takes
//! them as they come instead of trying each again: the app fetches upstream, merges what the champion lacks, builds the
//! result and checks its bench, and crowns it `v<n+1>`, recorded as coming from upstream with the commits it brought.
//! The types here are what that records and reports; the work itself is in `work::upstream`.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{Timestamp, VersionId, WorkId};

/// How often the app looks for new upstream commits on its own, while the automatic sync is on.
pub const EVERY_MS: i64 = 6 * 60 * 60 * 1000;

/// The fork's remote that is upstream Reckless.
pub const REMOTE: &str = "upstream";

/// Upstream's branch that is taken.
pub const BRANCH: &str = "upstream/main";

/// One upstream commit a sync brought.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpstreamCommit {
    pub sha: String,
    /// Its first line.
    pub subject: String,
}

/// What a champion taken from upstream brought: the range of upstream commits merged into the fork's `main`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpstreamMerge {
    /// The fork's `main` before (the last champion's commit).
    pub base: String,
    /// The upstream commit merged (upstream's `main` then).
    pub upstream: String,
    /// The upstream commits the champion lacked, newest first.
    pub commits: Vec<UpstreamCommit>,
    /// `main` simply moved to upstream's commit (the fork had nothing of its own on top); else a merge commit.
    pub fast_forward: bool,
}

impl UpstreamMerge {
    /// How the logbook reads it: "from upstream · 4 commits".
    pub fn label(&self) -> String {
        let n = self.commits.len();
        format!("from upstream · {n} commit{}", if n == 1 { "" } else { "s" })
    }

    /// The range in git's words: `base..upstream`, short.
    pub fn range(&self) -> String {
        format!("{}..{}", short(&self.base), short(&self.upstream))
    }
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

/// How a sync ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    /// The champion already had every upstream commit.
    UpToDate,
    /// Upstream's commits were merged: a new champion.
    Merged,
    /// A fetch, merge, build or bench went wrong, or the checkout refused: nothing changed.
    Failed,
}

/// One sync, as the logbook and `GET /api/upstream` show it (also sent as `upstream.synced`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct UpstreamSync {
    pub at: Timestamp,
    pub outcome: SyncOutcome,
    /// One sentence for people: "v3 from upstream: 4 commits (69aea908..a814d54e), bench 2837905".
    pub detail: String,
    /// The work item that ran it.
    pub work: WorkId,
    /// The champion it made (when merged).
    pub version: Option<VersionId>,
    /// Its name, `v3`.
    pub champion: Option<String>,
    /// What it brought (when merged), or would have brought (a failed merge).
    pub merge: Option<UpstreamMerge>,
}

/// The automatic sync's settings (`meta` key `upstream.settings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpstreamSettings {
    /// Look for new upstream commits every six hours and take them (on by default).
    pub auto: bool,
}

impl Default for UpstreamSettings {
    fn default() -> Self {
        Self { auto: true }
    }
}

/// `GET /api/upstream`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct UpstreamStatus {
    pub settings: UpstreamSettings,
    /// The engine checkout has a remote named `upstream`; without one there is nothing to sync.
    pub remote: bool,
    /// The last sync, whatever its outcome.
    pub last: Option<UpstreamSync>,
    /// A sync queued or running now.
    pub running: Option<WorkId>,
    /// When the automatic sync looks next (none while it is off).
    pub next_at: Option<Timestamp>,
}

/// When the automatic sync is due next: six hours after the last one, or now when there was none.
pub fn next_due(last_ms: Option<i64>, now_ms: i64) -> i64 {
    last_ms.map_or(now_ms, |l| (l + EVERY_MS).max(now_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merge_reads_as_the_logbook_shows_it() {
        let commit = |s: &str| UpstreamCommit { sha: s.into(), subject: "x".into() };
        let m = UpstreamMerge {
            base: "69aea908aaaa".into(),
            upstream: "a814d54ebbbb".into(),
            commits: vec![commit("a814d54e")],
            fast_forward: true,
        };
        assert_eq!(m.label(), "from upstream · 1 commit");
        assert_eq!(m.range(), "69aea908..a814d54e");
        let four = UpstreamMerge { commits: vec![commit("a"), commit("b"), commit("c"), commit("d")], ..m };
        assert_eq!(four.label(), "from upstream · 4 commits");
    }

    #[test]
    fn the_next_sync_is_six_hours_after_the_last() {
        assert_eq!(next_due(None, 1000), 1000);
        assert_eq!(next_due(Some(0), 1000), EVERY_MS);
        assert_eq!(next_due(Some(0), EVERY_MS + 5), EVERY_MS + 5);
        assert!(UpstreamSettings::default().auto);
    }
}
