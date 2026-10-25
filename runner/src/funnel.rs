//! What an experiment runs: an ordered list of stages, cheapest first, each with a pass rule. A stage that fails
//! stops the experiment, so most bad ideas cost seconds or minutes, and only survivors reach the long tests.
//!
//! The match presets copy Reckless's own testing (its OpenBench instance): time controls, hash sizes, book, engine
//! options, adjudication and SPRT bounds in normalized Elo, so results here compare directly with upstream's.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::stats::Sprt;

/// How long each move may take.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeControl {
    /// Seconds per game plus an increment per move.
    Clock { seconds: f64, increment: f64 },
    /// A fixed number of nodes per move: no clock, so speed does not count, only search and evaluation.
    Nodes { nodes: u64 },
}

/// One match between the dev and base builds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MatchSpec {
    pub tc: TimeControl,
    pub threads: u32,
    /// Hash table size in MiB.
    pub hash: u32,
    /// Stop when the SPRT decides. Without one, play `pairs` game pairs.
    pub sprt: Option<Sprt>,
    pub pairs: Option<u32>,
}

/// Named settings, as Reckless names its tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// 4+0.04: a quick screen before STC.
    Vstc,
    /// 8+0.08, 16 MiB: the standard short test.
    Stc,
    /// 40+0.4, 64 MiB: the confirmation.
    Ltc,
    /// 8+0.08 with 2 MiB of hash, for changes to the transposition table.
    StcHash,
    /// 40+0.4 with 8 MiB of hash.
    LtcHash,
    /// 40+0.4 on 6 threads with 512 MiB, for changes to the parallel search.
    Smp,
    /// 25,000 nodes per move, for changes that should not alter speed.
    Nodes,
}

impl Preset {
    pub fn spec(self, sprt: Option<Sprt>, pairs: Option<u32>) -> MatchSpec {
        let clock = |seconds, increment| TimeControl::Clock { seconds, increment };
        let (tc, threads, hash) = match self {
            Self::Vstc => (clock(4.0, 0.04), 1, 16),
            Self::Stc => (clock(8.0, 0.08), 1, 16),
            Self::Ltc => (clock(40.0, 0.4), 1, 64),
            Self::StcHash => (clock(8.0, 0.08), 1, 2),
            Self::LtcHash => (clock(40.0, 0.4), 1, 8),
            Self::Smp => (clock(40.0, 0.4), 6, 512),
            Self::Nodes => (TimeControl::Nodes { nodes: 25_000 }, 1, 16),
        };
        MatchSpec { tc, threads, hash, sprt, pairs }
    }
}

/// SPRT bounds, from Reckless's tests: alpha 0.05, beta 0.10.
pub fn bounds(elo0: f64, elo1: f64) -> Sprt {
    Sprt { elo0, elo1, alpha: 0.05, beta: 0.10 }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Stage {
    /// Bench both builds. A functional change must change the node count, and a non-functional one must not.
    Bench { functional: bool },
    /// Bench both builds in turn, several times, and fail when dev is measurably slower.
    Speed { runs: u32 },
    /// A match between the builds; it passes when its SPRT accepts, or when its fixed games are played.
    Match {
        name: String,
        #[serde(flatten)]
        spec: MatchSpec,
    },
}

impl Stage {
    pub fn name(&self) -> &str {
        match self {
            Self::Bench { .. } => "bench",
            Self::Speed { .. } => "speed",
            Self::Match { name, .. } => name,
        }
    }
}

/// What kind of change an experiment tests, which picks its funnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Meant to gain Elo.
    Gainer,
    /// A simplification or refactor that may change play: it must not lose Elo.
    Nonregression,
    /// Must not change play at all (same bench) and must not be slower.
    Nonfunctional,
}

/// The default funnel for a kind of change.
///
/// The screen is a wide non-regression SPRT, [-5, 0] nElo at VSTC: it rejects a clearly harmful change in a few
/// hundred game pairs and lets anything plausibly neutral or better through, which a gainer SPRT with wide bounds
/// would not (it would reject most real but small gains). The confirmation is what decides.
pub fn default_funnel(kind: Kind, ltc: bool) -> Vec<Stage> {
    let m = |name: &str, preset: Preset, sprt| Stage::Match { name: name.into(), spec: preset.spec(Some(sprt), None) };
    let screen = m("screen", Preset::Vstc, bounds(-5.0, 0.0));
    match kind {
        Kind::Gainer => {
            let mut stages = vec![Stage::Bench { functional: true }, screen, m("stc", Preset::Stc, bounds(0.0, 3.0))];
            if ltc {
                stages.push(m("ltc", Preset::Ltc, bounds(0.0, 3.0)));
            }
            stages
        }
        Kind::Nonregression => {
            let mut stages =
                vec![Stage::Bench { functional: true }, screen, m("stc", Preset::Stc, bounds(-2.75, 0.25))];
            if ltc {
                stages.push(m("ltc", Preset::Ltc, bounds(-2.75, 0.25)));
            }
            stages
        }
        Kind::Nonfunctional => vec![Stage::Bench { functional: false }, Stage::Speed { runs: 5 }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funnels_start_cheap() {
        let f = default_funnel(Kind::Gainer, true);
        let names: Vec<_> = f.iter().map(Stage::name).collect();
        assert_eq!(names, ["bench", "screen", "stc", "ltc"]);
        assert_eq!(default_funnel(Kind::Nonfunctional, true).len(), 2);
    }

    #[test]
    fn stages_round_trip_as_json() {
        let f = default_funnel(Kind::Nonregression, false);
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Stage>>(&json).unwrap(), f);
    }
}
