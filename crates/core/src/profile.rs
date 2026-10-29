//! Players and their profiles: a player's games (filtered by speed, rated and date) added up into results, accuracy by
//! phase and speed, classifications per game, openings, time, a trend, notable moments, weak spots and a performance
//! rating. A comparison is profiles side by side with who leads each row. Results count every finished game; engine
//! figures count reviewed games only, and `pending_review` says how many are not reviewed yet.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chess::Position,
    game::{GameSummary, Outcome, opening_family},
    ids::{GameId, PlayerKey, Side, Timestamp},
    lichess::Ratings,
    review::{
        ClassCounts, JudgementCounts, Review,
        judge::{Classification, Judgement, Phase},
    },
    time::Speed,
};

/// Someone who sits at a board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Player {
    pub key: PlayerKey,
    pub name: String,
    pub title: Option<String>,
    /// Lichess's ratings per speed, when known.
    pub ratings: Ratings,
    /// The Lichess username.
    pub lichess: Option<String>,
}

/// Which games a profile adds up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProfileFilter {
    /// Only these speeds; empty for all.
    #[serde(default)]
    pub speeds: Vec<Speed>,
    /// Only rated (true) or casual (false) games.
    pub rated: Option<bool>,
    /// Only games from this moment on.
    pub since: Option<Timestamp>,
    /// The newest games, at most.
    pub limit: u32,
}

/// Games a profile takes when the request gives no `limit`.
pub const DEFAULT_LIMIT: u32 = 200;

impl Default for ProfileFilter {
    fn default() -> Self {
        Self { speeds: Vec::new(), rated: None, since: None, limit: DEFAULT_LIMIT }
    }
}

impl ProfileFilter {
    /// Whether a game passes the filter (the limit is the caller's: it takes the newest that pass).
    pub fn matches(&self, g: &GameSummary) -> bool {
        let speed_ok = self.speeds.is_empty() || g.speed().is_some_and(|s| self.speeds.contains(&s));
        let rated_ok = self.rated.is_none_or(|r| g.source.rated() == r);
        let since_ok = self.since.as_ref().is_none_or(|s| g.started_at >= *s);
        speed_ok && rated_ok && since_ok
    }
}

/// Wins, draws and losses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Wdl {
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Points scored, as a percentage of the decided games.
    pub score: Option<f64>,
}

impl Wdl {
    fn add(&mut self, outcome: Option<i8>) {
        self.games += 1;
        match outcome {
            Some(1) => self.wins += 1,
            Some(0) => self.draws += 1,
            Some(_) => self.losses += 1,
            None => {}
        }
        let played = self.wins + self.draws + self.losses;
        self.score = (played > 0)
            .then(|| round1(100.0 * (f64::from(self.wins) + f64::from(self.draws) / 2.0) / f64::from(played)));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Results {
    pub overall: Wdl,
    pub white: Wdl,
    pub black: Wdl,
}

/// One speed's games.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeedStats {
    /// None for games without a speed (movetime or node limits).
    pub speed: Option<Speed>,
    pub games: u32,
    pub reviewed: u32,
    pub results: Wdl,
    pub accuracy: Option<f64>,
    pub acpl: Option<f64>,
}

/// Average game accuracy (each reviewed game counts once), overall, per phase and per speed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PhaseStats {
    pub overall: Option<f64>,
    pub opening: Option<f64>,
    pub middlegame: Option<f64>,
    pub endgame: Option<f64>,
    pub by_speed: Vec<SpeedStats>,
}

/// Classifications per reviewed game.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ClassRates {
    pub brilliant: f64,
    pub great: f64,
    pub best: f64,
    pub excellent: f64,
    pub good: f64,
    pub book: f64,
    pub inaccuracy: f64,
    pub mistake: f64,
    pub blunder: f64,
    pub miss: f64,
}

