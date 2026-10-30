//! A fake UCI engine for analysis's tests. It knows the rules (through `core`) and searches two plies by material:
//! each legal move is worth the material the side to move keeps (in centipawns) after the opponent's best reply, a
//! mate 20,000. Its lines are the moves from the most to the least, alphabetically by UCI between equals, each with that
//! best reply (the first alphabetically between equals), and each line's score is its worth plus a small amount fixed
//! by the position, less 25 for each line before it. So every search is deterministic, every move it names is legal,
//! giving material away is a drop, and its scores are the side to move's, as a real engine's are.
//!
//! Searches by `depth` or `nodes` report depths 1 to 3 at once; `infinite` (and `movetime`) report a deeper line every
//! 10 ms until `stop`.
//!
//! For tests of the order of searches: with `FAKE_EVAL_LOG` set to a file, each `ucinewgame` and `position` it gets is
//! appended to it as one line after its process id; with `FAKE_EVAL_DELAY_MS`, a search by `nodes` takes that long.

use std::{
    io::{BufRead, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

use dameless_core::chess::Position;

/// One line of a search: its moves and its score for the side to move.
struct Line {
    pv: Vec<String>,
    cp: i32,
}

/// What a mate is worth.
const MATE: i32 = 20_000;

/// A move's worth for the side that plays it, and the opponent's best reply.
fn worth(pos: &Position, m: &str) -> (i32, Option<String>) {
    let mover = pos.turn();
    let mut after = pos.clone();
    after.play(m).expect("a legal move");
    if after.is_checkmate() {
        return (MATE, None);
    }
    let mut replies = after.legal_moves();
    replies.sort();
    let mut best: Option<(i32, String)> = None;
    for r in replies {
        let mut next = after.clone();
        next.play(&r).expect("a legal move");
        let value = if next.is_checkmate() { -MATE } else { next.material_balance(mover) * 100 };
        if best.as_ref().is_none_or(|(v, _)| value < *v) {
            best = Some((value, r));
        }
    }
    match best {
        Some((value, reply)) => (value, Some(reply)),
        None => (0, None),
    }
}

fn lines(pos: &Position, multipv: usize) -> Vec<Line> {
    let jitter = (pos.key().bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b))) % 11) as i32 - 5;
    let mut moves: Vec<(i32, String, Vec<String>)> = pos
        .legal_moves()
        .into_iter()
        .map(|m| {
            let (value, reply) = worth(pos, &m);
            let pv = std::iter::once(m.clone()).chain(reply).collect();
            (value, m, pv)
        })
        .collect();
    moves.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    moves
        .into_iter()
        .take(multipv)
        .enumerate()
        .map(|(k, (value, _, pv))| Line { pv, cp: value + jitter - 25 * k as i32 })
        .collect()
}

fn report(out: &mut impl Write, lines: &[Line], depth: u64, nodes: u64) {
    for (k, line) in lines.iter().enumerate() {
        writeln!(
            out,
            "info depth {depth} seldepth {} multipv {} score cp {} nodes {nodes} nps 1000000 time 1 pv {}",
            depth + 2,
            k + 1,
            line.cp,
            line.pv.join(" ")
        )
        .unwrap();
    }
}

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

    // Opened for each line, so a test may remove it between parts.
    let log = std::env::var_os("FAKE_EVAL_LOG");
    let delay = std::env::var("FAKE_EVAL_DELAY_MS").ok().and_then(|ms| ms.parse().ok()).map(Duration::from_millis);
    let mut out = std::io::stdout().lock();
    let mut multipv = 1usize;
    let mut pos = Position::start();
    while let Ok(line) = rx.recv() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(path) = &log
            && matches!(words.first(), Some(&("ucinewgame" | "position")))
        {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("the log");
            // One write, so engines writing at once do not mix their lines.
            file.write_all(format!("{} {line}\n", std::process::id()).as_bytes()).unwrap();
        }
        match words.as_slice() {
            ["uci"] => {
                writeln!(out, "id name FakeEval 1.0").unwrap();
                writeln!(out, "option name Hash type spin default 16 min 1 max 1024").unwrap();
                writeln!(out, "option name Threads type spin default 1 min 1 max 64").unwrap();
                writeln!(out, "option name MultiPV type spin default 1 min 1 max 16").unwrap();
                writeln!(out, "uciok").unwrap();
            }
            ["isready"] => writeln!(out, "readyok").unwrap(),
            ["setoption", "name", "MultiPV", "value", n] => multipv = n.parse().unwrap_or(1),
            ["position", rest @ ..] => {
                let at = rest.iter().position(|w| *w == "moves");
                let head = &rest[..at.unwrap_or(rest.len())];
                let start = match head {
                    ["fen", fen @ ..] => Position::from_fen(&fen.join(" ")).expect("a legal FEN"),
                    _ => Position::start(),
                };
                pos = start;
                for m in at.map_or(&[][..], |i| &rest[i + 1..]) {
                    pos.play(m).expect("a legal move");
                }
            }
            ["go", rest @ ..] => {
                let value =
                    |key: &str| rest.iter().position(|w| *w == key).and_then(|i| rest.get(i + 1)?.parse::<u64>().ok());
                let found = lines(&pos, multipv);
                if found.is_empty() {
                    writeln!(out, "info depth 0 score {}", if pos.is_checkmate() { "mate 0" } else { "cp 0" }).unwrap();
                    writeln!(out, "bestmove (none)").unwrap();
                    out.flush().unwrap();
                    continue;
                }
                if rest.contains(&"infinite") || value("movetime").is_some() {
                    let until = value("movetime").map(|ms| Instant::now() + Duration::from_millis(ms));
                    let mut depth = 1;
                    report(&mut out, &found, depth, 100);
                    out.flush().unwrap();
                    loop {
                        if until.is_some_and(|t| Instant::now() >= t) {
                            break;
                        }
                        match rx.recv_timeout(Duration::from_millis(10)) {
                            Ok(l) if l.trim() == "stop" => break,
                            Ok(l) if l.trim() == "quit" => return,
                            Ok(l) if l.trim() == "isready" => writeln!(out, "readyok").unwrap(),
                            Ok(_) => {}
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                depth += 1;
                                report(&mut out, &found, depth, 100 * depth);
                                out.flush().unwrap();
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                } else {
                    let depth = value("depth").unwrap_or(3);
                    let nodes = value("nodes").unwrap_or(1000);
                    if let Some(delay) = delay.filter(|_| value("nodes").is_some()) {
                        std::thread::sleep(delay);
                    }
                    for d in 1..=depth {
                        report(&mut out, &found, d, nodes * d / depth);
                    }
                }
                writeln!(out, "bestmove {}", found[0].pv[0]).unwrap();
            }
            ["quit"] => return,
            _ => {}
        }
        out.flush().unwrap();
    }
}
