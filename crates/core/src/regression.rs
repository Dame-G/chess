//! The regression check: the backstop for the funnels' faster SPRTs.
//!
//! The funnels test at [0, 4] with beta 0.20 and no screen (see [`crate::ideas`]), so more ideas are tested a day but
//! a few more small losses get through as false accepts, and each promotion's Elo is the estimate of an SPRT that
//! stopped early, so the champion line's sum drifts. After every fifth promotion to champion (from ideas or from
//! upstream: every new champion counts), the app plays the new champion against the champion five promotions earlier
//! for a fixed number of pairs at STC. That is not an SPRT: it measures, it does not decide.
//!
//! The check **fails** when the new champion's Elo interval lies wholly below 0: it is measurably worse than five
//! versions back. A failure is said loudly (the logbook and the work log) and nothing is reverted: what to do about it
//! is the user's call. A pass, and a failure too, is a real measurement over five promotions, which the strength line
//! uses in place of the sum of their estimates (see [`crate::progress::dameless_series`]).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    ideas::Preset,
    ids::{IdeaId, Timestamp, VersionId},
    stats::{self, Pentanomial},
    version::Estimate,
};

/// The tag every regression check idea carries.
pub const TAG: &str = "regression";
/// A check is queued after every this many promotions.
pub const EVERY: u32 = 5;
/// Game pairs a check plays: about 20,000 games, a day of this machine at STC, for an interval of about ±3 Elo.
pub const PAIRS: u32 = 10_000;
/// The clock a check plays.
pub const PRESET: Preset = Preset::Stc;
/// Where finished checks are kept (in the store's `meta`), oldest first.
pub const META_KEY: &str = "regression.checks";

/// The champion a new champion is checked against, by number: `v6` against `v1`, `v11` against `v6`; none for a
/// champion that does not end a run of five promotions (`v1` is the first champion, so `v6` is the fifth promotion).
pub fn due(number: u32) -> Option<u32> {
    (number > EVERY && (number - 1).is_multiple_of(EVERY)).then(|| number - EVERY)
}

/// One finished check: `GET /api/dameless` lists them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RegressionCheck {
    /// The idea that played it.
    pub idea: IdeaId,
    /// The new champion, and its name (`v11`).
    pub champion: VersionId,
    pub champion_name: String,
    /// The champion five promotions earlier.
    pub against: VersionId,
    pub against_name: String,
    /// Passed: not measurably worse than `against`.
    pub passed: bool,
    /// The new champion's logistic Elo over `against`, ± 95 %.
    pub elo: Option<Estimate>,
    /// Game pairs played.
    pub pairs: u64,
    /// One sentence: "v11 over v6: +12.4 ± 3.1 Elo over 10000 pairs".
    pub summary: String,
    pub at: Timestamp,
}

/// Judges a check from its pairs: whether it passed, the Elo, and its sentence. It fails only when the interval lies
/// wholly below 0.
pub fn judge(champion: &str, against: &str, penta: &Pentanomial) -> (bool, Option<Estimate>, String) {
    let pairs = penta.pairs();
    let Some(elo) = stats::estimate(penta).map(|e| e.elo_interval()) else {
        return (true, None, format!("{champion} over {against}: no games to measure"));
    };
    let passed = elo.high >= 0.0;
    let summary = if passed {
        format!("{champion} over {against}: {} Elo over {pairs} pairs", elo.text())
    } else {
        format!(
            "Regression: {champion} is measurably worse than {against}, {} Elo over {pairs} pairs. Nothing was \
             reverted; read the promotions since {against}",
            elo.text()
        )
    };
    (passed, Some(elo), summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_follows_every_fifth_promotion() {
        assert_eq!(due(1), None);
        assert_eq!((2..=5).filter_map(due).count(), 0);
        assert_eq!(due(6), Some(1));
        assert_eq!((7..=10).filter_map(due).count(), 0);
        assert_eq!(due(11), Some(6));
        assert_eq!(due(16), Some(11));
    }

    #[test]
    fn it_fails_only_when_measurably_worse() {
        let even = Pentanomial([100, 2000, 5800, 2000, 100]);
        let (passed, elo, summary) = judge("v11", "v6", &even);
        assert!(passed && elo.unwrap().low < 0.0 && elo.unwrap().high > 0.0, "{summary}");
        assert!(summary.starts_with("v11 over v6: ") && summary.ends_with("Elo over 10000 pairs"), "{summary}");
        let worse = Pentanomial([300, 2600, 5800, 1300, 0]);
        let (passed, elo, summary) = judge("v11", "v6", &worse);
        assert!(!passed && elo.unwrap().high < 0.0);
        assert!(summary.starts_with("Regression: v11 is measurably worse than v6"), "{summary}");
        let better = Pentanomial([0, 1300, 5800, 2600, 300]);
        assert!(judge("v11", "v6", &better).0);
        assert_eq!(judge("v11", "v6", &Pentanomial::default()).1, None);
    }
}
