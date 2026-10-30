//! Opening prep: the lines you play as one colour, by the opening Lichess names them, with how you score in each and
//! how often you go wrong early in it. A line is flagged when you score badly in it or often make a mistake in its
//! first moves, and each line carries the games and the worst mistake to drill and to explore.
//!
//! Lines are the named openings of your games (their `ply` says how long the named line is), so a line means what
//! Lichess means by it. Early is the first twelve moves of either side. This is the pure part: the service adds each
//! line's moves from its newest game ([`PrepLine::set_moves`]).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    chess::Position,
    game::{GameStatus, Outcome, opening_title},
    ids::{GameId, PlayerKey, Side, Timestamp},
    profile::{ProfileGame, WeakSpot, weak_spots_of},
};

/// A mistake counts as early up to this ply (twelve moves each).
pub const EARLY_PLY: u32 = 24;
/// A line needs this many games to be listed.
pub const MIN_LINE_GAMES: u32 = 2;
/// Lines a prep lists, at most.
pub const LINES: usize = 12;
/// Games a line names for drilling, at most.
pub const DRILL_GAMES: usize = 60;
/// A line scores badly below this share of the points, once this many of its games were decided.
pub const BAD_SCORE: f64 = 0.45;
pub const BAD_SCORE_MIN_GAMES: u32 = 4;
/// A line goes wrong early when this share of its reviewed games has an early mistake, once this many are reviewed.
pub const EARLY_SHARE: f64 = 0.3;
pub const EARLY_MIN_REVIEWED: u32 = 3;

/// `GET /api/players/you/prep?side=&limit=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct PrepRequest {
    pub side: Side,
    /// Your newest finished games read, at most (500).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// One opening line you play and how it goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PrepLine {
    /// The opening's name: `Italian Game: Giuoco Piano`.
    pub name: String,
    pub eco: Option<String>,
    /// The line's moves (UCI) from the start, and as one PGN string (`1. e4 e5 2. Nf3 Nc6`); the position after it.
    pub moves: Vec<String>,
    pub pgn: String,
    pub fen: String,
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Your points per decided game, 0 to 1.
    pub score: Option<f64>,
    /// Games of the line that are reviewed, and how many of them have a mistake of yours in the first twelve moves.
    pub reviewed: u32,
    pub early_games: u32,
    /// Your early mistakes and blunders in the line, and the win percentage they cost in all.
    pub early_mistakes: u32,
    pub early_swing: f64,
    /// The costliest early mistake, to open on the board.
    pub worst: Option<WeakSpot>,
    /// The newest games with an early mistake, to drill (`GET /api/drills/today?games=…&max_ply=24`).
    pub drill_games: Vec<GameId>,
    /// Your score is under 45 % over at least four decided games.
    pub scores_badly: bool,
    /// At least a third of at least three reviewed games had an early mistake.
    pub goes_wrong_early: bool,
    /// The newest game of the line.
    pub example: GameId,
}

impl PrepLine {
    /// Fills the line's moves and the position after them from `moves` (UCI): the first plies of the newest game.
    /// Moves that stop being legal end the line there.
    pub fn set_moves(&mut self, moves: &[String], plies: u32) {
        let mut pos = Position::start();
        let mut taken = Vec::new();
        let mut pgn = Vec::new();
        for uci in moves.iter().take(plies as usize) {
            let number = pos.fullmoves();
            let white = pos.turn() == Side::White;
            let Ok(played) = pos.play(uci) else { break };
            pgn.push(match (white, taken.is_empty()) {
                (true, _) => format!("{number}. {}", played.san),
                (false, true) => format!("{number}... {}", played.san),
                (false, false) => played.san,
            });
            taken.push(played.uci);
        }
        self.moves = taken;
        self.pgn = pgn.join(" ");
        self.fen = pos.fen();
    }
}

/// Your lines with one colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Prep {
    pub side: Side,
    /// Finished standard games of yours with this colour that have a named opening.
    pub games: u32,
    /// Most played first, then the flagged ones.
    pub lines: Vec<PrepLine>,
}

#[derive(Default)]
struct Acc {
    eco: Option<String>,
    plies: u32,
    wdl: [u32; 3],
    reviewed: u32,
    early_games: u32,
    early_mistakes: u32,
    early_swing: f64,
    worst: Option<WeakSpot>,
    with_mistakes: Vec<(Timestamp, GameId)>,
    newest: Option<(Timestamp, GameId)>,
    games: u32,
}

