//! What an experiment runs: an ordered list of stages, cheapest first, each with a pass rule. A stage that fails
//! stops the experiment, so most bad ideas cost seconds or minutes, and only survivors reach the long tests.
//!
//! The match presets copy Reckless's own testing (its OpenBench instance): time controls, hash sizes, book, engine
//! options, adjudication and SPRT bounds in normalized Elo. One thing differs: OpenBench scales each worker's clock
//! to a reference speed (Reckless's is 660,000 nps of `bench`, measured with every game slot busy), and these clocks
//! are not scaled. On this machine, with a game in every slot, an engine searches well under that, so "8+0.08" here
//! is a shorter test in nodes than upstream's STC. Results are valid comparisons of dev against base at the clock
//! played; they are not upstream's numbers.

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
/// would not (it would reject most real but small gains). The confirmation is what decides. (Upstream, by contrast,
/// sometimes runs VSTC at the gainer's own [0, 3]: stricter, and slower to reject.)
///
/// So a screen accepts a change that does nothing about 90% of the time (that is its power at 0, 1 − beta): a passed
/// screen says only "not clearly worse". A check for false accepts must use the gainer bounds, where a change that
/// does nothing is accepted at most 5% of the time (alpha).
pub fn default_funnel(kind: Kind, ltc: bool) -> Vec<Stage> {
    match kind {
        Kind::Gainer => functional(bounds(0.0, 3.0), ltc),
        Kind::Nonregression => functional(bounds(-2.75, 0.25), ltc),
        Kind::Nonfunctional => vec![Stage::Bench { functional: false }, Stage::Speed { runs: 5 }],
    }
}

/// The confirmation of an SPSA result, as Reckless confirms its tunes: bench must change, the usual screen, then STC at
/// [0, 4] (and LTC at [0, 4] with `ltc`).
pub fn tuned_funnel(ltc: bool) -> Vec<Stage> {
    functional(bounds(0.0, 4.0), ltc)
}

/// A change to search: bench must change, then the screen, then STC (and LTC with `ltc`) at `confirm`.
fn functional(confirm: Sprt, ltc: bool) -> Vec<Stage> {
    let m = |name: &str, preset: Preset, sprt| Stage::Match { name: name.into(), spec: preset.spec(Some(sprt), None) };
    let mut stages = vec![
        Stage::Bench { functional: true },
        m("screen", Preset::Vstc, bounds(-5.0, 0.0)),
        m("stc", Preset::Stc, confirm),
    ];
    if ltc {
        stages.push(m("ltc", Preset::Ltc, confirm));
    }
    stages
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
    fn tunes_confirm_at_zero_four() {
        let f = tuned_funnel(false);
        let Some(Stage::Match { name, spec }) = f.last() else { panic!("no match stage") };
        assert_eq!((name.as_str(), spec.sprt), ("stc", Some(bounds(0.0, 4.0))));
    }

    #[test]
    fn stages_round_trip_as_json() {
        let f = default_funnel(Kind::Nonregression, false);
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Stage>>(&json).unwrap(), f);
    }
}
