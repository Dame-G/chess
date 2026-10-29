//! Work: anything running in the background — a build, a trial, a tune, a review, an import, a promotion.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{GameId, IdeaId, PlayerKey, Timestamp, TrialId, VersionId, WorkId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Build,
    Trial,
    Tune,
    Review,
    Import,
    Promote,
    /// Games between Dameless's levels, to fit their ratings.
    Calibrate,
    /// Self-play training data with Reckless's `datagen` branch.
    Datagen,
    /// A training run: bullet on the GPU, a few cores for its data loader.
    Train,
    /// Downloading a shard of training data (with your go-ahead) and converting it.
    DatasetImport,
    /// Taking upstream Reckless: fetch, merge into the fork's `main`, build and bench, crown the next champion.
    Upstream,
}

impl WorkKind {
    /// Whether it needs cores from the ledger. Imports use the network only and run at once (a dataset import's
    /// conversion is short and single-threaded).
    pub fn uses_cores(self) -> bool {
        !matches!(self, Self::Import | Self::DatasetImport)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    Queued,
    Running,
    /// Waiting between rounds, iterations or positions: by request, or for the fair-play policy.
    Paused,
    Done,
    Failed,
    Stopped,
}

impl WorkStatus {
    /// Whether it is over.
    pub fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Stopped)
    }
}

/// Which work runs first when cores are short: the lower, the sooner. Live games and live evaluation reserve their
/// cores outside the queue; these order the work items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Live = 0,
    Interactive = 1,
    /// Reviews asked for now: a game, or a few.
    Review = 2,
    Idea = 3,
    /// Long work that takes what nothing else wants: reviewing all of a player's games, calibrating the levels. It
    /// yields its cores to trials and to everything above them between units.
    Background = 4,
}

impl Priority {
    /// The number the store sorts by.
    pub fn rank(self) -> u8 {
        self as u8
    }

    pub fn from_rank(rank: u8) -> Option<Self> {
        [Self::Live, Self::Interactive, Self::Review, Self::Idea, Self::Background]
            .into_iter()
            .find(|p| p.rank() == rank)
    }
}

/// What a piece of work is about, so the UI can link to it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Subject {
    Idea { id: IdeaId },
    Trial { id: TrialId },
    Game { id: GameId },
    Player { key: PlayerKey },
    Version { id: VersionId },
}

/// How far along a piece of work is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
    /// What is counted: `pairs`, `iterations`, `games`, `positions`.
    pub unit: String,
    pub eta_seconds: Option<u64>,
    /// One sentence for people: "412 pairs, LLR 1.2 of 2.94".
    pub text: Option<String>,
}

impl Progress {
    /// Seconds left at the pace so far, from how long it has run.
    pub fn eta(done: u64, total: u64, elapsed_seconds: u64) -> Option<u64> {
        (done > 0 && total >= done).then(|| elapsed_seconds * (total - done) / done)
    }
}

/// One piece of background work.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Work {
    pub id: WorkId,
    pub kind: WorkKind,
    pub status: WorkStatus,
    pub priority: Priority,
    /// "Trying deeper-lmr · short games".
    pub title: String,
    pub subject: Subject,
    /// Cores granted now.
    pub cores: u32,
    pub progress: Option<Progress>,
    /// Why it is paused, for people: "Your Lichess game", "Live analysis".
    pub paused_for: Option<String>,
    pub error: Option<String>,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorities_order_live_first() {
        assert!(Priority::Live < Priority::Review && Priority::Review < Priority::Idea);
        assert_eq!(Priority::from_rank(2), Some(Priority::Review));
        assert_eq!(serde_json::to_value(Priority::Interactive).unwrap(), "interactive");
        let s = Subject::Player { key: PlayerKey::You };
        assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({ "type": "player", "key": "you" }));
        assert_eq!(Progress::eta(10, 40, 60), Some(180));
        assert!(!WorkKind::Import.uses_cores());
    }
}
