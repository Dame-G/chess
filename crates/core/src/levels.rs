//! Levels: an engine (Stockfish, Reckless, one you added) at a strength you choose, from about 800 to
//! full strength, and the maths that calibrates them.
//!
//! A level keeps the engine's own play. Its strength comes from a node budget, and at the lowest levels from a softmax
//! over the engine's MultiPV candidates ([`choose_move`]): the move is always one the engine itself put forward, never
//! a random blunder. Every level's rating is **measured**, not assumed: levels play each other (and full strength) in
//! the background, you play them, and [`fit`] turns all those results into an Elo per level with a 95 % interval,
//! anchored to your Lichess rating. [`plan`] says which pairs of levels to play next.
//!
//! Every engine that plays on the clock has this ladder: the same budgets and temperatures, its own ratings (each
//! starting from the nominal ones), its own calibration. An engine without UCI's `MultiPV` plays each budget alone: its
//! best move at that budget.
//!
//! The ladder ([`ladder`]) and the reasoning behind its node budgets are in `docs/roadmap.md` (R3, "Levels").

use std::{borrow::Cow, collections::HashMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{RefOr, Schema, schema::ObjectBuilder, schema::Type},
};

use crate::{
    game::Outcome,
    score::{Score, win_percent},
};

/// An Elo with its 95 % interval.
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
}

/// The ladder engine at one strength.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Level {
    /// 1 is the weakest; the last level of the ladder is full strength.
    pub id: u32,
    /// `Level 5`; the UI writes "Stockfish 1600" from the calibrated rating.
    pub name: String,
    /// Nodes per move. `None` is full strength: the engine plays on the clock, as in any other game (calibration
    /// games give it [`FULL_CALIBRATION_NODES`]).
    pub nodes: Option<u64>,
    /// Lines searched: the candidates [`choose_move`] picks among. 1 plays the engine's best move.
    pub multipv: u32,
    /// The softmax temperature, in win percentage points (0 to 100) of the side to move: a candidate that many points
    /// worse than the best is chosen `e` times less often. 0 always plays the best move.
    pub temperature: f64,
    /// The rating the level was designed for, until calibration measures it.
    pub nominal: u32,
}

impl Level {
    /// Whether this is full strength.
    pub fn is_full(&self) -> bool {
        self.nodes.is_none()
    }

    /// Nodes per move in a calibration game, where every level, full strength included, plays at fixed nodes.
    pub fn calibration_nodes(&self) -> u64 {
        self.nodes.unwrap_or(FULL_CALIBRATION_NODES)
    }
}

/// Full strength's node budget in calibration games: about a second of search on one core, so a calibration game
/// against it costs about as much as a short test game.
pub const FULL_CALIBRATION_NODES: u64 = 1_000_000;

/// Games a calibration match between two levels plays at once (colours alternate, the book picks the openings).
pub const CALIBRATION_GAMES: u32 = 20;

/// How sure the ladder's designed ratings are, as a standard deviation in Elo: a weak prior, so a few dozen games move
/// a level a long way, while a level nobody has played yet still reads as its design.
pub const NOMINAL_SD: f64 = 350.0;

/// How sure your Lichess rating is as an anchor when Lichess does not say (an established rating's deviation is
/// usually 45 to 80; the gap between Lichess's pool and play here adds to it).
pub const ANCHOR_SD: f64 = 120.0;

