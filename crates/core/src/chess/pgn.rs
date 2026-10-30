//! PGN, both ways.
//!
//! Writing: a game with a comment on every move in the style of the test PGNs (fastchess's), so the board reads both
//! the same way: `{+0.31/9 0.063s, n=9351, clk=9.85s}`, the score from the **mover's** side as engines report it.
//!
//! Reading: any number of games, as fastchess writes its trial PGNs and as people paste them (Lichess's exports
//! included). Move comments are read in three dialects — fastchess's and ours (`+0.31/9 0.063s, n=9351, sd=14,
//! nps=149921, clk=9.85s`, the mover's score), and Lichess's commands (`[%eval 0.17] [%clk 0:03:00]`, White's
//! score) — and every score comes out from White's side. Variations and NAGs are skipped.

use std::fmt::Write as _;

use super::{ChessError, Position, START_FEN};
use crate::{
    game::{Ending, Game, GameEnd, Move, Outcome, Source},
    ids::Side,
    score::Score,
    time::TimeControl,
};

fn quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The comment on a move: the mover's score and depth with the think time, nodes, and the clock after the move.
fn comment(m: &Move, mover: Side) -> Option<String> {
    let mut parts = Vec::new();
    let think = m.think_ms.map(|ms| format!("{:.3}s", ms as f64 / 1000.0));
    match (m.eval, m.depth) {
        (Some(eval), Some(depth)) => {
            let score = format!("{}/{depth}", eval.for_side(mover).pgn());
            parts.push(think.map_or(score.clone(), |t| format!("{score} {t}")));
        }
        _ => parts.extend(think),
    }
    if let Some(n) = m.nodes {
        parts.push(format!("n={n}"));
    }
    if let Some(ms) = m.clock_ms {
        parts.push(format!("clk={:.2}s", ms as f64 / 1000.0));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// A game as PGN, with seven-tag roster, ratings, titles, time control, termination and start position.
pub fn write(game: &Game) -> String {
    let s = &game.summary;
    let date = s.started_at.date().replace('-', ".");
    let mut out = String::new();
    let mut header = |key: &str, value: &str| {
        let _ = writeln!(out, "[{key} \"{}\"]", quoted(value));
    };
    let (event, site) = match &s.source {
        Source::Lichess { rated, speed, .. } | Source::Import { rated, speed, .. } => (
            format!("Lichess {} {} game", if *rated { "rated" } else { "casual" }, speed.lichess_key()),
            s.url.clone().unwrap_or_else(|| "https://lichess.org".into()),
        ),
        Source::Local => ("Chess game".into(), "Chess".into()),
    };
    header("Event", &event);
    header("Site", &site);
    header("Date", &date);
    header("Round", &s.id.to_string());
    header("White", &s.white.name);
    header("Black", &s.black.name);
    let end = s.outcome.map(|o| o.pgn()).unwrap_or("*");
    header("Result", end);
    for (key, seat) in [("White", &s.white), ("Black", &s.black)] {
        if let Some(rating) = seat.rating {
            header(&format!("{key}Elo"), &rating.to_string());
        }
        if let Some(title) = &seat.title {
            header(&format!("{key}Title"), title);
        }
    }
    header("TimeControl", &s.time.pgn_header());
    if let Some(opening) = &s.opening {
        if let Some(eco) = &opening.eco {
            header("ECO", eco);
        }
        header("Opening", &opening.name);
    }
    if let Some(ending) = s.ending {
        header("Termination", ending.termination());
    }
    let start = game.start_fen.as_deref().filter(|f| *f != START_FEN && *f != "startpos");
    if let Some(fen) = start {
        header("SetUp", "1");
        header("FEN", fen);
    }
    out.push('\n');

    // Move numbers follow the start position: it may be Black's turn, on any move.
    let (mut number, mut mover) = match start.and_then(|f| Position::from_fen(f).ok()) {
        Some(p) => (p.fullmoves(), p.turn()),
        None => (1, Side::White),
    };
    let mut tokens: Vec<String> = Vec::new();
    for (i, mv) in game.moves.iter().enumerate() {
        let san = match mover {
            Side::White => format!("{number}. {}", mv.san),
            Side::Black if i == 0 => format!("{number}... {}", mv.san),
            Side::Black => mv.san.clone(),
        };
        tokens.push(match comment(mv, mover) {
            Some(c) => format!("{san} {{{c}}}"),
            None => san,
        });
        if mover == Side::Black {
            number += 1;
        }
        mover = mover.other();
    }
    tokens.push(end.to_owned());

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

// ---------------------------------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------------------------------

/// A game read from PGN.
#[derive(Clone, Debug, PartialEq)]
pub struct PgnGame {
    /// Every tag pair, in order.
    pub headers: Vec<(String, String)>,
    /// The `FEN` tag, when the game does not start from the standard position.
    pub start_fen: Option<String>,
    /// Moves with UCI and SAN, and whatever their comments said: scores from White's side.
    pub moves: Vec<Move>,
    /// The result after the moves (or in the `Result` tag); none for `*`.
    pub outcome: Option<Outcome>,
}

impl PgnGame {
    /// The value of a tag.
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty() && *v != "?")
    }

    /// The `TimeControl` tag, read.
    pub fn time(&self) -> Option<TimeControl> {
        self.header("TimeControl").and_then(TimeControl::parse_pgn_header)
    }

    /// How the game ended: from the final position when it ended the game, else the `Termination` tag, else a
    /// resignation (decided) or an agreement (drawn). None while unfinished.
    pub fn end(&self) -> Option<GameEnd> {
        let outcome = self.outcome?;
        let by_position =
            Position::after(self.start_fen.as_deref(), &self.moves.iter().map(|m| &m.uci).collect::<Vec<_>>())
                .ok()
                .and_then(|p| p.ending())
                .filter(|e| e.outcome == Some(outcome));
        if let Some(e) = by_position {
            return Some(e);
        }
        let termination = self.header("Termination").unwrap_or_default().to_ascii_lowercase();
        let ending = match termination.as_str() {
            "time forfeit" => Ending::Timeout,
            "adjudication" => Ending::Adjudication,
            "abandoned" => Ending::Abandoned,
            "rules infraction" => Ending::IllegalMove,
            "stalemate" => Ending::Stalemate,
            "insufficient material" => Ending::InsufficientMaterial,
            _ if outcome == Outcome::Draw => Ending::Agreement,
            _ => Ending::Resignation,
        };
        Some(GameEnd::new(outcome, ending))
    }
}

/// What a move comment says, from whichever side's view its dialect uses.
#[derive(Clone, Debug, Default, PartialEq)]
struct Said {
    /// The mover's score (fastchess, ours).
    mover_eval: Option<Score>,
    /// White's score (Lichess's `%eval`).
    white_eval: Option<Score>,
    depth: Option<u32>,
    nodes: Option<u64>,
    think_ms: Option<u64>,
    clock_ms: Option<u64>,
}

/// `0:03:00`, `1:02:03.4`, `2:05` as milliseconds.
fn hms(text: &str) -> Option<u64> {
    let mut total = 0.0;
    for part in text.trim().split(':') {
        total = total * 60.0 + part.parse::<f64>().ok().filter(|v| *v >= 0.0)?;
    }
    Some((total * 1000.0).round() as u64)
}

/// `0.063s` as milliseconds.
fn seconds(text: &str) -> Option<u64> {
    let v: f64 = text.trim().strip_suffix('s')?.parse().ok()?;
    (v >= 0.0 && v.is_finite()).then(|| (v * 1000.0).round() as u64)
}

fn read_comment(text: &str) -> Said {
    let mut said = Said::default();
    // Lichess's commands: `[%eval 0.17]`, `[%eval #-3,25]` (with a depth), `[%clk 0:03:00]`, `[%emt 0:00:02]`.
    let mut rest = String::new();
    let mut s = text;
    while let Some(open) = s.find("[%") {
        rest.push_str(&s[..open]);
        let Some(close) = s[open..].find(']') else { break };
        let command = &s[open + 2..open + close];
        let (name, value) = command.split_once(' ').unwrap_or((command, ""));
        match name {
            "eval" => {
                let (score, depth) = value.split_once(',').unwrap_or((value, ""));
                said.white_eval = Score::parse_pgn(score);
                said.depth = said.depth.or(depth.trim().parse().ok());
            }
            "clk" => said.clock_ms = hms(value),
            "emt" => said.think_ms = hms(value),
            _ => {}
        }
        s = &s[open + close + 1..];
    }
    rest.push_str(s);

    // fastchess's and ours: `+0.31/9 0.063s, n=9351, sd=14, nps=149921, clk=9.85s`.
    for part in rest.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some((key, value)) = part.split_once('=') {
            match key.trim() {
                "n" => said.nodes = value.trim().parse().ok(),
                "clk" => said.clock_ms = seconds(value).or_else(|| hms(value)),
                "mt" => said.think_ms = value.trim().parse().ok(),
                _ => {}
            }
            continue;
        }
        for token in part.split_whitespace() {
            if let Some((score, depth)) = token.split_once('/')
                && let (Some(score), Ok(depth)) = (Score::parse_pgn(score), depth.parse::<u32>())
            {
                said.mover_eval = Some(score);
                said.depth = Some(depth);
            } else if let Some(ms) = seconds(token) {
                said.think_ms = Some(ms);
            }
        }
    }
    said
}

