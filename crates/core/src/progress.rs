//! Progress: the two players' lines in one shape. Yours are your rating, your accuracy and your drill hit rate over
//! time; Dameless's is its cumulative Elo over the champion line (each promotion's deciding estimate added up from
//! `v1`). Each is a [`Series`] of dated points, so the UI charts them side by side with one component.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    drills::HitRatePoint,
    ids::{GameId, PlayerKey, Timestamp, VersionId},
    profile::TrendPoint,
    regression::RegressionCheck,
    version::{Line, Version},
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
    /// Dameless's Elo over `v1` (or on the human scale, when the levels anchor it).
    Elo,
}

/// One point of a series.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProgressPoint {
    pub at: Timestamp,
    pub value: f64,
    /// A 95 % interval, where the value has one (Dameless's Elo).
    pub low: Option<f64>,
    pub high: Option<f64>,
    /// The game or version the point comes from, to open it.
    pub game: Option<GameId>,
    pub version: Option<VersionId>,
    /// `v14`, for a version.
    pub label: Option<String>,
    /// Dameless's point measured by a regression check (games between this champion and one five promotions back)
    /// rather than added up from promotions' estimates.
    #[serde(default)]
    pub measured: bool,
}

impl ProgressPoint {
    fn at(at: Timestamp, value: f64) -> Self {
        Self { at, value, low: None, high: None, game: None, version: None, label: None, measured: false }
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

/// Both players' lines: `GET /api/progress`.
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

/// Dameless's line: each champion version (others are skipped), in promotion order, at its Elo over `v1`, the sum of
/// each promotion's deciding estimate (a champion without one adds nothing). The interval adds the estimates' errors
/// in quadrature. A champion with a regression check (`checks`) is placed by it instead: the champion it was checked
/// against, plus the measured Elo, with their errors in quadrature; the champions after it add on from there. With
/// `anchor` (full strength's calibrated rating), the line is shifted so the newest champion sits there, putting it on
/// the same scale as your rating.
pub fn dameless_series(versions: &[Version], checks: &[RegressionCheck], anchor: Option<f64>) -> Series {
    let mut champions: Vec<(u32, &Version)> = versions
        .iter()
        .filter_map(|v| match v.line {
            Line::Champion { number } => Some((number, v)),
            _ => None,
        })
        .collect();
    champions.sort_by_key(|(n, _)| *n);
    let (mut elo, mut variance) = (0.0f64, 0.0f64);
    let mut points: Vec<ProgressPoint> = Vec::new();
    // Each champion's (Elo, variance) so far, for a check to start from.
    let mut placed: Vec<(VersionId, f64, f64)> = Vec::new();
    for (i, (_, v)) in champions.iter().enumerate() {
        let check = checks.iter().rev().find_map(|c| {
            let e = c.elo.filter(|_| c.champion == v.id)?;
            let (_, from, from_var) = placed.iter().find(|(id, _, _)| *id == c.against)?;
            Some((from + e.elo, from_var + e.error().powi(2)))
        });
        match check {
            Some((e, var)) => (elo, variance) = (e, var),
            None => {
                if i > 0
                    && let Some(e) = v.elo
                {
                    elo += e.elo;
                    variance += e.error().powi(2);
                }
            }
        }
        placed.push((v.id, elo, variance));
        let sd = variance.sqrt();
        points.push(ProgressPoint {
            low: Some(elo - sd),
            high: Some(elo + sd),
            version: Some(v.id),
            label: Some(v.name.clone()),
            measured: check.is_some(),
            ..ProgressPoint::at(v.created_at.clone(), elo)
        });
    }
    if let (Some(anchor), Some(last)) = (anchor, points.last().map(|p| p.value)) {
        let shift = anchor - last;
        for p in &mut points {
            p.value += shift;
            p.low = p.low.map(|x| x + shift);
            p.high = p.high.map(|x| x + shift);
        }
    }
    Series { player: PlayerKey::Dameless, metric: Metric::Elo, points }
}

/// Both players' lines together.
pub fn progress(
    trend: &[TrendPoint],
    drills: &[HitRatePoint],
    versions: &[Version],
    checks: &[RegressionCheck],
    anchor: Option<f64>,
    window: usize,
) -> Progress {
    let mut series = your_series(trend, drills, window);
    series.push(dameless_series(versions, checks, anchor));
    Progress { series }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ids::IdeaId, version::Estimate};

    fn t(game: i64, day: i64, accuracy: Option<f64>, rating: Option<u32>) -> TrendPoint {
        TrendPoint {
            game: GameId(game),
            played_at: Timestamp::from_unix_ms(day * 86_400_000),
            accuracy,
            rating,
            points: Some(1.0),
        }
    }

    fn v(id: i64, line: Line, elo: Option<Estimate>) -> Version {
        Version {
            net: None,
            upstream: None,
            id: VersionId(id),
            name: match line {
                Line::Champion { number } => Line::champion_name(number),
                _ => "idea".into(),
            },
            commit: "abc".into(),
            branch: None,
            line,
            parent: None,
            bench: None,
            elo,
            created_at: Timestamp::from_unix_ms(id * 1000),
        }
    }

    #[test]
    fn your_lines_are_dated_and_smoothed() {
        let trend = [t(3, 3, Some(90.0), Some(1520)), t(1, 1, Some(70.0), Some(1500)), t(2, 2, None, None)];
        let drills = [HitRatePoint { date: "1970-01-05".into(), answers: 4, passes: 3, rate: 0.75, rolling: 0.6 }];
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

    #[test]
    fn dameless_adds_up_its_promotions() {
        let versions = [
            v(3, Line::Champion { number: 3 }, Some(Estimate::plus_minus(4.0, 4.0))),
            v(1, Line::Champion { number: 1 }, Some(Estimate::plus_minus(99.0, 1.0))),
            v(2, Line::Champion { number: 2 }, Some(Estimate::plus_minus(3.0, 3.0))),
            v(9, Line::Upstream, Some(Estimate::plus_minus(50.0, 1.0))),
        ];
        let s = dameless_series(&versions, &[], None);
        assert_eq!(s.metric, Metric::Elo);
        assert_eq!(s.points.iter().map(|p| p.value).collect::<Vec<_>>(), [0.0, 3.0, 7.0]);
        assert_eq!(s.points[2].label.as_deref(), Some("v3"));
        assert_eq!((s.points[0].low, s.points[0].high), (Some(0.0), Some(0.0)));
        assert!((s.points[2].high.unwrap() - 12.0).abs() < 1e-9);
        let anchored = dameless_series(&versions, &[], Some(3000.0));
        assert_eq!(anchored.points.iter().map(|p| p.value).collect::<Vec<_>>(), [2993.0, 2996.0, 3000.0]);
        assert!(dameless_series(&[], &[], Some(1.0)).points.is_empty());
        assert!(!s.points.iter().any(|p| p.measured));
    }

    #[test]
    fn a_regression_check_places_its_champion() {
        let e = |x| Some(Estimate::plus_minus(x, 2.0));
        let versions: Vec<Version> = (1..=7).map(|n| v(n, Line::Champion { number: n as u32 }, e(3.0))).collect();
        let check = |elo: Option<Estimate>| RegressionCheck {
            idea: IdeaId(1),
            champion: VersionId(6),
            champion_name: "v6".into(),
            against: VersionId(1),
            against_name: "v1".into(),
            passed: true,
            elo,
            pairs: 10_000,
            summary: String::new(),
            at: Timestamp::from_unix_ms(0),
        };
        // Five promotions of +3 added up to +15; the check measured +4 over v1, and v7 adds its +3 from there.
        let s = dameless_series(&versions, &[check(Some(Estimate::plus_minus(4.0, 3.0)))], None);
        assert_eq!(s.points.iter().map(|p| p.value).collect::<Vec<_>>(), [0.0, 3.0, 6.0, 9.0, 12.0, 4.0, 7.0]);
        assert!(s.points[5].measured && !s.points[6].measured);
        assert!((s.points[5].high.unwrap() - 7.0).abs() < 1e-9);
        assert!((s.points[6].high.unwrap() - (7.0 + 13f64.sqrt())).abs() < 1e-9);
        // A check without an estimate changes nothing.
        let s = dameless_series(&versions, &[check(None)], None);
        assert_eq!(s.points[5].value, 15.0);
    }

    #[test]
    fn progress_has_both_players() {
        let p = progress(
            &[t(1, 1, Some(80.0), Some(1500))],
            &[],
            &[v(1, Line::Champion { number: 1 }, None)],
            &[],
            None,
            10,
        );
        assert_eq!(p.series.len(), 3);
        assert_eq!(p.series.last().unwrap().player, PlayerKey::Dameless);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["series"][0]["metric"], "rating");
        assert_eq!(json["series"][2]["player"], "dameless");
    }
}