/// The default ladder, weakest first. See `docs/roadmap.md` (R3, "Levels") for why these budgets.
pub fn ladder() -> Vec<Level> {
    // (nominal, nodes, multipv, temperature)
    const LADDER: [(u32, Option<u64>, u32, f64); 12] = [
        (800, Some(300), 8, 15.0),
        (1000, Some(300), 6, 10.0),
        (1200, Some(500), 5, 6.0),
        (1400, Some(800), 4, 3.5),
        (1600, Some(1_500), 3, 2.0),
        (1800, Some(3_000), 2, 1.0),
        (2000, Some(6_000), 1, 0.0),
        (2200, Some(15_000), 1, 0.0),
        (2400, Some(40_000), 1, 0.0),
        (2600, Some(120_000), 1, 0.0),
        (2800, Some(400_000), 1, 0.0),
        (3000, None, 1, 0.0),
    ];
    LADDER
        .iter()
        .enumerate()
        .map(|(i, &(nominal, nodes, multipv, temperature))| {
            let id = i as u32 + 1;
            let name = if nodes.is_none() { "Full strength".to_owned() } else { format!("Level {id}") };
            Level { id, name, nodes, multipv, temperature, nominal }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------------
// Choosing a move
// ---------------------------------------------------------------------------------------------------------------------

/// The softmax weights of `candidates` (each move and its score from the **side to move's** point of view, as the
/// engine reports a MultiPV line), in their order: `exp(−(best win% − win%) / temperature)`, normalised to sum to 1.
/// A temperature of 0 (or less) puts all the weight on the first best move.
pub fn weights(candidates: &[(String, Score)], temperature: f64) -> Vec<f64> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let wins: Vec<f64> = candidates.iter().map(|(_, s)| mover_win(*s)).collect();
    let best = wins.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if temperature <= 0.0 || !temperature.is_finite() {
        let first = wins.iter().position(|w| *w == best).unwrap_or(0);
        return (0..wins.len()).map(|i| if i == first { 1.0 } else { 0.0 }).collect();
    }
    let raw: Vec<f64> = wins.iter().map(|w| (-(best - w) / temperature).exp()).collect();
    let total: f64 = raw.iter().sum();
    raw.into_iter().map(|w| w / total).collect()
}

/// A score's win percentage, a quicker mate counting a little more than a slower one so a mate is never diluted
/// among moves that merely win (they share Lichess's ±1000 cap).
fn mover_win(score: Score) -> f64 {
    match score {
        Score::Mate(m) if m > 0 => 100.0 + 1.0 / f64::from(m),
        Score::Mate(m) => -1.0 / f64::from(m.unsigned_abs().max(1)),
        Score::Cp(cp) => win_percent(cp),
    }
}

/// The move a level plays among the engine's candidates (scores from the side to move's point of view), given a
/// uniform random number `u` in [0, 1): the first candidate whose cumulative [`weights`] reach `u`. The same `u`
/// always picks the same move, so a game can be replayed and the rule tested. `None` without candidates.
pub fn choose_move(candidates: &[(String, Score)], temperature: f64, u: f64) -> Option<String> {
    let weights = weights(candidates, temperature);
    let u = if u.is_finite() { u.clamp(0.0, 1.0) } else { 0.0 };
    let mut sum = 0.0;
    for (i, w) in weights.iter().enumerate() {
        sum += w;
        if *w > 0.0 && u < sum {
            return Some(candidates[i].0.clone());
        }
    }
    // Rounding can leave `sum` a hair under 1: the last candidate with weight takes the rest.
    weights.iter().rposition(|w| *w > 0.0).map(|i| candidates[i].0.clone())
}

// ---------------------------------------------------------------------------------------------------------------------
// Calibration
// ---------------------------------------------------------------------------------------------------------------------

/// Someone in the calibration: a level, or You. Serialized as `level:<id>` or `you`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entrant {
    Level(u32),
    You,
}

/// Why an entrant does not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not an entrant: use level:<id> or you")]
pub struct BadEntrant(pub String);

impl fmt::Display for Entrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Level(id) => write!(f, "level:{id}"),
            Self::You => f.write_str("you"),
        }
    }
}

impl FromStr for Entrant {
    type Err = BadEntrant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "you" => Ok(Self::You),
            other => other
                .strip_prefix("level:")
                .and_then(|id| id.parse::<u32>().ok())
                .filter(|id| *id > 0)
                .map(Self::Level)
                .ok_or_else(|| BadEntrant(s.to_owned())),
        }
    }
}

