//! A tiny UCI engine for the tests: legal moves, a material score, nothing more.
//!
//! It plays a capture when it has one (so games end, often by insufficient material), otherwise a move picked from
//! the position's hash, and scores the position as the material balance from the side to move's side, in cp. It
//! declares `UseSoftNodes`, `UCI_Chess960` and `Minimal` like Reckless's `datagen` branch, and accepts and ignores
//! any option.

use std::io::{BufRead, Write};

use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Position, Role, fen::Fen, uci::UciMove, zobrist::Zobrist64};

fn material(pos: &Chess, color: Color) -> i32 {
    let board = pos.board();
    [(Role::Pawn, 100), (Role::Knight, 300), (Role::Bishop, 300), (Role::Rook, 500), (Role::Queen, 900)]
        .iter()
        .map(|&(role, v)| (board.by_role(role) & board.by_color(color)).count() as i32 * v)
        .sum()
}

fn parse_position(tokens: &[&str]) -> Option<Chess> {
    let mut pos: Chess;
    let mut rest = tokens;
    match *rest.first()? {
        "startpos" => {
            pos = Chess::default();
            rest = &rest[1..];
        }
        "fen" => {
            let end = rest.iter().position(|t| *t == "moves").unwrap_or(rest.len());
            let fen: Fen = rest[1..end].join(" ").parse().ok()?;
            pos = fen.into_position(CastlingMode::Chess960).ok()?;
            rest = &rest[end..];
        }
        _ => return None,
    }
    if rest.first() == Some(&"moves") {
        for m in &rest[1..] {
            let uci: UciMove = m.parse().ok()?;
            let mv = uci.to_move(&pos).ok()?;
            pos.play_unchecked(mv);
        }
    }
    Some(pos)
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut pos = Chess::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.first().copied() {
            Some("uci") => {
                writeln!(out, "id name FakeUci").unwrap();
                writeln!(out, "option name Hash type spin default 16 min 1 max 1024").unwrap();
                writeln!(out, "option name Threads type spin default 1 min 1 max 64").unwrap();
                writeln!(out, "option name UCI_Chess960 type check default false").unwrap();
                writeln!(out, "option name UseSoftNodes type check default false").unwrap();
                writeln!(out, "option name Minimal type check default false").unwrap();
                writeln!(out, "uciok").unwrap();
            }
            Some("isready") => writeln!(out, "readyok").unwrap(),
            Some("position") => {
                if let Some(p) = parse_position(&tokens[1..]) {
                    pos = p;
                }
            }
            Some("go") => {
                let moves = pos.legal_moves();
                let score = material(&pos, pos.turn()) - material(&pos, !pos.turn());
                let nodes = tokens.iter().position(|t| *t == "nodes").and_then(|i| tokens.get(i + 1)).unwrap_or(&"1");
                if moves.is_empty() {
                    writeln!(out, "info depth 0 score {}", if pos.is_check() { "mate 0" } else { "cp 0" }).unwrap();
                    writeln!(out, "bestmove 0000").unwrap();
                } else {
                    let hash = pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;
                    let captures: Vec<_> = moves.iter().filter(|m| m.is_capture()).collect();
                    let mv = if !captures.is_empty() {
                        *captures[(hash % captures.len() as u64) as usize]
                    } else {
                        moves[(hash % moves.len() as u64) as usize]
                    };
                    writeln!(
                        out,
                        "info depth 1 score cp {score} nodes {nodes} pv {}",
                        mv.to_uci(CastlingMode::Chess960)
                    )
                    .unwrap();
                    writeln!(out, "bestmove {}", mv.to_uci(CastlingMode::Chess960)).unwrap();
                }
            }
            Some("quit") => break,
            _ => {}
        }
        out.flush().unwrap();
    }
}
