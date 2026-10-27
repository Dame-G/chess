//! PGN with engine scores in the move comments, as fastchess (and cutechess) write them: `{+0.31/9 0.063s, ...}`,
//! the score in pawns from the mover's side, then `/depth`. The games our trials play are this.
//!
//! Parsing follows Reckless's converter (`src/tools/pgn.rs` on `nnue-tools-rsn-3`, with its fastchess fix: the score
//! is what comes before the `/`): the score times 100 is the position's score; at the first mate score (`+M5/12`)
//! the rest of the game is dropped. Moves without a score before the first scored one (book moves, `{book}`) move the
//! game's start forward instead; a move without a score after it ends the game there. Games with result `*` are
//! skipped.

use std::io::BufRead;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position, san::San};

use crate::game::{self, Entry, Game, GameResult};

/// The unit scores are written in.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ScoreUnits {
    /// Centipawns as the engine reports them over UCI (Reckless normalises its eval to these).
    #[default]
    Cp,
    /// Reckless's internal eval: UCI centipawns turned back with Reckless's normalisation (`normalize_to_cp`).
    RecklessInternal,
}

impl ScoreUnits {
    pub fn convert(self, cp: i32, pos: &Chess) -> i16 {
        let value = match self {
            Self::Cp => cp,
            Self::RecklessInternal if cp.abs() < 20_000 => reckless_internal(cp, pos),
            Self::RecklessInternal => cp,
        };
        value.clamp(-i32::from(game::MATE_SCORE), i32::from(game::MATE_SCORE)) as i16
    }
}

/// The inverse of Reckless's `normalize_to_cp` (`src/types/score.rs`).
pub fn reckless_internal(cp: i32, pos: &Chess) -> i32 {
    let v = f64::from(game::material(pos).clamp(16, 78)) / 58.0;
    let normalization = -285.1 * v.powi(3) + 642.5 * v.powi(2) - 455.5 * v + 464.8;
    (f64::from(cp) * normalization / 100.0).round() as i32
}

#[derive(Default, Debug, Clone, Copy)]
pub struct PgnCounts {
    pub games: u64,
    pub skipped: u64,
}

pub struct Reader<R> {
    input: R,
    units: ScoreUnits,
    pending: Vec<String>,
    eof: bool,
    pub counts: PgnCounts,
}

impl<R: BufRead> Reader<R> {
    pub fn new(input: R, units: ScoreUnits) -> Self {
        Self { input, units, pending: Vec::new(), eof: false, counts: PgnCounts::default() }
    }

    /// The text of the next game: its tags and movetext.
    fn next_text(&mut self) -> Result<Option<String>> {
        let mut lines = std::mem::take(&mut self.pending);
        let mut seen_moves = lines.iter().any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('['));
        loop {
            if self.eof {
                break;
            }
            let mut line = String::new();
            if self.input.read_line(&mut line)? == 0 {
                self.eof = true;
                break;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with('[') && seen_moves {
                self.pending.push(line);
                break;
            }
            if !trimmed.is_empty() && !trimmed.starts_with('[') {
                seen_moves = true;
            }
            lines.push(line);
        }
        if lines.iter().all(|l| l.trim().is_empty()) {
            return Ok(None);
        }
        Ok(Some(lines.concat()))
    }

    pub fn next_game(&mut self) -> Result<Option<Game>> {
        while let Some(text) = self.next_text()? {
            match parse_game(&text, self.units) {
                Ok(Some(game)) => {
                    self.counts.games += 1;
                    return Ok(Some(game));
                }
                Ok(None) | Err(_) => self.counts.skipped += 1,
            }
        }
        Ok(None)
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Tag(String, String),
    Move(String),
    Comment(String),
    Result(String),
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' => {
                let end = chars[i..].iter().position(|&c| c == ']').map_or(chars.len(), |p| i + p);
                let inner: String = chars[i + 1..end].iter().collect();
                if let Some((name, rest)) = inner.trim().split_once(char::is_whitespace) {
                    tokens.push(Token::Tag(name.to_string(), rest.trim().trim_matches('"').to_string()));
                }
                i = end + 1;
            }
            '{' => {
                let end = chars[i..].iter().position(|&c| c == '}').map_or(chars.len(), |p| i + p);
                tokens.push(Token::Comment(chars[i + 1..end].iter().collect::<String>().trim().to_string()));
                i = end + 1;
            }
            ';' => {
                i = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| i + p);
            }
            '(' => {
                let mut depth = 0;
                while i < chars.len() {
                    match chars[i] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        '{' => {
                            i = chars[i..].iter().position(|&c| c == '}').map_or(chars.len(), |p| i + p);
                        }
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            _ => {
                let end = chars[i..]
                    .iter()
                    .position(|&c| c.is_whitespace() || matches!(c, '{' | '(' | ')' | '[' | ';'))
                    .map_or(chars.len(), |p| i + p);
                let word: String = chars[i..end].iter().collect();
                i = end;
                if matches!(word.as_str(), "1-0" | "0-1" | "1/2-1/2" | "*") {
                    tokens.push(Token::Result(word));
                } else if word.starts_with('$') {
                    // A NAG.
                } else {
                    // Strip a move number ("12." or "12...") stuck to the move.
                    let stripped = word.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches('.');
                    if !stripped.is_empty() {
                        tokens.push(Token::Move(stripped.to_string()));
                    }
                }
            }
        }
    }
    tokens
}