impl Serialize for Entrant {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Entrant {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Cow::<str>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl PartialSchema for Entrant {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .description(Some("`level:<id>` or `you`"))
            .examples(["level:5", "you"])
            .into()
    }
}

impl ToSchema for Entrant {}

/// One finished game of the calibration: who had White, who had Black, and how it ended.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LevelGame {
    pub white: Entrant,
    pub black: Entrant,
    pub outcome: Outcome,
}

/// The games between two entrants added up, from `a`'s side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PairResult {
    pub a: Entrant,
    pub b: Entrant,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl PairResult {
    pub fn games(&self) -> u32 {
        self.wins + self.draws + self.losses
    }

    /// `a`'s points.
    pub fn points(&self) -> f64 {
        f64::from(self.wins) + 0.5 * f64::from(self.draws)
    }
}

/// Games added up per pair of entrants (the smaller entrant as `a`, levels before You), sorted. Games of an entrant
/// against itself say nothing and are left out.
pub fn pair_results(games: &[LevelGame]) -> Vec<PairResult> {
    let mut pairs: HashMap<(Entrant, Entrant), PairResult> = HashMap::new();
    for g in games {
        if g.white == g.black {
            continue;
        }
        let (a, b) = if g.white < g.black { (g.white, g.black) } else { (g.black, g.white) };
        let r = pairs.entry((a, b)).or_insert(PairResult { a, b, wins: 0, draws: 0, losses: 0 });
        match g.outcome.winner() {
            None => r.draws += 1,
            Some(side) => {
                let winner = if side == crate::Side::White { g.white } else { g.black };
                if winner == a { r.wins += 1 } else { r.losses += 1 }
            }
        }
    }
    let mut out: Vec<PairResult> = pairs.into_values().collect();
    out.sort_by_key(|r| (r.a, r.b));
    out
}

/// What is believed about an entrant before the games: a rating and how sure (a standard deviation, in Elo). Every
/// entrant in a fit has one: the designed rating for a level ([`NOMINAL_SD`]), your Lichess rating for You
/// ([`ANCHOR_SD`] or Lichess's deviation). The priors are what put the levels on the human scale and keep a level that
/// won or lost every game finite.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Prior {
    pub entrant: Entrant,
    pub elo: f64,
    pub sd: f64,
}

/// The priors for a ladder: each level at its designed rating, and You at your rating when there is one.
pub fn priors(levels: &[Level], you: Option<(u32, Option<f64>)>) -> Vec<Prior> {
    let mut out: Vec<Prior> = levels
        .iter()
        .map(|l| Prior { entrant: Entrant::Level(l.id), elo: f64::from(l.nominal), sd: NOMINAL_SD })
        .collect();
    if let Some((rating, sd)) = you {
        out.push(Prior { entrant: Entrant::You, elo: f64::from(rating), sd: sd.unwrap_or(ANCHOR_SD).max(1.0) });
    }
    out
}

/// An entrant's fitted rating: the estimate with its 95 % interval, and the games it rests on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EntrantRating {
    pub entrant: Entrant,
    pub estimate: Estimate,
    pub games: u32,
}

/// The logistic Elo model's constant: `P(a beats b) = 1 / (1 + 10^((Rb − Ra) / 400))`, so `dP/dRa = K·P·(1 − P)`.
const K: f64 = std::f64::consts::LN_10 / 400.0;
const Z95: f64 = 1.959963984540054;

/// The expected score of a rating `a` against `b` under the logistic Elo model.
pub fn expected(a: f64, b: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((b - a) / 400.0))
}

