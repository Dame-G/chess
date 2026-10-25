//! A match as PGN, with a comment on every move in the style of the test PGNs (fastchess's), so the board reads
//! both the same way: `{+1.06/9 0.063s, n=9351, clk=9.85s}`, the score from the mover's side.

use std::fmt::Write;

use super::{Match, MatchMove, MatchTime, rules::Side};
use crate::engine::uci::Score;

/// The standard start position, for the games that start from it.
const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn score(s: Score) -> String {
    match s {
        Score::Cp(cp) => format!("{}{:.2}", if cp < 0 { "-" } else { "+" }, f64::from(cp.abs()) / 100.0),
        Score::Mate(m) => format!("{}M{}", if m < 0 { "-" } else { "+" }, m.abs()),
    }
}

fn comment(m: &MatchMove) -> String {
    let mut parts = Vec::new();
    if let (Some(eval), Some(depth)) = (m.eval, m.depth) {
        parts.push(format!("{}/{depth} {:.3}s", score(eval), m.time_ms as f64 / 1000.0));
    } else {
        parts.push(format!("{:.3}s", m.time_ms as f64 / 1000.0));
    }
    if let Some(n) = m.nodes {
        parts.push(format!("n={n}"));
    }
    let clock = match m.side {
        Side::White => m.clock_white_ms,
        Side::Black => m.clock_black_ms,
    };
    if let Some(ms) = clock {
        parts.push(format!("clk={:.2}s", ms as f64 / 1000.0));
    }
    parts.join(", ")
}

fn time_control(t: MatchTime) -> String {
    match t {
        MatchTime::Clock { seconds, increment } => format!("{seconds}+{increment}"),
        MatchTime::Nodes { nodes } => format!("nodes {nodes}"),
        MatchTime::Movetime { ms } => format!("{}/move", ms as f64 / 1000.0),
    }
}