impl ClassRates {
    fn of(counts: &ClassCounts, games: u32) -> Self {
        let r = |c: Classification| {
            if games == 0 { 0.0 } else { (100.0 * f64::from(counts.get(c)) / f64::from(games)).round() / 100.0 }
        };
        Self {
            brilliant: r(Classification::Brilliant),
            great: r(Classification::Great),
            best: r(Classification::Best),
            excellent: r(Classification::Excellent),
            good: r(Classification::Good),
            book: r(Classification::Book),
            inaccuracy: r(Classification::Inaccuracy),
            mistake: r(Classification::Mistake),
            blunder: r(Classification::Blunder),
            miss: r(Classification::Miss),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Opponents {
    /// The opponents' average rating, over the decided games where it is known.
    pub average_rating: Option<f64>,
    /// A simple performance rating: the opponents' average plus 400 × (wins − losses) / games.
    pub performance: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OpeningStats {
    /// The opening family as Lichess names it, without the ECO code or the variation ("Sicilian Defense").
    pub name: String,
    pub results: Wdl,
}

/// Think time per move, from reviewed games with a clock (Lichess records no time for each side's first move).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TimeStats {
    pub games: u32,
    pub average_ms: Option<f64>,
    pub opening_ms: Option<f64>,
    pub middlegame_ms: Option<f64>,
    pub endgame_ms: Option<f64>,
    /// Blunders played with less than a tenth of the starting clock left, in all and per game.
    pub time_trouble_blunders: u32,
    pub time_trouble_blunders_per_game: f64,
}

/// One game on the trend line, oldest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrendPoint {
    pub game: GameId,
    pub played_at: Timestamp,
    /// The player's accuracy in it, when reviewed.
    pub accuracy: Option<f64>,
    /// The player's rating in it, when known.
    pub rating: Option<u32>,
    /// 1, ½ or 0; none for an unfinished game.
    pub points: Option<f64>,
}

/// A move worth linking to: the board opens the game at `ply`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Moment {
    pub game: GameId,
    pub ply: u32,
    pub san: String,
    pub side: Side,
    pub classification: Classification,
    pub win_before: f64,
    pub win_after: f64,
    /// Pawns of material given up (brilliant moves).
    pub sacrifice: i32,
    pub opponent: String,
    pub played_at: Timestamp,
    /// The game on Lichess, at the move.
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GameMoment {
    pub game: GameId,
    pub accuracy: f64,
    pub opponent: String,
    pub outcome: Option<Outcome>,
    pub played_at: Timestamp,
}

/// The best and worst of a player's reviewed games.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Moments {
    /// The brilliant moves with the most material given up (at most 3), else the great ones.
    pub best: Vec<Moment>,
    /// The worst blunders by win percentage lost (at most 3).
    pub blunders: Vec<Moment>,
    pub most_accurate_game: Option<GameMoment>,
    pub least_accurate_game: Option<GameMoment>,
}

/// A position before one of the player's mistakes or blunders in a reviewed game, to play out again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WeakSpot {
    pub game: GameId,
    /// The ply of the mistake.
    pub ply: u32,
    /// The position before it.
    pub fen: String,
    /// What was played, and what was better, in SAN.
    pub played: String,
    pub best: String,
    /// Win percentage lost.
    pub swing: f64,
    pub opening: Option<String>,
    pub phase: Phase,
    /// How many weak spots share this one's opening family and phase: recurring themes come first.
    pub recurring: u32,
}

/// Weak spots a profile lists, at most.
pub const WEAK_SPOTS: usize = 20;

/// A player's games added up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Profile {
    pub player: Player,
    pub filter: ProfileFilter,
    /// Games considered, of them reviewed, and the rest.
    pub games: u32,
    pub reviewed: u32,
    pub pending_review: u32,
    /// All the player's games not yet reviewed at the default version and budget, whatever the filter: what
    /// "Review all" does. Only a single profile fills it; `None` when unknown (no champion yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreviewed_total: Option<u32>,
    /// Whether your whole Lichess history has been imported, so later imports only look for new games. Only your own
    /// profile fills it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_complete: Option<bool>,
    pub first_game: Option<Timestamp>,
    pub last_game: Option<Timestamp>,
    /// The rating in the newest game considered.
    pub rating: Option<u32>,
    pub results: Results,
    pub accuracy: PhaseStats,
    pub acpl: Option<f64>,
    /// Classifications in all reviewed games, and per game.
    pub counts: ClassCounts,
    pub counts_per_game: ClassRates,
    /// Lichess's own counts (a miss counts as its mistake or blunder).
    pub judgements: JudgementCounts,
    pub opponents: Opponents,
    /// The most played opening families, at most 8.
    pub openings: Vec<OpeningStats>,
    pub time: TimeStats,
    pub trend: Vec<TrendPoint>,
    pub longest_win_streak: u32,
    pub moments: Moments,
    /// Positions from the player's mistakes, recurring themes first; filled for You only.
    pub weak_spots: Vec<WeakSpot>,
}