/// Fits a rating to every entrant that has a prior: the maximum a posteriori of the Bradley–Terry (logistic Elo)
/// likelihood of `results`, a draw counting half a win, times the Gaussian priors. Newton's method on the log
/// posterior, which is strictly concave because every entrant has a prior. The 95 % intervals come from the inverse
/// of its curvature (the observed Fisher information plus the priors' precision). Results naming an entrant without a
/// prior are left out. Sorted as the priors are.
pub fn fit(priors: &[Prior], results: &[PairResult]) -> Vec<EntrantRating> {
    let n = priors.len();
    let index: HashMap<Entrant, usize> = priors.iter().enumerate().map(|(i, p)| (p.entrant, i)).collect();
    let pairs: Vec<(usize, usize, f64, f64)> = results
        .iter()
        .filter(|r| r.games() > 0)
        .filter_map(|r| Some((*index.get(&r.a)?, *index.get(&r.b)?, f64::from(r.games()), r.points())))
        .filter(|(a, b, _, _)| a != b)
        .collect();
    let mut games = vec![0u32; n];
    for &(a, b, g, _) in &pairs {
        games[a] += g as u32;
        games[b] += g as u32;
    }

    let mut r: Vec<f64> = priors.iter().map(|p| p.elo).collect();
    let precision: Vec<f64> = priors.iter().map(|p| 1.0 / p.sd.max(1.0).powi(2)).collect();
    let curvature = |r: &[f64]| -> (Vec<f64>, Vec<Vec<f64>>) {
        // The gradient and the negative Hessian of the log posterior.
        let mut g: Vec<f64> = (0..n).map(|i| -(r[i] - priors[i].elo) * precision[i]).collect();
        let mut h = vec![vec![0.0; n]; n];
        for i in 0..n {
            h[i][i] = precision[i];
        }
        for &(a, b, count, points) in &pairs {
            let p = expected(r[a], r[b]);
            let d = K * (points - count * p);
            g[a] += d;
            g[b] -= d;
            let w = K * K * count * p * (1.0 - p);
            h[a][a] += w;
            h[b][b] += w;
            h[a][b] -= w;
            h[b][a] -= w;
        }
        (g, h)
    };
    for _ in 0..100 {
        let (g, h) = curvature(&r);
        let Some(step) = solve(h, g) else { break };
        // Damped, so a first step from far away cannot overshoot into the flat tails of the logistic.
        let biggest = step.iter().fold(0.0f64, |m, s| m.max(s.abs()));
        let scale = if biggest > 200.0 { 200.0 / biggest } else { 1.0 };
        for i in 0..n {
            r[i] += step[i] * scale;
        }
        if biggest < 1e-6 {
            break;
        }
    }
    let (_, h) = curvature(&r);
    let variance = invert(h).map(|inv| (0..n).map(|i| inv[i][i]).collect::<Vec<_>>());
    priors
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let sd = variance.as_ref().map_or(p.sd, |v| v[i].max(0.0).sqrt());
            EntrantRating { entrant: p.entrant, estimate: Estimate::plus_minus(r[i], Z95 * sd), games: games[i] }
        })
        .collect()
}

/// Solves `a · x = b` by Gaussian elimination with partial pivoting; `None` when `a` is singular.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-15 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let (top, rest) = a.split_at_mut(col + 1);
        let pivot_row = &top[col];
        for (i, row) in rest.iter_mut().enumerate() {
            let f = row[col] / pivot_row[col];
            for (cell, p) in row.iter_mut().zip(pivot_row).skip(col) {
                *cell -= f * p;
            }
            b[col + 1 + i] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}

/// The inverse of `a`, column by column.
fn invert(a: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut columns = Vec::with_capacity(n);
    for c in 0..n {
        let e: Vec<f64> = (0..n).map(|i| if i == c { 1.0 } else { 0.0 }).collect();
        columns.push(solve(a.clone(), e)?);
    }
    Some((0..n).map(|i| (0..n).map(|j| columns[j][i]).collect()).collect())
}

