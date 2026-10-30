//! Progress: your lines side by side (`core::progress`): rating, accuracy (a rolling mean) and the drill hit rate, from
//! your games and drill answers.

use dameless_core::{
    Error, PlayerKey, drills,
    profile::ProfileFilter,
    progress::{self, Progress},
};

use crate::Profiles;

/// Your games the progress line reads, newest first.
pub const GAMES: u32 = 1000;

impl Profiles {
    /// Your progress; `window` is the games the accuracy's rolling mean takes (10).
    pub async fn progress(&self, window: Option<usize>) -> Result<Progress, Error> {
        let store = &self.hub().store;
        let you = self.profile(PlayerKey::You, ProfileFilter { limit: GAMES, ..ProfileFilter::default() }).await?;
        let answers = store.drill_answers(None).await?;
        let hit = drills::trend(&answers, crate::drills::DEFAULT_TREND_WINDOW);
        Ok(progress::progress(&you.trend, &hit, window.unwrap_or(progress::ACCURACY_WINDOW).max(1)))
    }
}