/// One game as a profile reads it: its summary, its start position (for weak spots) and its newest review.
#[derive(Clone, Debug)]
pub struct ProfileGame {
    pub summary: GameSummary,
    pub start_fen: Option<String>,
    pub review: Option<Review>,
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn average(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| round1(xs.iter().sum::<f64>() / xs.len() as f64))
}

/// 1 a win, 0 a draw, -1 a loss, `None` unfinished.
fn outcome_for(outcome: Option<Outcome>, side: Side) -> Option<i8> {
    match outcome? {
        Outcome::Draw => Some(0),
        o if o.winner() == Some(side) => Some(1),
        _ => Some(-1),
    }
}

/// Adds up `player`'s games, newest first. Games `player` did not sit in are left out. Weak spots are found when
/// `weak_spots` is set (the design shows them for You only).
pub fn profile(player: Player, filter: ProfileFilter, games: &[ProfileGame], weak_spots: bool) -> Profile {
    let mine: Vec<(&ProfileGame, Side)> =
        games.iter().filter_map(|g| Some((g, g.summary.side_of(&player.key)?))).collect();
    let mut results = Results::default();
    let mut speeds: Vec<(SpeedStats, Vec<f64>, Vec<f64>)> = Vec::new();
    let mut counts = ClassCounts::default();
    let mut judgements = JudgementCounts::default();
    let (mut accs, mut acpls) = (Vec::new(), Vec::new());
    let mut phase_accs: [Vec<f64>; 3] = Default::default();
    let mut phase_time: [(u64, u32); 3] = [(0, 0); 3];
    let (mut opp_ratings, mut rated_points) = (Vec::new(), 0i64);
    let mut openings: HashMap<String, Wdl> = HashMap::new();
    let mut time = TimeStats::default();
    let mut best: Vec<Moment> = Vec::new();
    let mut blunders: Vec<Moment> = Vec::new();
    let mut game_accs: Vec<GameMoment> = Vec::new();
    let mut trend: Vec<TrendPoint> = Vec::new();
    let mut spots: Vec<WeakSpot> = Vec::new();
    let mut reviewed = 0u32;

    for &(g, side) in &mine {
        let m = &g.summary;
        let o = outcome_for(m.outcome, side);
        results.overall.add(o);
        match side {
            Side::White => results.white.add(o),
            Side::Black => results.black.add(o),
        }
        let me = m.seat(side);
        let opp = m.seat(side.other());
        if let (Some(r), Some(o)) = (opp.rating, o) {
            opp_ratings.push(f64::from(r));
            rated_points += i64::from(o);
        }
        if let Some(opening) = &m.opening {
            openings.entry(opening.family()).or_default().add(o);
        }
        let speed = m.speed();
        let at = match speeds.iter().position(|s| s.0.speed == speed) {
            Some(i) => i,
            None => {
                let empty =
                    SpeedStats { speed, games: 0, reviewed: 0, results: Wdl::default(), accuracy: None, acpl: None };
                speeds.push((empty, Vec::new(), Vec::new()));
                speeds.len() - 1
            }
        };
        speeds[at].0.games += 1;
        speeds[at].0.results.add(o);
        let mut point = TrendPoint {
            game: m.id,
            played_at: m.started_at.clone(),
            accuracy: None,
            rating: me.rating,
            points: m.outcome.map(|o| o.points(side)),
        };

        if let Some(review) = &g.review {
            reviewed += 1;
            speeds[at].0.reviewed += 1;
            let s = review.summary.side(side);
            counts = counts.plus(&s.counts);
            judgements = judgements.plus(&s.judgements);
            if let Some(a) = s.accuracy {
                accs.push(a);
                speeds[at].1.push(a);
                point.accuracy = Some(a);
                game_accs.push(GameMoment {
                    game: m.id,
                    accuracy: a,
                    opponent: opp.name.clone(),
                    outcome: m.outcome,
                    played_at: m.started_at.clone(),
                });
            }
            if let Some(c) = s.acpl {
                acpls.push(f64::from(c));
                speeds[at].2.push(f64::from(c));
            }
            let mut timed = false;
            for p in Phase::ALL {
                let r = s.phases.get(p);
                if let Some(a) = r.accuracy {
                    phase_accs[p.index()].push(a);
                }
                phase_time[p.index()].0 += r.time_ms;
                phase_time[p.index()].1 += r.timed_moves;
                timed |= r.timed_moves > 0;
            }
            if timed {
                time.games += 1;
            }
            time.time_trouble_blunders += s.time_trouble_blunders;
            for p in review.plies.iter().filter(|p| p.side == side) {
                let moment = || Moment {
                    game: m.id,
                    ply: p.ply,
                    san: p.san.clone(),
                    side,
                    classification: p.classification,
                    win_before: p.win_before,
                    win_after: p.win_after,
                    sacrifice: p.sacrifice,
                    opponent: opp.name.clone(),
                    played_at: m.started_at.clone(),
                    url: m.url.as_ref().map(|u| format!("{u}#{}", p.ply)),
                };
                match p.classification {
                    Classification::Brilliant | Classification::Great => best.push(moment()),
                    _ if p.judgement == Some(Judgement::Blunder) => blunders.push(moment()),
                    _ => {}
                }
            }
            if weak_spots {
                spots.extend(weak_spots_of(g, review, side));
            }
        }
        trend.push(point);
    }
    trend.reverse();

    let mut longest = 0u32;
    let mut streak = 0u32;
    for &(g, side) in mine.iter().rev() {
        if outcome_for(g.summary.outcome, side) == Some(1) {
            streak += 1;
            longest = longest.max(streak);
        } else {
            streak = 0;
        }
    }
    let brilliant_rank = |m: &Moment| (u8::from(m.classification == Classification::Brilliant), m.sacrifice);
    best.sort_by(|a, b| brilliant_rank(b).cmp(&brilliant_rank(a)).then(b.win_after.total_cmp(&a.win_after)));
    best.truncate(3);
    blunders.sort_by(|a, b| (b.win_before - b.win_after).total_cmp(&(a.win_before - a.win_after)));
    blunders.truncate(3);
    let most = game_accs.iter().max_by(|a, b| a.accuracy.total_cmp(&b.accuracy)).cloned();
    let least = game_accs.iter().min_by(|a, b| a.accuracy.total_cmp(&b.accuracy)).cloned();
    let mut openings: Vec<OpeningStats> = openings
        .into_iter()
        .filter(|(n, _)| !n.is_empty())
        .map(|(name, results)| OpeningStats { name, results })
        .collect();
    openings.sort_by(|a, b| b.results.games.cmp(&a.results.games).then(a.name.cmp(&b.name)));
    openings.truncate(8);
    let n = opp_ratings.len();
    let exact_opp = (n > 0).then(|| opp_ratings.iter().sum::<f64>() / n as f64);
    let per_move = |(ms, moves): (u64, u32)| (moves > 0).then(|| round1(ms as f64 / f64::from(moves)));
    let all_time = phase_time.iter().fold((0u64, 0u32), |acc, t| (acc.0 + t.0, acc.1 + t.1));
    time.average_ms = per_move(all_time);
    time.opening_ms = per_move(phase_time[0]);
    time.middlegame_ms = per_move(phase_time[1]);
    time.endgame_ms = per_move(phase_time[2]);
    time.time_trouble_blunders_per_game = if time.games == 0 {
        0.0
    } else {
        (100.0 * f64::from(time.time_trouble_blunders) / f64::from(time.games)).round() / 100.0
    };
    let newest = mine.first();
    let total = mine.len() as u32;
    Profile {
        rating: newest.and_then(|(g, side)| g.summary.seat(*side).rating),
        player,
        filter,
        games: total,
        reviewed,
        pending_review: total - reviewed,
        unreviewed_total: None,
        history_complete: None,
        first_game: mine.last().map(|(g, _)| g.summary.started_at.clone()),
        last_game: newest.map(|(g, _)| g.summary.started_at.clone()),
        results,
        accuracy: PhaseStats {
            overall: average(&accs),
            opening: average(&phase_accs[0]),
            middlegame: average(&phase_accs[1]),
            endgame: average(&phase_accs[2]),
            by_speed: speeds
                .into_iter()
                .map(|(mut s, a, c)| {
                    s.accuracy = average(&a);
                    s.acpl = average(&c);
                    s
                })
                .collect(),
        },
        acpl: average(&acpls),
        counts,
        counts_per_game: ClassRates::of(&counts, reviewed),
        judgements,
        opponents: Opponents {
            average_rating: exact_opp.map(round1),
            performance: exact_opp.map(|a| round1(a + 400.0 * rated_points as f64 / n as f64)),
        },
        openings,
        time,
        trend,
        longest_win_streak: longest,
        moments: Moments { best, blunders, most_accurate_game: most, least_accurate_game: least },
        weak_spots: rank_weak_spots(spots),
    }
}