/// A token of PGN movetext.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Tag(String, String),
    Comment(&'a str),
    Word(&'a str),
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<Token<'_>>, ChessError> {
    let mut tokens = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut line_start = true;
    while i < bytes.len() {
        let c = bytes[i];
        // A `%` at the start of a line escapes the line.
        if line_start && c == b'%' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        line_start = c == b'\n';
        match c {
            b if b.is_ascii_whitespace() => i += 1,
            b'{' => {
                let end = text[i..]
                    .find('}')
                    .map(|e| i + e)
                    .ok_or_else(|| ChessError::BadPgn("a comment is not closed".into()))?;
                tokens.push(Token::Comment(&text[i + 1..end]));
                i = end + 1;
            }
            b';' => {
                let end = text[i..].find('\n').map_or(bytes.len(), |e| i + e);
                tokens.push(Token::Comment(&text[i + 1..end]));
                i = end;
            }
            b'[' => {
                let end = text[i..]
                    .find(']')
                    .map(|e| i + e)
                    .ok_or_else(|| ChessError::BadPgn("a tag is not closed".into()))?;
                let inner = text[i + 1..end].trim();
                let (key, value) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
                let value = value.trim();
                let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
                tokens.push(Token::Tag(key.to_owned(), value.replace("\\\"", "\"").replace("\\\\", "\\")));
                i = end + 1;
            }
            b'(' => {
                tokens.push(Token::Open);
                i += 1;
            }
            b')' => {
                tokens.push(Token::Close);
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && !b"{}()[];".contains(&bytes[i]) {
                    i += 1;
                }
                tokens.push(Token::Word(&text[start..i]));
            }
        }
    }
    Ok(tokens)
}

/// A move number (`12.`, `12...`) or the dots alone.
fn is_move_number(word: &str) -> bool {
    let digits = word.trim_end_matches('.');
    word.contains('.') && digits.chars().all(|c| c.is_ascii_digit())
}

/// The SAN part of a word that may carry its move number (`1.e4`, `12...Nf6`).
fn strip_number(word: &str) -> &str {
    match word.rfind('.') {
        Some(i) if word[..i].trim_end_matches('.').chars().all(|c| c.is_ascii_digit()) => &word[i + 1..],
        _ => word,
    }
}

const RESULTS: [&str; 4] = ["1-0", "0-1", "1/2-1/2", "*"];

struct Builder {
    headers: Vec<(String, String)>,
    pos: Position,
    start_fen: Option<String>,
    moves: Vec<Move>,
    movers: Vec<Side>,
    said: Vec<Said>,
    outcome: Option<Outcome>,
    result_seen: bool,
}

impl Builder {
    fn new(headers: Vec<(String, String)>) -> Result<Self, ChessError> {
        let fen = headers.iter().find(|(k, _)| k == "FEN").map(|(_, v)| v.clone());
        let pos = Position::from_start(fen.as_deref())?;
        let start_fen = fen.filter(|_| !pos.is_standard_start()).map(|_| pos.fen());
        let outcome = headers.iter().find(|(k, _)| k == "Result").and_then(|(_, v)| Outcome::parse_pgn(v));
        Ok(Self {
            headers,
            pos,
            start_fen,
            moves: Vec::new(),
            movers: Vec::new(),
            said: Vec::new(),
            outcome,
            result_seen: false,
        })
    }

    fn finish(self) -> PgnGame {
        let mut moves = self.moves;
        for ((m, said), mover) in moves.iter_mut().zip(self.said).zip(self.movers) {
            m.eval = said.white_eval.or(said.mover_eval.map(|s| s.from_side(mover)));
            m.depth = said.depth;
            m.nodes = said.nodes;
            m.think_ms = said.think_ms;
            m.clock_ms = said.clock_ms;
        }
        PgnGame { headers: self.headers, start_fen: self.start_fen, moves, outcome: self.outcome }
    }
}

/// Reads every game in `text`. A move that is not legal fails the whole read, naming the game and the move.
pub fn read(text: &str) -> Result<Vec<PgnGame>, ChessError> {
    let tokens = tokenize(text)?;
    let mut games = Vec::new();
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut game: Option<Builder> = None;
    let mut depth = 0usize;
    for token in tokens {
        if depth > 0 {
            match token {
                Token::Open => depth += 1,
                Token::Close => depth -= 1,
                _ => {}
            }
            continue;
        }
        match token {
            Token::Tag(key, value) => {
                // Tags after movetext start the next game.
                if let Some(g) = game.take() {
                    games.push(g.finish());
                }
                headers.push((key, value));
            }
            Token::Open => depth = 1,
            Token::Close => {}
            Token::Comment(c) => {
                if let Some(g) = game.as_mut()
                    && let Some(last) = g.said.last_mut()
                {
                    let more = read_comment(c);
                    *last = Said {
                        mover_eval: more.mover_eval.or(last.mover_eval),
                        white_eval: more.white_eval.or(last.white_eval),
                        depth: more.depth.or(last.depth),
                        nodes: more.nodes.or(last.nodes),
                        think_ms: more.think_ms.or(last.think_ms),
                        clock_ms: more.clock_ms.or(last.clock_ms),
                    };
                }
            }
            Token::Word(word) => {
                if game.as_ref().is_some_and(|g| g.result_seen) {
                    // Movetext after a result without new tags: a game without headers follows.
                    if let Some(g) = game.take() {
                        games.push(g.finish());
                    }
                }
                let g = match game.as_mut() {
                    Some(g) => g,
                    None => game.insert(Builder::new(std::mem::take(&mut headers))?),
                };
                if RESULTS.contains(&word) {
                    g.outcome = Outcome::parse_pgn(word);
                    g.result_seen = true;
                    continue;
                }
                if word.starts_with('$') || is_move_number(word) {
                    continue;
                }
                let san = strip_number(word);
                if san.is_empty() || matches!(san, "!" | "?" | "!!" | "??" | "!?" | "?!" | "--") {
                    continue;
                }
                let mover = g.pos.turn();
                let played = g.pos.play(san).map_err(|_| {
                    ChessError::BadPgn(format!(
                        "game {}: {san} is not a legal move after {} plies",
                        games.len() + 1,
                        g.moves.len()
                    ))
                })?;
                g.moves.push(Move::new(played.uci, played.san));
                g.movers.push(mover);
                g.said.push(Said::default());
            }
        }
    }
    if let Some(g) = game {
        games.push(g.finish());
    } else if !headers.is_empty() {
        games.push(Builder::new(headers)?.finish());
    }
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::{GameStatus, GameSummary, Offers, Opening, Seat},
        ids::{ConnectionKey, GameId, PlayerKey, Timestamp},
        time::Speed,
    };

    fn mv(san: &str, uci: &str, eval: Score, depth: u32, clock: u64) -> Move {
        Move {
            uci: uci.into(),
            san: san.into(),
            clock_ms: Some(clock),
            think_ms: Some(63),
            eval: Some(eval),
            depth: Some(depth),
            nodes: Some(9351),
        }
    }

    fn game(start_fen: Option<&str>) -> Game {
        let seat = |name: &str| Seat::new(PlayerKey::Engine("stockfish".into()), name);
        Game {
            summary: GameSummary {
                id: GameId(7),
                white: seat("v14"),
                black: seat("deeper-lmr"),
                source: Source::Local,
                time: TimeControl::clock(10_000, 100),
                status: GameStatus::Finished,
                outcome: Some(Outcome::BlackWins),
                ending: Some(Ending::Timeout),
                opening: None,
                plies: 3,
                started_at: Timestamp("2026-10-25T12:34:56.789Z".into()),
                ended_at: None,
                you: None,
                accuracy: None,
                url: None,
            },
            start_fen: start_fen.map(str::to_owned),
            // White's view: e4 +0.31 for White, e5 thought by Black at -0.28 for Black = +0.28 for White.
            moves: vec![
                mv("e4", "e2e4", Score::Cp(31), 9, 9_850),
                mv("e5", "e7e5", Score::Cp(28), 10, 9_900),
                mv("Qh5", "d1h5", Score::Mate(5), 12, 9_850),
            ],
            clocks: None,
            offers: Offers::default(),
            actions: Vec::new(),
        }
    }

    #[test]
    fn writes_headers_and_commented_moves_like_the_test_pgns() {
        let pgn = write(&game(None)).replace('\n', " ");
        assert!(pgn.contains("[Event \"Chess game\"]"), "{pgn}");
        assert!(pgn.contains("[White \"v14\"]"));
        assert!(pgn.contains("[Black \"deeper-lmr\"]"));
        assert!(pgn.contains("[Date \"2026.10.25\"]"));
        assert!(pgn.contains("[Result \"0-1\"]"));
        assert!(pgn.contains("[TimeControl \"10+0.1\"]"));
        assert!(pgn.contains("[Termination \"time forfeit\"]"));
        assert!(!pgn.contains("[FEN"));
        // Comments carry the mover's score.
        assert!(
            pgn.contains("1. e4 {+0.31/9 0.063s, n=9351, clk=9.85s} e5 {-0.28/10 0.063s, n=9351, clk=9.90s}"),
            "{pgn}"
        );
        assert!(pgn.contains("2. Qh5 {+M5/12 0.063s, n=9351, clk=9.85s} 0-1"), "{pgn}");
    }

    #[test]
    fn a_lichess_game_names_its_site_ratings_and_opponent() {
        let mut g = game(None);
        g.summary.source =
            Source::Lichess { connection: ConnectionKey::You, id: "q7ZvsdUF".into(), rated: true, speed: Speed::Blitz };
        g.summary.url = Some("https://lichess.org/q7ZvsdUF".into());
        g.summary.black = Seat {
            rating: Some(1873),
            title: Some("BOT".into()),
            ..Seat::new(PlayerKey::Lichess("someone".into()), "someone")
        };
        g.summary.opening = Some(Opening { eco: Some("C20".into()), name: "King's Pawn Game".into(), ply: Some(2) });
        let pgn = write(&g);
        assert!(pgn.contains("[Event \"Lichess rated blitz game\"]"), "{pgn}");
        assert!(pgn.contains("[Site \"https://lichess.org/q7ZvsdUF\"]"));
        assert!(pgn.contains("[Black \"someone\"]"));
        assert!(pgn.contains("[BlackElo \"1873\"]"));
        assert!(pgn.contains("[BlackTitle \"BOT\"]"));
        assert!(pgn.contains("[ECO \"C20\"]"));
    }

    #[test]
    fn numbers_moves_from_a_position_where_black_moves_first() {
        let mut g = game(Some("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 7"));
        g.moves = vec![Move::new("e7e5", "e5"), Move::new("g1f3", "Nf3")];
        let pgn = write(&g).replace('\n', " ");
        assert!(pgn.contains("[SetUp \"1\"]"));
        assert!(pgn.contains("[FEN \"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 7\"]"));
        assert!(pgn.contains("7... e5 8. Nf3 0-1"), "{pgn}");
    }

    #[test]
    fn an_unfinished_game_ends_in_a_star_and_lines_stay_short() {
        let mut g = game(None);
        g.summary.outcome = None;
        g.summary.ending = None;
        for _ in 0..40 {
            g.moves.push(mv("Nf6", "g8f6", Score::Cp(0), 8, 1_000));
        }
        let pgn = write(&g);
        assert!(pgn.trim_end().ends_with('*'));
        assert!(!pgn.contains("Termination"));
        assert!(pgn.lines().all(|l| l.len() <= 100), "{pgn}");
    }

    #[test]
    fn what_it_writes_it_reads_back() {
        let g = game(None);
        let read = read(&write(&g)).unwrap();
        assert_eq!(read.len(), 1);
        let r = &read[0];
        assert_eq!(r.moves, g.moves);
        assert_eq!(r.outcome, Some(Outcome::BlackWins));
        assert_eq!(r.header("White"), Some("v14"));
        assert_eq!(r.time(), Some(TimeControl::clock(10_000, 100)));
        assert_eq!(r.end(), Some(GameEnd::new(Outcome::BlackWins, Ending::Timeout)));
    }

    #[test]
    fn reads_fastchess_trial_pgns() {
        let text = r#"[Event "Fastchess Tournament"]
[Site "?"]
[Round "1"]
[White "dev"]
[Black "base"]
[Result "1/2-1/2"]
[FEN "rnbqkb1r/pppppppp/5n2/8/2PP4/8/PP2PPPP/RNBQKBNR b KQkq - 0 2"]
[SetUp "1"]
[TimeControl "8+0.08"]
[Termination "adjudication"]

2... e6 {+0.21/14 0.311s, n=55124, sd=21, nps=177248} 3. Nc3 {+0.35/13 0.204s, n=38512, sd=19, nps=188784}
3... Bb4 {-M3/7 0.1s, n=1, sd=1, nps=10} 1/2-1/2

[Event "Fastchess Tournament"]
[White "base"]
[Black "dev"]
[Result "1-0"]

1. e4 {book} e5 2. Qh5 {+1.00/1 0.010s} Nc6 3. Bc4 Nf6 4. Qxf7# {+M1/1 0.001s} 1-0
"#;
        let games = read(text).unwrap();
        assert_eq!(games.len(), 2);
        let g = &games[0];
        assert_eq!(g.start_fen.as_deref(), Some("rnbqkb1r/pppppppp/5n2/8/2PP4/8/PP2PPPP/RNBQKBNR b KQkq - 0 2"));
        assert_eq!(g.moves.len(), 3);
        // Black's +0.21 is White's -0.21.
        assert_eq!(g.moves[0].eval, Some(Score::Cp(-21)));
        assert_eq!((g.moves[0].depth, g.moves[0].nodes, g.moves[0].think_ms), (Some(14), Some(55124), Some(311)));
        assert_eq!(g.moves[1].eval, Some(Score::Cp(35)));
        assert_eq!(g.moves[2].eval, Some(Score::Mate(3)));
        assert_eq!(g.time(), Some(TimeControl::clock(8_000, 80)));
        assert_eq!(g.end(), Some(GameEnd::draw(Ending::Adjudication)));
        let mate = &games[1];
        assert_eq!(mate.moves[0].eval, None);
        assert_eq!(mate.moves.last().unwrap().san, "Qxf7#");
        assert_eq!(mate.end(), Some(GameEnd::new(Outcome::WhiteWins, Ending::Checkmate)));
    }

    #[test]
    fn reads_lichess_exports_and_skips_variations() {
        let text = "[Event \"Rated blitz game\"]\n[Site \"https://lichess.org/abcdefgh\"]\n[Result \"0-1\"]\n\
                    [TimeControl \"180+2\"]\n\n1. e4 { [%eval 0.17] [%clk 0:03:00] } 1... c5 { [%eval 0.25] \
                    [%clk 0:02:58.5] } 2. Nf3 $1 (2. Nc3 Nc6 (2... d6)) 2... d6 { [%eval #-3] } ; a line comment\n\
                    3. d4 0-1\n";
        let games = read(text).unwrap();
        let g = &games[0];
        assert_eq!(g.moves.iter().map(|m| m.san.as_str()).collect::<Vec<_>>(), ["e4", "c5", "Nf3", "d6", "d4"]);
        assert_eq!(g.moves[0].eval, Some(Score::Cp(17)));
        assert_eq!(g.moves[0].clock_ms, Some(180_000));
        // Lichess's eval is White's already, whoever moved.
        assert_eq!(g.moves[1].eval, Some(Score::Cp(25)));
        assert_eq!(g.moves[1].clock_ms, Some(178_500));
        assert_eq!(g.moves[3].eval, Some(Score::Mate(-3)));
        assert_eq!(g.end(), Some(GameEnd::new(Outcome::BlackWins, Ending::Resignation)));
    }

    #[test]
    fn pasted_movetext_without_headers_reads() {
        let games = read("1.e4 e5 2.Nf3 Nc6 *").unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].moves.len(), 4);
        assert_eq!(games[0].outcome, None);
        assert_eq!(games[0].end(), None);
        let bad = read("1. e4 e5 2. Ke3").unwrap_err();
        assert!(bad.to_string().contains("Ke3"), "{bad}");
        assert!(read("1. e4 {open").is_err());
    }
}
