//! Progress: the two players' lines side by side (`core::progress`). Yours: rating, accuracy (a rolling mean) and the
//! drill hit rate, from your games and drill answers. Dameless's: the champion line's cumulative Elo, put on the human
//! scale by full strength's calibrated rating when the levels have one.

use dameless_core::{
    Error, PlayerKey, drills,
    levels::{self, Entrant},
    profile::ProfileFilter,
    progress::{self, Progress},
    regression::{self, RegressionCheck},
};

use crate::Profiles;

/// Your games the progress line reads, newest first.
pub const GAMES: u32 = 1000;

impl Profiles {
    /// Both players' progress; `window` is the games the accuracy's rolling mean takes (10).
    pub async fn progress(&self, window: Option<usize>) -> Result<Progress, Error> {
        let store = &self.hub().store;
        let you = self.profile(PlayerKey::You, ProfileFilter { limit: GAMES, ..ProfileFilter::default() }).await?;
        let answers = store.drill_answers(None).await?;
        let hit = drills::trend(&answers, crate::drills::DEFAULT_TREND_WINDOW);
        let versions = store.versions(None).await?;
        let checks = store.meta::<Vec<RegressionCheck>>(regression::META_KEY).await?.unwrap_or_default();
        let full = levels::ladder().into_iter().find(|l| l.is_full()).map(|l| l.id);
        let (ratings, _) = store.level_ratings().await?;
        let anchor =
            ratings.iter().find(|r| Some(r.entrant) == full.map(Entrant::Level) && r.games > 0).map(|r| r.estimate.elo);
        Ok(progress::progress(
            &you.trend,
            &hit,
            &versions,
            &checks,
            anchor,
            window.unwrap_or(progress::ACCURACY_WINDOW).max(1),
        ))
    }
}