/// The positions before `side`'s mistakes and blunders (misses included) in one reviewed game.
fn weak_spots_of(g: &ProfileGame, review: &Review, side: Side) -> Vec<WeakSpot> {
    let Ok(mut pos) = Position::from_start(g.start_fen.as_deref()) else { return Vec::new() };
    let family = g.summary.opening.as_ref().map(|o| opening_family(&o.name)).filter(|f| !f.is_empty());
    let mut out = Vec::new();
    for p in &review.plies {
        let serious = matches!(p.judgement, Some(Judgement::Mistake | Judgement::Blunder));
        if serious && p.side == side {
            out.push(WeakSpot {
                game: g.summary.id,
                ply: p.ply,
                fen: pos.fen(),
                played: p.san.clone(),
                best: p.best_san.clone(),
                swing: round1(p.win_before - p.win_after),
                opening: family.clone(),
                phase: p.phase,
                recurring: 0,
            });
        }
        if pos.play(&p.uci).is_err() {
            break;
        }
    }
    out
}

/// Recurring themes first: spots grouped by opening family and phase, the biggest groups first, each by swing.
fn rank_weak_spots(mut spots: Vec<WeakSpot>) -> Vec<WeakSpot> {
    let mut groups: HashMap<(Option<String>, Phase), u32> = HashMap::new();
    for s in &spots {
        *groups.entry((s.opening.clone(), s.phase)).or_default() += 1;
    }
    for s in &mut spots {
        s.recurring = groups[&(s.opening.clone(), s.phase)];
    }
    spots.sort_by(|a, b| {
        b.recurring
            .cmp(&a.recurring)
            .then_with(|| a.opening.cmp(&b.opening))
            .then(a.phase.index().cmp(&b.phase.index()))
            .then(b.swing.total_cmp(&a.swing))
    });
    spots.truncate(WEAK_SPOTS);
    spots
}