enum Score {
    Cp(i32),
    Mate,
    None,
}

fn parse_comment(comment: Option<&str>) -> Score {
    let Some(comment) = comment else { return Score::None };
    let first = comment.split('/').next().unwrap_or("").trim();
    let first = first.split_whitespace().next().unwrap_or("");
    if first.trim_start_matches(['+', '-']).starts_with('M') {
        return Score::Mate;
    }
    match first.parse::<f32>() {
        Ok(v) if comment.contains('/') => Score::Cp((100.0 * v) as i32),
        _ => Score::None,
    }
}

/// One game's text into a game, `None` for one with no scored move or an unknown result.
pub fn parse_game(text: &str, units: ScoreUnits) -> Result<Option<Game>> {
    let tokens = tokenize(text);
    let mut fen = None;
    let mut result_tag = None;
    let mut result_token = None;
    for token in &tokens {
        match token {
            Token::Tag(name, value) if name == "FEN" => fen = Some(value.clone()),
            Token::Tag(name, value) if name == "Result" => result_tag = Some(value.clone()),
            Token::Result(r) => result_token = Some(r.clone()),
            _ => {}
        }
    }
    let result = match result_token.or(result_tag).as_deref() {
        Some("1-0") => GameResult::WhiteWins,
        Some("0-1") => GameResult::BlackWins,
        Some("1/2-1/2") => GameResult::Draw,
        _ => return Ok(None),
    };
    let start = match fen {
        Some(f) => game::parse_fen(&f)?,
        None => Chess::default(),
    };

    let mut game = Game::new(start.clone(), result);
    let mut pos = start;
    let mut i = 0;
    while i < tokens.len() {
        let Token::Move(san) = &tokens[i] else {
            i += 1;
            continue;
        };
        let comment = match tokens.get(i + 1) {
            Some(Token::Comment(c)) => Some(c.as_str()),
            _ => None,
        };
        let mv = San::from_ascii(san.trim_end_matches(['+', '#', '!', '?']).as_bytes())
            .map_err(|e| anyhow!("bad SAN {san}: {e}"))?
            .to_move(&pos)
            .map_err(|e| anyhow!("illegal SAN {san} in {}: {e}", game::fen(&pos)))?;
        match parse_comment(comment) {
            Score::Cp(cp) => {
                game.entries.push(Entry { mv, score: units.convert(cp, &pos) });
            }
            Score::Mate => break,
            Score::None if game.is_empty() => {
                game.start.play_unchecked(mv);
            }
            Score::None => break,
        }
        pos.play_unchecked(mv);
        i += 1;
    }
    if game.is_empty() {
        return Ok(None);
    }
    Ok(Some(game))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FASTCHESS: &str = r#"[Event "Fastchess Tournament"]
[White "Reckless-dev"]
[Black "Reckless-base"]
[Result "0-1"]
[FEN "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"]
[SetUp "1"]

1. e4 {book} e5 {book} 2. Nf3 {+0.31/9 0.063s, n=9351} Nc6 {-0.25/10 0.071s}
3. Bb5 {+0.40/11 0.080s} a6 {-1.20/9 0.050s} 4. Ba4 {+M3/12 0.01s} b5 {-M2/12 0.01s} 0-1

[Event "Second"]
[Result "*"]

1. d4 {+0.10/5 0.01s} *
"#;

    #[test]
    fn fastchess_games() {
        let mut reader = Reader::new(FASTCHESS.as_bytes(), ScoreUnits::Cp);
        let game = reader.next_game().unwrap().unwrap();
        // The book moves moved the start forward; the mate score ended the game.
        assert_eq!(game::fen(&game.start), "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
        let scores: Vec<i16> = game.entries.iter().map(|e| e.score).collect();
        assert_eq!(scores, vec![31, -25, 40, -120]);
        assert_eq!(game.result, GameResult::BlackWins);
        assert!(reader.next_game().unwrap().is_none());
        assert_eq!(reader.counts.games, 1);
        assert_eq!(reader.counts.skipped, 1);
    }

    #[test]
    fn internal_units() {
        let pos = Chess::default();
        // Full material is 78, where the normalisation is about 321.
        let internal = reckless_internal(100, &pos);
        assert!((315..=327).contains(&internal), "{internal}");
    }
}
