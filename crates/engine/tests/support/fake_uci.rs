//! A fake UCI engine for the tests: it speaks enough UCI for the handshake, options, searches by depth, nodes,
//! movetime, clock or `infinite` (stopped by `stop`). It plays no chess: its moves are fixed. A few `fake …` commands
//! let tests look inside it.
//!
//! - `fake newgames`: prints `info string newgames <n>`, how many `ucinewgame` it has had.
//! - `fake option <name>`: prints `info string option <name> <value>`.
//! - `fake exit`: exits at once, as a crashing engine would.

use std::{
    collections::HashMap,
    io::{BufRead, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

fn main() {
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
    let mut options: HashMap<String, String> = HashMap::new();
    let mut new_games = 0u32;
    let mut plies = 0usize;
    while let Ok(line) = rx.recv() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["uci"] => {
                writeln!(out, "id name Fake 1.0").unwrap();
                writeln!(out, "id author The tests").unwrap();
                writeln!(out, "option name Hash type spin default 16 min 1 max 1024").unwrap();
                writeln!(out, "option name Threads type spin default 1 min 1 max 64").unwrap();
                writeln!(out, "option name MultiPV type spin default 1 min 1 max 16").unwrap();
                writeln!(out, "option name Minimal type check default false").unwrap();
                writeln!(out, "option name MoveOverhead type spin default 0 min 0 max 5000").unwrap();
                writeln!(out, "option name Clear Hash type button").unwrap();
                writeln!(out, "uciok").unwrap();
            }
            ["isready"] => writeln!(out, "readyok").unwrap(),
            ["ucinewgame"] => new_games += 1,
            ["setoption", "name", rest @ ..] => {
                if let Some(at) = rest.iter().position(|w| *w == "value") {
                    options.insert(rest[..at].join(" "), rest[at + 1..].join(" "));
                }
            }
            ["position", rest @ ..] => {
                plies = rest.iter().position(|w| *w == "moves").map_or(0, |at| rest.len() - at - 1);
            }
            ["go", rest @ ..] => {
                let value =
                    |key: &str| rest.iter().position(|w| *w == key).and_then(|i| rest.get(i + 1)?.parse::<u64>().ok());
                let multipv: u32 = options.get("MultiPV").and_then(|v| v.parse().ok()).unwrap_or(1);
                let best = if plies.is_multiple_of(2) { "e2e4" } else { "e7e5" };
                let reply = if plies.is_multiple_of(2) { "e7e5" } else { "g1f3" };
                let report = |out: &mut std::io::StdoutLock, depth: u64, nodes: u64| {
                    for pv in 1..=multipv {
                        let score = 30 - 20 * i64::from(pv - 1);
                        writeln!(
                            out,
                            "info depth {depth} seldepth {} multipv {pv} score cp {score} nodes {nodes} nps 1000000 time 1 pv {best} {reply}",
                            depth + 2
                        )
                        .unwrap();
                    }
                };
                if rest.contains(&"infinite") || value("movetime").is_some() {
                    report(&mut out, 1, 100);
                    out.flush().unwrap();
                    let until = value("movetime").map(|ms| Instant::now() + Duration::from_millis(ms));
                    loop {
                        let wait =
                            until.map_or(Duration::from_secs(3600), |t| t.saturating_duration_since(Instant::now()));
                        match rx.recv_timeout(wait) {
                            Ok(l) if l.trim() == "stop" => break,
                            Ok(l) if l.trim() == "quit" => return,
                            Ok(l) if l.trim() == "isready" => writeln!(out, "readyok").unwrap(),
                            Ok(_) => {}
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    report(&mut out, 2, 200);
                } else {
                    let depth = value("depth").unwrap_or(3);
                    let nodes = value("nodes").unwrap_or(1000);
                    for d in 1..=depth {
                        report(&mut out, d, nodes * d / depth);
                    }
                }
                writeln!(out, "bestmove {best} ponder {reply}").unwrap();
            }
            ["stop"] => {}
            ["quit"] => return,
            ["fake", "newgames"] => writeln!(out, "info string newgames {new_games}").unwrap(),
            ["fake", "option", name @ ..] => {
                let name = name.join(" ");
                let value = options.get(&name).cloned().unwrap_or_default();
                writeln!(out, "info string option {name} {value}").unwrap();
            }
            ["fake", "exit"] => std::process::exit(3),
            _ => {}
        }
        out.flush().unwrap();
    }
}