/// The level whose rating is nearest `target` (your rating, for "Play this out" and the New game dialog's default).
/// `None` when no level has a rating.
pub fn nearest_level(ratings: &[EntrantRating], target: f64) -> Option<u32> {
    ratings
        .iter()
        .filter_map(|r| match r.entrant {
            Entrant::Level(id) => Some((id, (r.estimate.elo - target).abs())),
            Entrant::You => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

/// A calibration match to play next.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlannedMatch {
    /// The two levels, the weaker first.
    pub a: u32,
    pub b: u32,
    pub games: u32,
    /// How much the match is expected to narrow the ladder's intervals, relative to the others: the higher, the
    /// sooner.
    pub priority: f64,
}

/// The `count` calibration matches to play next, most useful first. Only neighbouring levels are paired (a game
/// between levels far apart is nearly always won by the stronger and says little), and a pair's priority is the sum
/// of its two intervals' widths times `p·(1 − p)` at their current ratings: the information one game carries about
/// the gap. So the widest, most even pairs come first. Levels without a rating in `ratings` count as their prior's
/// width ([`NOMINAL_SD`]).
pub fn plan(levels: &[Level], ratings: &[EntrantRating], count: usize) -> Vec<PlannedMatch> {
    let mut sorted: Vec<&Level> = levels.iter().collect();
    sorted.sort_by_key(|l| l.id);
    let rating = |l: &Level| -> (f64, f64) {
        ratings
            .iter()
            .find(|r| r.entrant == Entrant::Level(l.id))
            .map_or((f64::from(l.nominal), 2.0 * Z95 * NOMINAL_SD), |r| {
                (r.estimate.elo, r.estimate.high - r.estimate.low)
            })
    };
    let mut out: Vec<PlannedMatch> = sorted
        .windows(2)
        .map(|w| {
            let ((ra, wa), (rb, wb)) = (rating(w[0]), rating(w[1]));
            let p = expected(ra, rb);
            PlannedMatch { a: w[0].id, b: w[1].id, games: CALIBRATION_GAMES, priority: (wa + wb) * p * (1.0 - p) }
        })
        .collect();
    out.sort_by(|x, y| y.priority.total_cmp(&x.priority).then(x.a.cmp(&y.a)));
    out.truncate(count);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(moves: &[(&str, Score)]) -> Vec<(String, Score)> {
        moves.iter().map(|(m, s)| ((*m).to_owned(), *s)).collect()
    }

    #[test]
    fn the_ladder_climbs_from_800_to_full_strength() {
        let l = ladder();
        assert_eq!(l.first().unwrap().nominal, 800);
        assert!(l.last().unwrap().is_full());
        assert_eq!(l.last().unwrap().calibration_nodes(), FULL_CALIBRATION_NODES);
        assert!(l.windows(2).all(|w| w[0].nominal < w[1].nominal && w[0].id + 1 == w[1].id));
        // Weaker levels never search more, nor pick more deterministically.
        assert!(l.windows(2).all(|w| w[0].calibration_nodes() <= w[1].calibration_nodes()));
        assert!(l.windows(2).all(|w| w[0].temperature >= w[1].temperature && w[0].multipv >= w[1].multipv));
        assert!(l.iter().filter(|l| l.temperature == 0.0).all(|l| l.multipv == 1));
        assert_eq!(l[4].name, "Level 5");
    }

    #[test]
    fn zero_temperature_plays_the_best_move() {
        let cands = c(&[("e2e4", Score::Cp(30)), ("d2d4", Score::Cp(35)), ("g1f3", Score::Cp(35))]);
        for u in [0.0, 0.5, 0.999] {
            assert_eq!(choose_move(&cands, 0.0, u).as_deref(), Some("d2d4"));
        }
        assert_eq!(choose_move(&[], 5.0, 0.3), None);
    }

    #[test]
    fn softmax_follows_win_percentages_and_is_deterministic() {
        let cands = c(&[("a", Score::Cp(0)), ("b", Score::Cp(-100))]);
        let w = weights(&cands, 9.1);
        // -100 cp is 40.9 win% against 50: 9.1 points worse, so e times less likely.
        assert!((w[0] / w[1] - std::f64::consts::E).abs() < 0.01, "{w:?}");
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let cut = w[0];
        assert_eq!(choose_move(&cands, 9.1, cut - 1e-9).as_deref(), Some("a"));
        assert_eq!(choose_move(&cands, 9.1, cut + 1e-9).as_deref(), Some("b"));
        assert_eq!(choose_move(&cands, 9.1, 1.0).as_deref(), Some("b"));
        assert_eq!(choose_move(&cands, 9.1, f64::NAN).as_deref(), Some("a"));
        // A hotter level spreads its choices; a cold one almost never strays.
        assert!(weights(&cands, 30.0)[1] > weights(&cands, 3.0)[1]);
        assert!(weights(&cands, 0.5)[1] < 1e-7);
    }

    #[test]
    fn mates_outrank_winning_moves_and_getting_mated_ranks_last() {
        let cands = c(&[("win", Score::Cp(900)), ("mate", Score::Mate(3)), ("lose", Score::Mate(-2))]);
        let w = weights(&cands, 1.0);
        assert!(w[1] > w[0] && w[0] > w[2]);
        assert_eq!(choose_move(&cands, 0.0, 0.0).as_deref(), Some("mate"));
    }

    #[test]
    fn entrants_read_and_write_as_strings() {
        assert_eq!(Entrant::Level(3).to_string(), "level:3");
        assert_eq!("you".parse::<Entrant>(), Ok(Entrant::You));
        assert_eq!("level:12".parse::<Entrant>(), Ok(Entrant::Level(12)));
        assert!("level:0".parse::<Entrant>().is_err() && "lvl:1".parse::<Entrant>().is_err());
        assert_eq!(serde_json::to_value(Entrant::Level(2)).unwrap(), serde_json::json!("level:2"));
        assert_eq!(serde_json::from_value::<Entrant>(serde_json::json!("you")).unwrap(), Entrant::You);
    }

    #[test]
    fn games_add_up_per_pair() {
        let g = |w, b, o| LevelGame { white: Entrant::Level(w), black: Entrant::Level(b), outcome: o };
        let games = [
            g(1, 2, Outcome::WhiteWins),
            g(2, 1, Outcome::WhiteWins),
            g(2, 1, Outcome::Draw),
            g(3, 3, Outcome::Draw),
            LevelGame { white: Entrant::You, black: Entrant::Level(2), outcome: Outcome::BlackWins },
        ];
        let r = pair_results(&games);
        assert_eq!(
            r,
            [
                PairResult { a: Entrant::Level(1), b: Entrant::Level(2), wins: 1, draws: 1, losses: 1 },
                PairResult { a: Entrant::Level(2), b: Entrant::You, wins: 1, draws: 0, losses: 0 },
            ]
        );
        assert_eq!(r[0].points(), 1.5);
    }

    fn pair(a: Entrant, b: Entrant, wins: u32, draws: u32, losses: u32) -> PairResult {
        PairResult { a, b, wins, draws, losses }
    }

    #[test]
    fn without_games_the_fit_is_the_prior() {
        let p = priors(&ladder()[..3], None);
        let r = fit(&p, &[]);
        assert_eq!(r.len(), 3);
        for (r, p) in r.iter().zip(&p) {
            assert!((r.estimate.elo - p.elo).abs() < 1e-6);
            assert!((r.estimate.error() - Z95 * NOMINAL_SD).abs() < 1e-6);
            assert_eq!(r.games, 0);
        }
    }

    #[test]
    fn the_fit_recovers_a_known_gap() {
        // A beats B 76 % of the time: 200 Elo. With weak priors at equal ratings and many games, the gap is found.
        let (a, b) = (Entrant::Level(1), Entrant::Level(2));
        let p = [Prior { entrant: a, elo: 1500.0, sd: 10_000.0 }, Prior { entrant: b, elo: 1500.0, sd: 10_000.0 }];
        let score = expected(200.0, 0.0);
        let games = 10_000u32;
        let wins = (score * f64::from(games)).round() as u32;
        let r = fit(&p, &[pair(a, b, wins, 0, games - wins)]);
        let gap = r[0].estimate.elo - r[1].estimate.elo;
        assert!((gap - 200.0).abs() < 1.0, "{gap}");
        // The mean stays where the priors put it; with priors this weak, only the gap is known, not where it sits.
        assert!(((r[0].estimate.elo + r[1].estimate.elo) / 2.0 - 1500.0).abs() < 1.0);
        assert!(r[0].estimate.error() > 1000.0 && r[0].games == games);
        // Pin A, and B's interval is the gap's: narrow after many games.
        let pinned = [Prior { sd: 1.0, ..p[0] }, p[1]];
        let r = fit(&pinned, &[pair(a, b, wins, 0, games - wins)]);
        assert!((r[1].estimate.elo - 1300.0).abs() < 2.0, "{:?}", r[1]);
        assert!(r[1].estimate.error() < 15.0 && r[1].estimate.error() > 5.0, "{:?}", r[1]);
    }

    #[test]
    fn a_clean_sweep_stays_finite() {
        let (a, b) = (Entrant::Level(1), Entrant::Level(2));
        let p = priors(&ladder()[..2], None);
        let r = fit(&p, &[pair(a, b, 0, 0, 40)]);
        assert!(r.iter().all(|r| r.estimate.elo.is_finite() && r.estimate.error().is_finite()));
        assert!(r[1].estimate.elo - r[0].estimate.elo > 400.0);
    }

    #[test]
    fn your_rating_anchors_the_ladder() {
        // The levels think they are 800 apart in design, but they drew each other a lot, and you (1500 on Lichess,
        // well established) score 50 % against level 2: level 2 is pulled to about 1500.
        let ladder = &ladder()[..2];
        let (one, two) = (Entrant::Level(1), Entrant::Level(2));
        let p = priors(ladder, Some((1500, Some(50.0))));
        let results = [pair(one, two, 20, 60, 20), pair(two, Entrant::You, 50, 0, 50)];
        let r = fit(&p, &results);
        let elo = |e: Entrant| r.iter().find(|r| r.entrant == e).unwrap().estimate.elo;
        assert!((elo(Entrant::You) - 1500.0).abs() < 60.0, "{}", elo(Entrant::You));
        assert!((elo(two) - elo(Entrant::You)).abs() < 40.0);
        assert!((elo(one) - elo(two)).abs() < 80.0);
        assert_eq!(nearest_level(&r, 1480.0), Some(2));
        // Results with someone outside the fit are left out.
        let r2 = fit(&p[..2], &results);
        assert_eq!(r2.len(), 2);
    }

    #[test]
    fn the_plan_pairs_neighbours_with_the_widest_intervals() {
        let l = ladder();
        let mut ratings = fit(&priors(&l, None), &[]);
        // Levels 1 and 2 are settled; the rest are not.
        for r in ratings.iter_mut().take(2) {
            r.estimate = Estimate::plus_minus(r.estimate.elo, 20.0);
        }
        let planned = plan(&l, &ratings, 3);
        assert_eq!(planned.len(), 3);
        assert!(planned.iter().all(|m| m.b == m.a + 1 && m.games == CALIBRATION_GAMES));
        assert!(!planned.iter().any(|m| m.a == 1));
        assert!(planned.windows(2).all(|w| w[0].priority >= w[1].priority));
        // An even pair carries more information than a lopsided one of the same widths.
        let mut lopsided = fit(&priors(&l, None), &[]);
        lopsided[1].estimate = Estimate::plus_minus(2000.0, Z95 * NOMINAL_SD);
        let first = |r: &[EntrantRating]| plan(&l[..3], r, 2).into_iter().find(|m| m.a == 1).unwrap().priority;
        assert!(first(&lopsided) < first(&fit(&priors(&l, None), &[])));
    }

    #[test]
    fn the_linear_algebra_is_right() {
        let a = vec![vec![4.0, 1.0], vec![1.0, 3.0]];
        let x = solve(a.clone(), vec![1.0, 2.0]).unwrap();
        assert!((x[0] - 1.0 / 11.0).abs() < 1e-12 && (x[1] - 7.0 / 11.0).abs() < 1e-12);
        let inv = invert(a).unwrap();
        assert!((inv[0][0] - 3.0 / 11.0).abs() < 1e-12 && (inv[0][1] + 1.0 / 11.0).abs() < 1e-12);
        assert!(solve(vec![vec![1.0, 2.0], vec![2.0, 4.0]], vec![1.0, 2.0]).is_none());
    }
}
