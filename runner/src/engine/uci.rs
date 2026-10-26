//! Talking to an engine over UCI.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, sync::mpsc};
use utoipa::ToSchema;

use crate::proc;

/// A search score from the side to move's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Score {
    /// Centipawns.
    Cp(i32),
    /// Mate in this many moves; negative when the side to move is mated.
    Mate(i32),
}

/// One `info` line with a principal variation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Info {
    pub depth: u32,
    pub seldepth: u32,
    pub multipv: u32,
    pub score: Score,
    pub nodes: u64,
    pub nps: u64,
    /// Milliseconds since the search started.
    pub time: u64,
    /// The principal variation in UCI notation (`e2e4`).
    pub pv: Vec<String>,
}

impl Info {
    /// Parses an `info` line. Lines without a score and a PV (`info string`, currmove updates) are `None`.
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        if words.next()? != "info" {
            return None;
        }
        let (mut depth, mut seldepth, mut multipv, mut nodes, mut nps, mut time) = (0, 0, 1, 0, 0, 0);
        let mut score = None;
        let mut pv = Vec::new();
        while let Some(word) = words.next() {
            match word {
                "depth" => depth = words.next()?.parse().ok()?,
                "seldepth" => seldepth = words.next()?.parse().ok()?,
                "multipv" => multipv = words.next()?.parse().ok()?,
                "nodes" => nodes = words.next()?.parse().ok()?,
                "nps" => nps = words.next()?.parse().ok()?,
                "time" => time = words.next()?.parse().ok()?,
                "score" => {
                    score = match words.next()? {
                        "cp" => Some(Score::Cp(words.next()?.parse().ok()?)),
                        "mate" => Some(Score::Mate(words.next()?.parse().ok()?)),
                        _ => return None,
                    }
                }
                "pv" => {
                    pv = words.by_ref().map(str::to_owned).collect();
                }
                "string" => return None,
                _ => {}
            }
        }
        if pv.is_empty() {
            return None;
        }
        Some(Self { depth, seldepth, multipv, score: score?, nodes, nps, time, pv })
    }
}

/// How long to search.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct Limits {
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    /// Milliseconds.
    pub movetime: Option<u64>,
}

impl Limits {
    fn go(&self) -> String {
        let mut go = String::from("go");
        if let Some(d) = self.depth {
            go += &format!(" depth {d}");
        }
        if let Some(n) = self.nodes {
            go += &format!(" nodes {n}");
        }
        if let Some(t) = self.movetime {
            go += &format!(" movetime {t}");
        }
        if go == "go" {
            go += " infinite";
        }
        go
    }
}

pub struct Engine {
    child: proc::Child,
    lines: mpsc::UnboundedReceiver<String>,
    pub name: String,
}

impl Engine {
    pub async fn start(path: &Path) -> Result<Self> {
        let mut child = proc::spawn(proc::Spawn { program: path, args: &[], cwd: None, env: &[], stdin: true })?;
        let lines = proc::lines(&mut child);
        let mut engine = Self { child, lines, name: String::new() };
        engine.send("uci").await?;
        while let Some(line) = engine.lines.recv().await {
            if let Some(name) = line.strip_prefix("id name ") {
                engine.name = name.to_owned();
            }
            if line == "uciok" {
                return Ok(engine);
            }
        }
        bail!("{} exited before uciok", path.display())
    }

    /// Sends one command. A line break inside it (from a FEN or a move in a request) would smuggle in another, so it is
    /// refused.
    pub async fn send(&mut self, command: &str) -> Result<()> {
        if command.contains(['\n', '\r']) {
            bail!("a UCI command cannot contain a line break");
        }
        let stdin = self.child.stdin().as_mut().context("engine stdin closed")?;
        stdin.write_all(format!("{command}\n").as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn set_option(&mut self, name: &str, value: impl std::fmt::Display) -> Result<()> {
        self.send(&format!("setoption name {name} value {value}")).await
    }

    pub async fn ready(&mut self) -> Result<()> {
        self.send("isready").await?;
        while let Some(line) = self.lines.recv().await {
            if line == "readyok" {
                return Ok(());
            }
        }
        bail!("engine exited before readyok")
    }

    /// Sets the position (a FEN, or `startpos`) and starts a search. Read its output with [`Engine::next`].
    pub async fn go(&mut self, fen: &str, moves: &[String], limits: &Limits) -> Result<()> {
        self.go_command(fen, moves, &limits.go()).await
    }

    /// Like [`Engine::go`] with the whole `go` command given, for clocks and other limits `Limits` does not carry.
    pub async fn go_command(&mut self, fen: &str, moves: &[String], command: &str) -> Result<()> {
        let mut position =
            if fen == "startpos" { "position startpos".to_owned() } else { format!("position fen {fen}") };
        if !moves.is_empty() {
            position += " moves ";
            position += &moves.join(" ");
        }
        self.send(&position).await?;
        self.send(command).await
    }

    /// The next event of a running search.
    pub async fn next(&mut self) -> Result<Search> {
        while let Some(line) = self.lines.recv().await {
            if let Some(best) = line.strip_prefix("bestmove ") {
                return Ok(Search::Done(best.split_whitespace().next().unwrap_or_default().to_owned()));
            }
            if let Some(info) = Info::parse(&line) {
                return Ok(Search::Info(info));
            }
        }
        bail!("engine exited during the search")
    }
}

pub enum Search {
    Info(Info),
    Done(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_info_lines() {
        let line = "info depth 10 seldepth 13 multipv 1 score cp 42 nodes 15690 time 61 nps 254110 hashfull 4 tbhits 0 pv e2e4 e7e6";
        let info = Info::parse(line).unwrap();
        assert_eq!(info.depth, 10);
        assert_eq!(info.score, Score::Cp(42));
        assert_eq!(info.pv, vec!["e2e4", "e7e6"]);
        assert_eq!(Info::parse("info depth 3 score mate -2 pv a1a2").unwrap().score, Score::Mate(-2));
        assert_eq!(Info::parse("info string NNUE loaded"), None);
        assert_eq!(Info::parse("info depth 5 currmove e2e4 currmovenumber 1"), None);
    }

    #[test]
    fn builds_go_commands() {
        assert_eq!(Limits::default().go(), "go infinite");
        assert_eq!(Limits { depth: Some(12), ..Default::default() }.go(), "go depth 12");
    }
}
