//! Versions of Dameless and the binaries built from them.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    ids::{BuildId, IdeaId, Timestamp, VersionId},
    training::NetId,
};

/// One commit of Dameless that matters: the champion line, an idea's change, an upstream tip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Version {
    pub id: VersionId,
    /// `v14` on the champion line, else the branch name.
    pub name: String,
    /// The full sha.
    pub commit: String,
    pub branch: Option<String>,
    pub line: Line,
    /// The champion it was built on.
    pub parent: Option<VersionId>,
    /// The commit's `Bench:` line: the engine's functional fingerprint.
    pub bench: Option<u64>,
    /// Elo against its parent, from the deciding trial.
    pub elo: Option<Estimate>,
    /// The net its builds embed; none for the commit's own (the file its build script names). A version of the
    /// champion's code with a trained net is named `v14 + net-3`, and a champion can be code plus a net.
    #[serde(default)]
    pub net: Option<NetId>,
    pub created_at: Timestamp,
}

/// Which line of development a version is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Line {
    /// The fork's `main`, numbered `v1`, `v2`, … in order of promotion.
    Champion { number: u32 },
    /// An idea's change (or a tune's result).
    Idea { idea: IdeaId },
    /// A tip of upstream Reckless.
    Upstream,
    /// Anything else built by ref.
    Other,
}

impl Line {
    /// The name a champion version gets: `v14`.
    pub fn champion_name(number: u32) -> String {
        format!("v{number}")
    }
}

/// A binary of a version: a cache keyed by commit, cargo features and net, shown in diagnostics only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Build {
    pub id: BuildId,
    pub version: VersionId,
    /// Cargo features it was built with, such as `spsa`.
    pub features: Vec<String>,
    /// The net it embeds, when not the commit's own.
    #[serde(default)]
    pub net: Option<NetId>,
    /// What its `bench` measured: must equal the commit's `Bench:` line.
    pub bench: Option<u64>,
    /// Nodes per second of a single `bench`.
    pub nps: Option<u64>,
    pub path: String,
    pub status: BuildStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildStatus {
    Queued,
    Building,
    Ready,
    Failed,
    /// Its binary was removed; the record stays for what used it.
    Deleted,
}

/// An estimate with its 95 % interval: `elo` and the interval's ends.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Estimate {
    pub elo: f64,
    pub low: f64,
    pub high: f64,
}

impl Estimate {
    /// From a value and its ± error.
    pub fn plus_minus(elo: f64, error: f64) -> Self {
        Self { elo, low: elo - error, high: elo + error }
    }

    /// Half the interval's width: the ± people read.
    pub fn error(&self) -> f64 {
        (self.high - self.low) / 2.0
    }

    /// `+3.1 ± 2.0`.
    pub fn text(&self) -> String {
        format!("{:+.1} ± {:.1}", self.elo, self.error())
    }
}

/// A trial's log-likelihood ratio and the bounds that decide it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Llr {
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_read_as_plus_minus() {
        let e = Estimate::plus_minus(3.12, 2.0);
        assert_eq!(e.text(), "+3.1 ± 2.0");
        assert!((e.low - 1.12).abs() < 1e-9);
        assert_eq!(Line::champion_name(14), "v14");
        let json = serde_json::to_value(Line::Champion { number: 3 }).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "champion", "number": 3 }));
    }
}