/// The lines of `side` in your finished games (`games`, which carry reviews for the mistakes). A line's moves are
/// empty until [`PrepLine::set_moves`]; its newest game is `example`, and `plies` on the result is how many of its
/// moves to take, returned beside it.
pub fn build(side: Side, games: &[ProfileGame]) -> (Prep, Vec<u32>) {
    let mut lines: HashMap<String, Acc> = HashMap::new();
    let mut total = 0;
    for g in games {
        let s = &g.summary;
        let standard =
            g.start_fen.as_deref().is_none_or(|f| Position::from_fen(f).is_ok_and(|p| p.is_standard_start()));
        if s.status != GameStatus::Finished || !standard || s.side_of(&PlayerKey::You) != Some(side) {
            continue;
        }
        let Some(opening) = s.opening.as_ref().filter(|o| o.ply.is_some_and(|p| p > 0)) else { continue };
        let name = opening_title(&opening.name);
        if name.is_empty() {
            continue;
        }
        total += 1;
        let acc = lines.entry(name).or_default();
        acc.games += 1;
        acc.eco = acc.eco.take().or_else(|| opening.eco.clone());
        acc.plies = acc.plies.max(opening.ply.unwrap_or_default());
        match s.outcome {
            Some(Outcome::Draw) => acc.wdl[1] += 1,
            Some(o) if o.winner() == Some(side) => acc.wdl[0] += 1,
            Some(_) => acc.wdl[2] += 1,
            None => {}
        }
        let key = (s.started_at.clone(), s.id);
        if acc.newest.as_ref().is_none_or(|n| key > *n) {
            acc.newest = Some(key.clone());
        }
        if let Some(review) = &g.review {
            acc.reviewed += 1;
            let early: Vec<WeakSpot> =
                weak_spots_of(g, review, side).into_iter().filter(|w| w.ply <= EARLY_PLY).collect();
            if !early.is_empty() {
                acc.early_games += 1;
                acc.early_mistakes += early.len() as u32;
                acc.early_swing += early.iter().map(|w| w.swing).sum::<f64>();
                acc.with_mistakes.push(key);
                for w in early {
                    if acc.worst.as_ref().is_none_or(|b| w.swing > b.swing) {
                        acc.worst = Some(w);
                    }
                }
            }
        }
    }

    let mut out: Vec<(PrepLine, u32)> = lines
        .into_iter()
        .filter(|(_, a)| a.games >= MIN_LINE_GAMES)
        .map(|(name, mut a)| {
            let decided: u32 = a.wdl.iter().sum();
            let score = (decided > 0).then(|| (f64::from(a.wdl[0]) + 0.5 * f64::from(a.wdl[1])) / f64::from(decided));
            a.with_mistakes.sort_by(|x, y| y.cmp(x));
            let (_, example) = a.newest.clone().unwrap_or((Timestamp::from_unix_ms(0), GameId(0)));
            let line = PrepLine {
                name,
                eco: a.eco.clone(),
                moves: Vec::new(),
                pgn: String::new(),
                fen: Position::start().fen(),
                games: a.games,
                wins: a.wdl[0],
                draws: a.wdl[1],
                losses: a.wdl[2],
                score,
                reviewed: a.reviewed,
                early_games: a.early_games,
                early_mistakes: a.early_mistakes,
                early_swing: (a.early_swing * 10.0).round() / 10.0,
                worst: a.worst.clone(),
                drill_games: a.with_mistakes.iter().take(DRILL_GAMES).map(|(_, id)| *id).collect(),
                scores_badly: decided >= BAD_SCORE_MIN_GAMES && score.is_some_and(|s| s < BAD_SCORE),
                goes_wrong_early: a.reviewed >= EARLY_MIN_REVIEWED
                    && f64::from(a.early_games) / f64::from(a.reviewed) >= EARLY_SHARE,
                example,
            };
            (line, a.plies)
        })
        .collect();
    out.sort_by(|(a, _), (b, _)| b.games.cmp(&a.games).then(a.name.cmp(&b.name)));
    out.truncate(LINES);
    let plies = out.iter().map(|(_, p)| *p).collect();
    let lines = out.into_iter().map(|(l, _)| l).collect();
    (Prep { side, games: total, lines }, plies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::{Ending, GameSummary, Opening, Seat, Source},
        ids::ConnectionKey,
        review::{
            Review, ReviewSummary, ReviewedMove, SideReview,
            judge::{Classification, Judgement, Phase},
        },
        score::Score,
        time::{Speed, TimeControl},
    };

    fn summary(id: i64, you_white: bool, outcome: Outcome, opening: &str, ply: u32) -> GameSummary {
        let you = Seat::new(PlayerKey::You, "Dame");
        let them = Seat::new(PlayerKey::Lichess("bob".into()), "Bob");
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
            opening: Some(Opening { eco: Some("C50".into()), name: format!("C50 {opening}"), ply: Some(ply) }),
            plies: 8,
            started_at: Timestamp::from_unix_ms(id * 1000),
            ended_at: None,
            you: Some(if you_white { Side::White } else { Side::Black }),
            accuracy: None,
            url: None,
        }
    }

    /// A review of 1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4?? with White's third move a mistake (4. Nxe5 was played instead of
    /// Nxd4 in the drill tests: here the White mistake is the third move).
    fn review(game: i64, mistake_ply: Option<u32>) -> Review {
        let line = ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6"];
        let sans = ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6"];
        let plies = line
            .iter()
            .enumerate()
            .map(|(i, uci)| {
                let n = i as u32 + 1;
                let bad = mistake_ply == Some(n);
                ReviewedMove {
                    ply: n,
                    side: if i % 2 == 0 { Side::White } else { Side::Black },
                    uci: (*uci).into(),
                    san: sans[i].into(),
                    eval_before: Score::Cp(20),
                    eval_after: Score::Cp(if bad { -300 } else { 20 }),
                    best_uci: (*uci).into(),
                    best_san: sans[i].into(),
                    best_pv: vec![(*uci).into()],
                    second_uci: None,
                    eval_second: None,
                    win_before: 52.0,
                    win_after: if bad { 20.0 } else { 52.0 },
                    accuracy: 90.0,
                    cp_loss: 0,
                    classification: if bad { Classification::Blunder } else { Classification::Best },
                    judgement: bad.then_some(Judgement::Blunder),
                    phase: Phase::Opening,
                    sacrifice: 0,
                    clock_before_ms: None,
                    depth: 14,
                }
            })
            .collect();
        Review {
            id: crate::ids::ReviewId(game),
            game: GameId(game),
            engine: "Stockfish 19".into(),
            nodes: 1000,
            summary: ReviewSummary {
                engine: "Stockfish 19".into(),
                nodes: 1000,
                white: SideReview::default(),
                black: SideReview::default(),
                middlegame_ply: None,
                endgame_ply: None,
            },
            plies,
            created_at: Timestamp::from_unix_ms(0),
        }
    }

    fn game(id: i64, white: bool, outcome: Outcome, opening: &str, mistake: Option<Option<u32>>) -> ProfileGame {
        ProfileGame {
            summary: summary(id, white, outcome, opening, 5),
            start_fen: None,
            review: mistake.map(|m| review(id, m)),
        }
    }

    #[test]
    fn lines_count_games_score_and_early_mistakes() {
        let games = [
            game(1, true, Outcome::WhiteWins, "Italian Game", Some(None)),
            game(2, true, Outcome::BlackWins, "Italian Game", Some(Some(5))),
            game(3, true, Outcome::BlackWins, "Italian Game", Some(Some(5))),
            game(4, true, Outcome::Draw, "Italian Game", None),
            // As Black, and a line with one game: left out of White's prep.
            game(5, false, Outcome::BlackWins, "Italian Game", Some(None)),
            game(6, true, Outcome::WhiteWins, "Ruy Lopez", None),
        ];
        let (prep, plies) = build(Side::White, &games);
        assert_eq!((prep.side, prep.games, prep.lines.len(), plies), (Side::White, 5, 1, vec![5]));
        let l = &prep.lines[0];
        assert_eq!(
            (l.name.as_str(), l.eco.as_deref(), l.games, l.example),
            ("Italian Game", Some("C50"), 4, GameId(4))
        );
        assert_eq!((l.wins, l.draws, l.losses), (1, 1, 2));
        assert_eq!(l.score, Some(0.375));
        assert_eq!((l.reviewed, l.early_games, l.early_mistakes), (3, 2, 2));
        assert_eq!(l.early_swing, 64.0);
        assert_eq!(l.worst.as_ref().map(|w| (w.game, w.ply)), Some((GameId(2), 5)));
        assert_eq!(l.drill_games, [GameId(3), GameId(2)]);
        // 3 of 4 decided games... 4 games decided, 37.5 %: badly; 2 of 3 reviewed games went wrong early.
        assert!(l.scores_badly && l.goes_wrong_early);

        let (black, _) = build(Side::Black, &games);
        assert_eq!((black.games, black.lines.len()), (1, 0));
    }

    #[test]
    fn a_line_with_few_games_or_a_good_run_is_not_flagged() {
        let games = [
            game(1, true, Outcome::WhiteWins, "Italian Game", Some(None)),
            game(2, true, Outcome::WhiteWins, "Italian Game", Some(None)),
            game(3, true, Outcome::BlackWins, "Italian Game", Some(Some(5))),
        ];
        let (prep, _) = build(Side::White, &games);
        let l = &prep.lines[0];
        assert!(!l.scores_badly, "three games are too few to say");
        assert!(l.goes_wrong_early, "one in three reviewed is a third");
        assert_eq!(l.score.map(|s| (s * 100.0).round()), Some(67.0));
    }

    #[test]
    fn a_line_gets_its_moves_and_pgn_from_a_game() {
        let games = [
            game(1, true, Outcome::WhiteWins, "Italian Game", None),
            game(2, true, Outcome::WhiteWins, "Italian Game", None),
        ];
        let (mut prep, plies) = build(Side::White, &games);
        let moves: Vec<String> =
            ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6"].iter().map(|m| (*m).to_owned()).collect();
        prep.lines[0].set_moves(&moves, plies[0]);
        let l = &prep.lines[0];
        assert_eq!(l.pgn, "1. e4 e5 2. Nf3 Nc6 3. Bc4");
        assert_eq!(l.moves.len(), 5);
        assert_eq!(l.fen, "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3");
        // A line of another opening reads the same way.
        let mut line = l.clone();
        line.set_moves(&["d2d4".to_owned(), "d7d5".to_owned()], 2);
        assert_eq!(line.pgn, "1. d4 d5");
    }
}