// ---------------------------------------------------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------------------------------------------------

/// A row of the comparison and who leads it (indexes into the profiles; several on a tie, none without figures).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Leader {
    /// `score`, `accuracy`, `opening`, `middlegame`, `endgame`, `acpl`, `blunders`, `mistakes`, `brilliant`,
    /// `performance`, `streak`, `time_trouble`.
    pub row: String,
    pub players: Vec<usize>,
}

/// Profiles side by side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Comparison {
    pub profiles: Vec<Profile>,
    pub leaders: Vec<Leader>,
}

/// Who leads each row: the highest score, accuracies, performance and streak; the lowest ACPL, blunders and mistakes
/// per game and time-trouble blunders; the most brilliant moves per game.
pub fn leaders(profiles: &[Profile]) -> Vec<Leader> {
    type Pick = fn(&Profile) -> Option<f64>;
    let rows: [(&str, Pick, bool); 12] = [
        ("score", |p| p.results.overall.score, true),
        ("accuracy", |p| p.accuracy.overall, true),
        ("opening", |p| p.accuracy.opening, true),
        ("middlegame", |p| p.accuracy.middlegame, true),
        ("endgame", |p| p.accuracy.endgame, true),
        ("acpl", |p| p.acpl, false),
        ("blunders", |p| (p.reviewed > 0).then_some(p.counts_per_game.blunder), false),
        ("mistakes", |p| (p.reviewed > 0).then_some(p.counts_per_game.mistake), false),
        ("brilliant", |p| (p.reviewed > 0).then_some(p.counts_per_game.brilliant), true),
        ("performance", |p| p.opponents.performance, true),
        ("streak", |p| (p.games > 0).then_some(f64::from(p.longest_win_streak)), true),
        ("time_trouble", |p| (p.time.games > 0).then_some(p.time.time_trouble_blunders_per_game), false),
    ];
    rows.iter()
        .map(|(row, pick, higher)| {
            let values: Vec<Option<f64>> = profiles.iter().map(pick).collect();
            let best = values.iter().flatten().copied().reduce(|a, b| if (b > a) == *higher { b } else { a });
            let players = match best {
                // A row only one player has figures for leads nobody.
                Some(best) if values.iter().flatten().count() > 1 => values
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_some_and(|v| (v - best).abs() < 1e-9))
                    .map(|(i, _)| i)
                    .collect(),
                _ => Vec::new(),
            };
            Leader { row: (*row).to_owned(), players }
        })
        .collect()
}

