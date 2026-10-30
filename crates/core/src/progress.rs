//! Progress: your lines in one shape: your rating, your accuracy and your drill hit rate over time. Each is a
//! [`Series`] of dated points, so the UI charts them side by side with one component.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    drills::HitRatePoint,
    ids::{GameId, PlayerKey, Timestamp},
    profile::TrendPoint,
};

/// What a series measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Your Lichess rating in each game.
    Rating,
    /// Your game accuracy, a rolling mean over the last few reviewed games.
    Accuracy,
    /// Your drill hit rate, 0 to 1, rolling over the last answers.
    DrillHitRate,
}

/// One point of a series.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProgressPoint {
    pub at: Timestamp,
    pub value: f64,
    /// The game the point comes from, to open it.
    pub game: Option<GameId>,
}

impl ProgressPoint {
    fn at(at: Timestamp, value: f64) -> Self {
        Self { at, value, game: None }
    }
}

/// One line of the chart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Series {
    pub player: PlayerKey,
    pub metric: Metric,
    /// Oldest first.
    pub points: Vec<ProgressPoint>,
}

/// Your lines: `GET /api/progress`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = ProgressChart)]
pub struct Progress {
    pub series: Vec<Series>,
}

/// Accuracy is a rolling mean over this many reviewed games when the request names no window.
pub const ACCURACY_WINDOW: usize = 10;

/// Your lines: rating, accuracy (a rolling mean over `window` reviewed games) and drill hit rate (each day's rolling
/// rate). From a profile's trend (any order) and the drills' trend. A series without points is left out.
pub fn your_series(trend: &[TrendPoint], drills: &[HitRatePoint], window: usize) -> Vec<Series> {
    let mut games: Vec<&TrendPoint> = trend.iter().collect();
    games.sort_by(|a, b| a.played_at.cmp(&b.played_at).then(a.game.cmp(&b.game)));
    let rating = games
        .iter()
        .filter_map(|t| {
            let r = t.rating?;
            Some(ProgressPoint { game: Some(t.game), ..ProgressPoint::at(t.played_at.clone(), f64::from(r)) })
        })
        .collect();
    let window = window.max(1);
    let mut recent: Vec<f64> = Vec::new();
    let mut accuracy = Vec::new();
    for t in &games {
        let Some(a) = t.accuracy else { continue };
        recent.push(a);
        let last = &recent[recent.len().saturating_sub(window)..];
        let mean = last.iter().sum::<f64>() / last.len() as f64;
        accuracy.push(ProgressPoint { game: Some(t.game), ..ProgressPoint::at(t.played_at.clone(), mean) });
    }
    let hit = drills.iter().filter_map(|d| Some(ProgressPoint::at(Timestamp::parse(&d.date)?, d.rolling))).collect();
    [(Metric::Rating, rating), (Metric::Accuracy, accuracy), (Metric::DrillHitRate, hit)]
        .into_iter()
        .filter(|(_, points): &(Metric, Vec<ProgressPoint>)| !points.is_empty())
        .map(|(metric, points)| Series { player: PlayerKey::You, metric, points })
        .collect()
}

/// Your lines together.
pub fn progress(trend: &[TrendPoint], drills: &[HitRatePoint], window: usize) -> Progress {
    Progress { series: your_series(trend, drills, window) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(game: i64, day: i64, accuracy: Option<f64>, rating: Option<u32>) -> TrendPoint {
        TrendPoint {
            game: GameId(game),
            played_at: Timestamp::from_unix_ms(day * 86_400_000),
            accuracy,
            rating,
            points: Some(1.0),
        }
    }

    #[test]
    fn your_lines_are_dated_and_smoothed() {
        let trend = [t(3, 3, Some(90.0), Some(1520)), t(1, 1, Some(70.0), Some(1500)), t(2, 2, None, None)];
        let drills =
            [HitRatePoint { date: "1970-01-05".into(), answers: 4, passes: 3, hinted: 0, rate: 0.75, rolling: 0.6 }];
        let s = your_series(&trend, &drills, 2);
        assert_eq!(
            s.iter().map(|s| s.metric).collect::<Vec<_>>(),
            [Metric::Rating, Metric::Accuracy, Metric::DrillHitRate]
        );
        assert_eq!(s[0].points.iter().map(|p| p.value).collect::<Vec<_>>(), [1500.0, 1520.0]);
        assert_eq!(s[0].points[0].game, Some(GameId(1)));
        // The second accuracy point is the mean of the last two reviewed games.
        assert_eq!(s[1].points.iter().map(|p| p.value).collect::<Vec<_>>(), [70.0, 80.0]);
        assert_eq!(s[2].points[0].at, Timestamp::from_unix_ms(4 * 86_400_000));
        assert_eq!(s[2].points[0].value, 0.6);
        assert!(your_series(&[], &[], 10).is_empty());
    }
}
