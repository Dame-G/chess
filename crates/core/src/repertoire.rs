//! Your openings: the tree of positions your games went through as White or as Black, with your moves and how often
//! you play them, the moves you met, how you scored after each, and where you (or your opponents) left theory.
//!
//! Positions are keyed by the FEN without move counters, so transpositions meet in one node. Whether a position is
//! theory is the caller's: a set of known positions, or the explorer's answer, passed in as a predicate.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chess::Position,
    game::{Game, GameStatus, Outcome},
    ids::{GameId, PlayerKey, Side, Timestamp},
};

/// One of your games as the repertoire reads it: your side, the moves (UCI) from the standard start, the result.
#[derive(Clone, Debug, PartialEq)]
pub struct RepertoireGame {
    pub game: GameId,
    pub side: Side,
    pub moves: Vec<String>,
    pub outcome: Option<Outcome>,
    pub played_at: Timestamp,
}

impl RepertoireGame {
    /// `player`'s side of a finished game from the standard start; `None` for a game they did not play, one from
    /// another position, or one not finished.
    pub fn from_game(game: &Game, player: &PlayerKey) -> Option<Self> {
        let s = &game.summary;
        let standard =
            game.start_fen.as_deref().is_none_or(|f| Position::from_fen(f).is_ok_and(|p| p.is_standard_start()));
        if !standard || s.status != GameStatus::Finished {
            return None;
        }
        Some(Self {
            game: s.id,
            side: s.side_of(player)?,
            moves: game.moves.iter().map(|m| m.uci.clone()).collect(),
            outcome: s.outcome,
            played_at: s.started_at.clone(),
        })
    }
}

/// How deep and how wide the tree is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RepertoireOptions {
    /// Plies from the start read into the tree.
    pub max_plies: u32,
    /// Moves (and positions) seen in fewer games are left out, the start position aside.
    pub min_games: u32,
}

impl Default for RepertoireOptions {
    fn default() -> Self {
        Self { max_plies: 20, min_games: 2 }
    }
}

/// A move from a position, with the games that played it and how you scored after it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RepertoireMove {
    pub uci: String,
    pub san: String,
    pub games: u32,
    /// Of the position's games, 0 to 1.
    pub share: f64,
    /// Your results after it.
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Your points per game after it, 0 to 1.
    pub score: f64,
    /// The key of the position it leads to.
    pub next: String,
    /// The position it leads to is out of theory while this one was in (`None` without a theory predicate).
    pub leaves_theory: Option<bool>,
}

/// A position of the tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RepertoireNode {
    /// The FEN without move counters.
    pub key: String,
    pub fen: String,
    /// The fewest plies it was reached in.
    pub ply: u32,
    /// Games that reached it.
    pub games: u32,
    /// Whether you are to move here: then `moves` are your choices, else what you met.
    pub your_turn: bool,
    /// Whether it is theory (`None` without a theory predicate).
    pub theory: Option<bool>,
    /// Most played first.
    pub moves: Vec<RepertoireMove>,
}

/// Where a game first left theory, added up over the games that left at the same move.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Departure {
    /// The last position in theory, and the move that left it.
    pub key: String,
    pub fen: String,
    pub ply: u32,
    pub uci: String,
    pub san: String,
    /// You left theory, rather than your opponent.
    pub by_you: bool,
    pub games: u32,
    /// Your points per game in those games, 0 to 1.
    pub score: f64,
    /// The newest such game, to open on the board.
    pub example: GameId,
}

/// Your openings with one colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Repertoire {
    pub side: Side,
    /// Games read.
    pub games: u32,
    pub options: RepertoireOptions,
    /// The start position first, then by ply and by games.
    pub nodes: Vec<RepertoireNode>,
    /// Where games left theory, the most common first (empty without a theory predicate).
    pub departures: Vec<Departure>,
}

impl Repertoire {
    pub fn node(&self, key: &str) -> Option<&RepertoireNode> {
        self.nodes.iter().find(|n| n.key == key)
    }

    /// The main line: the most played move from the start, then from each position it leads to, while the tree goes.
    pub fn main_line(&self) -> Vec<String> {
        let mut line = Vec::new();
        let mut seen = HashSet::new();
        let mut node = self.nodes.first();
        while let Some(n) = node {
            if !seen.insert(n.key.clone()) {
                break;
            }
            let Some(m) = n.moves.first() else { break };
            line.push(m.uci.clone());
            node = self.node(&m.next);
        }
        line
    }

    /// Your own departures from theory, the most common first: the lines where your preparation ends.
    pub fn your_deviations(&self) -> Vec<&Departure> {
        self.departures.iter().filter(|d| d.by_you).collect()
    }
}