pub fn to_pgn(m: &Match) -> String {
    let date = m.created_at.get(..10).unwrap_or("????-??-??").replace('-', ".");
    let name = |p: &super::Player| format!("{} {}", p.name, p.commit).trim_end().to_owned();
    let mut out = String::new();
    let mut header = |key: &str, value: &str| {
        let _ = writeln!(out, "[{key} \"{}\"]", quoted(value));
    };
    match &m.lichess {
        Some(l) => {
            header("Event", &format!("Lichess {} {} game", if l.rated { "rated" } else { "casual" }, l.speed));
            header("Site", &l.url);
        }
        None => {
            header("Event", "Reckless Lab match");
            header("Site", "Reckless Lab");
        }
    }
    header("Date", &date);
    header("Round", &m.id.to_string());
    header("White", &name(&m.white));
    header("Black", &name(&m.black));
    header("Result", m.result.as_str());
    for (key, p) in [("White", &m.white), ("Black", &m.black)] {
        if let Some(rating) = p.rating {
            header(&format!("{key}Elo"), &rating.to_string());
        }
        if let Some(title) = &p.title {
            header(&format!("{key}Title"), title);
        }
    }
    header("TimeControl", &time_control(m.time));
    if let Some(reason) = m.reason {
        header("Termination", reason.termination());
    }
    if m.start_fen != "startpos" && m.start_fen != START {
        header("SetUp", "1");
        header("FEN", &m.start_fen);
    }
    out.push('\n');

    // Move numbers follow the start position: it may be Black's turn, on any move.
    let (mut number, mut white_to_move) = if m.start_fen == "startpos" {
        (1, true)
    } else {
        let mut f = m.start_fen.split_whitespace().skip(1);
        let white = f.next() != Some("b");
        (f.nth(3).and_then(|n| n.parse().ok()).unwrap_or(1u32), white)
    };
    let mut tokens: Vec<String> = Vec::new();
    for (i, mv) in m.moves.iter().enumerate() {
        let san = if white_to_move {
            format!("{number}. {}", mv.san)
        } else if i == 0 {
            format!("{number}... {}", mv.san)
        } else {
            mv.san.clone()
        };
        tokens.push(format!("{san} {{{}}}", comment(mv)));
        if !white_to_move {
            number += 1;
        }
        white_to_move = !white_to_move;
    }
    tokens.push(m.result.as_str().to_owned());

    // Lines of about eighty columns, broken between moves.
    let mut line = String::new();
    for token in tokens {
        if !line.is_empty() && line.len() + 1 + token.len() > 80 {
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&token);
    }
    out.push_str(&line);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matches::{
        MatchStatus, Player,
        rules::{GameResult, Reason},
    };

    fn mv(ply: u32, side: Side, san: &str, eval: Score, depth: u32) -> MatchMove {
        MatchMove {
            ply,
            side,
            uci: String::new(),
            san: san.into(),
            fen_after: String::new(),
            clock_white_ms: Some(9_850),
            clock_black_ms: Some(9_900),
            eval: Some(eval),
            depth: Some(depth),
            nodes: Some(9351),
            time_ms: 63,
        }
    }

    fn game(start_fen: &str) -> Match {
        let player =
            |name: &str| Player { build: Some(1), name: name.into(), commit: "abc1234".into(), ..Player::default() };
        Match {
            id: 7,
            white: player("main"),
            black: player("test/no-nmp"),
            time: MatchTime::Clock { seconds: 10.0, increment: 0.1 },
            start_fen: start_fen.into(),
            book: false,
            hash: 16,
            threads: 1,
            adjudicate: false,
            minimal: false,
            status: MatchStatus::Finished,
            result: GameResult::Black,
            reason: Some(Reason::Time),
            turn: Side::White,
            clock_white_ms: Some(0),
            clock_black_ms: Some(1000),
            moves: vec![
                mv(1, Side::White, "e4", Score::Cp(31), 9),
                mv(2, Side::Black, "e5", Score::Cp(-28), 10),
                mv(3, Side::White, "Qh5", Score::Mate(5), 12),
            ],
            created_at: "2026-10-25T12:34:56.789Z".into(),
            finished_at: None,
            lichess: None,
        }
    }

    #[test]
    fn a_lichess_game_names_its_site_ratings_and_opponent() {
        let mut g = game("startpos");
        g.lichess = Some(crate::matches::LichessGame {
            id: "q7ZvsdUF".into(),
            url: "https://lichess.org/q7ZvsdUF".into(),
            rated: true,
            speed: "blitz".into(),
            our_side: Side::White,
        });
        g.black = Player {
            name: "someone".into(),
            rating: Some(1873),
            title: Some("BOT".into()),
            lichess: Some("someone".into()),
            ..Player::default()
        };
        let pgn = to_pgn(&g);
        assert!(pgn.contains("[Event \"Lichess rated blitz game\"]"), "{pgn}");
        assert!(pgn.contains("[Site \"https://lichess.org/q7ZvsdUF\"]"));
        assert!(pgn.contains("[Black \"someone\"]"));
        assert!(pgn.contains("[BlackElo \"1873\"]"));
        assert!(pgn.contains("[BlackTitle \"BOT\"]"));
    }

    #[test]
    fn an_old_stored_match_still_reads() {
        // A record from before Lichess games: a plain build id, no ratings, no `lichess` block.
        let json = serde_json::json!({
            "id": 3, "white": { "build": 1, "name": "main", "commit": "abc1234" },
            "black": { "build": 2, "name": "dev", "commit": "def5678" },
            "time": { "type": "clock", "seconds": 10.0, "increment": 0.1 }, "start_fen": "startpos", "book": false,
            "hash": 16, "threads": 1, "adjudicate": false, "status": "finished", "result": "1-0",
            "reason": "checkmate", "turn": "black", "clock_white_ms": 100, "clock_black_ms": 200, "moves": [],
            "created_at": "2026-10-25T12:00:00Z", "finished_at": null
        });
        let m: Match = serde_json::from_value(json).unwrap();
        assert_eq!(m.white.build, Some(1));
        assert!(m.lichess.is_none());
    }

    #[test]
    fn writes_headers_and_commented_moves_like_the_test_pgns() {
        let pgn = to_pgn(&game("startpos")).replace('\n', " ");
        assert!(pgn.contains("[White \"main abc1234\"]"), "{pgn}");
        assert!(pgn.contains("[Black \"test/no-nmp abc1234\"]"));
        assert!(pgn.contains("[Date \"2026.10.25\"]"));
        assert!(pgn.contains("[Result \"0-1\"]"));
        assert!(pgn.contains("[TimeControl \"10+0.1\"]"));
        assert!(pgn.contains("[Termination \"time forfeit\"]"));
        assert!(!pgn.contains("[FEN"));
        assert!(
            pgn.contains("1. e4 {+0.31/9 0.063s, n=9351, clk=9.85s} e5 {-0.28/10 0.063s, n=9351, clk=9.90s}"),
            "{pgn}"
        );
        assert!(pgn.contains("2. Qh5 {+M5/12 0.063s, n=9351, clk=9.85s} 0-1"), "{pgn}");
    }

    #[test]
    fn numbers_moves_from_a_position_where_black_moves_first() {
        let mut g = game("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 7");
        g.moves.truncate(2);
        g.moves[0].side = Side::Black;
        let pgn = to_pgn(&g).replace('\n', " ");
        assert!(pgn.contains("[SetUp \"1\"]"));
        assert!(pgn.contains("[FEN \"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 7\"]"));
        assert!(pgn.contains("7... e4 {"), "{pgn}");
        assert!(pgn.contains("} 8. e5 {"), "{pgn}");
    }

    #[test]
    fn an_unfinished_match_ends_in_a_star_and_lines_stay_short() {
        let mut g = game("startpos");
        g.result = GameResult::Unfinished;
        g.reason = None;
        for i in 0..40 {
            g.moves.push(mv(4 + i, Side::Black, "Nf6", Score::Cp(0), 8));
        }
        let pgn = to_pgn(&g);
        assert!(pgn.trim_end().ends_with('*'));
        assert!(!pgn.contains("Termination"));
        assert!(pgn.lines().all(|l| l.len() <= 100), "{pgn}");
    }
}
