//! A fake UCI engine for the tests of the Referee runtime: it plays legal chess without thinking (a mate if it sees
//! one, else a capture, else its first legal move) and reports one `info` line per search. Arguments change how it
//! behaves:
//!
//! - `--score <cp>`: the score it reports, from the side to move (default 20).
//! - `--delay <ms>`: how long it takes over each move (default none); `stop` cuts it short.
//! - `--illegal`: answers every search with a move that is not legal.
//! - `--die`: exits on its first `go`, as a crashing engine would.

use std::{
    io::{BufRead, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

use dameless_core::chess::Position;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |key: &str| args.iter().position(|a| a == key).and_then(|i| args.get(i + 1)?.parse::<i64>().ok());
    let score = value("--score").unwrap_or(20);
    let delay = value("--delay").map(|ms| Duration::from_millis(ms as u64));
    let illegal = args.iter().any(|a| a == "--illegal");
    let die = args.iter().any(|a| a == "--die");

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut out = std::io::stdout().lock();
    let mut position = Position::start();
    while let Ok(line) = rx.recv() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["uci"] => {
                writeln!(out, "id name PlayFake 1.0").unwrap();
                writeln!(out, "id author The tests").unwrap();
                writeln!(out, "option name Hash type spin default 16 min 1 max 1024").unwrap();
                writeln!(out, "option name Threads type spin default 1 min 1 max 64").unwrap();
                writeln!(out, "option name MoveOverhead type spin default 0 min 0 max 5000").unwrap();
                writeln!(out, "option name Minimal type check default false").unwrap();
                writeln!(out, "uciok").unwrap();
            }
            ["isready"] => writeln!(out, "readyok").unwrap(),
            ["position", rest @ ..] => {
                let (start, moves): (Option<String>, Vec<String>) = match rest {
                    ["startpos", tail @ ..] => (None, moves_of(tail)),
                    ["fen", tail @ ..] => {
                        let end = tail.iter().position(|w| *w == "moves").unwrap_or(tail.len());
                        (Some(tail[..end].join(" ")), moves_of(&tail[end..]))
                    }
                    _ => (None, Vec::new()),
                };
                position = Position::after(start.as_deref(), &moves).unwrap_or_default();
            }
            ["go", ..] => {
                if die {
                    std::process::exit(3);
                }
                if let Some(delay) = delay {
                    let until = Instant::now() + delay;
                    loop {
                        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                            Ok(l) if l.trim() == "stop" => break,
                            Ok(l) if l.trim() == "quit" => return,
                            Ok(_) => {}
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
                let best = if illegal { Some("a1a8".to_owned()) } else { choose(&position) };
                match best {
                    Some(m) => {
                        writeln!(
                            out,
                            "info depth 1 seldepth 1 multipv 1 score cp {score} nodes 100 nps 100000 time 1 pv {m}"
                        )
                        .unwrap();
                        writeln!(out, "bestmove {m}").unwrap();
                    }
                    None => writeln!(out, "bestmove (none)").unwrap(),
                }
            }
            ["quit"] => return,
            _ => {}
        }
        out.flush().unwrap();
    }
}

fn moves_of(words: &[&str]) -> Vec<String> {
    match words {
        ["moves", rest @ ..] => rest.iter().map(|m| (*m).to_owned()).collect(),
        _ => Vec::new(),
    }
}

/// A mate in one if there is one, else a capture, else the first legal move.
fn choose(pos: &Position) -> Option<String> {
    let moves = pos.legal_moves();
    let mut capture = None;
    for m in &moves {
        let mut after = pos.clone();
        let before = after.material_balance(pos.turn());
        if after.play(m).is_err() {
            continue;
        }
        if after.is_checkmate() {
            return Some(m.clone());
        }
        if capture.is_none() && after.material_balance(pos.turn()) > before {
            capture = Some(m.clone());
        }
    }
    capture.or_else(|| moves.first().cloned())
}
