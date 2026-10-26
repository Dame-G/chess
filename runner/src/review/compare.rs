//! Comparing players: each player's games (yours, the bot's, or another Lichess player's imported ones), filtered
//! by speed, rated and date, summed up side by side. Results count every game; engine figures (accuracy, ACPL,
//! classifications, phases, time trouble, notable moments) count only reviewed games, and `pending_review` says how
//! many are not reviewed yet.

use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::{Connection, ToSql, params_from_iter};
use serde::Serialize;
use utoipa::ToSchema;

use super::{
    ClassCounts, JudgementCounts, Review, Who, game_filters,
    judge::{Classification, Phase},
    load_many,
};
use crate::matches::{
    Match, MatchSummary,
    rules::{GameResult, Side},
};

/// Wins, draws and losses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, ToSchema)]
pub struct Wdl {
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Points scored, as a percentage of the games.
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

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct Results {
    pub overall: Wdl,
    pub white: Wdl,
    pub black: Wdl,
}

/// Average game accuracy (each reviewed game counts once).
#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct AccuracyStats {
    pub overall: Option<f64>,
    pub opening: Option<f64>,
    pub middlegame: Option<f64>,
    pub endgame: Option<f64>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SpeedStats {
    pub speed: String,
    pub games: u32,
    pub reviewed: u32,
    pub results: Wdl,
    pub accuracy: Option<f64>,
    pub acpl: Option<f64>,
}

/// Classifications per reviewed game.
#[derive(Clone, Debug, Default, Serialize, ToSchema)]
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

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct Opponents {
    /// The opponents' average rating, over the games where it is known.
    pub average_rating: Option<f64>,
    /// A simple performance rating: the opponents' average plus 400 × (wins − losses) / games.
    pub performance: Option<f64>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct OpeningStats {
    /// The opening family as Lichess names it, without the ECO code or the variation ("Sicilian Defense").
    pub name: String,
    pub results: Wdl,
}

/// Think time per move, from reviewed games with a clock (Lichess records no time for each side's first move).
#[derive(Clone, Debug, Default, Serialize, ToSchema)]
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

/// A move worth linking to: the board opens `/matches/{match_id}` at `ply`.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Moment {
    pub match_id: i64,
    pub ply: u32,
    pub san: String,
    pub side: Side,
    pub classification: Classification,
    pub win_before: f64,
    pub win_after: f64,
    /// Pawns of material given up (brilliant moves).
    pub sacrifice: i32,
    pub opponent: String,
    pub played_at: String,
    /// The game on Lichess, at the move.
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GameMoment {
    pub match_id: i64,
    pub accuracy: f64,
    pub opponent: String,
    pub result: GameResult,
    pub played_at: String,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct Notable {
    /// The brilliant moves with the most material given up (at most 3), else the great ones.
    pub brilliant: Vec<Moment>,
    /// The worst blunders by win percentage lost (at most 3).
    pub blunders: Vec<Moment>,
    pub most_accurate_game: Option<GameMoment>,
    pub least_accurate_game: Option<GameMoment>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PlayerStats {
    /// As asked: `me`, `bot` or a Lichess username (lower case).
    pub player: String,
    /// The Lichess name, title and rating in the newest game considered.
    pub username: Option<String>,
    pub title: Option<String>,
    pub rating: Option<u32>,
    /// Games considered, of them reviewed, and the rest.
    pub games: u32,
    pub reviewed: u32,
    pub pending_review: u32,
    pub first_game: Option<String>,
    pub last_game: Option<String>,
    pub results: Results,
    pub accuracy: AccuracyStats,
    pub acpl: Option<f64>,
    pub by_speed: Vec<SpeedStats>,
    /// Classifications in all reviewed games, and per game.
    pub counts: ClassCounts,
    pub per_game: ClassRates,
    /// Lichess's own counts (a miss counts as its mistake or blunder).
    pub judgements: JudgementCounts,
    pub opponents: Opponents,
    /// The most played opening families, at most 8.
    pub openings: Vec<OpeningStats>,
    pub time: TimeStats,
    pub longest_win_streak: u32,
    pub notable: Notable,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Comparison {
    pub players: Vec<PlayerStats>,
    pub speeds: Vec<String>,
    pub rated: Option<bool>,
    pub since: Option<String>,
    /// Newest games per player, at most.
    pub limit: u32,
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn average(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| round1(xs.iter().sum::<f64>() / xs.len() as f64))
}

/// Our player's side of a game: the account's (or the imported player's) side.
fn side_of(m: &MatchSummary) -> Side {
    m.lichess.as_ref().map_or(Side::White, |l| l.our_side)
}

/// 1 a win, 0 a draw, -1 a loss, `None` unfinished.
fn outcome(result: GameResult, side: Side) -> Option<i8> {
    match result {
        GameResult::Draw => Some(0),
        GameResult::Unfinished => None,
        r if r == GameResult::won_by(side) => Some(1),
        _ => Some(-1),
    }
}

/// "C20 King's Pawn Game: Busch-Gass Gambit" is the family "King's Pawn Game".
pub fn opening_family(name: &str) -> String {
    let mut words = name.splitn(2, ' ');
    let first = words.next().unwrap_or_default();
    let is_eco = first.len() == 3
        && first.starts_with(['A', 'B', 'C', 'D', 'E'])
        && first[1..].chars().all(|c| c.is_ascii_digit());
    let rest = if is_eco { words.next().unwrap_or_default() } else { name };
    rest.split(':').next().unwrap_or_default().trim().to_owned()
}

/// Sums up one player's games, newest first, each with its newest review if any.
pub fn aggregate(player: &str, games: &[(MatchSummary, Option<Review>)]) -> PlayerStats {
    let newest = games.first().map(|(m, _)| m);
    let me = |m: &MatchSummary| match side_of(m) {
        Side::White => m.white.clone(),
        Side::Black => m.black.clone(),
    };
    let opponent = |m: &MatchSummary| match side_of(m) {
        Side::White => m.black.clone(),
        Side::Black => m.white.clone(),
    };
    let mut results = Results::default();
    let mut speeds: Vec<(String, SpeedStats, Vec<f64>, Vec<f64>)> = Vec::new();
    let mut counts = ClassCounts::default();
    let mut judgements = JudgementCounts::default();
    let (mut accs, mut acpls) = (Vec::new(), Vec::new());
    let mut phase_accs: [Vec<f64>; 3] = Default::default();
    let mut phase_time: [(u64, u32); 3] = [(0, 0); 3];
    let (mut opp_ratings, mut rated_points) = (Vec::new(), 0i64);
    let mut openings: HashMap<String, Wdl> = HashMap::new();
    let mut time = TimeStats::default();
    let mut moments: Vec<Moment> = Vec::new();
    let mut blunders: Vec<Moment> = Vec::new();
    let mut game_accs: Vec<GameMoment> = Vec::new();
    let mut reviewed = 0u32;

    for (m, review) in games {
        let side = side_of(m);
        let o = outcome(m.result, side);
        results.overall.add(o);
        match side {
            Side::White => results.white.add(o),
            Side::Black => results.black.add(o),
        }
        let opp = opponent(m);
        let opp_name = opp.lichess.clone().unwrap_or(opp.name.clone());
        if let (Some(r), Some(o)) = (opp.rating, o) {
            opp_ratings.push(f64::from(r));
            rated_points += i64::from(o);
        }
        if let Some(name) = m.lichess.as_ref().and_then(|l| l.opening.as_deref()) {
            openings.entry(opening_family(name)).or_default().add(o);
        }
        let speed = m.lichess.as_ref().map_or("local".to_owned(), |l| l.speed.clone());
        let at = match speeds.iter().position(|s| s.0 == speed) {
            Some(i) => i,
            None => {
                let empty = SpeedStats {
                    speed: speed.clone(),
                    games: 0,
                    reviewed: 0,
                    results: Wdl::default(),
                    accuracy: None,
                    acpl: None,
                };
                speeds.push((speed, empty, Vec::new(), Vec::new()));
                speeds.len() - 1
            }
        };
        speeds[at].1.games += 1;
        speeds[at].1.results.add(o);

        let Some(review) = review else { continue };
        reviewed += 1;
        speeds[at].1.reviewed += 1;
        let s = review.summary.side(side);
        counts = add_counts(counts, &s.counts);
        judgements.inaccuracy += s.judgements.inaccuracy;
        judgements.mistake += s.judgements.mistake;
        judgements.blunder += s.judgements.blunder;
        if let Some(a) = s.accuracy {
            accs.push(a);
            speeds[at].2.push(a);
            game_accs.push(GameMoment {
                match_id: m.id,
                accuracy: a,
                opponent: opp_name.clone(),
                result: m.result,
                played_at: m.created_at.clone(),
            });
        }
        if let Some(c) = s.acpl {
            acpls.push(f64::from(c));
            speeds[at].3.push(f64::from(c));
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
        let url = m.lichess.as_ref().map(|l| l.url.clone());
        for p in review.plies.iter().filter(|p| p.side == side) {
            let moment = || Moment {
                match_id: m.id,
                ply: p.ply,
                san: p.san.clone(),
                side,
                classification: p.classification,
                win_before: p.win_before,
                win_after: p.win_after,
                sacrifice: p.sacrifice,
                opponent: opp_name.clone(),
                played_at: m.created_at.clone(),
                url: url.as_ref().map(|u| format!("{u}#{}", p.ply)),
            };
            match p.classification {
                Classification::Brilliant | Classification::Great => moments.push(moment()),
                Classification::Blunder | Classification::Miss | Classification::Mistake
                    if p.judgement == Some(super::judge::Judgement::Blunder) =>
                {
                    blunders.push(moment())
                }
                _ => {}
            }
        }
    }

    let mut longest = 0u32;
    let mut streak = 0u32;
    for (m, _) in games.iter().rev() {
        if outcome(m.result, side_of(m)) == Some(1) {
            streak += 1;
            longest = longest.max(streak);
        } else {
            streak = 0;
        }
    }
    let brilliant_rank = |m: &Moment| (u8::from(m.classification == Classification::Brilliant), m.sacrifice);
    moments.sort_by(|a, b| brilliant_rank(b).cmp(&brilliant_rank(a)).then(b.win_after.total_cmp(&a.win_after)));
    moments.truncate(3);
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
    let avg_opp = exact_opp.map(round1);
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
    let mine = newest.map(me);
    PlayerStats {
        player: player.to_owned(),
        username: mine.as_ref().and_then(|p| p.lichess.clone()),
        title: mine.as_ref().and_then(|p| p.title.clone()),
        rating: mine.as_ref().and_then(|p| p.rating),
        games: games.len() as u32,
        reviewed,
        pending_review: games.len() as u32 - reviewed,
        first_game: games.last().map(|(m, _)| m.created_at.clone()),
        last_game: newest.map(|m| m.created_at.clone()),
        results,
        accuracy: AccuracyStats {
            overall: average(&accs),
            opening: average(&phase_accs[0]),
            middlegame: average(&phase_accs[1]),
            endgame: average(&phase_accs[2]),
        },
        acpl: average(&acpls),
        by_speed: speeds
            .into_iter()
            .map(|(_, mut s, a, c)| {
                s.accuracy = average(&a);
                s.acpl = average(&c);
                s
            })
            .collect(),
        counts,
        per_game: ClassRates::of(&counts, reviewed),
        judgements,
        opponents: Opponents {
            average_rating: avg_opp,
            performance: exact_opp.map(|a| round1(a + 400.0 * rated_points as f64 / n as f64)),
        },
        openings,
        time,
        longest_win_streak: longest,
        notable: Notable { brilliant: moments, blunders, most_accurate_game: most, least_accurate_game: least },
    }
}

fn add_counts(mut a: ClassCounts, b: &ClassCounts) -> ClassCounts {
    for c in Classification::ALL {
        a.add(c, b.get(c));
    }
    a
}

/// A player's finished games, newest first, each with its newest review.
pub fn games_of(
    c: &Connection,
    who: &Who,
    speeds: &[String],
    rated: Option<bool>,
    since: Option<&str>,
    limit: u32,
) -> Result<Vec<(MatchSummary, Option<Review>)>> {
    let (owner, owner_arg) = who.sql();
    let (filters, filter_args) = game_filters(speeds, rated);
    let since_sql = if since.is_some() { " AND m.created_at >= ?" } else { "" };
    let sql = format!(
        "SELECT m.data FROM matches m WHERE m.status = 'finished' AND {owner}{filters}{since_sql}
         ORDER BY m.created_at DESC, m.id DESC LIMIT {limit}"
    );
    let args: Vec<String> = owner_arg.into_iter().chain(filter_args).chain(since.map(str::to_owned)).collect();
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter().map(|a| a as &dyn ToSql)), |r| r.get::<_, String>(0))?;
    let mut games = Vec::new();
    for data in rows {
        games.push(serde_json::from_str::<Match>(&data?).context("read a stored match")?.summary());
    }
    let ids: Vec<i64> = games.iter().map(|m| m.id).collect();
    let mut reviews = load_many(c, &ids)?;
    Ok(games
        .into_iter()
        .map(|m| {
            let r = reviews.remove(&m.id);
            (m, r)
        })
        .collect())
}
