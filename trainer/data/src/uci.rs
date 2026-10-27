//! A small synchronous UCI driver: one engine process, its output read by a thread into a channel so every wait has a
//! timeout. Datagen and rescoring each run one per worker thread.
//!
//! This workspace does not use the app's `dameless-engine` (async, tokio, and it pulls the app's build and download
//! code); what datagen needs from UCI is `position`, `go` and the last score.

use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    /// Moves to mate, negative when being mated.
    Mate(i32),
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub bestmove: String,
    pub score: Option<Score>,
    pub depth: u32,
    pub nodes: u64,
}

pub struct Engine {
    path: PathBuf,
    child: Child,
    stdin: BufWriter<ChildStdin>,
    lines: Receiver<String>,
    /// Option names the engine declared after `uci`.
    pub options: Vec<String>,
    pub name: String,
}

pub const START_TIMEOUT: Duration = Duration::from_secs(20);

impl Engine {
    pub fn start(path: &Path) -> Result<Self> {
        let mut command = Command::new(path);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().with_context(|| format!("start {}", path.display()))?;
        let stdout = child.stdout.take().expect("piped");
        let stdin = BufWriter::new(child.stdin.take().expect("piped"));
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut engine =
            Self { path: path.to_path_buf(), child, stdin, lines, options: Vec::new(), name: String::new() };
        engine.send("uci")?;
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            let line = engine.recv_until(deadline)?;
            if let Some(rest) = line.strip_prefix("id name ") {
                engine.name = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("option name ") {
                let name = rest.split(" type ").next().unwrap_or(rest).trim();
                engine.options.push(name.to_string());
            } else if line.trim() == "uciok" {
                break;
            }
        }
        Ok(engine)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn has_option(&self, name: &str) -> bool {
        self.options.iter().any(|o| o.eq_ignore_ascii_case(name))
    }

    pub fn send(&mut self, command: &str) -> Result<()> {
        writeln!(self.stdin, "{command}")?;
        self.stdin.flush().with_context(|| format!("engine {} closed its input", self.path.display()))?;
        Ok(())
    }

    fn recv_until(&mut self, deadline: Instant) -> Result<String> {
        let wait = deadline.saturating_duration_since(Instant::now());
        match self.lines.recv_timeout(wait) {
            Ok(line) => Ok(line),
            Err(RecvTimeoutError::Timeout) => bail!("engine {} did not answer in time", self.path.display()),
            Err(RecvTimeoutError::Disconnected) => bail!("engine {} exited", self.path.display()),
        }
    }

    pub fn set_option(&mut self, name: &str, value: impl std::fmt::Display) -> Result<()> {
        self.send(&format!("setoption name {name} value {value}"))
    }

    pub fn ready(&mut self) -> Result<()> {
        self.send("isready")?;
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            if self.recv_until(deadline)?.trim() == "readyok" {
                return Ok(());
            }
        }
    }

    pub fn new_game(&mut self) -> Result<()> {
        self.send("ucinewgame")?;
        self.ready()
    }

    /// Searches `position` (a full `position ...` command) with `go` (e.g. `go nodes 5000`), returning the best move
    /// and the score of the last `info` line that has one (bounds are skipped when an exact score exists).
    pub fn search(&mut self, position: &str, go: &str, timeout: Duration) -> Result<SearchResult> {
        self.send(position)?;
        self.send(go)?;
        let deadline = Instant::now() + timeout;
        let mut result = SearchResult { bestmove: String::new(), score: None, depth: 0, nodes: 0 };
        let mut score_is_bound = false;
        loop {
            let line = match self.recv_until(deadline) {
                Ok(line) => line,
                Err(e) => {
                    let _ = self.send("stop");
                    return Err(e);
                }
            };
            let mut words = line.split_whitespace();
            match words.next() {
                Some("info") => {
                    let tokens: Vec<&str> = words.collect();
                    let mut i = 0;
                    let mut score = None;
                    let mut bound = false;
                    while i < tokens.len() {
                        match tokens[i] {
                            "depth" => result.depth = tokens.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(0),
                            "nodes" => result.nodes = tokens.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(0),
                            "score" => {
                                let value = tokens.get(i + 2).and_then(|v| v.parse::<i32>().ok());
                                score = match (tokens.get(i + 1).copied(), value) {
                                    (Some("cp"), Some(v)) => Some(Score::Cp(v)),
                                    (Some("mate"), Some(v)) => Some(Score::Mate(v)),
                                    _ => None,
                                };
                                if matches!(tokens.get(i + 3).copied(), Some("lowerbound" | "upperbound")) {
                                    bound = true;
                                }
                            }
                            "pv" | "string" => break,
                            _ => {}
                        }
                        i += 1;
                    }
                    if let Some(s) = score
                        && (!bound || result.score.is_none() || score_is_bound)
                    {
                        result.score = Some(s);
                        score_is_bound = bound;
                    }
                }
                Some("bestmove") => {
                    result.bestmove = words.next().ok_or_else(|| anyhow!("bestmove without a move"))?.to_string();
                    return Ok(result);
                }
                _ => {}
            }
        }
    }

    pub fn quit(mut self) {
        let _ = self.send("quit");
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = writeln!(self.stdin, "quit");
            let _ = self.stdin.flush();
            std::thread::sleep(Duration::from_millis(20));
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// `position fen <fen> [moves ...]`.
pub fn position_command(fen: &str, moves: &[String]) -> String {
    if moves.is_empty() {
        format!("position fen {fen}")
    } else {
        format!("position fen {fen} moves {}", moves.join(" "))
    }
}