#[derive(Default)]
struct NodeAcc {
    fen: String,
    ply: u32,
    games: HashSet<GameId>,
    moves: HashMap<String, MoveAcc>,
}

#[derive(Default)]
struct MoveAcc {
    san: String,
    next: String,
    games: u32,
    wdl: [u32; 3],
}

struct DepartureAcc {
    fen: String,
    ply: u32,
    san: String,
    by_you: bool,
    games: u32,
    points: f64,
    example: (Timestamp, GameId),
}

/// Win, draw or loss index for `side` (0, 1, 2); `None` unfinished.
fn wdl_index(outcome: Option<Outcome>, side: Side) -> Option<usize> {
    match outcome? {
        Outcome::Draw => Some(1),
        o if o.winner() == Some(side) => Some(0),
        _ => Some(2),
    }
}

/// The tree of `side`'s games (others are skipped), to `options.max_plies`. `theory` says whether a position (by key)
/// is theory; with it, each game's first move out of theory is a departure. A game with an illegal move is read up to
/// it.
pub fn build(
    side: Side,
    games: &[RepertoireGame],
    options: RepertoireOptions,
    theory: Option<&dyn Fn(&str) -> bool>,
) -> Repertoire {
    let mut nodes: HashMap<String, NodeAcc> = HashMap::new();
    let mut departures: HashMap<(String, String), DepartureAcc> = HashMap::new();
    let start = Position::start();
    let root = start.key();
    let mut count = 0;
    for g in games.iter().filter(|g| g.side == side) {
        count += 1;
        let result = wdl_index(g.outcome, side);
        let points = g.outcome.map_or(0.0, |o| o.points(side));
        let mut pos = start.clone();
        let mut in_theory = true;
        for (ply, uci) in g.moves.iter().take(options.max_plies as usize).enumerate() {
            let key = pos.key();
            let fen = pos.fen();
            let mover = pos.turn();
            let Ok(played) = pos.play(uci) else { break };
            let next = pos.key();
            let node = nodes.entry(key.clone()).or_insert_with(|| NodeAcc {
                fen: fen.clone(),
                ply: ply as u32,
                ..Default::default()
            });
            node.ply = node.ply.min(ply as u32);
            node.games.insert(g.game);
            let m = node.moves.entry(played.uci.clone()).or_default();
            m.san = played.san.clone();
            m.next = next.clone();
            m.games += 1;
            if let Some(i) = result {
                m.wdl[i] += 1;
            }
            if let Some(known) = theory
                && in_theory
                && !known(&next)
            {
                in_theory = false;
                let d = departures.entry((key.clone(), played.uci.clone())).or_insert_with(|| DepartureAcc {
                    fen,
                    ply: ply as u32,
                    san: played.san.clone(),
                    by_you: mover == side,
                    games: 0,
                    points: 0.0,
                    example: (g.played_at.clone(), g.game),
                });
                d.games += 1;
                d.points += points;
                if (g.played_at.clone(), g.game) > d.example {
                    d.example = (g.played_at.clone(), g.game);
                }
            }
        }
        // The last position reached counts as reached, even with no move from it in the tree.
        let last = pos.key();
        let ply = g.moves.len().min(options.max_plies as usize) as u32;
        let fen = pos.fen();
        let node = nodes.entry(last).or_insert_with(|| NodeAcc { fen, ply, ..Default::default() });
        node.ply = node.ply.min(ply);
        node.games.insert(g.game);
    }

    let is_theory = |key: &str| theory.map(|t| t(key));
    let min = options.min_games.max(1);
    let mut out: Vec<RepertoireNode> = nodes
        .iter()
        .filter(|(key, n)| **key == root || n.games.len() as u32 >= min)
        .map(|(key, n)| {
            let total = n.games.len() as u32;
            let mut moves: Vec<RepertoireMove> = n
                .moves
                .iter()
                .filter(|(_, m)| m.games >= min)
                .map(|(uci, m)| {
                    let decided = m.wdl.iter().sum::<u32>();
                    let score = if decided == 0 {
                        0.0
                    } else {
                        (f64::from(m.wdl[0]) + 0.5 * f64::from(m.wdl[1])) / f64::from(decided)
                    };
                    RepertoireMove {
                        uci: uci.clone(),
                        san: m.san.clone(),
                        games: m.games,
                        share: f64::from(m.games) / f64::from(total.max(1)),
                        wins: m.wdl[0],
                        draws: m.wdl[1],
                        losses: m.wdl[2],
                        score,
                        next: m.next.clone(),
                        leaves_theory: theory.map(|t| t(key) && !t(&m.next)),
                    }
                })
                .collect();
            moves.sort_by(|a, b| b.games.cmp(&a.games).then(a.uci.cmp(&b.uci)));
            let your_turn = Position::from_fen(&n.fen).is_ok_and(|p| p.turn() == side);
            RepertoireNode {
                key: key.clone(),
                fen: n.fen.clone(),
                ply: n.ply,
                games: total,
                your_turn,
                theory: is_theory(key),
                moves,
            }
        })
        .collect();
    if !out.iter().any(|n| n.key == root) {
        out.push(RepertoireNode {
            key: root.clone(),
            fen: start.fen(),
            ply: 0,
            games: 0,
            your_turn: side == Side::White,
            theory: is_theory(&root),
            moves: Vec::new(),
        });
    }
    out.sort_by(|a, b| {
        (b.key == root)
            .cmp(&(a.key == root))
            .then(a.ply.cmp(&b.ply))
            .then(b.games.cmp(&a.games))
            .then(a.key.cmp(&b.key))
    });

    let mut departures: Vec<Departure> = departures
        .into_iter()
        .map(|((key, uci), d)| Departure {
            key,
            fen: d.fen,
            ply: d.ply,
            uci,
            san: d.san,
            by_you: d.by_you,
            games: d.games,
            score: d.points / f64::from(d.games.max(1)),
            example: d.example.1,
        })
        .collect();
    departures.sort_by(|a, b| b.games.cmp(&a.games).then(a.ply.cmp(&b.ply)).then(a.uci.cmp(&b.uci)));

    Repertoire { side, games: count, options, nodes: out, departures }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::{GameSummary, Move, Offers, Seat, Source},
        time::TimeControl,
    };

    fn g(id: i64, side: Side, moves: &[&str], outcome: Outcome) -> RepertoireGame {
        RepertoireGame {
            game: GameId(id),
            side,
            moves: moves.iter().map(|m| (*m).to_owned()).collect(),
            outcome: Some(outcome),
            played_at: Timestamp::from_unix_ms(id * 1000),
        }
    }

    fn key_after(moves: &[&str]) -> String {
        Position::after(None, moves).unwrap().key()
    }

    fn games() -> Vec<RepertoireGame> {
        vec![
            g(1, Side::White, &["e2e4", "e7e5", "g1f3", "b8c6"], Outcome::WhiteWins),
            g(2, Side::White, &["e2e4", "e7e5", "g1f3", "g8f6"], Outcome::Draw),
            g(3, Side::White, &["e2e4", "c7c5", "g1f3"], Outcome::BlackWins),
            g(4, Side::White, &["d2d4", "d7d5"], Outcome::WhiteWins),
            g(5, Side::Black, &["e2e4", "c7c5"], Outcome::BlackWins),
        ]
    }

    #[test]
    fn the_tree_counts_your_moves_and_scores() {
        let r = build(Side::White, &games(), RepertoireOptions { max_plies: 10, min_games: 1 }, None);
        assert_eq!(r.games, 4);
        let root = &r.nodes[0];
        assert_eq!((root.ply, root.games, root.your_turn), (0, 4, true));
        assert_eq!(root.moves[0].uci, "e2e4");
        assert_eq!((root.moves[0].games, root.moves[0].san.as_str()), (3, "e4"));
        assert!((root.moves[0].share - 0.75).abs() < 1e-9);
        // After e4: one win, one draw, one loss.
        assert_eq!((root.moves[0].wins, root.moves[0].draws, root.moves[0].losses), (1, 1, 1));
        assert!((root.moves[0].score - 0.5).abs() < 1e-9);
        let after_e4 = r.node(&key_after(&["e2e4"])).unwrap();
        assert!(!after_e4.your_turn);
        assert_eq!(after_e4.moves.iter().map(|m| m.san.as_str()).collect::<Vec<_>>(), ["e5", "c5"]);
        assert_eq!(r.main_line(), ["e2e4", "e7e5", "g1f3", "b8c6"]);
        assert!(r.departures.is_empty() && root.theory.is_none() && root.moves[0].leaves_theory.is_none());
        assert!(r.nodes.windows(2).skip(1).all(|w| w[0].ply <= w[1].ply));
    }

    #[test]
    fn rare_moves_are_left_out_and_depth_is_capped() {
        let r = build(Side::White, &games(), RepertoireOptions { max_plies: 2, min_games: 2 }, None);
        let root = &r.nodes[0];
        assert_eq!(root.moves.len(), 1);
        assert!(r.node(&key_after(&["d2d4"])).is_none());
        let after_e4 = r.node(&key_after(&["e2e4"])).unwrap();
        assert_eq!(after_e4.moves.iter().map(|m| m.uci.as_str()).collect::<Vec<_>>(), ["e7e5"]);
        assert_eq!(r.node(&key_after(&["e2e4", "e7e5"])).unwrap().games, 2);
        assert!(r.node(&key_after(&["e2e4", "e7e5", "g1f3"])).is_none());
    }

    #[test]
    fn transpositions_meet_in_one_node() {
        let games = [
            g(1, Side::White, &["g1f3", "d7d5", "d2d4"], Outcome::Draw),
            g(2, Side::White, &["d2d4", "d7d5", "g1f3"], Outcome::Draw),
        ];
        let r = build(Side::White, &games, RepertoireOptions { max_plies: 10, min_games: 1 }, None);
        assert_eq!(r.node(&key_after(&["d2d4", "d7d5", "g1f3"])).unwrap().games, 2);
    }

    #[test]
    fn theory_marks_where_games_leave_it() {
        let book: HashSet<String> =
            [key_after(&[]), key_after(&["e2e4"]), key_after(&["e2e4", "e7e5"]), key_after(&["e2e4", "c7c5"])].into();
        let known = |k: &str| book.contains(k);
        let r = build(Side::White, &games(), RepertoireOptions { max_plies: 10, min_games: 1 }, Some(&known));
        // Games 1 to 3 leave with your third move or theirs, game 4 with your first.
        let nf3 = r.departures.iter().find(|d| d.uci == "g1f3" && d.key == key_after(&["e2e4", "e7e5"])).unwrap();
        assert_eq!((nf3.games, nf3.by_you, nf3.ply, nf3.san.as_str()), (2, true, 2, "Nf3"));
        assert!((nf3.score - 0.75).abs() < 1e-9);
        assert_eq!(nf3.example, GameId(2));
        assert_eq!(r.departures[0].games, 2);
        assert_eq!(r.departures.iter().map(|d| d.games).sum::<u32>(), 4);
        assert_eq!(r.your_deviations().len(), r.departures.len());
        let root = &r.nodes[0];
        assert_eq!(root.theory, Some(true));
        let d4 = root.moves.iter().find(|m| m.uci == "d2d4").unwrap();
        assert_eq!(d4.leaves_theory, Some(true));
        assert_eq!(root.moves.iter().find(|m| m.uci == "e2e4").unwrap().leaves_theory, Some(false));
    }

    #[test]
    fn black_games_make_blacks_tree_and_opponents_can_leave_theory() {
        let book: HashSet<String> = [key_after(&[]), key_after(&["e2e4"])].into();
        let known = |k: &str| book.contains(k);
        let r = build(Side::Black, &games(), RepertoireOptions::default(), Some(&known));
        assert_eq!(r.games, 1);
        assert!(!r.nodes[0].your_turn);
        assert_eq!(r.departures[0].uci, "c7c5");
        assert!(r.departures[0].by_you);
        let early: Vec<RepertoireGame> = vec![g(9, Side::Black, &["a2a3"], Outcome::Draw)];
        let r = build(Side::Black, &early, RepertoireOptions::default(), Some(&known));
        assert!(!r.departures[0].by_you && r.your_deviations().is_empty());
        assert!(build(Side::White, &[], RepertoireOptions::default(), None).nodes.len() == 1);
    }

    #[test]
    fn games_read_from_records() {
        let seat = |p: PlayerKey| Seat::new(p, "x");
        let game = Game {
            summary: GameSummary {
                id: GameId(3),
                white: seat(PlayerKey::Dameless),
                black: seat(PlayerKey::You),
                source: Source::Local,
                time: TimeControl::clock(180_000, 2_000),
                status: GameStatus::Finished,
                outcome: Some(Outcome::BlackWins),
                ending: None,
                opening: None,
                plies: 2,
                started_at: Timestamp::from_unix_ms(0),
                ended_at: None,
                you: Some(Side::Black),
                accuracy: None,
                url: None,
            },
            start_fen: None,
            moves: vec![Move::new("e2e4", "e4"), Move::new("c7c5", "c5")],
            clocks: None,
            offers: Offers::default(),
            actions: Vec::new(),
        };
        let r = RepertoireGame::from_game(&game, &PlayerKey::You).unwrap();
        assert_eq!((r.side, r.moves.len()), (Side::Black, 2));
        assert!(RepertoireGame::from_game(&game, &PlayerKey::Stockfish(3)).is_none());
        let other = Game { start_fen: Some("8/8/8/8/8/k7/8/K7 w - - 0 1".into()), ..game.clone() };
        assert!(RepertoireGame::from_game(&other, &PlayerKey::You).is_none());
    }
}