/// Profiles side by side, with the leaders.
pub fn compare(profiles: Vec<Profile>) -> Comparison {
    Comparison { leaders: leaders(&profiles), profiles }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::{Ending, GameStatus, Opening, Seat, Source},
        ids::{ConnectionKey, VersionId},
        review::{ReviewSummary, ReviewedMove, SideReview},
        score::Score,
        time::TimeControl,
    };

    fn seat(key: PlayerKey, name: &str, rating: u32) -> Seat {
        Seat { rating: Some(rating), ..Seat::new(key, name) }
    }

    fn bob() -> PlayerKey {
        PlayerKey::Lichess("bob".into())
    }

    /// A blitz game on Lichess between You (as `you_white` says) and someone.
    fn game(id: i64, you_white: bool, opponent: (&str, u32), outcome: Outcome, day: u32) -> GameSummary {
        let you = seat(PlayerKey::You, "Dame", 1800);
        let them = seat(PlayerKey::Lichess(opponent.0.to_lowercase()), opponent.0, opponent.1);
        let (white, black) = if you_white { (you, them) } else { (them, you) };
        GameSummary {
            id: GameId(id),
            white,
            black,
            source: Source::Lichess {
                connection: ConnectionKey::You,
                id: format!("g{id}"),
                rated: true,
                speed: Speed::Blitz,
            },
            time: TimeControl::clock(180_000, 0),
            status: GameStatus::Finished,
            outcome: Some(outcome),
            ending: Some(Ending::Resignation),
            opening: Some(Opening {
                eco: Some("C20".into()),
                name: "C20 King's Pawn Game: Busch-Gass Gambit".into(),
                ply: Some(2),
            }),
            plies: 4,
            started_at: Timestamp(format!("2023-11-{day:02}T12:00:00.000Z")),
            ended_at: None,
            you: Some(if you_white { Side::White } else { Side::Black }),
            accuracy: None,
            url: Some(format!("https://lichess.org/g{id}")),
        }
    }

    fn ply(
        n: u32,
        side: Side,
        uci: &str,
        san: &str,
        class: Classification,
        judgement: Option<Judgement>,
    ) -> ReviewedMove {
        let (wb, wa) = if judgement.is_some() { (60.0, 20.0) } else { (50.0, 50.0) };
        ReviewedMove {
            ply: n,
            side,
            uci: uci.into(),
            san: san.into(),
            eval_before: Score::Cp(0),
            eval_after: Score::Cp(0),
            best_uci: "b1c3".into(),
            best_san: "Nc3".into(),
            best_pv: vec![],
            second_uci: None,
            eval_second: None,
            win_before: wb,
            win_after: wa,
            accuracy: 90.0,
            cp_loss: 10,
            classification: class,
            judgement,
            phase: Phase::Opening,
            sacrifice: if class == Classification::Brilliant { 3 } else { 0 },
            clock_before_ms: None,
            depth: 12,
        }
    }

    fn review(game: i64, white: (f64, ClassCounts), black: (f64, ClassCounts), plies: Vec<ReviewedMove>) -> Review {
        let side = |(acc, counts): (f64, ClassCounts)| SideReview {
            moves: 2,
            accuracy: Some(acc),
            acpl: Some(30),
            counts,
            ..SideReview::default()
        };
        Review {
            id: crate::ids::ReviewId(game),
            game: GameId(game),
            version: VersionId(1),
            nodes: 1000,
            summary: ReviewSummary {
                version: VersionId(1),
                nodes: 1000,
                white: side(white),
                black: side(black),
                middlegame_ply: None,
                endgame_ply: None,
            },
            plies,
            created_at: Timestamp("2023-11-20T00:00:00.000Z".into()),
        }
    }

    fn you() -> Player {
        Player {
            key: PlayerKey::You,
            name: "Dame".into(),
            title: None,
            ratings: Ratings::default(),
            lichess: Some("Dame".into()),
        }
    }

    fn counts(best: u32, blunder: u32) -> ClassCounts {
        ClassCounts { best, blunder, ..ClassCounts::default() }
    }

    fn games() -> Vec<ProfileGame> {
        // Newest first: a loss to Carl, a win as Black over Bob, a win as White over Bob.
        let blunder = vec![
            ply(1, Side::White, "e2e4", "e4", Classification::Book, None),
            ply(2, Side::Black, "e7e5", "e5", Classification::Book, None),
            ply(3, Side::White, "g2g4", "g4", Classification::Blunder, Some(Judgement::Blunder)),
            ply(4, Side::Black, "d8h4", "Qh4", Classification::Brilliant, None),
        ];
        vec![
            ProfileGame {
                summary: game(3, true, ("Carl", 1700), Outcome::BlackWins, 15),
                start_fen: None,
                review: None,
            },
            ProfileGame {
                summary: game(2, false, ("Bob", 1800), Outcome::BlackWins, 14),
                start_fen: None,
                review: Some(review(2, (70.0, counts(1, 1)), (90.0, counts(1, 0)), blunder.clone())),
            },
            ProfileGame {
                summary: game(1, true, ("Bob", 1700), Outcome::WhiteWins, 13),
                start_fen: None,
                review: Some(review(1, (80.0, counts(1, 2)), (60.0, counts(2, 0)), blunder)),
            },
        ]
    }

    #[test]
    fn results_count_all_games_and_engine_figures_reviewed_ones() {
        let p = profile(you(), ProfileFilter::default(), &games(), true);
        assert_eq!((p.games, p.reviewed, p.pending_review), (3, 2, 1));
        assert_eq!((p.results.overall.wins, p.results.overall.draws, p.results.overall.losses), (2, 0, 1));
        assert_eq!((p.results.white.wins, p.results.white.losses, p.results.black.wins), (1, 1, 1));
        assert_eq!(p.results.overall.score, Some(66.7));
        assert_eq!(p.longest_win_streak, 2);
        assert_eq!(p.rating, Some(1800));
        // You were Black (90) in game 2 and White (80) in game 1.
        assert_eq!(p.accuracy.overall, Some(85.0));
        assert_eq!(p.counts.best, 2);
        assert_eq!(p.counts.blunder, 2);
        assert_eq!(p.counts_per_game.best, 1.0);
        assert_eq!(p.openings[0].name, "King's Pawn Game");
        assert_eq!(p.openings[0].results.games, 3);
        // Opponents averaged 1733.3 (Carl 1700, Bob 1800, Bob 1700); +1 net over 3 games.
        assert_eq!(p.opponents.average_rating, Some(1733.3));
        assert_eq!(p.opponents.performance, Some(round1(1733.333 + 400.0 / 3.0)));
        assert_eq!(p.accuracy.by_speed.len(), 1);
        assert_eq!(p.accuracy.by_speed[0].speed, Some(Speed::Blitz));
        // The trend runs oldest first.
        assert_eq!(p.trend.iter().map(|t| t.game.0).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(p.trend[0].accuracy, Some(80.0));
        assert_eq!(p.trend[2].points, Some(0.0));
        assert_eq!(p.first_game.as_ref().map(|t| t.date()), Some("2023-11-13"));
    }

    #[test]
    fn moments_and_weak_spots_are_yours() {
        let p = profile(you(), ProfileFilter::default(), &games(), true);
        // Your brilliant move as Black in game 2; your blunder as White in game 1.
        assert_eq!(p.moments.best.len(), 1);
        assert_eq!((p.moments.best[0].game, p.moments.best[0].san.as_str()), (GameId(2), "Qh4"));
        assert_eq!(p.moments.blunders.len(), 1);
        assert_eq!(p.moments.blunders[0].url.as_deref(), Some("https://lichess.org/g1#3"));
        assert_eq!(p.moments.most_accurate_game.as_ref().map(|g| g.game), Some(GameId(2)));
        assert_eq!(p.moments.least_accurate_game.as_ref().map(|g| g.accuracy), Some(80.0));
        let spot = &p.weak_spots[0];
        assert_eq!(p.weak_spots.len(), 1);
        assert_eq!((spot.game, spot.ply, spot.played.as_str(), spot.best.as_str()), (GameId(1), 3, "g4", "Nc3"));
        assert_eq!(spot.fen, "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
        assert_eq!((spot.swing, spot.recurring), (40.0, 1));
        assert_eq!(spot.opening.as_deref(), Some("King's Pawn Game"));
        assert!(profile(you(), ProfileFilter::default(), &games(), false).weak_spots.is_empty());
    }

    #[test]
    fn another_players_profile_reads_their_side() {
        let bob = Player {
            key: bob(),
            name: "Bob".into(),
            title: None,
            ratings: Ratings::default(),
            lichess: Some("Bob".into()),
        };
        let p = profile(bob, ProfileFilter::default(), &games(), false);
        assert_eq!(p.games, 2);
        assert_eq!(p.results.overall.losses, 2);
        assert_eq!(p.accuracy.overall, Some(65.0));
        assert_eq!(p.rating, Some(1800));
    }

    #[test]
    fn filters_pick_games() {
        let g = &games()[0].summary;
        assert!(ProfileFilter::default().matches(g));
        assert!(!ProfileFilter { speeds: vec![Speed::Rapid], ..ProfileFilter::default() }.matches(g));
        assert!(!ProfileFilter { rated: Some(false), ..ProfileFilter::default() }.matches(g));
        let late = ProfileFilter { since: Timestamp::parse("2023-11-15T13:00:00Z"), ..ProfileFilter::default() };
        assert!(!late.matches(g));
        assert!(ProfileFilter { since: Timestamp::parse("2023-11-15"), ..ProfileFilter::default() }.matches(g));
    }

    #[test]
    fn leaders_lead_their_rows() {
        let gs = games();
        let a = profile(you(), ProfileFilter::default(), &gs, false);
        let bob = Player { key: bob(), name: "Bob".into(), title: None, ratings: Ratings::default(), lichess: None };
        let b = profile(bob, ProfileFilter::default(), &gs, false);
        let c = compare(vec![a, b]);
        let row = |name: &str| c.leaders.iter().find(|l| l.row == name).unwrap().players.clone();
        assert_eq!(row("score"), [0]);
        assert_eq!(row("accuracy"), [0]);
        // Same ACPL: a tie.
        assert_eq!(row("acpl"), [0, 1]);
        // Bob made one blunder in two games; You two.
        assert_eq!(row("blunders"), [1]);
        assert!(row("time_trouble").is_empty());
    }
}
